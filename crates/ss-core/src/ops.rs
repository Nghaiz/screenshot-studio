//! Write APIs of `ss-core`. Every one takes `caller: Caller` first (contract-tauri-ipc rule 8):
//! a file write under a protected root or any `\OneDrive\` path is allowed only for `Caller::Ui`.
//!
//! Phase 1.1 pins the names and the `caller` parameter only; every body is a stub returning
//! `not implemented`. The owning core phase (1.2, 2.1, 4.1) implements it and may add a catalog
//! handle after `caller`, but never drops `caller`.

use crate::contract::ipc::*;
use crate::contract::{AppError, Caller};

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
    "1.2" add_library_root(AddLibraryRootReq) -> LibraryRoot;
    /// Catalog only, never touches files; not journaled.
    "1.2" remove_library_root(RemoveLibraryRootReq) -> OpResult;
    "1.2" set_setting(SetSettingReq) -> ();
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
