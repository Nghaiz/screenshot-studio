#![allow(unused_variables)] // stubs: `req` must keep its name, it is the IPC argument key (rule 1)

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller};
use ss_core::ops;

use super::not_implemented;

#[tauri::command]
pub async fn list_tags() -> Result<Vec<Tag>, AppError> {
    not_implemented("list_tags", "2.1")
}

#[tauri::command]
pub async fn create_tag(req: CreateTagReq) -> Result<Tag, AppError> {
    ops::create_tag(Caller::Ui, req)
}

#[tauri::command]
pub async fn update_tag(req: UpdateTagReq) -> Result<Tag, AppError> {
    ops::update_tag(Caller::Ui, req)
}

#[tauri::command]
pub async fn delete_tag(req: DeleteTagReq) -> Result<OpResult, AppError> {
    ops::delete_tag(Caller::Ui, req)
}

#[tauri::command]
pub async fn set_tags(req: SetTagsReq) -> Result<OpResult, AppError> {
    ops::set_tags(Caller::Ui, req)
}

#[tauri::command]
pub async fn list_albums() -> Result<Vec<Album>, AppError> {
    not_implemented("list_albums", "2.1")
}

#[tauri::command]
pub async fn create_album(req: CreateAlbumReq) -> Result<Album, AppError> {
    ops::create_album(Caller::Ui, req)
}

#[tauri::command]
pub async fn rename_album(req: RenameAlbumReq) -> Result<Album, AppError> {
    ops::rename_album(Caller::Ui, req)
}

#[tauri::command]
pub async fn delete_album(req: DeleteAlbumReq) -> Result<OpResult, AppError> {
    ops::delete_album(Caller::Ui, req)
}

#[tauri::command]
pub async fn album_add(req: AlbumImagesReq) -> Result<OpResult, AppError> {
    ops::album_add(Caller::Ui, req)
}

#[tauri::command]
pub async fn album_remove(req: AlbumImagesReq) -> Result<OpResult, AppError> {
    ops::album_remove(Caller::Ui, req)
}

#[tauri::command]
pub async fn album_reorder(req: AlbumImagesReq) -> Result<OpResult, AppError> {
    ops::album_reorder(Caller::Ui, req)
}

#[tauri::command]
pub async fn set_favorite(req: SetFavoriteReq) -> Result<OpResult, AppError> {
    ops::set_favorite(Caller::Ui, req)
}

#[tauri::command]
pub async fn set_rating(req: SetRatingReq) -> Result<OpResult, AppError> {
    ops::set_rating(Caller::Ui, req)
}

#[tauri::command]
pub async fn set_note(req: SetNoteReq) -> Result<OpResult, AppError> {
    ops::set_note(Caller::Ui, req)
}

#[tauri::command]
pub async fn list_smart_filters() -> Result<Vec<SmartFilter>, AppError> {
    not_implemented("list_smart_filters", "2.1")
}

#[tauri::command]
pub async fn save_smart_filter(req: SaveSmartFilterReq) -> Result<SmartFilter, AppError> {
    ops::save_smart_filter(Caller::Ui, req)
}

#[tauri::command]
pub async fn delete_smart_filter(req: DeleteSmartFilterReq) -> Result<OpResult, AppError> {
    ops::delete_smart_filter(Caller::Ui, req)
}
