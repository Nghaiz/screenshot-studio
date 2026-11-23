//! Library scan: walk a root, keep image files by extension, upsert rows by `(root, folder, file_name)`,
//! mark vanished files `missing`, set `taken_ms` (filename pattern > created > mtime) and read
//! dimensions from the header of local files. Cloud-only files are listed from metadata and never
//! opened (contract-tauri-ipc rule 9).

pub mod patterns;

use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use rusqlite::params;
use serde::Serialize;

use crate::catalog::{self, Catalog, WRITE_BATCH};
use crate::contract::ipc::{CloudState, LibraryRoot, ThumbState};
use crate::contract::{AppError, ErrorKind};
use crate::errors::{enum_str, now_ms, system_time_ms};
use crate::fsaccess::FileAccess;

/// Extensions indexed (lower case, no dot). Printed by `ssctl extensions`.
pub const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];

pub fn has_image_extension(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Progress callback: `(done, total, current_path)`; `total` is 0 while unknown.
pub type Progress<'a> = &'a (dyn Fn(i64, i64, Option<&str>) + Sync);

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub seen: i64,
    pub added: i64,
    pub updated: i64,
    pub unchanged: i64,
    pub missing: i64,
    pub restored: i64,
    pub cloud_only: i64,
    pub walk_errors: i64,
}

struct Found {
    folder: String,
    file_name: String,
    abs: PathBuf,
    size: i64,
    mtime_ms: i64,
    created_ms: Option<i64>,
    cloud: CloudState,
}

struct Existing {
    id: i64,
    size: i64,
    mtime_ms: i64,
    cloud: String,
    thumb: String,
    missing: bool,
}

enum Change {
    New(Found, Option<(i64, i64)>),
    Changed(i64, Found, Option<(i64, i64)>),
    Touch { id: i64, cloud: CloudState, thumb: String },
    Missing(i64),
}

pub(crate) fn cancelled() -> AppError {
    AppError::new(ErrorKind::Cancelled, "job cancelled")
}

/// Thumbnail state a file should have when its thumbnail does not exist yet.
fn target_thumb(cloud: CloudState, auto_download_cloud: bool) -> ThumbState {
    if cloud == CloudState::CloudOnly && !auto_download_cloud { ThumbState::Offline } else { ThumbState::Pending }
}

/// Scans one root. `auto_download_cloud` is the `autoDownloadCloudOnly` setting: only when true do
/// cloud-only files get `pending` thumbnails (which opens them later); dimensions are still never
/// read from a cloud-only file here.
pub fn scan_root(
    cat: &Catalog,
    fa: &dyn FileAccess,
    root: &LibraryRoot,
    auto_download_cloud: bool,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<ScanReport, AppError> {
    let mut report = ScanReport::default();
    let found = walk(fa, Path::new(&root.path), cancel, progress, &mut report)?;
    let root_id = root.id;
    let mut existing: HashMap<(String, String), Existing> = cat.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, folder, file_name, size_bytes, mtime_ms, cloud_state, thumb_state, missing
             FROM image WHERE root_id = ?1",
        )?;
        let rows = stmt.query_map([root_id], |r| {
            Ok((
                (r.get::<_, String>(1)?, r.get::<_, String>(2)?),
                Existing {
                    id: r.get(0)?,
                    size: r.get(3)?,
                    mtime_ms: r.get(4)?,
                    cloud: r.get(5)?,
                    thumb: r.get(6)?,
                    missing: r.get::<_, i64>(7)? != 0,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    })?;

    // Classify; header dimension reads (local files only) run in parallel.
    let mut needs_dims = Vec::new();
    let mut changes = Vec::new();
    for f in found {
        if f.cloud == CloudState::CloudOnly {
            report.cloud_only += 1;
        }
        match existing.remove(&(f.folder.clone(), f.file_name.clone())) {
            None => needs_dims.push((None, f)),
            Some(e) if e.size != f.size || e.mtime_ms != f.mtime_ms => needs_dims.push((Some(e.id), f)),
            Some(e) => {
                let cloud_str = enum_str(&f.cloud);
                let target = enum_str(&target_thumb(f.cloud, auto_download_cloud));
                let thumb = match (e.thumb.as_str(), target.as_str()) {
                    ("offline", "pending") | ("pending", "offline") => target,
                    _ => e.thumb.clone(),
                };
                if e.missing {
                    report.restored += 1;
                }
                if e.missing || cloud_str != e.cloud || thumb != e.thumb {
                    changes.push(Change::Touch { id: e.id, cloud: f.cloud, thumb });
                } else {
                    report.unchanged += 1;
                }
            }
        }
    }
    let with_dims: Vec<(Option<i64>, Found, Option<(i64, i64)>)> = crate::thumbs::worker_pool()?.install(|| {
        needs_dims
            .into_par_iter()
            .map(|(id, f)| {
                let dims = if f.cloud == CloudState::CloudOnly { None } else { header_dims(fa, &f.abs) };
                (id, f, dims)
            })
            .collect()
    });
    for (id, f, dims) in with_dims {
        match id {
            None => changes.push(Change::New(f, dims)),
            Some(id) => changes.push(Change::Changed(id, f, dims)),
        }
    }
    for e in existing.into_values().filter(|e| !e.missing) {
        changes.push(Change::Missing(e.id));
    }

    let total = changes.len() as i64;
    let mut done = 0i64;
    let mut iter = changes.into_iter().peekable();
    while iter.peek().is_some() {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let batch: Vec<Change> = iter.by_ref().take(WRITE_BATCH).collect();
        let n = batch.len() as i64;
        let delta = cat.write(move |tx| write_batch(tx, root_id, auto_download_cloud, batch))?;
        report.added += delta.added;
        report.updated += delta.updated;
        report.missing += delta.missing;
        done += n;
        progress(done, total, None);
    }
    Ok(report)
}

fn walk(
    fa: &dyn FileAccess,
    root: &Path,
    cancel: &AtomicBool,
    progress: Progress<'_>,
    report: &mut ScanReport,
) -> Result<Vec<Found>, AppError> {
    if !root.is_dir() {
        return Err(AppError::new(ErrorKind::NotFound, format!("library root folder not found: {}", root.display())));
    }
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                report.walk_errors += 1;
                eprintln!("ss-core scan: skipped an unreadable entry: {e}");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !has_image_extension(&file_name) {
            continue;
        }
        // DirEntry metadata comes from the directory listing on Windows: no handle, no content read.
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                report.walk_errors += 1;
                eprintln!("ss-core scan: no metadata for {}: {e}", entry.path().display());
                continue;
            }
        };
        let folder = entry
            .path()
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
            .unwrap_or_default();
        let abs = entry.path().to_path_buf();
        let cloud = fa.cloud_state(&abs, &meta);
        out.push(Found {
            folder,
            file_name,
            size: meta.len() as i64,
            mtime_ms: meta.modified().ok().and_then(system_time_ms).unwrap_or(0),
            created_ms: meta.created().ok().and_then(system_time_ms),
            cloud,
            abs,
        });
        report.seen += 1;
        progress(report.seen, 0, entry.path().to_str());
    }
    Ok(out)
}

