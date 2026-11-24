//! Screenshot Studio core: catalog, indexer, thumbnails, `ssimg`, jobs, file operations, sidecar
//! supervisor, live watcher, edit/export and contracts. UI-free: `src-tauri` and the `ssctl` CLI are thin wrappers over [`app::AppCore`].

pub mod ai;
pub mod app;
mod app_live;
mod app_smart;
pub mod catalog;
pub mod contract;
mod errors;
pub mod export;
pub mod filter;
pub mod fsaccess;
pub mod jobs;
pub mod ops;
pub mod paths;
pub mod query;
pub mod scan;
pub mod sidecar;
pub mod smart;
pub mod ssimg;
pub mod thumbs;
pub mod watcher;

pub use app::{AppCore, IndexOptions, IndexReport};
