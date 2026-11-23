//! Write APIs of `ss-core`. Every one takes `caller: Caller` first (contract-tauri-ipc rule 8):
//! a file write under a protected root or any `\OneDrive\` path is allowed only for `Caller::Ui`.
//!
//! Phase 1.1 pinned the names and the `caller` parameter; the owning core phase (1.2, 2.1, 4.1)
//! implements each one and adds a catalog handle after `caller`, but never drops `caller`.
//! Phase 1.2 implements `add_library_root`, `remove_library_root` and `set_setting` (none of them
//! writes a file, so both callers may use them); the rest are still stubs.

use std::path::{Path, PathBuf};

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
        for w in ssimg::THUMB_WIDTHS {
            let p = paths::thumb_path(cat.data_dir(), *id, w);
            if let Err(e) = std::fs::remove_file(&p) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("ss-core: could not remove thumbnail {}: {e}", p.display());
                }
            }
        }
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
    "2.1" create_tag(CreateTagReq) -> Tag;
    "2.1" update_tag(UpdateTagReq) -> Tag;
    "2.1" delete_tag(DeleteTagReq) -> OpResult;
    "2.1" set_tags(SetTagsReq) -> OpResult;
    "2.1" create_album(CreateAlbumReq) -> Album;
    "2.1" rename_album(RenameAlbumReq) -> Album;
    "2.1" delete_album(DeleteAlbumReq) -> OpResult;
    "2.1" album_add(AlbumImagesReq) -> OpResult;
    "2.1" album_remove(AlbumImagesReq) -> OpResult;
    "2.1" album_reorder(AlbumImagesReq) -> OpResult;
    "2.1" set_favorite(SetFavoriteReq) -> OpResult;
    "2.1" set_rating(SetRatingReq) -> OpResult;
    "2.1" set_note(SetNoteReq) -> OpResult;
    "2.1" save_smart_filter(SaveSmartFilterReq) -> SmartFilter;
    "2.1" delete_smart_filter(DeleteSmartFilterReq) -> OpResult;
    "2.1" move_images(MoveImagesReq) -> OpResult;
    "2.1" rename_images(RenameImagesReq) -> OpResult;
    "2.1" trash_images(ImageIdsReq) -> OpResult;
    "2.1" restore_images(ImageIdsReq) -> OpResult;
    "2.1" recycle_images(ImageIdsReq) -> OpResult;
    /// Issues the 60 s token bound to the exact id set (D8 step 1).
    "2.1" request_permanent_delete(ImageIdsReq) -> PermanentDeleteTicket;
    /// Journaled as `purge`, `undoable: false` (D8 step 2). Never reachable from `Caller::Cli`.
    "2.1" delete_permanently(DeletePermanentlyReq) -> OpResult;
    "2.1" undo(UndoReq) -> OpResult;
    "4.1" save_edited_image(SaveEditedImage) -> ImageSummary;
    "4.1" convert_images(ConvertImagesReq) -> OpResult;
    "4.1" stitch_images(StitchImagesReq) -> ImageSummary;
}
