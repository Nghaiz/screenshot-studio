//! The undo journal (`op_journal`): one batch per mutation call, one row per changed image, written
//! in the same transaction as the change. `undo` replays the inverse of a batch's rows (newest
//! first): file step first, then one transaction; rows whose DB step fails get their file step
//! reversed. `purge` batches are never undoable.

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::Value as Json;

use crate::catalog::Catalog;
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller, ErrorKind};
use crate::errors::{enum_str, internal, invalid, not_found, now_ms, parse_enum};

use super::edit::{self, ConvertPayload, OverwritePayload};
use super::fsio::{self, FileAction};
use super::snapshot::{self, Snapshot};
use super::{failure, guard_file_write};

pub const JOURNAL_LIMIT_MAX: i64 = 500;

/// One journal row to write.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub op: JournalOp,
    pub image_id: Option<i64>,
    pub payload: Json,
}

impl Entry {
    pub fn new(op: JournalOp, image_id: i64, payload: impl Serialize) -> Self {
        Self { op, image_id: Some(image_id), payload: serde_json::to_value(payload).expect("payload serializes") }
    }
}

// --- payloads (everything undo needs) --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagPayload {
    pub tag_id: i64,
}

/// favorite (bool), rating (number), note (string|null), trash/restore (trashedAt number|null).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OldValue {
    pub old: Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlbumPayload {
    pub album_id: i64,
    /// Position before the change (albumRemove, albumReorder).
    pub position: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Location {
    pub abs: String,
    /// None = outside every library root.
    pub root_id: Option<i64>,
    pub folder: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MovePayload {
    pub from: Location,
    pub to: Location,
    /// The removed row when the file left every library root.
    pub snapshot: Option<Snapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CopyPayload {
    pub to_abs: String,
    /// The new row when the copy landed inside a library root.
    pub new_image_id: Option<i64>,
    pub size_bytes: i64,
    pub mtime_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RenamePayload {
    pub dir: String,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecyclePayload {
    /// Original path (the Recycle Bin item is matched by this + the deletion time window).
    pub abs: String,
    pub deleted_from_s: i64,
    pub deleted_to_s: i64,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PurgePayload {
    pub abs: String,
    pub size_bytes: i64,
}

// --- writing ---------------------------------------------------------------------------------

/// Writes `entries` as one new batch; `None` (nothing written) when empty.
pub(crate) fn write_batch(tx: &Transaction<'_>, entries: &[Entry]) -> Result<Option<i64>, AppError> {
    if entries.is_empty() {
        return Ok(None);
    }
    let batch: i64 = tx.query_row("SELECT COALESCE(max(batch_id), 0) + 1 FROM op_journal", [], |r| r.get(0))?;
    let now = now_ms();
    let mut stmt = tx.prepare_cached(
        "INSERT INTO op_journal (batch_id, op, image_id, payload_json, undoable, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for e in entries {
        let undoable = e.op != JournalOp::Purge;
        stmt.execute(params![batch, enum_str(&e.op), e.image_id, e.payload.to_string(), undoable as i64, now])?;
    }
    Ok(Some(batch))
}

/// Runs `f` inside a savepoint: on error only its own statements are rolled back.
pub(crate) fn savepoint<T>(tx: &Transaction<'_>, f: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    tx.execute_batch("SAVEPOINT ss_item")?;
    match f() {
        Ok(v) => {
            tx.execute_batch("RELEASE ss_item")?;
            Ok(v)
        }
        Err(e) => {
            tx.execute_batch("ROLLBACK TO ss_item; RELEASE ss_item")?;
            Err(e)
        }
    }
}

/// Work after the commit (thumbnail cache moves); receives the batch id.
pub(crate) type AfterCommit = Box<dyn FnOnce(i64) + Send>;
pub(crate) type DbStep = Box<dyn FnOnce(&Transaction<'_>) -> Result<(Entry, Option<AfterCommit>), AppError> + Send>;

/// One item whose file step is done: its DB step and how to reverse the file step.
pub(crate) struct Step {
    pub image_id: i64,
    pub action: FileAction,
    pub db: DbStep,
}

/// Commits the DB steps of `steps` in ONE transaction with their journal batch. A step whose DB step
/// fails becomes an `OpFailure` and its file step is reversed; when the transaction itself fails,
/// every file step is reversed and the error returned.
pub(crate) fn commit_steps(cat: &Catalog, steps: Vec<Step>, mut failed: Vec<OpFailure>) -> Result<(OpResult, Vec<i64>), AppError> {
    let mut actions = Vec::with_capacity(steps.len());
    let mut dbs = Vec::with_capacity(steps.len());
    for s in steps {
        actions.push((s.image_id, s.action));
        dbs.push((s.image_id, s.db));
    }
    let outcome = cat.write(move |tx| {
        let mut entries = Vec::new();
        let mut afters = Vec::new();
        let mut results = Vec::new();
        for (id, db) in dbs {
            match savepoint(tx, || db(tx)) {
                Ok((entry, after)) => {
                    entries.push(entry);
                    afters.extend(after);
                    results.push(Ok(id));
                }
                Err(e) => results.push(Err(failure(id, &e))),
            }
        }
        let batch = write_batch(tx, &entries)?;
        Ok((batch, afters, results))
    });
    let (batch, afters, results) = match outcome {
        Ok(v) => v,
        Err(e) => {
            actions.iter().for_each(|(_, a)| a.rollback());
            return Err(e);
        }
    };
    if let Some(b) = batch {
        afters.into_iter().for_each(|f| f(b));
    }
    let mut ok_ids = Vec::new();
    for (r, (_, action)) in results.into_iter().zip(&actions) {
        match r {
            Ok(id) => ok_ids.push(id),
            Err(f) => {
                action.rollback();
                failed.push(f);
            }
        }
    }
    Ok((OpResult { batch_id: batch, affected: ok_ids.len() as i64, failed }, ok_ids))
}

// --- list_journal ----------------------------------------------------------------------------

pub fn list_journal(cat: &Catalog, req: LimitReq) -> Result<JournalPage, AppError> {
    if !(1..=JOURNAL_LIMIT_MAX).contains(&req.limit) {
        return Err(invalid(format!("limit must be 1..{JOURNAL_LIMIT_MAX}, got {}", req.limit)));
    }
    cat.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT batch_id,
                    (SELECT op FROM op_journal f WHERE f.batch_id = j.batch_id ORDER BY f.id LIMIT 1),
                    count(DISTINCT image_id), min(created_at), min(undoable),
                    CASE WHEN count(undone_at) = count(*) THEN max(undone_at) END
             FROM op_journal j GROUP BY batch_id ORDER BY batch_id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([req.limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)?, r.get(5)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (batch_id, op, count, created_at, undoable, undone_at) = row?;
            items.push(JournalEntry { batch_id, op: parse_enum(&op)?, count, created_at, undoable: undoable != 0, undone_at });
        }
        Ok(JournalPage { items })
    })
}

// --- undo ------------------------------------------------------------------------------------

struct Row {
    row_id: i64,
    op: JournalOp,
    image_id: Option<i64>,
    payload: String,
}

fn payload<T: DeserializeOwned>(r: &Row) -> Result<T, AppError> {
    serde_json::from_str(&r.payload).map_err(|e| internal("journal payload is unreadable").with_detail(e.to_string()))
}

/// Reverts one batch (`batchId: null` = the latest batch with rows not undone yet).
pub fn undo(caller: Caller, cat: &Catalog, req: UndoReq) -> Result<OpResult, AppError> {
    undo_detailed(caller, cat, req).map(|(r, _)| r)
}

/// [`undo`], plus the ids whose file content changed back (an undone edit/convert overwrite): their
/// thumbnails and hashes must be made again by the caller ([`crate::thumbs::thumb_ids`]).
pub(crate) fn undo_detailed(caller: Caller, cat: &Catalog, req: UndoReq) -> Result<(OpResult, Vec<i64>), AppError> {
    let batch = match req.batch_id {
        Some(b) => b,
        None => cat
            .read(|c| {
                Ok(c.query_row(
                    "SELECT max(batch_id) FROM op_journal WHERE undoable = 1 AND undone_at IS NULL",
                    [],
                    |r| r.get::<_, Option<i64>>(0),
                )?)
            })?
            .ok_or_else(|| invalid("nothing to undo"))?,
    };
    let (rows, undoable) = cat.read(|c| {
        let undoable: Option<i64> =
            c.query_row("SELECT min(undoable) FROM op_journal WHERE batch_id = ?1", [batch], |r| r.get(0)).optional()?.flatten();
        let mut stmt = c.prepare(
            "SELECT id, op, image_id, payload_json FROM op_journal WHERE batch_id = ?1 AND undone_at IS NULL ORDER BY id DESC",
        )?;
        let rows = stmt
            .query_map([batch], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((rows, undoable))
    })?;
    match undoable {
        None => return Err(not_found(format!("journal batch {batch} not found"))),
        Some(0) => return Err(invalid(format!("batch {batch} is a permanent delete and cannot be undone"))),
        _ => {}
    }
    if rows.is_empty() {
        return Err(invalid(format!("batch {batch} is already undone")));
    }
    let rows: Vec<Row> = rows
        .into_iter()
        .map(|(row_id, op, image_id, payload)| Ok(Row { row_id, op: parse_enum(&op)?, image_id, payload }))
        .collect::<Result<_, AppError>>()?;
    let roots = cat.read(crate::catalog::list_roots)?;

    // File steps first.
    let mut failed = Vec::new();
    let mut ready = Vec::new();
    for r in rows {
        match file_undo(caller, &roots, &r) {
            Ok(action) => ready.push((r, action)),
            Err(e) => failed.push(failure(r.image_id.unwrap_or_default(), &e)),
        }
    }
    let data_dir = cat.data_dir().to_path_buf();
    let rows_for_db: Vec<(i64, JournalOp, Option<i64>, String)> =
        ready.iter().map(|(r, _)| (r.row_id, r.op, r.image_id, r.payload.clone())).collect();
    let outcome = cat.write(move |tx| {
        let now = now_ms();
        let mut results = Vec::new();
        for (row_id, op, image_id, payload) in rows_for_db {
            let r = Row { row_id, op, image_id, payload };
            let res = savepoint(tx, || {
                let after = db_undo(tx, &r, &data_dir, batch)?;
                tx.execute("UPDATE op_journal SET undone_at = ?2 WHERE id = ?1", params![r.row_id, now])?;
                Ok(after)
            });
            results.push(res);
        }
        Ok(results)
    });
    let results = match outcome {
        Ok(r) => r,
        Err(e) => {
            ready.iter().for_each(|(_, a)| a.rollback());
            return Err(e);
        }
    };
    let mut affected = 0;
    let mut rethumb = Vec::new();
    for (res, (r, action)) in results.into_iter().zip(&ready) {
        match res {
            Ok(after) => {
                affected += 1;
                if let After::Edited { id, .. } = &after {
                    rethumb.push(*id);
                }
                after.run(cat.data_dir(), batch);
            }
            Err(e) => {
                action.rollback();
                failed.push(failure(r.image_id.unwrap_or_default(), &e));
            }
        }
    }
    Ok((OpResult { batch_id: Some(batch), affected, failed }, rethumb))
}

fn copy_file_undo(caller: Caller, roots: &[LibraryRoot], p: &CopyPayload) -> Result<FileAction, AppError> {
    let to = PathBuf::from(&p.to_abs);
    guard_file_write(caller, roots, &[&to])?;
    let meta = std::fs::metadata(&to)?;
    let mtime = meta.modified().ok().and_then(crate::errors::system_time_ms).unwrap_or(0);
    if meta.len() as i64 != p.size_bytes || mtime != p.mtime_ms {
        return Err(AppError::new(ErrorKind::Conflict, format!("the copy {} changed since; not deleted", to.display())));
    }
    std::fs::remove_file(&to)?;
    Ok(FileAction::None)
}

fn copy_db_undo(tx: &Transaction<'_>, p: &CopyPayload) -> Result<After, AppError> {
    match p.new_image_id {
        Some(new_id) => {
            snapshot::remove(tx, new_id)?;
            Ok(After::DropThumbs(new_id))
        }
        None => Ok(After::None),
    }
}

fn file_undo(caller: Caller, roots: &[LibraryRoot], r: &Row) -> Result<FileAction, AppError> {
    Ok(match r.op {
        JournalOp::Move => {
            let p: MovePayload = payload(r)?;
            let (from, to) = (PathBuf::from(&p.from.abs), PathBuf::from(&p.to.abs));
            guard_file_write(caller, roots, &[&from, &to])?;
            fsio::move_file(&to, &from)?;
            FileAction::Moved { from: to, to: from }
        }
        JournalOp::Copy => copy_file_undo(caller, roots, &payload(r)?)?,
        JournalOp::Rename => {
            let p: RenamePayload = payload(r)?;
            let dir = PathBuf::from(&p.dir);
            let (new, old) = (dir.join(&p.new_name), dir.join(&p.old_name));
            guard_file_write(caller, roots, &[&new, &old])?;
            fsio::move_file(&new, &old)?;
            FileAction::Moved { from: new, to: old }
        }
        JournalOp::Recycle => {
            let p: RecyclePayload = payload(r)?;
            let path = PathBuf::from(&p.abs);
            guard_file_write(caller, roots, &[&path])?;
            fsio::unrecycle(&path, p.deleted_from_s, p.deleted_to_s)?;
            FileAction::Unrecycled { path }
        }
        JournalOp::EditOverwrite => edit::overwrite_file_undo(caller, roots, &payload(r)?)?,
        JournalOp::Convert => match payload::<ConvertPayload>(r)? {
            ConvertPayload::Copy(p) => copy_file_undo(caller, roots, &p)?,
            ConvertPayload::Overwrite(p) => edit::overwrite_file_undo(caller, roots, &p)?,
        },
        _ => FileAction::None,
    })
}

/// Cache work after a row's undo committed.
enum After {
    None,
    Unpark { old: i64, new: i64 },
    DropThumbs(i64),
    /// An undone overwrite: the old thumbnails are stale (the caller regenerates them) and the backup is spent.
    Edited { id: i64, backup: PathBuf },
}

impl After {
    fn run(self, data_dir: &Path, batch: i64) {
        match self {
            After::None => {}
            After::Unpark { old, new } => snapshot::unpark_thumbs(data_dir, batch, old, new),
            After::DropThumbs(id) => crate::thumbs::remove_thumbs(data_dir, id),
            After::Edited { id, backup } => {
                crate::thumbs::remove_thumbs(data_dir, id);
                if let Err(e) = std::fs::remove_file(&backup) {
                    eprintln!("ss-core: could not remove the edit backup {}: {e}", backup.display());
                }
            }
        }
    }
}

fn changed(n: usize, image_id: Option<i64>) -> Result<(), AppError> {
    if n == 0 { Err(not_found(format!("image {} is no longer in the catalog", image_id.unwrap_or_default()))) } else { Ok(()) }
}

fn db_undo(tx: &Transaction<'_>, r: &Row, data_dir: &Path, batch: i64) -> Result<After, AppError> {
    let id = r.image_id;
    let set = |col: &str, v: &Json| -> Result<(), AppError> {
        let value = super::organize::json_param(v);
        changed(tx.execute(&format!("UPDATE image SET {col} = ?2 WHERE id = ?1"), params![id, value])?, id)
    };
    match r.op {
        JournalOp::Tag => {
            let p: TagPayload = payload(r)?;
            tx.execute("DELETE FROM image_tag WHERE image_id = ?1 AND tag_id = ?2", params![id, p.tag_id])?;
        }
        JournalOp::Untag => {
            let p: TagPayload = payload(r)?;
            let n = tx.execute(
                "INSERT OR IGNORE INTO image_tag (image_id, tag_id)
                 SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM tag WHERE id = ?2) AND EXISTS (SELECT 1 FROM image WHERE id = ?1)",
                params![id, p.tag_id],
            )?;
            if n == 0 {
                return Err(not_found("the tag or the image no longer exists"));
            }
        }
        JournalOp::Favorite => set("favorite", &payload::<OldValue>(r)?.old)?,
        JournalOp::Rating => set("rating", &payload::<OldValue>(r)?.old)?,
        JournalOp::Trash | JournalOp::Restore => set("trashed_at", &payload::<OldValue>(r)?.old)?,
        JournalOp::Note => {
            let old = payload::<OldValue>(r)?.old;
            set("note", &old)?;
            super::organize::fts_set(tx, id.unwrap_or_default(), "note", old.as_str().unwrap_or_default())?;
        }
        JournalOp::AlbumAdd => {
            let p: AlbumPayload = payload(r)?;
            tx.execute("DELETE FROM album_image WHERE album_id = ?1 AND image_id = ?2", params![p.album_id, id])?;
        }
        JournalOp::AlbumRemove => {
            let p: AlbumPayload = payload(r)?;
            let n = tx.execute(
                "INSERT OR IGNORE INTO album_image (album_id, image_id, position)
                 SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM album WHERE id = ?1) AND EXISTS (SELECT 1 FROM image WHERE id = ?2)",
                params![p.album_id, id, p.position.unwrap_or_default()],
            )?;
            if n == 0 {
                return Err(not_found("the album or the image no longer exists"));
            }
        }
        JournalOp::AlbumReorder => {
            let p: AlbumPayload = payload(r)?;
            tx.execute(
                "UPDATE album_image SET position = ?3 WHERE album_id = ?1 AND image_id = ?2",
                params![p.album_id, id, p.position.unwrap_or_default()],
            )?;
        }
        JournalOp::Move => {
            let p: MovePayload = payload(r)?;
            match &p.snapshot {
                Some(snap) => {
                    let new = snapshot::restore(tx, snap, data_dir, batch)?;
                    return Ok(After::Unpark { old: snap.id()?, new });
                }
                None => {
                    let root = p.from.root_id.ok_or_else(|| internal("move payload without a source root"))?;
                    changed(
                        tx.execute(
                            "UPDATE image SET root_id = ?2, folder = ?3, file_name = ?4 WHERE id = ?1",
                            params![id, root, p.from.folder, p.from.file_name],
                        )?,
                        id,
                    )?;
                    super::organize::fts_set(tx, id.unwrap_or_default(), "file_name", &p.from.file_name)?;
                }
            }
        }
        JournalOp::Copy => return copy_db_undo(tx, &payload(r)?),
        JournalOp::EditOverwrite | JournalOp::Convert => {
            let over = match r.op {
                JournalOp::EditOverwrite => payload::<OverwritePayload>(r)?,
                _ => match payload::<ConvertPayload>(r)? {
                    ConvertPayload::Copy(p) => return copy_db_undo(tx, &p),
                    ConvertPayload::Overwrite(p) => p,
                },
            };
            let id = id.ok_or_else(|| internal("edit journal row without an image"))?;
            edit::overwrite_db_undo(tx, id, &over)?;
            return Ok(After::Edited { id, backup: PathBuf::from(&over.backup) });
        }
        JournalOp::Rename => {
            let p: RenamePayload = payload(r)?;
            changed(tx.execute("UPDATE image SET file_name = ?2 WHERE id = ?1", params![id, p.old_name])?, id)?;
            super::organize::fts_set(tx, id.unwrap_or_default(), "file_name", &p.old_name)?;
        }
        JournalOp::Recycle => {
            let p: RecyclePayload = payload(r)?;
            let new = snapshot::restore(tx, &p.snapshot, data_dir, batch)?;
            return Ok(After::Unpark { old: p.snapshot.id()?, new });
        }
        JournalOp::Purge => {
            return Err(invalid(format!("journal op {} cannot be undone here", enum_str(&r.op))));
        }
    }
    Ok(After::None)
}
