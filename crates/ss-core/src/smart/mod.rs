//! Smart features that never call the sidecar (contract-sidecar-jsonrpc.md § Degradation): duplicate
//! groups, capture sessions, blurry / tiny images, library stats, plus the zero-shot label list the
//! AI job sends.

pub mod blur;
pub mod categories;
pub mod duplicates;
pub mod issues;
pub mod near_copy;
pub mod pixel_check;
pub mod sessions;
pub mod stats;
