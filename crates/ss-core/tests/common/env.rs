//! Shared setup of the phase 2.1 tests: a core over a fresh temp data dir plus a fixture library
//! root (README D10: never the real catalog or the OneDrive folder).

#![allow(dead_code)] // each test crate uses a different subset

use std::fs::{File, Metadata};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ss_core::contract::Caller;
use ss_core::contract::ipc::*;
use ss_core::fsaccess::{FileAccess, OsFileAccess};
use ss_core::jobs::NullSink;
use ss_core::{AppCore, IndexOptions, query};
use tempfile::TempDir;

use super::fixtures::{self, Fixture};

/// Field order = drop order: the core (and its SQLite handles) closes before the temp dirs go.
pub struct Env {
    pub core: Arc<AppCore>,
    pub root: LibraryRoot,
    pub fixtures: Vec<Fixture>,
    pub lib: TempDir,
    pub data: TempDir,
}

impl Env {
    pub fn new(n: usize) -> Self {
        Self::with(n, Arc::new(OsFileAccess), "")
    }

    /// `sub` = a sub path of the temp dir to use as the root (e.g. `OneDrive/Pictures` makes a
    /// protected root by path).
    pub fn with(n: usize, fa: Arc<dyn FileAccess>, sub: &str) -> Self {
        let data = tempfile::tempdir().expect("data dir");
        let lib = tempfile::tempdir().expect("library dir");
        let root_dir = lib.path().join(sub);
        std::fs::create_dir_all(&root_dir).expect("root dir");
        let fixtures = fixtures::write_library(&root_dir, n);
        let core = AppCore::open(data.path(), fa).expect("open core");
        let root = core
            .add_library_root(Caller::Ui, AddLibraryRootReq { path: root_dir.to_string_lossy().into_owned() })
            .expect("add root");
        Env { core, root, fixtures, lib, data }
    }

    pub fn index(&self, thumbs: bool) -> ss_core::IndexReport {
        self.core.index_blocking(None, IndexOptions { thumbs }, &NullSink).expect("index")
    }

    /// Every row matching `q` (all pages).
    pub fn all_of(&self, mut q: ImageQuery) -> Vec<ImageSummary> {
        let mut out = Vec::new();
        loop {
            let page = self.core.query_images(&q).expect("query");
            out.extend(page.items);
            match page.next_cursor {
                Some(c) => q.cursor = Some(c),
                None => return out,
            }
        }
    }

    pub fn all(&self) -> Vec<ImageSummary> {
        self.all_of(query::default_query(500))
    }

    pub fn by_name(&self, name: &str) -> ImageSummary {
        self.all().into_iter().find(|s| s.file_name == name).unwrap_or_else(|| panic!("no row named {name}"))
    }

    pub fn detail(&self, id: i64) -> ImageDetail {
        self.core.get_image(GetImageReq { id }).expect("get_image")
    }

    pub fn sql_i64(&self, sql: &str) -> i64 {
        let sql = sql.to_owned();
        self.core.catalog().read(move |c| Ok(c.query_row(&sql, [], |r| r.get(0))?)).expect("sql")
    }

    pub fn exec(&self, sql: &str) {
        let sql = sql.to_owned();
        self.core.catalog().write(move |tx| Ok(tx.execute_batch(&sql)?)).expect("exec");
    }

    pub fn root_dir(&self) -> PathBuf {
        PathBuf::from(&self.root.path)
    }
}

pub fn blake3_of(p: &Path) -> String {
    blake3::hash(&std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))).to_hex().to_string()
}

/// Simulated OneDrive: files in `cloud` are cloud-only until opened (opening hydrates them, as
/// OneDrive does) unless `fail_cloud_reads` is set, in which case opening fails and they stay
/// cloud-only (client stopped / offline). Records every open.
#[derive(Default)]
pub struct CloudMock {
    pub cloud: Mutex<HashSet<PathBuf>>,
    pub opened: Mutex<Vec<PathBuf>>,
    pub fail_cloud_reads: Mutex<bool>,
}

impl CloudMock {
    pub fn set_cloud(&self, p: &Path, on: bool) {
        let mut c = self.cloud.lock().unwrap();
        if on { c.insert(p.to_path_buf()) } else { c.remove(p) };
    }

    pub fn opens_of(&self, p: &Path) -> usize {
        self.opened.lock().unwrap().iter().filter(|x| x.as_path() == p).count()
    }
}

impl FileAccess for CloudMock {
    fn cloud_state(&self, path: &Path, meta: &Metadata) -> CloudState {
        if self.cloud.lock().unwrap().contains(path) { CloudState::CloudOnly } else { OsFileAccess.cloud_state(path, meta) }
    }

    fn open(&self, path: &Path) -> std::io::Result<File> {
        self.opened.lock().unwrap().push(path.to_path_buf());
        if self.cloud.lock().unwrap().contains(path) {
            if *self.fail_cloud_reads.lock().unwrap() {
                return Err(std::io::Error::other("The cloud operation was unsuccessful (simulated)"));
            }
            self.cloud.lock().unwrap().remove(path);
        }
        File::open(path)
    }
}
