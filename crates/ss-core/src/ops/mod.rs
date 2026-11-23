//! Write APIs of `ss-core`. Every one takes `caller: Caller` first (contract-tauri-ipc rule 8):
//! a file write under a protected root or any `\OneDrive\` path is allowed only for `Caller::Ui`.
//!
//! - this module: library roots, settings, the shared row loader and the protected-path guard;
//! - [`organize`]: tags, albums, favourites, ratings, notes, smart filters (phase 2.1);
//! - [`files`]: move/copy, rename + preview, in-app trash/restore, Recycle Bin (phase 2.1);
//! - [`purge`]: the two-step permanent delete (D8, phase 2.1);
//! - [`journal`]: the `op_journal` batches, `list_journal` and `undo` (phase 2.1);
//! - edit/convert/stitch are phase 4.1 stubs below.
//!
//! Order of every journaled file mutation: file op first, then ONE SQLite transaction holding the row
//! changes and the `op_journal` batch; a file op whose DB step fails is reversed.

pub mod files;
mod fsio;
pub mod journal;
pub mod organize;
mod pattern;
pub mod purge;
mod snapshot;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params_from_iter};

use crate::catalog::{self, Catalog};
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller, ErrorKind};
use crate::errors::{invalid, not_found, now_ms};
use crate::paths;

/// Adds a library root (catalog only). The path must be an existing absolute folder that is not
/// already a root, inside one, or a parent of one (nested roots would index files twice).
/// `protectedRoot` is set for any path under a OneDrive folder.
pub fn add_library_root(caller: Caller, cat: &Catalog, req: AddLibraryRootReq) -> Result<LibraryRoot, AppError> {
    let _ = caller; // not a file write: allowed for both callers
    let path = normalize_root(&req.path)?;
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err(not_found(format!("folder not found: {path}")));
    }
    let protected = paths::is_protected_path(&path);
    let roots = cat.read(catalog::list_roots)?;
    let lower = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let new_l = lower(&path);
    for r in &roots {
        let old_l = lower(&r.path);
        let nested = new_l == old_l
            || new_l.starts_with(&format!("{old_l}\\"))
            || old_l.starts_with(&format!("{new_l}\\"));
        if nested {
            return Err(AppError::new(ErrorKind::Conflict, format!("overlaps library root {}: {}", r.id, r.path)));
        }
    }
    let id = cat.write(move |tx| catalog::insert_root(tx, &path, protected, now_ms()))?;
    cat.read(|conn| catalog::get_root(conn, id))
}

fn normalize_root(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid("path is empty"));
    }
    let p = PathBuf::from(trimmed);
    if !p.is_absolute() {
        return Err(invalid(format!("path must be absolute: {trimmed}")));
    }
    // Drop trailing separators except on a drive root (`D:\`).
    let s = trimmed.trim_end_matches(['\\', '/']);
    Ok(if s.ends_with(':') || s.is_empty() { trimmed.to_owned() } else { s.to_owned() })
}

/// Catalog only, never touches the user's files; not journaled. Removes the root's rows and their
/// cached thumbnails (files in the data dir, owned by the app).
pub fn remove_library_root(caller: Caller, cat: &Catalog, req: RemoveLibraryRootReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let root_id = req.root_id;
    let ids = cat.write(move |tx| catalog::delete_root(tx, root_id))?;
    for id in &ids {
        crate::thumbs::remove_thumbs(cat.data_dir(), *id);
    }
    Ok(OpResult { batch_id: None, affected: ids.len() as i64, failed: Vec::new() })
}

/// Validates the value against the key's type (contract "Setting keys") and stores it.
pub fn set_setting(caller: Caller, cat: &Catalog, req: SetSettingReq) -> Result<(), AppError> {
    let _ = caller;
    validate_setting(req.key, &req.value)?;
    cat.write(move |tx| catalog::put_setting(tx, req.key, &req.value))
}

fn validate_setting(key: SettingKey, v: &serde_json::Value) -> Result<(), AppError> {
    let int_in = |lo: i64, hi: i64| v.as_i64().is_some_and(|n| (lo..=hi).contains(&n));
    let ok = match key {
        SettingKey::Hotkey => v.as_str().is_some_and(|s| !s.trim().is_empty()),
        SettingKey::WatcherEnabled | SettingKey::Autostart | SettingKey::AutoDownloadCloudOnly => v.is_boolean(),
        SettingKey::AiDevice => serde_json::from_value::<AiDevice>(v.clone()).is_ok(),
        SettingKey::DupHamming => int_in(0, 64),
        SettingKey::SessionGapMinutes => int_in(1, 7 * 24 * 60),
        SettingKey::TinyMaxPixels => int_in(0, i64::MAX),
        SettingKey::BlurThreshold => v.as_f64().is_some_and(|n| n >= 0.0),
        SettingKey::SidecarVenv => v.is_null() || v.as_str().is_some_and(|s| !s.trim().is_empty()),
    };
    if ok {
        Ok(())
    } else {
        Err(invalid(format!("invalid value for setting `{}`: {v}", crate::errors::enum_str(&key))))
    }
}

