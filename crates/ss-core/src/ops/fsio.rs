//! File primitives of the 2.1 write APIs: no-overwrite move/copy, the OS Recycle Bin, and
//! [`FileAction`], a done file step that can be reversed when its DB step fails.

use std::io;
use std::path::{Path, PathBuf};

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
    let want = norm_path(path);
    let found = on_fresh_thread(move || -> Result<(), AppError> {
        let items = trash::os_limited::list().map_err(|e| trash_err("Recycle Bin listing", e))?;
        let item = items
            .into_iter()
            .filter(|it| norm_path(&it.original_path()) == want)
            .filter(|it| (from_s - RECYCLE_TIME_SLACK_S..=to_s + RECYCLE_TIME_SLACK_S).contains(&it.time_deleted))
            .max_by_key(|it| it.time_deleted)
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "the file is no longer in the Recycle Bin"))?;
        trash::os_limited::restore_all([item]).map_err(|e| trash_err("Recycle Bin restore", e))
    })?;
    found?;
    if !path.is_file() {
        return Err(internal(format!("Recycle Bin restore did not bring back {}", path.display())));
    }
    Ok(())
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
        };
        if let Err(e) = r {
            eprintln!("ss-core: could not reverse {self:?}: {}: {}", e.message, e.detail.unwrap_or_default());
        }
    }
}
