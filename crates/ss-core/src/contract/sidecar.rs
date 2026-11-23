//! Rust <-> Python sidecar contract (contract-sidecar-jsonrpc.md, version 1): JSON-RPC 2.0 over
//! newline-delimited stdio. Wire shapes only; the supervisor (lifecycle, timeouts, restart backoff)
//! is `ss-core::sidecar` (phase 3.2) and its constants live in `sidecar/config.rs`.
//!
//! Schema twin: `contracts/sidecar-jsonrpc.schema.json`; examples: `contracts/fixtures/sidecar/`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ipc::{AiDevice, CategoryScore, ComputeDevice, PiiFinding};

pub const JSONRPC_VERSION: &str = "2.0";
pub const PROTOCOL_VERSION: i64 = 1;

pub const OCR_MAX_ITEMS: usize = 16;
pub const EMBED_IMAGES_MAX_ITEMS: usize = 32;
pub const EMBED_TEXT_MAX_ITEMS: usize = 16;

/// Whole-request JSON-RPC error codes.
pub mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL: i64 = -32000;
    pub const MODEL_NOT_LOADED: i64 = -32001;
    pub const DEVICE_UNAVAILABLE: i64 = -32002;
    pub const CAPABILITY_DISABLED: i64 = -32003;
    pub const CANCELLED: i64 = -32004;
}

pub mod methods {
    pub const INITIALIZE: &str = "initialize";
    pub const OCR_RECOGNIZE: &str = "ocr.recognize";
    pub const EMBED_IMAGES: &str = "embed.images";
    pub const EMBED_TEXT: &str = "embed.text";
    pub const CLASSIFY_ZERO_SHOT: &str = "classify.zeroShot";
    pub const COLOR_DOMINANT: &str = "color.dominant";
    pub const PII_DETECT: &str = "pii.detect";
    pub const SHUTDOWN: &str = "shutdown";
    /// Notification sidecar -> Rust.
    pub const PROGRESS: &str = "progress";
    /// Notification Rust -> sidecar.
    pub const CANCEL: &str = "$/cancel";
    /// Every request method, in contract order.
    pub const ALL_REQUESTS: [&str; 8] =
        [INITIALIZE, OCR_RECOGNIZE, EMBED_IMAGES, EMBED_TEXT, CLASSIFY_ZERO_SHOT, COLOR_DOMINANT, PII_DETECT, SHUTDOWN];
}

// ---------------------------------------------------------------------------------------------
// Envelopes
// ---------------------------------------------------------------------------------------------

/// `{ "jsonrpc": "2.0", "id": N, "method": ..., "params": ... }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct RpcRequest {
    pub jsonrpc: String,
    #[ts(type = "number")]
    pub id: i64,
    #[serde(flatten)]
    pub call: RpcCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "params")]
#[ts(export_to = "sidecar/")]
pub enum RpcCall {
    #[serde(rename = "initialize")]
    Initialize(InitializeParams),
    #[serde(rename = "ocr.recognize")]
    OcrRecognize(OcrRecognizeParams),
    #[serde(rename = "embed.images")]
    EmbedImages(EmbedImagesParams),
    #[serde(rename = "embed.text")]
    EmbedText(EmbedTextParams),
    #[serde(rename = "classify.zeroShot")]
    ClassifyZeroShot(ClassifyZeroShotParams),
    #[serde(rename = "color.dominant")]
    ColorDominant(ColorDominantParams),
    #[serde(rename = "pii.detect")]
    PiiDetect(PiiDetectParams),
    #[serde(rename = "shutdown")]
    Shutdown(EmptyObject),
}

/// `{ "jsonrpc": "2.0", "id": N, "result": R }`; R is the result type of the request's method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct RpcResponse<R> {
    pub jsonrpc: String,
    #[ts(type = "number")]
    pub id: i64,
    pub result: R,
}

