//! File access seam: every content read of an image goes through [`FileAccess::open`], and every
//! cloud-state decision through [`FileAccess::cloud_state`] (metadata only). Tests swap in a mock
//! to simulate OneDrive cloud-only files and to prove they are never opened (contract rule 9).

use std::fs::{File, Metadata};
use std::io;
use std::path::Path;

use crate::contract::ipc::CloudState;

/// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`: reading the content makes OneDrive download it.
pub const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
/// `FILE_ATTRIBUTE_OFFLINE`: content not available locally.
pub const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;

pub trait FileAccess: Send + Sync {
    /// Cloud state from metadata only. Never opens the content, never changes attributes.
    fn cloud_state(&self, path: &Path, meta: &Metadata) -> CloudState;
    /// Opens the content for reading. Callers check [`FileAccess::cloud_state`] first in
    /// background passes; an explicit user action may open a cloud-only file.
    fn open(&self, path: &Path) -> io::Result<File>;
}

/// Cloud state from raw Windows file attributes.
pub fn cloud_state_from_attributes(attrs: u32) -> CloudState {
    if attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_OFFLINE) != 0 {
        CloudState::CloudOnly
    } else {
        CloudState::Local
    }
}

/// The real file system.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsFileAccess;

impl FileAccess for OsFileAccess {
    #[cfg(windows)]
    fn cloud_state(&self, _path: &Path, meta: &Metadata) -> CloudState {
        use std::os::windows::fs::MetadataExt;
        cloud_state_from_attributes(meta.file_attributes())
    }

    #[cfg(not(windows))]
    fn cloud_state(&self, _path: &Path, _meta: &Metadata) -> CloudState {
        CloudState::Local
    }

    fn open(&self, path: &Path) -> io::Result<File> {
        File::open(path)
    }
}
