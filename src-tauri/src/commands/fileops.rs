#![allow(unused_variables)] // stubs: `req` must keep its name, it is the IPC argument key (rule 1)

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller};
use ss_core::ops;

use super::not_implemented;

#[tauri::command]
pub async fn move_images(req: MoveImagesReq) -> Result<OpResult, AppError> {
    ops::move_images(Caller::Ui, req)
}

#[tauri::command]
pub async fn preview_rename(req: PreviewRenameReq) -> Result<RenamePreview, AppError> {
    not_implemented("preview_rename", "2.1")
}

#[tauri::command]
pub async fn rename_images(req: RenameImagesReq) -> Result<OpResult, AppError> {
    ops::rename_images(Caller::Ui, req)
}

#[tauri::command]
pub async fn trash_images(req: ImageIdsReq) -> Result<OpResult, AppError> {
    ops::trash_images(Caller::Ui, req)
}

#[tauri::command]
pub async fn restore_images(req: ImageIdsReq) -> Result<OpResult, AppError> {
    ops::restore_images(Caller::Ui, req)
}

#[tauri::command]
pub async fn recycle_images(req: ImageIdsReq) -> Result<OpResult, AppError> {
    ops::recycle_images(Caller::Ui, req)
}

#[tauri::command]
pub async fn request_permanent_delete(req: ImageIdsReq) -> Result<PermanentDeleteTicket, AppError> {
    ops::request_permanent_delete(Caller::Ui, req)
}

#[tauri::command]
pub async fn delete_permanently(req: DeletePermanentlyReq) -> Result<OpResult, AppError> {
    ops::delete_permanently(Caller::Ui, req)
}

#[tauri::command]
pub async fn undo(req: UndoReq) -> Result<OpResult, AppError> {
    ops::undo(Caller::Ui, req)
}

#[tauri::command]
pub async fn export_zip(req: ExportZipReq) -> Result<JobStarted, AppError> {
    not_implemented("export_zip", "4.1")
}

#[tauri::command]
pub async fn export_pdf(req: ExportPdfReq) -> Result<JobStarted, AppError> {
    not_implemented("export_pdf", "4.1")
}

#[tauri::command]
pub async fn copy_image_to_clipboard(req: ImageIdReq) -> Result<(), AppError> {
    not_implemented("copy_image_to_clipboard", "4.1")
}
