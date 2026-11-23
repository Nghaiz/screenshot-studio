#![allow(unused_variables)] // stubs: `req` must keep its name, it is the IPC argument key (rule 1)

use ss_core::contract::AppError;
use ss_core::contract::ipc::*;

use super::not_implemented;

#[tauri::command]
pub async fn get_sidecar_status() -> Result<SidecarStatus, AppError> {
    not_implemented("get_sidecar_status", "3.2")
}

#[tauri::command]
pub async fn restart_sidecar() -> Result<SidecarStatus, AppError> {
    not_implemented("restart_sidecar", "3.2")
}

#[tauri::command]
pub async fn start_ai(req: StartAiReq) -> Result<JobStarted, AppError> {
    not_implemented("start_ai", "3.2")
}

#[tauri::command]
pub async fn find_duplicates(req: FindDuplicatesReq) -> Result<DuplicateResult, AppError> {
    not_implemented("find_duplicates", "3.2")
}

#[tauri::command]
pub async fn get_sessions(req: GetSessionsReq) -> Result<SessionsResult, AppError> {
    not_implemented("get_sessions", "3.2")
}

#[tauri::command]
pub async fn detect_issues(req: DetectIssuesReq) -> Result<IssuesResult, AppError> {
    not_implemented("detect_issues", "3.2")
}

#[tauri::command]
pub async fn detect_pii(req: ImageIdReq) -> Result<PiiResult, AppError> {
    not_implemented("detect_pii", "3.2")
}