// ---------------------------------------------------------------------------------------------
// Shared helpers of the 2.1 write APIs
// ---------------------------------------------------------------------------------------------

/// One image row with its root, as the write APIs need it.
#[derive(Debug, Clone)]
pub(crate) struct ImageRow {
    pub id: i64,
    pub root_id: i64,
    pub root_path: String,
    pub folder: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub missing: bool,
    pub trashed_at: Option<i64>,
    pub favorite: bool,
    pub rating: i64,
    pub note: Option<String>,
}

impl ImageRow {
    pub fn abs(&self) -> PathBuf {
        paths::image_abs_path(&self.root_path, &self.folder, &self.file_name)
    }

    /// The folder holding the file.
    pub fn dir(&self) -> PathBuf {
        let mut p = PathBuf::from(&self.root_path);
        for part in self.folder.split('/').filter(|s| !s.is_empty()) {
            p.push(part);
        }
        p
    }
}

/// Rows of `ids` that exist (missing ids are simply absent from the map).
pub(crate) fn load_rows(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, ImageRow>, AppError> {
    let mut out = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT i.id, i.root_id, r.path, i.folder, i.file_name, i.size_bytes,
                    i.width, i.height, i.missing, i.trashed_at, i.favorite, i.rating, i.note
             FROM image i JOIN library_root r ON r.id = i.root_id WHERE i.id IN ({})",
            vec!["?"; chunk.len()].join(", ")
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(chunk), |r| {
            Ok(ImageRow {
                id: r.get(0)?,
                root_id: r.get(1)?,
                root_path: r.get(2)?,
                folder: r.get(3)?,
                file_name: r.get(4)?,
                size_bytes: r.get(5)?,
                width: r.get(6)?,
                height: r.get(7)?,
                missing: r.get::<_, i64>(8)? != 0,
                trashed_at: r.get(9)?,
                favorite: r.get::<_, i64>(10)? != 0,
                rating: r.get(11)?,
                note: r.get(12)?,
            })
        })?;
        for row in rows {
            let row = row?;
            out.insert(row.id, row);
        }
    }
    Ok(out)
}

/// `ids` without duplicates, first occurrence order kept.
pub(crate) fn unique_ids(ids: &[i64]) -> Vec<i64> {
    let mut seen = std::collections::HashSet::new();
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

pub(crate) fn failure(image_id: i64, e: &AppError) -> OpFailure {
    let message = match &e.detail {
        Some(d) => format!("{} ({d})", e.message),
        None => e.message.clone(),
    };
    OpFailure { image_id, kind: e.kind, message }
}

pub(crate) fn not_found_failure(image_id: i64) -> OpFailure {
    failure(image_id, &not_found(format!("image {image_id} not found")))
}

/// Lower-case, `\`-separated, without the `\\?\` verbatim prefix or a trailing separator.
pub(crate) fn norm_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_lowercase()
}

/// True when `child` is `parent` or inside it (case-insensitive, Windows paths).
pub(crate) fn path_under(child: &Path, parent: &Path) -> bool {
    let (c, p) = (norm_path(child), norm_path(parent));
    !p.is_empty() && (c == p || c.starts_with(&format!("{p}\\")))
}

/// Rule 8: a file write touching `paths` (sources and destinations) is refused for `Caller::Cli`
/// when any of them is under a protected root or a OneDrive path.
pub(crate) fn guard_file_write(caller: Caller, roots: &[LibraryRoot], paths_touched: &[&Path]) -> Result<(), AppError> {
    if caller == Caller::Ui {
        return Ok(());
    }
    for p in paths_touched {
        let protected = paths::is_protected_path(&p.to_string_lossy())
            || roots.iter().any(|r| r.protected_root && path_under(p, Path::new(&r.path)));
        if protected {
            return Err(AppError::new(ErrorKind::ProtectedRoot, format!("refused: {} is under a protected root", p.display()))
                .with_detail("file writes under a protected root or a OneDrive path are UI-only (contract rule 8)"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Phase 4.1 stubs
// ---------------------------------------------------------------------------------------------

/// Decoded `save_edited_image` request (raw PNG body + `x-*` headers).
#[derive(Debug, Clone)]
pub struct SaveEditedImage {
    pub image_id: i64,
    pub mode: EditMode,
    pub format: ImageFormat,
    pub quality: Option<i64>,
    pub png: Vec<u8>,
}

macro_rules! write_stubs {
    ($($(#[$doc:meta])* $phase:literal $name:ident($req:ty) -> $res:ty;)*) => {$(
        $(#[$doc])*
        #[doc = concat!("Implemented in phase ", $phase, ".")]
        pub fn $name(caller: Caller, req: $req) -> Result<$res, AppError> {
            let _ = (caller, req);
            Err(AppError::not_implemented().with_detail(concat!("ss_core::ops::", stringify!($name), " lands in phase ", $phase)))
        }
    )*};
}

write_stubs! {
    "4.1" save_edited_image(SaveEditedImage) -> ImageSummary;
    "4.1" convert_images(ConvertImagesReq) -> OpResult;
    "4.1" stitch_images(StitchImagesReq) -> ImageSummary;
}