/// `{ "jsonrpc": "2.0", "id": N | null, "error": RpcError }` (id null only for a parse error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct RpcErrorResponse {
    pub jsonrpc: String,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    pub error: RpcError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct RpcError {
    /// See [`error_codes`].
    #[ts(type = "number")]
    pub code: i64,
    pub message: String,
    pub data: RpcErrorData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct RpcErrorData {
    #[serde(default)]
    pub detail: Option<String>,
}

/// Notification (no `id`), either direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct RpcNotification {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub note: RpcNote,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "params")]
#[ts(export_to = "sidecar/")]
pub enum RpcNote {
    /// sidecar -> Rust
    #[serde(rename = "progress")]
    Progress(ProgressParams),
    /// Rust -> sidecar; the sidecar answers the cancelled request with error -32004.
    #[serde(rename = "$/cancel")]
    Cancel(CancelParams),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ProgressParams {
    #[ts(type = "number")]
    pub request_id: i64,
    #[ts(type = "number")]
    pub done: i64,
    #[ts(type = "number")]
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct CancelParams {
    #[ts(type = "number")]
    pub id: i64,
}

/// `{}` (shutdown params and result).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[ts(export_to = "sidecar/")]
pub struct EmptyObject {}

// ---------------------------------------------------------------------------------------------
// Shared item types
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub enum OcrEngine {
    Rapidocr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub enum SidecarCapability {
    Ocr,
    Embed,
    Classify,
    Color,
    Pii,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ImageItem {
    #[ts(type = "number")]
    pub image_id: i64,
    /// Absolute Windows path; the sidecar reads the file itself (no image bytes on the pipe).
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub enum RpcItemErrorCode {
    Unreadable,
    Unsupported,
    Timeout,
    Internal,
}

/// Per-item failure inside a successful batch; the batch still returns every item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct RpcItemError {
    pub code: RpcItemErrorCode,
    pub message: String,
}

// ---------------------------------------------------------------------------------------------
// Methods: params and results
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct InitializeParams {
    /// Always [`PROTOCOL_VERSION`].
    #[ts(type = "number")]
    pub protocol_version: i64,
    pub device: AiDevice,
    /// `["vi", "en"]`
    pub languages: Vec<String>,
    pub embed_model: String,
    pub ocr_engine: OcrEngine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct InitializeResult {
    #[ts(type = "number")]
    pub protocol_version: i64,
    pub sidecar_version: String,
    pub capabilities: Vec<SidecarCapability>,
    pub device: ComputeDevice,
    pub embed_model: String,
    #[ts(type = "number")]
    pub embed_dim: i64,
}

/// `items` max [`OCR_MAX_ITEMS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct OcrRecognizeParams {
    pub items: Vec<ImageItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct OcrLine {
    pub text: String,
    /// [x1, y1, x2, y2] in image pixels.
    #[serde(rename = "box")]
    pub bbox: [f64; 4],
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct OcrItemResult {
    #[ts(type = "number")]
    pub image_id: i64,
    pub text: String,
    pub lines: Vec<OcrLine>,
    #[serde(default)]
    pub error: Option<RpcItemError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct OcrRecognizeResult {
    pub results: Vec<OcrItemResult>,
}

/// `items` max [`EMBED_IMAGES_MAX_ITEMS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct EmbedImagesParams {
    pub items: Vec<ImageItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct EmbedImageResult {
    #[ts(type = "number")]
    pub image_id: i64,
    /// base64 of little-endian float32, L2-normalized.
    #[serde(default)]
    pub vector: Option<String>,
    #[serde(default)]
    pub error: Option<RpcItemError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct EmbedImagesResult {
    pub model: String,
    #[ts(type = "number")]
    pub dim: i64,
    pub results: Vec<EmbedImageResult>,
}

/// `texts` max [`EMBED_TEXT_MAX_ITEMS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct EmbedTextParams {
    pub texts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct EmbedTextResult {
    pub model: String,
    #[ts(type = "number")]
    pub dim: i64,
    /// base64 of little-endian float32, L2-normalized; same order as `texts`.
    pub vectors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ClassifyZeroShotParams {
    pub items: Vec<ImageItem>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ClassifyItemResult {
    #[ts(type = "number")]
    pub image_id: i64,
    pub scores: Vec<CategoryScore>,
    #[serde(default)]
    pub error: Option<RpcItemError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ClassifyZeroShotResult {
    pub results: Vec<ClassifyItemResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ColorDominantParams {
    pub items: Vec<ImageItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ColorItemResult {
    #[ts(type = "number")]
    pub image_id: i64,
    /// "#RRGGBB"
    #[serde(default)]
    pub rgb: Option<String>,
    pub palette: Vec<String>,
    #[serde(default)]
    pub error: Option<RpcItemError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct ColorDominantResult {
    pub results: Vec<ColorItemResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct PiiLine {
    pub text: String,
    #[serde(rename = "box")]
    pub bbox: [f64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct PiiDetectParams {
    #[ts(type = "number")]
    pub image_id: i64,
    pub path: String,
    /// null = the sidecar runs OCR itself.
    #[serde(default)]
    pub lines: Option<Vec<PiiLine>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "sidecar/")]
pub struct PiiDetectResult {
    pub findings: Vec<PiiFinding>,
}
