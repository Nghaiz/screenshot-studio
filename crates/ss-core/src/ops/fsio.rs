//! File primitives of the 2.1 write APIs: no-overwrite move/copy, the OS Recycle Bin, and
//! [`FileAction`], a done file step that can be reversed when its DB step fails.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::contract::{AppError, ErrorKind};
use crate::errors::internal;

use super::norm_path;

/// Seconds of slack around a recycle when matching the Recycle Bin's deletion time.
const RECYCLE_TIME_SLACK_S: i64 = 5;

fn exists_err(to: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, format!("{} already exists", to.display()))
}

/// Same file under a different case (Windows names are case-insensitive): a case-only rename.
fn same_file(a: &Path, b: &Path) -> bool {
    norm_path(a) == norm_path(b)
}

/// Moves a file, never replacing an existing one. Falls back to copy + remove across volumes.
pub(crate) fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() && !same_file(from, to) {
        return Err(exists_err(to));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            copy_file(from, to)?;
            if let Err(e) = std::fs::remove_file(from) {
                let _ = std::fs::remove_file(to);
                return Err(e);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Copies a file (timestamps kept), never replacing an existing one.
pub(crate) fn copy_file(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        return Err(exists_err(to));
    }
    std::fs::copy(from, to).map(|_| ())
}

/// `.<name>.<pid>-<seq>.ss-tmp` next to `target`: no image extension, so neither the scanner nor the live
/// watcher ever indexes a half-written file.
pub(crate) fn tmp_beside(target: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = format!(".{name}.{}-{}.ss-tmp", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
    target.with_file_name(tmp)
}

/// Writes `bytes` and flushes them to disk (`FlushFileBuffers`) before returning.
pub(crate) fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Writes `bytes` to a temp file beside `target`, syncs it, then renames it over `target`
/// (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`): readers see the old or the new file, never half of one.
pub(crate) fn replace_with(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_beside(target);
    let r = write_synced(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, target));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// Like [`replace_with`] for a file that must not exist yet.
pub(crate) fn create_new_with(target: &Path, bytes: &[u8]) -> io::Result<()> {
    if target.exists() {
        return Err(exists_err(target));
    }
    replace_with(target, bytes)
}

/// Copies `src` to `backup` (timestamps kept) and flushes the copy to disk: the backup of an overwrite is
/// durable before the original is touched (phase 4.1 risk "overwrite loses the original").
pub(crate) fn backup_copy(src: &Path, backup: &Path) -> io::Result<()> {
    if let Some(dir) = backup.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(src, backup)?;
    std::fs::OpenOptions::new().write(true).open(backup)?.sync_all()
}

/// Puts the bytes of `backup` back at `target` (replacing it), through a synced temp copy beside `target`.
pub(crate) fn restore_from_backup(backup: &Path, target: &Path) -> io::Result<()> {
    let tmp = tmp_beside(target);
    let r = std::fs::copy(backup, &tmp)
        .and_then(|_| std::fs::OpenOptions::new().write(true).open(&tmp)?.sync_all())
        .and_then(|_| std::fs::rename(&tmp, target));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// Runs `f` on a new thread. The `trash` crate initializes COM per thread and panics when the
/// thread already holds COM in another mode, so Recycle Bin calls never run on a pooled thread.
fn on_fresh_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, AppError> {
    std::thread::Builder::new()
        .name("ss-recycle".into())
        .spawn(f)
        .map_err(|e| internal("cannot start the Recycle Bin thread").with_detail(e.to_string()))?
        .join()
        .map_err(|_| internal("the Recycle Bin call panicked"))
}

fn trash_err(what: &str, e: trash::Error) -> AppError {
    AppError::new(ErrorKind::Io, format!("{what} failed")).with_detail(e.to_string())
}

/// Sends a file to the OS Recycle Bin. Returns the wall-clock window (Unix seconds) of the call,
/// which [`unrecycle`] matches against the bin's deletion time.
pub(crate) fn recycle(path: &Path) -> Result<(i64, i64), AppError> {
    if !path.is_file() {
        return Err(AppError::new(ErrorKind::NotFound, format!("file not found: {}", path.display())));
    }
    let p = path.to_path_buf();
    let from = now_s();
    on_fresh_thread(move || trash::delete(&p))?.map_err(|e| trash_err("Recycle Bin delete", e))?;
    Ok((from, now_s()))
}

/// Restores the Recycle Bin item whose original path is `path` and whose deletion time lies in
/// `[from_s, to_s]` (with slack); the most recent one when several match.
pub(crate) fn unrecycle(path: &Path, from_s: i64, to_s: i64) -> Result<(), AppError> {
    if path.exists() {
        return Err(AppError::new(ErrorKind::Conflict, format!("cannot restore: {} exists again", path.display())));
    }
    // The bin records the canonical (long-name) path: `trash` canonicalizes before deleting, so a
    // root picked through an 8.3 short name (`C:\Users\RUNNER~1\...`) must be compared canonically.
    let want = norm_path(&canonical_in_parent(path));
    let name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    let found = on_fresh_thread(move || -> Result<(), AppError> {
        let items = trash::os_limited::list().map_err(|e| trash_err("Recycle Bin listing", e))?;
        let window = from_s - RECYCLE_TIME_SLACK_S..=to_s + RECYCLE_TIME_SLACK_S;
        let same_name: Vec<_> = items.into_iter().filter(|it| it.name == name).collect();
        let item = same_name
            .iter()
            .filter(|it| norm_path(&canonical_in_parent(&it.original_path())) == want)
            .filter(|it| window.contains(&it.time_deleted))
            .max_by_key(|it| it.time_deleted)
            .cloned()
            .ok_or_else(|| {
                let seen: Vec<String> =
                    same_name.iter().map(|it| format!("{} @{}", it.original_path().display(), it.time_deleted)).collect();
                AppError::new(ErrorKind::NotFound, "the file is no longer in the Recycle Bin").with_detail(format!(
                    "wanted {want} deleted in {from_s}..={to_s}; same-named items: [{}]",
                    seen.join(", ")
                ))
            })?;
        trash::os_limited::restore_all([item]).map_err(|e| trash_err("Recycle Bin restore", e))
    })?;
    found?;
    if !path.is_file() {
        return Err(internal(format!("Recycle Bin restore did not bring back {}", path.display())));
    }
    Ok(())
}

/// `canonicalize` without the `\\?\` verbatim prefix (real on-disk case, long names); the path
/// itself when it cannot be resolved.
pub(crate) fn canonical(p: &Path) -> PathBuf {
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

/// Canonical parent folder + file name, for a file that may not exist right now.
fn canonical_in_parent(p: &Path) -> PathBuf {
    match (p.parent(), p.file_name()) {
        (Some(dir), Some(name)) => canonical(dir).join(name),
        _ => p.to_path_buf(),
    }
}

fn now_s() -> i64 {
    crate::errors::now_ms() / 1000
}

/// A done file step, reversible when the DB step that follows it fails.
#[derive(Debug, Clone)]
pub(crate) enum FileAction {
    None,
    Moved { from: PathBuf, to: PathBuf },
    Copied { to: PathBuf },
    Recycled { path: PathBuf, from_s: i64, to_s: i64 },
    Unrecycled { path: PathBuf },
    /// An edit/convert overwrite: `orig` was backed up to `backup`, then replaced by `new` (same path, or
    /// a new extension with `orig` removed).
    Overwritten { orig: PathBuf, new: PathBuf, backup: PathBuf },
}

impl FileAction {
    /// Best effort; a failure is logged (the DB error is what the caller reports).
    pub fn rollback(&self) {
        let r = match self {
            FileAction::None => Ok(()),
            FileAction::Moved { from, to } => move_file(to, from).map_err(AppError::from),
            FileAction::Copied { to } => std::fs::remove_file(to).map_err(AppError::from),
            FileAction::Recycled { path, from_s, to_s } => unrecycle(path, *from_s, *to_s),
            FileAction::Unrecycled { path } => recycle(path).map(|_| ()),
            FileAction::Overwritten { orig, new, backup } => (|| {
                restore_from_backup(backup, orig)?;
                if !same_file(orig, new) {
                    std::fs::remove_file(new)?;
                }
                std::fs::remove_file(backup)
            })()
            .map_err(AppError::from),
        };
        if let Err(e) = r {
            eprintln!("ss-core: could not reverse {self:?}: {}: {}", e.message, e.detail.unwrap_or_default());
        }
    }
}
