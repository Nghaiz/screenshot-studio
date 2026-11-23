//! File operations: move/copy, rename (+ pattern preview), in-app trash/restore, OS Recycle Bin.
//! Each item: guard (rule 8), file step, then all DB steps in one transaction with the journal
//! batch ([`commit_steps`]); per-item failures are `OpFailure`s, never a silent skip.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Transaction, params};
use serde_json::json;

use crate::catalog::{self, Catalog};
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller, ErrorKind};
use crate::errors::{invalid, not_found, now_ms, system_time_ms};
use crate::scan::has_image_extension;

use super::fsio::{self, FileAction};
use super::journal::{
    CopyPayload, Entry, Location, MovePayload, OldValue, RecyclePayload, RenamePayload, Step, commit_steps, write_batch,
};
use super::organize::fts_set;
use super::{ImageRow, failure, guard_file_write, load_rows, norm_path, not_found_failure, path_under, snapshot, unique_ids};

const NAME_MAX_CHARS: usize = 255;
const RENAME_SUFFIX_MAX: u32 = 9_999;
const RESERVED_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn conflict(msg: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::Conflict, msg)
}

/// `canonicalize` without the `\\?\` verbatim prefix (real on-disk case, long names).
fn canonical(p: &Path) -> PathBuf {
    match std::fs::canonicalize(p) {
        Ok(c) => {
            let s = c.to_string_lossy();
            if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
                PathBuf::from(format!(r"\\{rest}"))
            } else if let Some(rest) = s.strip_prefix(r"\\?\") {
                PathBuf::from(rest)
            } else {
                c
            }
        }
        Err(_) => p.to_path_buf(),
    }
}

/// The library root holding `dir` and the `/`-separated folder of `dir` inside it.
fn locate(dir: &Path, roots: &[LibraryRoot]) -> Option<(i64, String)> {
    roots.iter().find_map(|r| {
        let root = canonical(Path::new(&r.path));
        if !path_under(dir, &root) {
            return None;
        }
        let skip = root.components().count();
        let folder = dir.components().skip(skip).map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        Some((r.id, folder))
    })
}

/// Checks a new file name (Windows rules; must keep an indexed image extension).
pub(crate) fn check_file_name(name: &str) -> Result<(), AppError> {
    let bad = |why: &str| invalid(format!("invalid file name `{name}`: {why}"));
    if name.is_empty() || name.chars().count() > NAME_MAX_CHARS {
        return Err(bad("must be 1..255 characters"));
    }
    if name.chars().any(|c| c.is_control() || r#"<>:"/\|?*"#.contains(c)) {
        return Err(bad(r#"contains one of < > : " / \ | ? * or a control character"#));
    }
    if name.ends_with(' ') || name.ends_with('.') || name.starts_with(' ') {
        return Err(bad("starts with a space or ends with a space or a dot"));
    }
    let stem = name.split('.').next().unwrap_or_default().to_ascii_lowercase();
    if RESERVED_NAMES.contains(&stem.as_str()) {
        return Err(bad("is a reserved device name"));
    }
    if !has_image_extension(name) {
        return Err(bad("must keep an image extension (png, jpg, jpeg, webp, gif, bmp)"));
    }
    Ok(())
}

/// Lower-case names the catalog holds in one folder of a root.
fn catalog_names(cat: &Catalog, root_id: i64, folder: &str) -> Result<HashSet<String>, AppError> {
    let folder = folder.to_owned();
    cat.read(move |c| {
        let mut stmt = c.prepare("SELECT file_name FROM image WHERE root_id = ?1 AND folder = ?2")?;
        let names = stmt.query_map(params![root_id, folder], |r| r.get::<_, String>(0))?;
        Ok(names.map(|n| n.map(|n| n.to_lowercase())).collect::<Result<_, _>>()?)
    })
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i + 1..]),
        _ => (name, ""),
    }
}

// --- move / copy -----------------------------------------------------------------------------

pub fn move_images(caller: Caller, cat: &Catalog, req: MoveImagesReq) -> Result<OpResult, AppError> {
    let raw = PathBuf::from(req.dest_dir.trim());
    if !raw.is_absolute() {
        return Err(invalid(format!("destination must be an absolute folder path: {}", req.dest_dir)));
    }
    if !raw.is_dir() {
        return Err(not_found(format!("destination folder not found: {}", raw.display())));
    }
    let dest = canonical(&raw);
    let roots = cat.read(catalog::list_roots)?;
    let target = locate(&dest, &roots);
    let mut taken = match &target {
        Some((root, folder)) => catalog_names(cat, *root, folder)?,
        None => HashSet::new(),
    };
    let ids = unique_ids(&req.image_ids);
    let rows = cat.read(|c| load_rows(c, &ids))?;
    let (mut steps, mut failed) = (Vec::new(), Vec::new());
    for id in ids {
        let Some(row) = rows.get(&id).cloned() else {
            failed.push(not_found_failure(id));
            continue;
        };
        match move_one(caller, &roots, &dest, target.as_ref(), &mut taken, req.mode, req.on_conflict, row, cat.data_dir()) {
            Ok(step) => steps.push(step),
            Err(e) => failed.push(failure(id, &e)),
        }
    }
    commit_steps(cat, steps, failed).map(|(r, _)| r)
}

