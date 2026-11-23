//! Error envelope shared by every Tauri command (contract-tauri-ipc.md, global rule 5).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Machine-readable error class; the UI maps it to an i18n string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    NotFound,
    Io,
    Db,
    InvalidArgument,
    Conflict,
    SidecarUnavailable,
    Cancelled,
    PermissionDenied,
    ProtectedRoot,
    ConfirmationRequired,
    Internal,
}

/// The rejected-promise value of every command: `{ kind, message, detail }`.
/// `message` is human-readable English; `detail` is never omitted (null when absent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{kind:?}: {message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), detail: None }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Returned by every command stub until its owning phase implements it.
    pub fn not_implemented() -> Self {
        Self::new(ErrorKind::Internal, "not implemented")
    }
}
