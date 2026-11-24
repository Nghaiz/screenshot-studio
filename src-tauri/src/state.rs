//! Managed app state, the Tauri event sink for `ss-core` jobs, and the `ssimg` response adapter.

use std::sync::Arc;

use ss_core::AppCore;
use ss_core::contract::ipc::{JobProgress, LibraryChangeReason, LibraryChanged, NewScreenshot, OpResult, events};
use ss_core::contract::{AppError, ErrorKind};
use ss_core::jobs::EventSink;
use ss_core::ssimg::SsimgResponse;
use tauri::{AppHandle, Emitter};

pub struct AppState {
    pub core: Arc<AppCore>,
}

/// Emits `job-progress` and `library-changed` to every webview.
pub struct TauriSink {
    pub app: AppHandle,
}

impl EventSink for TauriSink {
    fn job_progress(&self, p: &JobProgress) {
        if let Err(e) = self.app.emit(events::JOB_PROGRESS, p) {
            eprintln!("screenshot-studio: emit {}: {e}", events::JOB_PROGRESS);
        }
    }

    fn library_changed(&self, c: &LibraryChanged) {
        if let Err(e) = self.app.emit(events::LIBRARY_CHANGED, c) {
            eprintln!("screenshot-studio: emit {}: {e}", events::LIBRARY_CHANGED);
        }
        if c.reason != LibraryChangeReason::Indexed {
            crate::tray::refresh(&self.app, false);
        }
    }

    /// One `new-screenshot` per file, one toast for the batch, then the tray's Recent menu.
    fn new_screenshots(&self, items: &[NewScreenshot]) {
        for n in items {
            if let Err(e) = self.app.emit(events::NEW_SCREENSHOT, n) {
                eprintln!("screenshot-studio: emit {}: {e}", events::NEW_SCREENSHOT);
            }
        }
        crate::live::toast_new(&self.app, items);
        crate::tray::refresh(&self.app, true);
    }
}

/// Emits `library-changed` after a mutation that changed something (`imageIds: null` = refetch all).
pub fn emit_changed(app: &AppHandle, reason: LibraryChangeReason, r: &OpResult, image_ids: Option<Vec<i64>>) {
    if r.affected > 0 {
        TauriSink { app: app.clone() }.library_changed(&LibraryChanged { reason, image_ids });
    }
}

/// Runs blocking `ss-core` work (SQLite, file reads) off the async runtime's worker threads.
pub async fn blocking<T, F>(f: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::new(ErrorKind::Internal, "background task failed").with_detail(e.to_string()))?
}

pub fn http_response(r: SsimgResponse) -> tauri::http::Response<Vec<u8>> {
    let mut b = tauri::http::Response::builder().status(r.status);
    for (k, v) in &r.headers {
        b = b.header(*k, v);
    }
    b.body(r.body).unwrap_or_else(|e| {
        eprintln!("screenshot-studio: ssimg response: {e}");
        let mut resp = tauri::http::Response::new(Vec::new());
        *resp.status_mut() = tauri::http::StatusCode::INTERNAL_SERVER_ERROR;
        resp
    })
}