/// Picks a free name in `dest` (on disk and in the catalog), reserving it in `taken`.
fn free_name(name: &str, dest: &Path, taken: &mut HashSet<String>, on_conflict: OnConflict) -> Result<String, AppError> {
    let free = |n: &str, taken: &HashSet<String>| !taken.contains(&n.to_lowercase()) && !dest.join(n).exists();
    let chosen = if free(name, taken) {
        name.to_owned()
    } else {
        if on_conflict == OnConflict::Skip {
            return Err(conflict(format!("`{name}` already exists in {}", dest.display())));
        }
        let (stem, ext) = split_ext(name);
        (1..=RENAME_SUFFIX_MAX)
            .map(|k| if ext.is_empty() { format!("{stem} ({k})") } else { format!("{stem} ({k}).{ext}") })
            .find(|n| free(n, taken))
            .ok_or_else(|| conflict(format!("no free name for `{name}` in {}", dest.display())))?
    };
    taken.insert(chosen.to_lowercase());
    Ok(chosen)
}

#[allow(clippy::too_many_arguments)]
fn move_one(
    caller: Caller,
    roots: &[LibraryRoot],
    dest: &Path,
    target: Option<&(i64, String)>,
    taken: &mut HashSet<String>,
    mode: MoveMode,
    on_conflict: OnConflict,
    row: ImageRow,
    data_dir: &Path,
) -> Result<Step, AppError> {
    let src = row.abs();
    guard_file_write(caller, roots, &[&src, dest])?;
    if row.missing || !src.is_file() {
        return Err(not_found(format!("file not found: {}", src.display())));
    }
    if mode == MoveMode::Move && norm_path(&canonical(&row.dir())) == norm_path(dest) {
        return Err(invalid("the image is already in that folder"));
    }
    let name = free_name(&row.file_name, dest, taken, on_conflict)?;
    let to = dest.join(&name);
    let from_loc =
        Location { abs: src.to_string_lossy().into_owned(), root_id: Some(row.root_id), folder: row.folder.clone(), file_name: row.file_name.clone() };
    let to_loc = Location {
        abs: to.to_string_lossy().into_owned(),
        root_id: target.map(|t| t.0),
        folder: target.map(|t| t.1.clone()).unwrap_or_default(),
        file_name: name.clone(),
    };
    let id = row.id;
    let data_dir = data_dir.to_path_buf();
    match mode {
        MoveMode::Move => {
            fsio::move_file(&src, &to)?;
            let action = FileAction::Moved { from: src, to };
            let db: super::journal::DbStep = Box::new(move |tx: &Transaction<'_>| {
                match to_loc.root_id {
                    Some(root) => {
                        tx.execute(
                            "UPDATE image SET root_id = ?2, folder = ?3, file_name = ?4, missing = 0 WHERE id = ?1",
                            params![id, root, to_loc.folder, to_loc.file_name],
                        )?;
                        fts_set(tx, id, "file_name", &to_loc.file_name)?;
                        let p = MovePayload { from: from_loc, to: to_loc, snapshot: None };
                        Ok((Entry::new(JournalOp::Move, id, p), None))
                    }
                    None => {
                        let snap = snapshot::take(tx, id)?;
                        snapshot::remove(tx, id)?;
                        let p = MovePayload { from: from_loc, to: to_loc, snapshot: Some(snap) };
                        let after: super::journal::AfterCommit = Box::new(move |batch| snapshot::park_thumbs(&data_dir, batch, id));
                        Ok((Entry::new(JournalOp::Move, id, p), Some(after)))
                    }
                }
            });
            Ok(Step { image_id: id, action, db })
        }
        MoveMode::Copy => {
            fsio::copy_file(&src, &to)?;
            let meta = std::fs::metadata(&to)?;
            let (size, mtime) = (meta.len() as i64, meta.modified().ok().and_then(system_time_ms).unwrap_or(0));
            let action = FileAction::Copied { to: to.clone() };
            let db: super::journal::DbStep = Box::new(move |tx: &Transaction<'_>| {
                let new_image_id = match to_loc.root_id {
                    Some(root) => Some(insert_copy_row(tx, id, root, &to_loc, size, mtime, &data_dir)?),
                    None => None,
                };
                let p = CopyPayload { to_abs: to_loc.abs.clone(), new_image_id, size_bytes: size, mtime_ms: mtime };
                Ok((Entry::new(JournalOp::Copy, id, p), None))
            });
            Ok(Step { image_id: id, action, db })
        }
    }
}

