//! Data dir and derived file locations (design-intent.md § Names and paths).

use std::path::{Path, PathBuf};

/// Default data dir. `SS_DATA_DIR` overrides it (tests and Accept scripts only, README D10).
pub const DEFAULT_DATA_DIR: &str = r"D:\IOT_Tools\data\ScreenshotStudio";
/// Env var that overrides [`DEFAULT_DATA_DIR`].
pub const DATA_DIR_ENV: &str = "SS_DATA_DIR";
/// Catalog file name inside the data dir.
pub const CATALOG_FILE: &str = "catalog.sqlite";
/// Thumbnail dir name inside the data dir.
pub const THUMBS_DIR: &str = "thumbs";

/// `SS_DATA_DIR` when set and non-empty, else [`DEFAULT_DATA_DIR`].
pub fn data_dir() -> PathBuf {
    match std::env::var_os(DATA_DIR_ENV) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(DEFAULT_DATA_DIR),
    }
}

pub fn catalog_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CATALOG_FILE)
}

/// `<data>\thumbs\<id % 256 as 2 hex>\<id>_<w>.webp` (contract-sqlite-schema.md); computed, never stored.
pub fn thumb_path(data_dir: &Path, image_id: i64, width: i64) -> PathBuf {
    data_dir
        .join(THUMBS_DIR)
        .join(format!("{:02x}", image_id.rem_euclid(256)))
        .join(format!("{image_id}_{width}.webp"))
}

/// Absolute path of an image: its root joined with the `/`-separated relative path.
pub fn image_abs_path(root: &str, folder: &str, file_name: &str) -> PathBuf {
    let mut p = PathBuf::from(root);
    for part in folder.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p.push(file_name);
    p
}

/// Environment variables that name the user's OneDrive folders.
const ONEDRIVE_ENVS: [&str; 3] = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"];

/// True when `path` is under a OneDrive folder: under `%OneDrive%` (or the consumer/commercial
/// variants), or containing a `\OneDrive\` / `\OneDrive - <org>\` segment (contract-tauri-ipc rule 8).
pub fn is_protected_path(path: &str) -> bool {
    let norm = normalize(path);
    let segment_hit = norm.contains(r"\onedrive\")
        || norm.ends_with(r"\onedrive")
        || norm.contains(r"\onedrive - ");
    segment_hit
        || ONEDRIVE_ENVS.iter().filter_map(std::env::var_os).any(|dir| {
            let dir = normalize(&dir.to_string_lossy());
            let dir = dir.trim_end_matches('\\');
            !dir.is_empty() && (norm == dir || norm.starts_with(&format!(r"{dir}\")))
        })
}

fn normalize(path: &str) -> String {
    path.replace('/', r"\").to_lowercase()
}