/// Width and height from the image header only (no full decode).
pub fn header_dims(fa: &dyn FileAccess, path: &Path) -> Option<(i64, i64)> {
    let file = fa.open(path).ok()?;
    let reader = image::ImageReader::new(BufReader::new(file)).with_guessed_format().ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    Some((w as i64, h as i64))
}

#[derive(Default)]
struct BatchDelta {
    added: i64,
    updated: i64,
    missing: i64,
}

fn write_batch(
    tx: &rusqlite::Transaction<'_>,
    root_id: i64,
    auto_download_cloud: bool,
    batch: Vec<Change>,
) -> Result<BatchDelta, AppError> {
    let now = now_ms();
    let mut d = BatchDelta::default();
    for change in batch {
        match change {
            Change::New(f, dims) => {
                let taken = patterns::parse_file_name(&f.file_name).map(|t| t.to_sqlite());
                tx.prepare_cached(
                    "INSERT INTO image (root_id, folder, file_name, size_bytes, mtime_ms, created_ms, taken_ms,
                                        width, height, thumb_state, cloud_state, indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             COALESCE(CAST(strftime('%s', ?7, 'utc') AS INTEGER) * 1000, ?6, ?5),
                             ?8, ?9, ?10, ?11, ?12)",
                )?
                .execute(params![
                    root_id,
                    f.folder,
                    f.file_name,
                    f.size,
                    f.mtime_ms,
                    f.created_ms,
                    taken,
                    dims.map(|d| d.0),
                    dims.map(|d| d.1),
                    enum_str(&target_thumb(f.cloud, auto_download_cloud)),
                    enum_str(&f.cloud),
                    now,
                ])?;
                let id = tx.last_insert_rowid();
                catalog::fts_insert(tx, id, &f.file_name)?;
                d.added += 1;
            }
            Change::Changed(id, f, dims) => {
                let taken = patterns::parse_file_name(&f.file_name).map(|t| t.to_sqlite());
                tx.prepare_cached(
                    "UPDATE image SET size_bytes = ?2, mtime_ms = ?3, created_ms = ?4,
                        taken_ms = COALESCE(CAST(strftime('%s', ?5, 'utc') AS INTEGER) * 1000, ?4, ?3),
                        width = ?6, height = ?7, content_hash = NULL, dhash = NULL, phash = NULL,
                        blur_score = NULL, thumb_state = ?8, cloud_state = ?9, missing = 0, indexed_at = ?10
                     WHERE id = ?1",
                )?
                .execute(params![
                    id,
                    f.size,
                    f.mtime_ms,
                    f.created_ms,
                    taken,
                    dims.map(|d| d.0),
                    dims.map(|d| d.1),
                    enum_str(&target_thumb(f.cloud, auto_download_cloud)),
                    enum_str(&f.cloud),
                    now,
                ])?;
                d.updated += 1;
            }
            Change::Touch { id, cloud, thumb } => {
                tx.prepare_cached(
                    "UPDATE image SET cloud_state = ?2, thumb_state = ?3, missing = 0, indexed_at = ?4 WHERE id = ?1",
                )?
                .execute(params![id, enum_str(&cloud), thumb, now])?;
                d.updated += 1;
            }
            Change::Missing(id) => {
                tx.prepare_cached("UPDATE image SET missing = 1 WHERE id = ?1")?.execute([id])?;
                d.missing += 1;
            }
        }
    }
    Ok(d)
}
