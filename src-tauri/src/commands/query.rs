#![allow(unused_variables)] // stubs: `req` must keep its name, it is the IPC argument key (rule 1)

use ss_core::contract::AppError;
use ss_core::contract::ipc::*;

use super::not_implemented;

#[tauri::command]
pub async fn query_images(req: ImageQuery) -> Result<ImagePage, AppError> {
    not_implemented("query_images", "1.2")
}

#[tauri::command]
pub async fn get_image(req: GetImageReq) -> Result<ImageDetail, AppError> {
    not_implemented("get_image", "1.2")
}

#[tauri::command]
pub async fn get_timeline(req: GetTimelineReq) -> Result<TimelineResult, AppError> {
    not_implemented("get_timeline", "1.2")
}

#[tauri::command]
pub async fn list_journal(req: LimitReq) -> Result<JournalPage, AppError> {
    not_implemented("list_journal", "2.1")
}

#[tauri::command]
pub async fn get_stats() -> Result<Stats, AppError> {
    not_implemented("get_stats", "3.2")
}
