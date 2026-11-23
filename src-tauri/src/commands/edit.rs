#![allow(unused_variables)] // stubs: `req` must keep its name, it is the IPC argument key (rule 1)

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller};
use ss_core::ops;

use super::not_implemented;

/// The one raw-body command (rule 1): body = PNG bytes, headers `x-image-id`, `x-mode`,
/// `x-format`, `x-quality` (`ss_core::contract::ipc::edit_headers`). Decoding lands in 4.1.
#[tauri::command]
pub async fn save_edited_image(request: tauri::ipc::Request<'_>) -> Result<ImageSummary, AppError> {
    not_implemented("save_edited_image", "4.1")
}

#[tauri::command]
pub async fn convert_images(req: ConvertImagesReq) -> Result<OpResult, AppError> {
    ops::convert_images(Caller::Ui, req)
}

#[tauri::command]
pub async fn stitch_images(req: StitchImagesReq) -> Result<ImageSummary, AppError> {
    ops::stitch_images(Caller::Ui, req)
}
