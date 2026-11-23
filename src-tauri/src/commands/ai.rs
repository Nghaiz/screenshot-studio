//! AI sidecar, AI job and smart features. Thin wrappers over `ss_core::AppCore` (phase 3.2).

use std::sync::Arc;

use ss_core::contract::AppError;
use ss_core::contract::ipc::*;
use tauri::{AppHandle, State};

use crate::state::{AppState, TauriSink, blocking};

#[tauri::command]
pub async fn get_sidecar_status(state: State<'_, AppState>) -> Result<SidecarStatus, AppError> {
    Ok(state.core.get_sidecar_status())
}

#[tauri::command]
pub async fn restart_sidecar(state: State<'_, AppState>) -> Result<SidecarStatus, AppError> {
    let core = state.core.clone();
    blocking(move || core.restart_sidecar()).await
}

#[tauri::command]
pub async fn start_ai(req: StartAiReq, state: State<'_, AppState>, app: AppHandle) -> Result<JobStarted, AppError> {
    let core = state.core.clone();
    blocking(move || core.start_ai(req, Arc::new(TauriSink { app }))).await
}

#[tauri::command]
pub async fn find_duplicates(req: FindDuplicatesReq, state: State<'_, AppState>) -> Result<DuplicateResult, AppError> {
    let core = state.core.clone();
    blocking(move || core.find_duplicates(req)).await
}

#[tauri::command]
pub async fn get_sessions(req: GetSessionsReq, state: State<'_, AppState>) -> Result<SessionsResult, AppError> {
    let core = state.core.clone();
    blocking(move || core.get_sessions(req)).await
}

#[tauri::command]
pub async fn detect_issues(req: DetectIssuesReq, state: State<'_, AppState>) -> Result<IssuesResult, AppError> {
    let core = state.core.clone();
    blocking(move || core.detect_issues(req)).await
}

#[tauri::command]
pub async fn detect_pii(req: ImageIdReq, state: State<'_, AppState>) -> Result<PiiResult, AppError> {
    let core = state.core.clone();
    blocking(move || core.detect_pii(req)).await
}