/// The catalog row of a copy inside a library root: the source's metadata, its thumbnails copied.
fn insert_copy_row(
    tx: &Transaction<'_>,
    src_id: i64,
    root: i64,
    to: &Location,
    size: i64,
    mtime: i64,
    data_dir: &Path,
) -> Result<i64, AppError> {
    tx.execute(
        "INSERT INTO image (root_id, folder, file_name, size_bytes, mtime_ms, created_ms, taken_ms, width, height,
                            content_hash, dhash, phash, blur_score, dominant_rgb, thumb_state, cloud_state, indexed_at)
         SELECT ?2, ?3, ?4, ?5, ?6, created_ms, taken_ms, width, height, content_hash, dhash, phash, blur_score,
                dominant_rgb, 'pending', 'local', ?7 FROM image WHERE id = ?1",
        params![src_id, root, to.folder, to.file_name, size, mtime, now_ms()],
    )?;
    let new_id = tx.last_insert_rowid();
    catalog::fts_insert(tx, new_id, &to.file_name)?;
    let copied = crate::thumbs::copy_thumbs(data_dir, src_id, new_id);
    if copied {
        tx.execute("UPDATE image SET thumb_state = 'ok' WHERE id = ?1", [new_id])?;
    }
    Ok(new_id)
}

// --- rename ----------------------------------------------------------------------------------

pub fn rename_images(caller: Caller, cat: &Catalog, req: RenameImagesReq) -> Result<OpResult, AppError> {
    let roots = cat.read(catalog::list_roots)?;
    let ids: Vec<i64> = req.items.iter().map(|i| i.image_id).collect();
    if unique_ids(&ids).len() != ids.len() {
        return Err(invalid("an image appears twice in the rename list"));
    }
    let rows = cat.read(|c| load_rows(c, &ids))?;
    let mut reserved: HashSet<(i64, String, String)> = HashSet::new();
    let (mut steps, mut failed) = (Vec::new(), Vec::new());
    for item in req.items {
        let Some(row) = rows.get(&item.image_id).cloned() else {
            failed.push(not_found_failure(item.image_id));
            continue;
        };
        match rename_one(caller, cat, &roots, &mut reserved, row, item.new_name.trim()) {
            Ok(Some(step)) => steps.push(step),
            Ok(None) => {}
            Err(e) => failed.push(failure(item.image_id, &e)),
        }
    }
    commit_steps(cat, steps, failed).map(|(r, _)| r)
}

fn rename_one(
    caller: Caller,
    cat: &Catalog,
    roots: &[LibraryRoot],
    reserved: &mut HashSet<(i64, String, String)>,
    row: ImageRow,
    new_name: &str,
) -> Result<Option<Step>, AppError> {
    check_file_name(new_name)?;
    if new_name == row.file_name {
        return Ok(None);
    }
    let (dir, old) = (row.dir(), row.abs());
    let new = dir.join(new_name);
    guard_file_write(caller, roots, &[&old, &new])?;
    if row.missing || !old.is_file() {
        return Err(not_found(format!("file not found: {}", old.display())));
    }
    let case_only = new_name.to_lowercase() == row.file_name.to_lowercase();
    let in_catalog = catalog_names(cat, row.root_id, &row.folder)?.contains(&new_name.to_lowercase());
    if !case_only && (new.exists() || in_catalog) {
        return Err(conflict(format!("`{new_name}` already exists in {}", dir.display())));
    }
    if !reserved.insert((row.root_id, row.folder.clone(), new_name.to_lowercase())) {
        return Err(conflict(format!("two images would be renamed to `{new_name}`")));
    }
    fsio::move_file(&old, &new)?;
    let (id, old_name, new_name) = (row.id, row.file_name.clone(), new_name.to_owned());
    let payload = RenamePayload { dir: dir.to_string_lossy().into_owned(), old_name, new_name: new_name.clone() };
    let db: super::journal::DbStep = Box::new(move |tx: &Transaction<'_>| {
        tx.execute("UPDATE image SET file_name = ?2 WHERE id = ?1", params![id, new_name])?;
        fts_set(tx, id, "file_name", &new_name)?;
        Ok((Entry::new(JournalOp::Rename, id, payload), None))
    });
    Ok(Some(Step { image_id: id, action: FileAction::Moved { from: old, to: new }, db }))
}

