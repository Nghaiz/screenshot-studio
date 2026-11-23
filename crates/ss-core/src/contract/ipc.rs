//! Rust <-> UI contract (contract-tauri-ipc.md, version 1). Transcription only: no behaviour.
//!
//! Rules applied to every type here:
//! - fields camelCase on the wire, enum variants camelCase string literals (rule 2);
//! - every integer is `i64` with an explicit ts override per field shape (rule 3):
//!   `i64` -> `number`, `Option<i64>` -> `number | null`, `Vec<i64>` -> `number[]`;
//! - `Option<T>` is serialized as `null`, never omitted; a missing optional input key reads as `null` (rule 4).
//!
//! Request structs are named `<CommandInPascalCase>Req`; the full command table is [`COMMANDS`].

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use ts_rs::TS;

use super::error::AppError;

// ---------------------------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AspectClass {
    Phone,
    Pc,
    Square,
    Wide,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CloudState {
    Local,
    CloudOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ThumbState {
    Pending,
    Ok,
    Error,
    Offline,
}

/// `relevance` is valid only when `ImageQuery.text` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SortField {
    Taken,
    Name,
    Size,
    Width,
    Rating,
    Random,
    Relevance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TextMode {
    Any,
    Filename,
    Ocr,
    Semantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TagMatch {
    Any,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TrashFilter {
    Exclude,
    Only,
    Include,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SidecarState {
    Absent,
    Starting,
    Ready,
    Error,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ComputeDevice {
    Cuda,
    Cpu,
}

/// Value of the `aiDevice` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AiDevice {
    Auto,
    Cuda,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AppWarningCode {
    HotkeyUnavailable,
    WatcherFailed,
    LowDiskSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TimelineBucketSize {
    Day,
    Month,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum MoveMode {
    Move,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum OnConflict {
    Skip,
    Rename,
}

/// Journal operation; the wire strings equal the `op_journal.op` CHECK values in 0001_init.sql.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JournalOp {
    Move,
    Copy,
    Rename,
    Trash,
    Restore,
    Recycle,
    Purge,
    Tag,
    Untag,
    Favorite,
    Rating,
    Note,
    AlbumAdd,
    AlbumRemove,
    AlbumReorder,
    EditOverwrite,
    Convert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AiTask {
    Ocr,
    Embed,
    Classify,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateMode {
    Exact,
    Similar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateReason {
    Largest,
    Newest,
    Sharpest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PiiKind {
    Email,
    Phone,
    Url,
    Card,
}

/// `copy` writes a new file next to the original; `overwrite` replaces it and keeps a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum EditMode {
    Copy,
    Overwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum StitchDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PdfPageSize {
    A4,
    Fit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Index,
    CloudDownload,
    Ai,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LibraryChangeReason {
    Indexed,
    Watcher,
    FileOp,
    Undo,
    Ai,
}

/// Keys of the backend `setting` table (contract "Setting keys"); see [`SettingKey::default_value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingKey {
    Hotkey,
    WatcherEnabled,
    Autostart,
    AutoDownloadCloudOnly,
    AiDevice,
    DupHamming,
    SessionGapMinutes,
    TinyMaxPixels,
    BlurThreshold,
    SidecarVenv,
}

impl SettingKey {
    pub const ALL: [SettingKey; 10] = [
        SettingKey::Hotkey,
        SettingKey::WatcherEnabled,
        SettingKey::Autostart,
        SettingKey::AutoDownloadCloudOnly,
        SettingKey::AiDevice,
        SettingKey::DupHamming,
        SettingKey::SessionGapMinutes,
        SettingKey::TinyMaxPixels,
        SettingKey::BlurThreshold,
        SettingKey::SidecarVenv,
    ];

    /// Default value as stored JSON. `sidecarVenv: null` means the default venv
    /// `D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv`.
    pub fn default_value(self) -> JsonValue {
        use serde_json::json;
        match self {
            SettingKey::Hotkey => json!("Ctrl+Alt+S"),
            SettingKey::WatcherEnabled => json!(true),
            SettingKey::Autostart => json!(false),
            SettingKey::AutoDownloadCloudOnly => json!(false),
            SettingKey::AiDevice => json!("auto"),
            SettingKey::DupHamming => json!(6),
            SettingKey::SessionGapMinutes => json!(30),
            SettingKey::TinyMaxPixels => json!(40000),
            SettingKey::BlurThreshold => json!(60),
            SettingKey::SidecarVenv => JsonValue::Null,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRoot {
    #[ts(type = "number")]
    pub id: i64,
    pub path: String,
    #[ts(type = "number")]
    pub added_at: i64,
    #[ts(type = "number")]
    pub image_count: i64,
    /// true for any root under a OneDrive folder.
    pub protected_root: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageSummary {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub root_id: i64,
    pub folder: String,
    pub file_name: String,
    #[ts(type = "number")]
    pub size_bytes: i64,
    #[ts(type = "number")]
    pub taken_ms: i64,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub width: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub height: Option<i64>,
    #[serde(default)]
    pub aspect: Option<AspectClass>,
    pub cloud_state: CloudState,
    pub thumb_state: ThumbState,
    pub favorite: bool,
    /// 0..5
    #[ts(type = "number")]
    pub rating: i64,
    /// null = no ocr_result row yet; true/false = ocr_result.text non-empty after trim.
    #[serde(default)]
    pub has_text: Option<bool>,
    /// "#RRGGBB"
    #[serde(default)]
    pub dominant_rgb: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub trashed_at: Option<i64>,
    pub missing: bool,
    #[ts(type = "number")]
    pub mtime_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageDetail {
    #[serde(flatten)]
    pub summary: ImageSummary,
    pub abs_path: String,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub created_ms: Option<i64>,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub tags: Vec<Tag>,
    pub albums: Vec<AlbumRef>,
    #[serde(default)]
    pub ocr_text: Option<String>,
    pub categories: Vec<CategoryScore>,
    #[serde(default)]
    pub blur_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    /// "#RRGGBB"
    pub color: String,
    #[ts(type = "number")]
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AlbumRef {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub count: i64,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub cover_image_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CategoryScore {
    pub label: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub field: SortField,
    pub dir: SortDir,
}

/// Filter + sort + page. An empty array means "no constraint". `cursor` is opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageQuery {
    #[serde(default)]
    pub text: Option<String>,
    pub text_mode: TextMode,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub date_from: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub date_to: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub min_width: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub max_width: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub min_height: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub max_height: Option<i64>,
    pub aspects: Vec<AspectClass>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub min_size: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub max_size: Option<i64>,
    pub folders: Vec<String>,
    #[ts(type = "number[]")]
    pub tag_ids: Vec<i64>,
    pub tag_match: TagMatch,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub album_id: Option<i64>,
    #[serde(default)]
    pub color: Option<String>,
    /// 0..100
    #[ts(type = "number")]
    pub color_tolerance: i64,
    pub categories: Vec<String>,
    pub favorite_only: bool,
    #[ts(type = "number")]
    pub min_rating: i64,
    #[serde(default)]
    pub has_text: Option<bool>,
    pub trash: TrashFilter,
    pub include_missing: bool,
    pub sort: SortSpec,
    #[serde(default)]
    pub cursor: Option<String>,
    /// 1..500
    #[ts(type = "number")]
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImagePage {
    pub items: Vec<ImageSummary>,
    #[ts(type = "number")]
    pub total: i64,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpFailure {
    #[ts(type = "number")]
    pub image_id: i64,
    pub kind: super::error::ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    #[serde(default)]
    #[ts(type = "number | null")]
    pub batch_id: Option<i64>,
    #[ts(type = "number")]
    pub affected: i64,
    pub failed: Vec<OpFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SidecarStatus {
    pub state: SidecarState,
    #[serde(default)]
    pub detail: Option<String>,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub device: Option<ComputeDevice>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppWarning {
    pub code: AppWarningCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    #[ts(type = "number")]
    pub contract_version: i64,
    pub data_dir: String,
    pub library_roots: Vec<LibraryRoot>,
    pub sidecar: SidecarStatus,
    pub warnings: Vec<AppWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobStarted {
    #[ts(type = "number")]
    pub job_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SmartFilter {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub query: ImageQuery,
    #[ts(type = "number")]
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCounts {
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub total_bytes: i64,
    #[ts(type = "number")]
    pub thumb_ok: i64,
    #[ts(type = "number")]
    pub thumb_pending: i64,
    #[ts(type = "number")]
    pub cloud_only: i64,
    #[ts(type = "number")]
    pub missing: i64,
    #[ts(type = "number")]
    pub trashed: i64,
    pub roots: Vec<LibraryRoot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CountBy {
    pub key: String,
    #[ts(type = "number")]
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    #[serde(flatten)]
    pub counts: CatalogCounts,
    /// key "YYYY-MM"
    pub by_month: Vec<CountBy>,
    pub by_aspect: Vec<CountBy>,
    pub by_folder: Vec<CountBy>,
    pub by_category: Vec<CountBy>,
    pub top_tags: Vec<CountBy>,
    #[ts(type = "number")]
    pub duplicates_exact: i64,
    #[ts(type = "number")]
    pub duplicates_similar: i64,
}

/// `get_settings` result: setting key -> stored JSON value. A serde newtype struct is already
/// serialized as its inner map (no `transparent` needed; ts-rs does not parse that attribute).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SettingsMap(#[ts(type = "Record<string, unknown>")] pub BTreeMap<String, JsonValue>);

// ---------------------------------------------------------------------------------------------
// Command requests and command-specific results (table order of contract-tauri-ipc.md)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AddLibraryRootReq {
    pub path: String,
}

/// Catalog only, never touches files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemoveLibraryRootReq {
    #[ts(type = "number")]
    pub root_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartIndexReq {
    #[serde(default)]
    #[ts(type = "number | null")]
    pub root_id: Option<i64>,
}

/// Reads cloud-only files so OneDrive hydrates them; never pins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCloudOnlyReq {
    #[ts(type = "number")]
    pub root_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelJobReq {
    #[ts(type = "number")]
    pub job_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GetImageReq {
    #[ts(type = "number")]
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GetTimelineReq {
    pub query: ImageQuery,
    pub bucket: TimelineBucketSize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TimelineBucket {
    #[ts(type = "number")]
    pub start_ms: i64,
    #[ts(type = "number")]
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TimelineResult {
    pub buckets: Vec<TimelineBucket>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetSettingReq {
    pub key: SettingKey,
    #[ts(type = "unknown")]
    pub value: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateTagReq {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTagReq {
    #[ts(type = "number")]
    pub tag_id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTagReq {
    #[ts(type = "number")]
    pub tag_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetTagsReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    #[ts(type = "number[]")]
    pub add: Vec<i64>,
    #[ts(type = "number[]")]
    pub remove: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlbumReq {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenameAlbumReq {
    #[ts(type = "number")]
    pub album_id: i64,
    pub name: String,
}

/// Deletes the album only, never its images.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAlbumReq {
    #[ts(type = "number")]
    pub album_id: i64,
}

/// Used by `album_add`, `album_remove` and `album_reorder` (for reorder `imageIds` is the new order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AlbumImagesReq {
    #[ts(type = "number")]
    pub album_id: i64,
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetFavoriteReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetRatingReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    /// 0..5
    #[ts(type = "number")]
    pub rating: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetNoteReq {
    #[ts(type = "number")]
    pub image_id: i64,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SaveSmartFilterReq {
    #[serde(default)]
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    pub name: String,
    pub query: ImageQuery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSmartFilterReq {
    #[ts(type = "number")]
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MoveImagesReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    pub dest_dir: String,
    pub mode: MoveMode,
    pub on_conflict: OnConflict,
}

/// `pattern` tokens: `{name}` `{n}` `{n:03}` `{date:YYYYMMDD}` `{time:HHmmss}` `{w}` `{h}` `{ext}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRenameReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreviewItem {
    #[ts(type = "number")]
    pub image_id: i64,
    pub old_name: String,
    pub new_name: String,
    pub conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreview {
    pub items: Vec<RenamePreviewItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenameItem {
    #[ts(type = "number")]
    pub image_id: i64,
    pub new_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenameImagesReq {
    pub items: Vec<RenameItem>,
}

/// Used by `trash_images`, `restore_images`, `recycle_images`, `request_permanent_delete`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageIdsReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
}

/// Result of `request_permanent_delete`: token valid 60 s, bound to the exact id set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PermanentDeleteTicket {
    pub token: String,
    #[ts(type = "number")]
    pub count: i64,
    #[ts(type = "number")]
    pub total_bytes: i64,
    #[ts(type = "number")]
    pub expires_at_ms: i64,
}

/// `typedConfirmation` must equal `"DELETE"`, else `confirmationRequired`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeletePermanentlyReq {
    pub token: String,
    pub typed_confirmation: String,
}

/// `batchId: null` = latest undoable batch (UI only; `ssctl undo` always passes an id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UndoReq {
    #[serde(default)]
    #[ts(type = "number | null")]
    pub batch_id: Option<i64>,
}

/// Used by `list_journal` and `get_recent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LimitReq {
    #[ts(type = "number")]
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    #[ts(type = "number")]
    pub batch_id: i64,
    pub op: JournalOp,
    #[ts(type = "number")]
    pub count: i64,
    #[ts(type = "number")]
    pub created_at: i64,
    pub undoable: bool,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub undone_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JournalPage {
    pub items: Vec<JournalEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartAiReq {
    pub tasks: Vec<AiTask>,
    #[serde(default)]
    #[ts(type = "number[] | null")]
    pub image_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FindDuplicatesReq {
    pub mode: DuplicateMode,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub hamming: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub root_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    #[ts(type = "number")]
    pub group_id: i64,
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    #[ts(type = "number")]
    pub best_id: i64,
    pub reason: DuplicateReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateResult {
    pub groups: Vec<DuplicateGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionsReq {
    pub query: ImageQuery,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub gap_minutes: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageSession {
    #[ts(type = "number")]
    pub start_ms: i64,
    #[ts(type = "number")]
    pub end_ms: i64,
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionsResult {
    pub sessions: Vec<ImageSession>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DetectIssuesReq {
    #[serde(default)]
    #[ts(type = "number | null")]
    pub root_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IssuesResult {
    #[ts(type = "number[]")]
    pub blurry: Vec<i64>,
    #[ts(type = "number[]")]
    pub tiny: Vec<i64>,
}

/// Used by `detect_pii` and `copy_image_to_clipboard`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageIdReq {
    #[ts(type = "number")]
    pub image_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PiiFinding {
    pub kind: PiiKind,
    pub text: String,
    /// [x1, y1, x2, y2] in image pixels.
    #[serde(rename = "box")]
    pub bbox: [f64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PiiResult {
    pub findings: Vec<PiiFinding>,
}

/// Headers of the raw-body `save_edited_image` command (body = PNG bytes).
pub mod edit_headers {
    pub const IMAGE_ID: &str = "x-image-id";
    /// `copy` | `overwrite` ([`super::EditMode`]).
    pub const MODE: &str = "x-mode";
    /// `png` | `jpeg` | `webp` ([`super::ImageFormat`]).
    pub const FORMAT: &str = "x-format";
    pub const QUALITY: &str = "x-quality";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConvertImagesReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    pub format: ImageFormat,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub quality: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub max_edge: Option<i64>,
    pub output: EditMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StitchImagesReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    pub direction: StitchDirection,
    #[ts(type = "number")]
    pub gap: i64,
    pub background: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportZipReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    pub dest_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfReq {
    #[ts(type = "number[]")]
    pub image_ids: Vec<i64>,
    pub dest_path: String,
    pub page_size: PdfPageSize,
    /// 1 | 2 | 4
    #[ts(type = "number")]
    pub per_page: i64,
}

// ---------------------------------------------------------------------------------------------
// Events (Rust -> UI)
// ---------------------------------------------------------------------------------------------

pub mod events {
    pub const JOB_PROGRESS: &str = "job-progress";
    pub const LIBRARY_CHANGED: &str = "library-changed";
    pub const SIDECAR_STATUS: &str = "sidecar-status";
    pub const NEW_SCREENSHOT: &str = "new-screenshot";
    pub const OPEN_IMAGE: &str = "open-image";
    /// `job-progress` at most this many events per second per job; the final one has `finished: true`.
    pub const JOB_PROGRESS_MAX_PER_SEC: u32 = 10;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    #[ts(type = "number")]
    pub job_id: i64,
    pub kind: JobKind,
    pub stage: String,
    #[ts(type = "number")]
    pub done: i64,
    #[ts(type = "number")]
    pub total: i64,
    #[serde(default)]
    pub current_path: Option<String>,
    pub finished: bool,
    #[serde(default)]
    pub error: Option<AppError>,
}

/// `imageIds: null` = refetch all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LibraryChanged {
    pub reason: LibraryChangeReason,
    #[serde(default)]
    #[ts(type = "number[] | null")]
    pub image_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NewScreenshot {
    #[ts(type = "number")]
    pub image_id: i64,
    pub file_name: String,
}

/// Payload of `open-image` (tray / single-instance second launch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpenImage {
    #[ts(type = "number")]
    pub image_id: i64,
}

// ---------------------------------------------------------------------------------------------
// `ssimg` custom scheme (D5)
// ---------------------------------------------------------------------------------------------

pub mod ssimg {
    pub const SCHEME: &str = "ssimg";
    /// Grid thumbnail long edge.
    pub const THUMB_GRID: i64 = 320;
    /// Lightbox preview long edge.
    pub const THUMB_PREVIEW: i64 = 1280;
    /// The only `w` values `thumb/<id>/<w>` accepts; anything else -> 400.
    pub const THUMB_WIDTHS: [i64; 2] = [THUMB_GRID, THUMB_PREVIEW];
    /// Header on a 202 response for a thumbnail not generated yet (`pending` | `offline`).
    pub const THUMB_STATE_HEADER: &str = "x-thumb-state";
    pub const CACHE_CONTROL: &str = "max-age=31536000, immutable";
}

// ---------------------------------------------------------------------------------------------
// Command table
// ---------------------------------------------------------------------------------------------

/// One Tauri command: name, TS type of its `req` (None = no argument), TS type of its result.
/// `raw_body` marks the one command that takes a raw binary body instead of `req` (rule 1).
/// The TS type strings name generated binding types; `contract_test.rs` writes
/// `bindings/commands.ts` from this table and checks every name is an exported binding.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub req: Option<&'static str>,
    pub res: &'static str,
    pub raw_body: bool,
}

const fn cmd(name: &'static str, req: Option<&'static str>, res: &'static str) -> CommandSpec {
    CommandSpec { name, req, res, raw_body: false }
}

pub const COMMANDS: &[CommandSpec] = &[
    cmd("get_app_info", None, "AppInfo"),
    cmd("add_library_root", Some("AddLibraryRootReq"), "LibraryRoot"),
    cmd("remove_library_root", Some("RemoveLibraryRootReq"), "OpResult"),
    cmd("start_index", Some("StartIndexReq"), "JobStarted"),
    cmd("download_cloud_only", Some("DownloadCloudOnlyReq"), "JobStarted"),
    cmd("cancel_job", Some("CancelJobReq"), "null"),
    cmd("query_images", Some("ImageQuery"), "ImagePage"),
    cmd("get_image", Some("GetImageReq"), "ImageDetail"),
    cmd("get_timeline", Some("GetTimelineReq"), "TimelineResult"),
    cmd("get_settings", None, "SettingsMap"),
    cmd("set_setting", Some("SetSettingReq"), "null"),
    cmd("list_tags", None, "Tag[]"),
    cmd("create_tag", Some("CreateTagReq"), "Tag"),
    cmd("update_tag", Some("UpdateTagReq"), "Tag"),
    cmd("delete_tag", Some("DeleteTagReq"), "OpResult"),
    cmd("set_tags", Some("SetTagsReq"), "OpResult"),
    cmd("list_albums", None, "Album[]"),
    cmd("create_album", Some("CreateAlbumReq"), "Album"),
    cmd("rename_album", Some("RenameAlbumReq"), "Album"),
    cmd("delete_album", Some("DeleteAlbumReq"), "OpResult"),
    cmd("album_add", Some("AlbumImagesReq"), "OpResult"),
    cmd("album_remove", Some("AlbumImagesReq"), "OpResult"),
    cmd("album_reorder", Some("AlbumImagesReq"), "OpResult"),
    cmd("set_favorite", Some("SetFavoriteReq"), "OpResult"),
    cmd("set_rating", Some("SetRatingReq"), "OpResult"),
    cmd("set_note", Some("SetNoteReq"), "OpResult"),
    cmd("list_smart_filters", None, "SmartFilter[]"),
    cmd("save_smart_filter", Some("SaveSmartFilterReq"), "SmartFilter"),
    cmd("delete_smart_filter", Some("DeleteSmartFilterReq"), "OpResult"),
    cmd("move_images", Some("MoveImagesReq"), "OpResult"),
    cmd("preview_rename", Some("PreviewRenameReq"), "RenamePreview"),
    cmd("rename_images", Some("RenameImagesReq"), "OpResult"),
    cmd("trash_images", Some("ImageIdsReq"), "OpResult"),
    cmd("restore_images", Some("ImageIdsReq"), "OpResult"),
    cmd("recycle_images", Some("ImageIdsReq"), "OpResult"),
    cmd("request_permanent_delete", Some("ImageIdsReq"), "PermanentDeleteTicket"),
    cmd("delete_permanently", Some("DeletePermanentlyReq"), "OpResult"),
    cmd("undo", Some("UndoReq"), "OpResult"),
    cmd("list_journal", Some("LimitReq"), "JournalPage"),
    cmd("get_sidecar_status", None, "SidecarStatus"),
    cmd("restart_sidecar", None, "SidecarStatus"),
    cmd("start_ai", Some("StartAiReq"), "JobStarted"),
    cmd("find_duplicates", Some("FindDuplicatesReq"), "DuplicateResult"),
    cmd("get_sessions", Some("GetSessionsReq"), "SessionsResult"),
    cmd("detect_issues", Some("DetectIssuesReq"), "IssuesResult"),
    cmd("get_counts", None, "CatalogCounts"),
    cmd("get_stats", None, "Stats"),
    cmd("detect_pii", Some("ImageIdReq"), "PiiResult"),
    CommandSpec { name: "save_edited_image", req: None, res: "ImageSummary", raw_body: true },
    cmd("convert_images", Some("ConvertImagesReq"), "OpResult"),
    cmd("stitch_images", Some("StitchImagesReq"), "ImageSummary"),
    cmd("export_zip", Some("ExportZipReq"), "JobStarted"),
    cmd("export_pdf", Some("ExportPdfReq"), "JobStarted"),
    cmd("copy_image_to_clipboard", Some("ImageIdReq"), "null"),
    cmd("get_recent", Some("LimitReq"), "ImageSummary[]"),
];
