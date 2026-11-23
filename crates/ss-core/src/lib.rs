//! Screenshot Studio core: catalog, indexer, thumbnails, `ssimg`, jobs, file operations, sidecar
//! supervisor and contracts. UI-free: `src-tauri` and the `ssctl` CLI are thin wrappers over [`app::AppCore`].

pub mod app;
pub mod catalog;
pub mod contract;
mod errors;
pub mod filter;
pub mod fsaccess;
pub mod jobs;
pub mod ops;
pub mod paths;
pub mod query;
pub mod scan;
pub mod ssimg;
pub mod thumbs;

pub use app::{AppCore, IndexOptions, IndexReport};
