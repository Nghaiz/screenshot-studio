//! Targeted catalog updates for the live watcher (phase 4.1): sync a handful of named files of one root
//! (new, changed, gone) without walking the whole root, and carry an Explorer rename over to the row so
//! its tags, notes and AI results stay with the file. Same rules as [`super::scan_root`]: cloud-only
//! files are listed from metadata and never opened (rule 9).

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{OptionalExtension, params};

use crate::catalog::{Catalog, WRITE_BATCH};
use crate::contract::AppError;
use crate::contract::ipc::{CloudState, LibraryRoot};
use crate::errors::{enum_str, system_time_ms};
use crate::fsaccess::FileAccess;
use crate::ops::organize::fts_set;

use super::{Change, Existing, Found, has_image_extension, header_dims, target_thumb, write_batch};

/// What one [`sync_paths`] call changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// Rows inserted for files the catalog did not know.
    pub added_ids: Vec<i64>,
    /// Rows whose file changed, came back, or went missing.
    pub changed_ids: Vec<i64>,
}

impl SyncReport {
    pub fn is_empty(&self) -> bool {
        self.added_ids.is_empty() && self.changed_ids.is_empty()
    }
}

/// `/`-separated folder of `path`'s parent inside `root` and the file name; None when `path` is not under it.
pub fn relative(root: &Path, path: &Path) -> Option<(String, String)> {
    let rest = strip_root(root, path)?;
    let parts: Vec<String> = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let (name, folder) = parts.split_last()?;
    Some((folder.join("/"), name.clone()))
}

/// `path` without the `root` prefix (case-insensitive, as Windows paths are).
fn strip_root<'a>(root: &Path, path: &'a Path) -> Option<&'a Path> {
    let n = root.components().count();
    let same = root.components().zip(path.components()).take(n).all(|(a, b)| {
        a.as_os_str().to_string_lossy().eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
    });
    if !same || path.components().count() <= n {
        return None;
    }
    let mut it = path.components();
    for _ in 0..n {
        it.next();
    }
    Some(it.as_path())
}

/// Brings the rows of `paths` (files of `root`, existing or not) in line with the disk: a new image file
/// gets a row, a changed one is re-read (thumbnail `pending` again), a vanished one is marked `missing`.
/// Paths outside the root or without an image extension are ignored.
pub fn sync_paths(
    cat: &Catalog,
    fa: &dyn FileAccess,
    root: &LibraryRoot,
    paths: &[&Path],
    auto_download_cloud: bool,
) -> Result<SyncReport, AppError> {
    let root_dir = Path::new(&root.path);
    let mut keyed: Vec<((String, String), &Path)> = paths
        .iter()
        .filter_map(|p| relative(root_dir, p).filter(|(_, name)| has_image_extension(name)).map(|k| (k, *p)))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.dedup_by(|a, b| a.0 == b.0);
    if keyed.is_empty() {
        return Ok(SyncReport::default());
    }
    let root_id = root.id;
    let keys: Vec<(String, String)> = keyed.iter().map(|(k, _)| k.clone()).collect();
    let mut existing: HashMap<(String, String), Existing> = cat.read(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, size_bytes, mtime_ms, cloud_state, thumb_state, missing
             FROM image WHERE root_id = ?1 AND folder = ?2 AND file_name = ?3",
        )?;
        let mut out = HashMap::new();
        for (folder, name) in keys {
            let row = stmt
                .query_row(params![root_id, folder, name], |r| {
                    Ok(Existing {
                        id: r.get(0)?,
                        size: r.get(1)?,
                        mtime_ms: r.get(2)?,
                        cloud: r.get(3)?,
                        thumb: r.get(4)?,
                        missing: r.get::<_, i64>(5)? != 0,
                    })
                })
                .optional()?;
            if let Some(e) = row {
                out.insert((folder, name), e);
            }
        }
        Ok(out)
    })?;

    let mut changes = Vec::new();
    let mut changed_ids = Vec::new();
    for ((folder, file_name), path) in keyed {
        let old = existing.remove(&(folder.clone(), file_name.clone()));
        let meta = match std::fs::metadata(path) {
            Ok(m) if m.is_file() => m,
            _ => {
                if let Some(e) = old.filter(|e| !e.missing) {
                    changes.push(Change::Missing(e.id));
                    changed_ids.push(e.id);
                }
                continue;
            }
        };
        let cloud = fa.cloud_state(path, &meta);
        let f = Found {
            folder,
            file_name,
            abs: path.to_path_buf(),
            size: meta.len() as i64,
            mtime_ms: meta.modified().ok().and_then(system_time_ms).unwrap_or(0),
            created_ms: meta.created().ok().and_then(system_time_ms),
            cloud,
        };
        let dims = || if cloud == CloudState::CloudOnly { None } else { header_dims(fa, path) };
        match old {
            None => changes.push(Change::New(f.clone_shallow(), dims())),
            Some(e) if e.size != f.size || e.mtime_ms != f.mtime_ms => {
                changed_ids.push(e.id);
                changes.push(Change::Changed(e.id, f.clone_shallow(), dims()));
            }
            Some(e) => {
                let target = enum_str(&target_thumb(cloud, auto_download_cloud));
                let thumb = match (e.thumb.as_str(), target.as_str()) {
                    ("offline", "pending") | ("pending", "offline") => target,
                    _ => e.thumb.clone(),
                };
                if e.missing || enum_str(&cloud) != e.cloud || thumb != e.thumb {
                    changed_ids.push(e.id);
                    changes.push(Change::Touch { id: e.id, cloud, thumb });
                }
            }
        }
    }
    let mut report = SyncReport { added_ids: Vec::new(), changed_ids };
    let mut iter = changes.into_iter().peekable();
    while iter.peek().is_some() {
        let batch: Vec<Change> = iter.by_ref().take(WRITE_BATCH).collect();
        let delta = cat.write(move |tx| write_batch(tx, root_id, auto_download_cloud, batch))?;
        report.added_ids.extend(delta.added_ids);
    }
    Ok(report)
}

