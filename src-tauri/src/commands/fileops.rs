//! File ops, permanent delete and undo. Thin wrappers over `ss_core::AppCore` (writes as
//! `Caller::Ui`); successful changes emit `library-changed` (`fileOp`, or `undo`).

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller};
use tauri::{AppHandle, State};

use super::not_implemented;
use crate::state::{AppState, blocking, emit_changed};

/// Runs a file op and announces a refetch (rows may have left or joined the library).
async fn file_op<F>(state: State<'_, AppState>, app: AppHandle, reason: LibraryChangeReason, f: F) -> Result<OpResult, AppError>
where
    F: FnOnce(&ss_core::AppCore) -> Result<OpResult, AppError> + Send + 'static,
{
    let core = state.core.clone();
    let r = blocking(move || f(&core)).await?;
    emit_changed(&app, reason, &r, None);
    Ok(r)
}

#[tauri::command]
pub async fn move_images(req: MoveImagesReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    file_op(state, app, LibraryChangeReason::FileOp, move |c| c.move_images(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn preview_rename(req: PreviewRenameReq, state: State<'_, AppState>) -> Result<RenamePreview, AppError> {
    let core = state.core.clone();
    blocking(move || core.preview_rename(req)).await
}

#[tauri::command]
pub async fn rename_images(req: RenameImagesReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    file_op(state, app, LibraryChangeReason::FileOp, move |c| c.rename_images(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn trash_images(req: ImageIdsReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    file_op(state, app, LibraryChangeReason::FileOp, move |c| c.trash_images(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn restore_images(req: ImageIdsReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    file_op(state, app, LibraryChangeReason::FileOp, move |c| c.restore_images(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn recycle_images(req: ImageIdsReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    file_op(state, app, LibraryChangeReason::FileOp, move |c| c.recycle_images(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn request_permanent_delete(req: ImageIdsReq, state: State<'_, AppState>) -> Result<PermanentDeleteTicket, AppError> {
    let core = state.core.clone();
    blocking(move || core.request_permanent_delete(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn delete_permanently(req: DeletePermanentlyReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    file_op(state, app, LibraryChangeReason::FileOp, move |c| c.delete_permanently(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn undo(req: UndoReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    file_op(state, app, LibraryChangeReason::Undo, move |c| c.undo(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn export_zip(req: ExportZipReq) -> Result<JobStarted, AppError> {
    let _ = req;
    not_implemented("export_zip", "4.1")
}

#[tauri::command]
pub async fn export_pdf(req: ExportPdfReq) -> Result<JobStarted, AppError> {
    let _ = req;
    not_implemented("export_pdf", "4.1")
}

#[tauri::command]
pub async fn copy_image_to_clipboard(req: ImageIdReq) -> Result<(), AppError> {
    let _ = req;
    not_implemented("copy_image_to_clipboard", "4.1")
}
