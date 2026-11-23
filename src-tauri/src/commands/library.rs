//! Library, jobs, settings and counts. Thin wrappers over `ss_core::AppCore` (writes as `Caller::Ui`).

use std::sync::Arc;

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller};
use tauri::{AppHandle, State};

use super::not_implemented;
use crate::state::{AppState, TauriSink, blocking};

#[tauri::command]
pub async fn get_app_info(state: State<'_, AppState>) -> Result<AppInfo, AppError> {
    let core = state.core.clone();
    blocking(move || core.get_app_info()).await
}

#[tauri::command]
pub async fn add_library_root(req: AddLibraryRootReq, state: State<'_, AppState>) -> Result<LibraryRoot, AppError> {
    let core = state.core.clone();
    blocking(move || core.add_library_root(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn remove_library_root(req: RemoveLibraryRootReq, state: State<'_, AppState>) -> Result<OpResult, AppError> {
    let core = state.core.clone();
    blocking(move || core.remove_library_root(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn start_index(req: StartIndexReq, state: State<'_, AppState>, app: AppHandle) -> Result<JobStarted, AppError> {
    let core = state.core.clone();
    blocking(move || core.start_index(req, Arc::new(TauriSink { app }))).await
}

#[tauri::command]
pub async fn download_cloud_only(req: DownloadCloudOnlyReq, state: State<'_, AppState>, app: AppHandle) -> Result<JobStarted, AppError> {
    let core = state.core.clone();
    blocking(move || core.start_download_cloud_only(req, Arc::new(TauriSink { app }))).await
}

#[tauri::command]
pub async fn cancel_job(req: CancelJobReq, state: State<'_, AppState>) -> Result<(), AppError> {
    state.core.cancel_job(req)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsMap, AppError> {
    let core = state.core.clone();
    blocking(move || core.get_settings()).await
}

#[tauri::command]
pub async fn set_setting(req: SetSettingReq, state: State<'_, AppState>) -> Result<(), AppError> {
    let core = state.core.clone();
    blocking(move || core.set_setting(Caller::Ui, req)).await
}

#[tauri::command]
pub async fn get_counts(state: State<'_, AppState>) -> Result<CatalogCounts, AppError> {
    let core = state.core.clone();
    blocking(move || core.get_counts()).await
}

#[tauri::command]
pub async fn get_recent(req: LimitReq) -> Result<Vec<ImageSummary>, AppError> {
    let _ = req;
    not_implemented("get_recent", "4.1")
}