/// Renders `pattern` for every image (1-based `{n}` in request order); `conflict` marks a name that
/// is invalid or collides with a file, a catalog row or another new name in the same folder.
pub fn preview_rename(cat: &Catalog, req: PreviewRenameReq) -> Result<RenamePreview, AppError> {
    super::pattern::validate(&req.pattern)?;
    let ids = unique_ids(&req.image_ids);
    let rows = cat.read(|c| load_rows(c, &ids))?;
    let times = cat.read(|c| super::pattern::local_times(c, &ids))?;
    let mut seen: HashSet<(i64, String, String)> = HashSet::new();
    let mut items = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let row = rows.get(id).ok_or_else(|| not_found(format!("image {id} not found")))?;
        let new_name = super::pattern::render(&req.pattern, row, i as i64 + 1, times.get(id))?;
        let key = (row.root_id, row.folder.clone(), new_name.to_lowercase());
        let unchanged = new_name == row.file_name;
        let case_only = !unchanged && new_name.to_lowercase() == row.file_name.to_lowercase();
        let clash = !unchanged
            && !case_only
            && (row.dir().join(&new_name).exists() || catalog_names(cat, row.root_id, &row.folder)?.contains(&key.2));
        let conflict = check_file_name(&new_name).is_err() || clash || !seen.insert(key);
        items.push(RenamePreviewItem { image_id: *id, old_name: row.file_name.clone(), new_name, conflict });
    }
    Ok(RenamePreview { items })
}

// --- in-app trash / restore ------------------------------------------------------------------

fn set_trashed(cat: &Catalog, ids: &[i64], trash: bool) -> Result<OpResult, AppError> {
    let ids = unique_ids(ids);
    cat.write(move |tx| {
        let rows = load_rows(tx, &ids)?;
        let now = now_ms();
        let (mut entries, mut failed) = (Vec::new(), Vec::new());
        for id in &ids {
            let Some(row) = rows.get(id) else {
                failed.push(not_found_failure(*id));
                continue;
            };
            let (op, new) = match (trash, row.trashed_at) {
                (true, None) => (JournalOp::Trash, Some(now)),
                (false, Some(_)) => (JournalOp::Restore, None),
                _ => continue,
            };
            tx.execute("UPDATE image SET trashed_at = ?2 WHERE id = ?1", params![id, new])?;
            entries.push(Entry::new(op, *id, OldValue { old: json!(row.trashed_at) }));
        }
        let batch_id = write_batch(tx, &entries)?;
        Ok(OpResult { batch_id, affected: entries.len() as i64, failed })
    })
}

/// In-app trash: sets `trashedAt`, the file is untouched.
pub fn trash_images(caller: Caller, cat: &Catalog, req: ImageIdsReq) -> Result<OpResult, AppError> {
    let _ = caller;
    set_trashed(cat, &req.image_ids, true)
}

pub fn restore_images(caller: Caller, cat: &Catalog, req: ImageIdsReq) -> Result<OpResult, AppError> {
    let _ = caller;
    set_trashed(cat, &req.image_ids, false)
}

// --- OS Recycle Bin --------------------------------------------------------------------------

/// Sends the files to the OS Recycle Bin and removes their rows (snapshotted for `undo`).
pub fn recycle_images(caller: Caller, cat: &Catalog, req: ImageIdsReq) -> Result<OpResult, AppError> {
    let roots = cat.read(catalog::list_roots)?;
    let ids = unique_ids(&req.image_ids);
    let rows = cat.read(|c| load_rows(c, &ids))?;
    let (mut steps, mut failed) = (Vec::new(), Vec::new());
    for id in ids {
        let Some(row) = rows.get(&id) else {
            failed.push(not_found_failure(id));
            continue;
        };
        let abs = row.abs();
        let res = guard_file_write(caller, &roots, &[&abs]).and_then(|_| fsio::recycle(&abs));
        let (from_s, to_s) = match res {
            Ok(w) => w,
            Err(e) => {
                failed.push(failure(id, &e));
                continue;
            }
        };
        let data_dir = cat.data_dir().to_path_buf();
        let abs_s = abs.to_string_lossy().into_owned();
        let db: super::journal::DbStep = Box::new(move |tx: &Transaction<'_>| {
            let snap = snapshot::take(tx, id)?;
            snapshot::remove(tx, id)?;
            let p = RecyclePayload { abs: abs_s, deleted_from_s: from_s, deleted_to_s: to_s, snapshot: snap };
            let after: super::journal::AfterCommit = Box::new(move |batch| snapshot::park_thumbs(&data_dir, batch, id));
            Ok((Entry::new(JournalOp::Recycle, id, p), Some(after)))
        });
        steps.push(Step { image_id: id, action: FileAction::Recycled { path: abs, from_s, to_s }, db });
    }
    commit_steps(cat, steps, failed).map(|(r, _)| r)
}

