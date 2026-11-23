//! Read queries. Thin wrappers over `ss_core::AppCore`.

use ss_core::contract::AppError;
use ss_core::contract::ipc::*;
use tauri::State;

use super::not_implemented;
use crate::state::{AppState, blocking};

#[tauri::command]
pub async fn query_images(req: ImageQuery, state: State<'_, AppState>) -> Result<ImagePage, AppError> {
    let core = state.core.clone();
    blocking(move || core.query_images(&req)).await
}

#[tauri::command]
pub async fn get_image(req: GetImageReq, state: State<'_, AppState>) -> Result<ImageDetail, AppError> {
    let core = state.core.clone();
    blocking(move || core.get_image(req)).await
}

#[tauri::command]
pub async fn get_timeline(req: GetTimelineReq, state: State<'_, AppState>) -> Result<TimelineResult, AppError> {
    let core = state.core.clone();
    blocking(move || core.get_timeline(&req)).await
}

#[tauri::command]
pub async fn list_journal(req: LimitReq) -> Result<JournalPage, AppError> {
    let _ = req;
    not_implemented("list_journal", "2.1")
}

#[tauri::command]
pub async fn get_stats() -> Result<Stats, AppError> {
    not_implemented("get_stats", "3.2")
}
