#![allow(unused_variables)] // stubs: `req` must keep its name, it is the IPC argument key (rule 1)

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller};
use ss_core::ops;

use super::not_implemented;

#[tauri::command]
pub async fn get_app_info() -> Result<AppInfo, AppError> {
    not_implemented("get_app_info", "1.2")
}

#[tauri::command]
pub async fn add_library_root(req: AddLibraryRootReq) -> Result<LibraryRoot, AppError> {
    ops::add_library_root(Caller::Ui, req)
}

#[tauri::command]
pub async fn remove_library_root(req: RemoveLibraryRootReq) -> Result<OpResult, AppError> {
    ops::remove_library_root(Caller::Ui, req)
}

#[tauri::command]
pub async fn start_index(req: StartIndexReq) -> Result<JobStarted, AppError> {
    not_implemented("start_index", "1.2")
}

#[tauri::command]
pub async fn download_cloud_only(req: DownloadCloudOnlyReq) -> Result<JobStarted, AppError> {
    not_implemented("download_cloud_only", "1.2")
}

#[tauri::command]
pub async fn cancel_job(req: CancelJobReq) -> Result<(), AppError> {
    not_implemented("cancel_job", "1.2")
}

#[tauri::command]
pub async fn get_settings() -> Result<SettingsMap, AppError> {
    not_implemented("get_settings", "1.2")
}

#[tauri::command]
pub async fn set_setting(req: SetSettingReq) -> Result<(), AppError> {
    ops::set_setting(Caller::Ui, req)
}

#[tauri::command]
pub async fn get_counts() -> Result<CatalogCounts, AppError> {
    not_implemented("get_counts", "1.2")
}

#[tauri::command]
pub async fn get_recent(req: LimitReq) -> Result<Vec<ImageSummary>, AppError> {
    not_implemented("get_recent", "4.1")
}
