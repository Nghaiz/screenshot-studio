//! Tags, albums, favourites, ratings, notes, smart filters. Thin wrappers over `ss_core::AppCore`
//! (writes as `Caller::Ui`); image-level changes emit `library-changed` (`fileOp`).

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller};
use tauri::{AppHandle, State};

use crate::state::{AppState, blocking, emit_changed};

/// Runs a journaled organize op and announces the images it changed.
async fn changed_op<F>(state: State<'_, AppState>, app: AppHandle, ids: Vec<i64>, f: F) -> Result<OpResult, AppError>
where
    F: FnOnce(&ss_core::AppCore) -> Result<OpResult, AppError> + Send + 'static,
{
    let core = state.core.clone();
    let r = blocking(move || f(&core)).await?;
    emit_changed(&app, LibraryChangeReason::FileOp, &r, Some(ids));
    Ok(r)
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, AppError> {
    let core = state.core.clone();
    blocking(move || core.list_tags()).await
}

#[tauri::command]
pub async fn create_tag(req: CreateTagReq, state: State<'_, AppState>) -> Result<Tag, AppError> {
    let core = state.core.clone();
    blocking(move || core.create_tag(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn update_tag(req: UpdateTagReq, state: State<'_, AppState>) -> Result<Tag, AppError> {
    let core = state.core.clone();
    blocking(move || core.update_tag(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn delete_tag(req: DeleteTagReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let core = state.core.clone();
    let r = blocking(move || core.delete_tag(Caller::Ui, req)).await?;
    emit_changed(&app, LibraryChangeReason::FileOp, &r, None);
    Ok(r)
}

#[tauri::command]
pub async fn set_tags(req: SetTagsReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let ids = req.image_ids.clone();
    changed_op(state, app, ids, move |c| c.set_tags(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn list_albums(state: State<'_, AppState>) -> Result<Vec<Album>, AppError> {
    let core = state.core.clone();
    blocking(move || core.list_albums()).await
}

#[tauri::command]
pub async fn create_album(req: CreateAlbumReq, state: State<'_, AppState>) -> Result<Album, AppError> {
    let core = state.core.clone();
    blocking(move || core.create_album(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn rename_album(req: RenameAlbumReq, state: State<'_, AppState>) -> Result<Album, AppError> {
    let core = state.core.clone();
    blocking(move || core.rename_album(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn delete_album(req: DeleteAlbumReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let core = state.core.clone();
    let r = blocking(move || core.delete_album(Caller::Ui, req)).await?;
    emit_changed(&app, LibraryChangeReason::FileOp, &r, None);
    Ok(r)
}

#[tauri::command]
pub async fn album_add(req: AlbumImagesReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let ids = req.image_ids.clone();
    changed_op(state, app, ids, move |c| c.album_add(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn album_remove(req: AlbumImagesReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let ids = req.image_ids.clone();
    changed_op(state, app, ids, move |c| c.album_remove(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn album_reorder(req: AlbumImagesReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let ids = req.image_ids.clone();
    changed_op(state, app, ids, move |c| c.album_reorder(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn set_favorite(req: SetFavoriteReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let ids = req.image_ids.clone();
    changed_op(state, app, ids, move |c| c.set_favorite(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn set_rating(req: SetRatingReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let ids = req.image_ids.clone();
    changed_op(state, app, ids, move |c| c.set_rating(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn set_note(req: SetNoteReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let ids = vec![req.image_id];
    changed_op(state, app, ids, move |c| c.set_note(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn list_smart_filters(state: State<'_, AppState>) -> Result<Vec<SmartFilter>, AppError> {
    let core = state.core.clone();
    blocking(move || core.list_smart_filters()).await
}

#[tauri::command]
pub async fn save_smart_filter(req: SaveSmartFilterReq, state: State<'_, AppState>) -> Result<SmartFilter, AppError> {
    let core = state.core.clone();
    blocking(move || core.save_smart_filter(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn delete_smart_filter(req: DeleteSmartFilterReq, state: State<'_, AppState>) -> Result<OpResult, AppError> {
    let core = state.core.clone();
    blocking(move || core.delete_smart_filter(Caller::Ui, req)).await
}