impl Found {
    fn clone_shallow(&self) -> Found {
        Found {
            folder: self.folder.clone(),
            file_name: self.file_name.clone(),
            abs: self.abs.clone(),
            size: self.size,
            mtime_ms: self.mtime_ms,
            created_ms: self.created_ms,
            cloud: self.cloud,
        }
    }
}

/// Carries a rename done outside the app (Explorer) over to the catalog: the row of `from` (a file) or the
/// rows under `from` (a folder) take the new location inside the same root. Returns the ids moved; nothing
/// moves when the destination already has a row (the caller's [`sync_paths`] then sorts it out).
pub fn apply_rename(cat: &Catalog, root: &LibraryRoot, from: &Path, to: &Path) -> Result<Vec<i64>, AppError> {
    let root_dir = Path::new(&root.path);
    let (Some(from_rel), Some(to_rel)) = (relative(root_dir, from), relative(root_dir, to)) else {
        return Ok(Vec::new());
    };
    let root_id = root.id;
    let is_dir = to.is_dir();
    cat.write(move |tx| {
        let mut moved = Vec::new();
        if !is_dir {
            let (from_folder, from_name) = from_rel;
            let (to_folder, to_name) = to_rel;
            let taken: bool = tx.query_row(
                "SELECT count(*) > 0 FROM image WHERE root_id = ?1 AND folder = ?2 AND file_name = ?3",
                params![root_id, to_folder, to_name],
                |r| r.get(0),
            )?;
            if taken || !has_image_extension(&to_name) {
                return Ok(moved);
            }
            let id: Option<i64> = tx
                .query_row(
                    "SELECT id FROM image WHERE root_id = ?1 AND folder = ?2 AND file_name = ?3",
                    params![root_id, from_folder, from_name],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = id {
                tx.execute(
                    "UPDATE image SET folder = ?2, file_name = ?3, missing = 0 WHERE id = ?1",
                    params![id, to_folder, to_name],
                )?;
                fts_set(tx, id, "file_name", &to_name)?;
                moved.push(id);
            }
            return Ok(moved);
        }
        // A folder: `from_rel`/`to_rel` are (parent folder, folder name); rows live under parent/name.
        let join = |(parent, name): &(String, String)| if parent.is_empty() { name.clone() } else { format!("{parent}/{name}") };
        let (old, new) = (join(&from_rel), join(&to_rel));
        let rows: Vec<(i64, String, String)> = {
            let mut stmt = tx.prepare("SELECT id, folder, file_name FROM image WHERE root_id = ?1")?;
            let it = stmt.query_map([root_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            it.collect::<Result<Vec<_>, _>>()?
        };
        let old_prefix = format!("{}/", old.to_lowercase());
        for (id, folder, name) in rows {
            let lower = folder.to_lowercase();
            let rest = if lower == old.to_lowercase() {
                String::new()
            } else if lower.starts_with(&old_prefix) {
                folder[old_prefix.len()..].to_owned()
            } else {
                continue;
            };
            let dest = if rest.is_empty() { new.clone() } else { format!("{new}/{rest}") };
            let taken: bool = tx.query_row(
                "SELECT count(*) > 0 FROM image WHERE root_id = ?1 AND folder = ?2 AND file_name = ?3",
                params![root_id, dest, name],
                |r| r.get(0),
            )?;
            if !taken {
                tx.execute("UPDATE image SET folder = ?2, missing = 0 WHERE id = ?1", params![id, dest])?;
                moved.push(id);
            }
        }
        Ok(moved)
    })
}
