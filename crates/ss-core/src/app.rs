//! `AppCore`: the UI-free facade over the catalog, scanner, thumbnail pass, jobs and `ssimg`.
//! `src-tauri` commands and `ssctl` are thin wrappers over these methods. The phase 3.2 surface
//! (sidecar, AI job, semantic search, smart features) is in `app_smart.rs`.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::catalog::{self, Catalog};
use crate::contract::ipc::*;
use crate::contract::{AppError, CONTRACT_VERSION, Caller};
use crate::fsaccess::{FileAccess, OsFileAccess};
use crate::jobs::{EventSink, JobRegistry, ProgressReporter};
use crate::scan::{self, ScanReport};
use crate::ssimg::{self, SsimgResponse};
use crate::thumbs::{self, ThumbReport, ThumbScope};
use crate::ai::semantic::SemanticIndex;
use crate::ops::purge::DeleteTokens;
use crate::ops::{files, journal, organize, purge};
use crate::sidecar::{Sidecar, SidecarLaunch, SidecarLayout};
use crate::{ops, paths, query, smart};

/// What an index run does after the scan.
#[derive(Debug, Clone, Copy)]
pub struct IndexOptions {
    /// Run the thumbnail/hash pass after the scan (false = scan only, thumbnails stay `pending`).
    pub thumbs: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self { thumbs: true }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReport {
    pub scan: ScanReport,
    pub thumbs: ThumbReport,
}

pub struct AppCore {
    pub(crate) catalog: Catalog,
    pub(crate) fa: Arc<dyn FileAccess>,
    pub(crate) jobs: JobRegistry,
    delete_tokens: DeleteTokens,
    /// The AI sidecar; not started until [`AppCore::start_sidecar`] (state `absent` until then).
    pub(crate) sidecar: Sidecar,
    pub(crate) semantic: SemanticIndex,
    /// Where the sidecar source lives (None = [`SidecarLayout::detect`] at start).
    pub(crate) sidecar_layout: Mutex<Option<SidecarLayout>>,
    /// A fixed launch replacing the settings-based one (tests, [`AppCore::start_sidecar_with`]).
    pub(crate) sidecar_launch: Mutex<Option<SidecarLaunch>>,
    /// `AppInfo.warnings` (hotkey, watcher), set by the shell and the watcher (`app_live.rs`).
    pub(crate) warnings: Mutex<Vec<AppWarning>>,
    /// The live folder watcher while it runs (`app_live.rs`).
    pub(crate) watcher: Mutex<Option<crate::watcher::WatcherHandle>>,
}

impl AppCore {
    pub fn open(data_dir: &Path, fa: Arc<dyn FileAccess>) -> Result<Arc<Self>, AppError> {
        Ok(Arc::new(Self {
            catalog: Catalog::open(data_dir)?,
            fa,
            jobs: JobRegistry::default(),
            delete_tokens: DeleteTokens::default(),
            sidecar: Sidecar::default(),
            semantic: SemanticIndex::default(),
            sidecar_layout: Mutex::new(None),
            sidecar_launch: Mutex::new(None),
            warnings: Mutex::new(Vec::new()),
            watcher: Mutex::new(None),
        }))
    }

    /// The real data dir ([`paths::data_dir`]) and the real file system.
    pub fn open_default() -> Result<Arc<Self>, AppError> {
        Self::open(&paths::data_dir(), Arc::new(OsFileAccess))
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn data_dir(&self) -> &Path {
        self.catalog.data_dir()
    }

    pub fn get_app_info(&self) -> Result<AppInfo, AppError> {
        Ok(AppInfo {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: CONTRACT_VERSION,
            data_dir: self.data_dir().to_string_lossy().into_owned(),
            library_roots: self.catalog.read(catalog::list_roots)?,
            sidecar: self.sidecar.status(),
            warnings: self.warnings(),
        })
    }

    pub fn add_library_root(&self, caller: Caller, req: AddLibraryRootReq) -> Result<LibraryRoot, AppError> {
        let r = ops::add_library_root(caller, &self.catalog, req)?;
        self.refresh_watcher();
        Ok(r)
    }

    pub fn remove_library_root(&self, caller: Caller, req: RemoveLibraryRootReq) -> Result<OpResult, AppError> {
        let r = ops::remove_library_root(caller, &self.catalog, req)?;
        self.refresh_watcher();
        Ok(r)
    }

    pub fn get_settings(&self) -> Result<SettingsMap, AppError> {
        self.catalog.read(catalog::get_settings)
    }

    pub fn set_setting(&self, caller: Caller, req: SetSettingReq) -> Result<(), AppError> {
        let key = req.key;
        ops::set_setting(caller, &self.catalog, req)?;
        if key == SettingKey::WatcherEnabled {
            self.refresh_watcher();
        }
        Ok(())
    }

    /// `textMode` `semantic` / `any` add the semantic arm when the sidecar is ready (`app_smart.rs`).
    pub fn query_images(&self, q: &ImageQuery) -> Result<ImagePage, AppError> {
        let sem = self.semantic_arm(q)?;
        self.catalog.read(|conn| query::query_images_with(conn, q, sem.as_ref()))
    }

    pub fn get_image(&self, req: GetImageReq) -> Result<ImageDetail, AppError> {
        self.catalog.read(|conn| query::get_image(conn, req.id))
    }

    pub fn get_timeline(&self, req: &GetTimelineReq) -> Result<TimelineResult, AppError> {
        let sem = self.semantic_arm(&req.query)?;
        self.catalog.read(|conn| query::get_timeline_with(conn, req, sem.as_ref()))
    }

    pub fn get_counts(&self) -> Result<CatalogCounts, AppError> {
        self.catalog.read(query::get_counts)
    }

    /// Serves one `ssimg` request path (`/thumb/<id>/<w>` or `/full/<id>`).
    pub fn ssimg(&self, path: &str) -> SsimgResponse {
        ssimg::handle(&self.catalog, self.fa.as_ref(), path)
    }

    // -----------------------------------------------------------------------------------------
    // Phase 2.1: organize, file ops, permanent delete, journal
    // -----------------------------------------------------------------------------------------

    pub fn list_tags(&self) -> Result<Vec<Tag>, AppError> {
        self.catalog.read(organize::list_tags)
    }
    pub fn create_tag(&self, caller: Caller, req: CreateTagReq) -> Result<Tag, AppError> {
        organize::create_tag(caller, &self.catalog, req)
    }
    pub fn update_tag(&self, caller: Caller, req: UpdateTagReq) -> Result<Tag, AppError> {
        organize::update_tag(caller, &self.catalog, req)
    }
    pub fn delete_tag(&self, caller: Caller, req: DeleteTagReq) -> Result<OpResult, AppError> {
        organize::delete_tag(caller, &self.catalog, req)
    }
    pub fn set_tags(&self, caller: Caller, req: SetTagsReq) -> Result<OpResult, AppError> {
        organize::set_tags(caller, &self.catalog, req)
    }
    pub fn list_albums(&self) -> Result<Vec<Album>, AppError> {
        self.catalog.read(organize::list_albums)
    }
    pub fn create_album(&self, caller: Caller, req: CreateAlbumReq) -> Result<Album, AppError> {
        organize::create_album(caller, &self.catalog, req)
    }
    pub fn rename_album(&self, caller: Caller, req: RenameAlbumReq) -> Result<Album, AppError> {
        organize::rename_album(caller, &self.catalog, req)
    }
    pub fn delete_album(&self, caller: Caller, req: DeleteAlbumReq) -> Result<OpResult, AppError> {
        organize::delete_album(caller, &self.catalog, req)
    }
    pub fn album_add(&self, caller: Caller, req: AlbumImagesReq) -> Result<OpResult, AppError> {
        organize::album_add(caller, &self.catalog, req)
    }
    pub fn album_remove(&self, caller: Caller, req: AlbumImagesReq) -> Result<OpResult, AppError> {
        organize::album_remove(caller, &self.catalog, req)
    }
    pub fn album_reorder(&self, caller: Caller, req: AlbumImagesReq) -> Result<OpResult, AppError> {
        organize::album_reorder(caller, &self.catalog, req)
    }
    pub fn set_favorite(&self, caller: Caller, req: SetFavoriteReq) -> Result<OpResult, AppError> {
        organize::set_favorite(caller, &self.catalog, req)
    }
    pub fn set_rating(&self, caller: Caller, req: SetRatingReq) -> Result<OpResult, AppError> {
        organize::set_rating(caller, &self.catalog, req)
    }
    pub fn set_note(&self, caller: Caller, req: SetNoteReq) -> Result<OpResult, AppError> {
        organize::set_note(caller, &self.catalog, req)
    }
    pub fn list_smart_filters(&self) -> Result<Vec<SmartFilter>, AppError> {
        self.catalog.read(organize::list_smart_filters)
    }
    pub fn save_smart_filter(&self, caller: Caller, req: SaveSmartFilterReq) -> Result<SmartFilter, AppError> {
        organize::save_smart_filter(caller, &self.catalog, req)
    }
    pub fn delete_smart_filter(&self, caller: Caller, req: DeleteSmartFilterReq) -> Result<OpResult, AppError> {
        organize::delete_smart_filter(caller, &self.catalog, req)
    }
    pub fn move_images(&self, caller: Caller, req: MoveImagesReq) -> Result<OpResult, AppError> {
        files::move_images(caller, &self.catalog, req)
    }
    pub fn preview_rename(&self, req: PreviewRenameReq) -> Result<RenamePreview, AppError> {
        files::preview_rename(&self.catalog, req)
    }
    pub fn rename_images(&self, caller: Caller, req: RenameImagesReq) -> Result<OpResult, AppError> {
        files::rename_images(caller, &self.catalog, req)
    }
    pub fn trash_images(&self, caller: Caller, req: ImageIdsReq) -> Result<OpResult, AppError> {
        files::trash_images(caller, &self.catalog, req)
    }
    pub fn restore_images(&self, caller: Caller, req: ImageIdsReq) -> Result<OpResult, AppError> {
        files::restore_images(caller, &self.catalog, req)
    }
    pub fn recycle_images(&self, caller: Caller, req: ImageIdsReq) -> Result<OpResult, AppError> {
        files::recycle_images(caller, &self.catalog, req)
    }
    pub fn request_permanent_delete(&self, caller: Caller, req: ImageIdsReq) -> Result<PermanentDeleteTicket, AppError> {
        purge::request_permanent_delete(caller, &self.catalog, &self.delete_tokens, req)
    }
    pub fn delete_permanently(&self, caller: Caller, req: DeletePermanentlyReq) -> Result<OpResult, AppError> {
        purge::delete_permanently(caller, &self.catalog, &self.delete_tokens, req)
    }
    /// Tests only (see [`DeleteTokens::set_ttl_ms`]).
    #[doc(hidden)]
    pub fn delete_tokens(&self) -> &DeleteTokens {
        &self.delete_tokens
    }
    /// Undoes one batch; an undone edit/convert overwrite gets its thumbnails and hashes made again.
    pub fn undo(&self, caller: Caller, req: UndoReq) -> Result<OpResult, AppError> {
        let (r, rethumb) = journal::undo_detailed(caller, &self.catalog, req)?;
        self.after_content_change(&rethumb);
        Ok(r)
    }
    pub fn list_journal(&self, req: LimitReq) -> Result<JournalPage, AppError> {
        journal::list_journal(&self.catalog, req)
    }

    pub fn cancel_job(&self, req: CancelJobReq) -> Result<(), AppError> {
        self.jobs.cancel(req.job_id)
    }

    // -----------------------------------------------------------------------------------------
    // Index job
    // -----------------------------------------------------------------------------------------

    /// Starts an index job on a background thread and returns at once (contract rule 6).
    pub fn start_index(self: &Arc<Self>, req: StartIndexReq, sink: Arc<dyn EventSink>) -> Result<JobStarted, AppError> {
        if let Some(id) = req.root_id {
            self.catalog.read(|conn| catalog::get_root(conn, id))?;
        }
        let (job_id, cancel) = self.jobs.begin(JobKind::Index)?;
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name(format!("ss-index-{job_id}"))
            .spawn(move || {
                let _ = me.run_index(job_id, req.root_id, IndexOptions::default(), sink.as_ref(), &cancel);
                me.jobs.end(job_id);
            })
            .map_err(|e| crate::errors::internal("cannot start the index job").with_detail(e.to_string()))?;
        Ok(JobStarted { job_id })
    }

    /// Runs an index to completion on the calling thread (ssctl, tests).
    pub fn index_blocking(&self, root_id: Option<i64>, opts: IndexOptions, sink: &dyn EventSink) -> Result<IndexReport, AppError> {
        let (job_id, cancel) = self.jobs.begin(JobKind::Index)?;
        let r = self.run_index(job_id, root_id, opts, sink, &cancel);
        self.jobs.end(job_id);
        r
    }

    fn run_index(
        &self,
        job_id: i64,
        root_id: Option<i64>,
        opts: IndexOptions,
        sink: &dyn EventSink,
        cancel: &AtomicBool,
    ) -> Result<IndexReport, AppError> {
        let reporter = ProgressReporter::new(sink, job_id, JobKind::Index);
        let result = self.index_inner(root_id, opts, sink, &reporter, cancel);
        match &result {
            Ok(r) => reporter.finish("done", r.thumbs.generated + r.scan.seen, r.thumbs.generated + r.scan.seen, None),
            Err(e) => reporter.finish("done", 0, 0, Some(e.clone())),
        }
        result
    }

    fn index_inner(
        &self,
        root_id: Option<i64>,
        opts: IndexOptions,
        sink: &dyn EventSink,
        reporter: &ProgressReporter<'_>,
        cancel: &AtomicBool,
    ) -> Result<IndexReport, AppError> {
        let roots = match root_id {
            Some(id) => vec![self.catalog.read(|conn| catalog::get_root(conn, id))?],
            None => self.catalog.read(catalog::list_roots)?,
        };
        let auto_cloud = self.auto_download_cloud_only()?;
        let mut report = IndexReport::default();
        if thumbs::ensure_format(&self.catalog)? {
            eprintln!("ss-core: thumbnails switched to {}: each is regenerated once", thumbs::THUMB_FORMAT);
        }
        for root in &roots {
            let progress = |done: i64, total: i64, path: Option<&str>| reporter.report("scan", done, total, path);
            let s = scan::scan_root(&self.catalog, self.fa.as_ref(), root, auto_cloud, cancel, &progress)?;
            add_scan(&mut report.scan, &s);
        }
        sink.library_changed(&LibraryChanged { reason: LibraryChangeReason::Indexed, image_ids: None });
        if opts.thumbs {
            for root in &roots {
                let t = self.thumbs_for(Some(root.id), auto_cloud, sink, reporter, cancel)?;
                add_thumbs(&mut report.thumbs, &t);
            }
            // Rows thumbnailed before phase 3.2 have no blur score yet (thumbnail files only).
            reporter.report("blur", 0, 0, None);
            for root in &roots {
                smart::issues::backfill_blur(&self.catalog, Some(root.id), cancel)?;
            }
        }
        Ok(report)
    }

    fn thumbs_for(
        &self,
        root_id: Option<i64>,
        allow_cloud: bool,
        sink: &dyn EventSink,
        reporter: &ProgressReporter<'_>,
        cancel: &AtomicBool,
    ) -> Result<ThumbReport, AppError> {
        let progress = |done: i64, total: i64, path: Option<&str>| reporter.report("thumbs", done, total, path);
        let on_batch = |ids: Vec<i64>| {
            sink.library_changed(&LibraryChanged { reason: LibraryChangeReason::Indexed, image_ids: Some(ids) })
        };
        thumbs::thumb_pass(&self.catalog, self.fa.as_ref(), &ThumbScope { root_id, allow_cloud, ids: None }, cancel, &progress, &on_batch)
    }

    fn auto_download_cloud_only(&self) -> Result<bool, AppError> {
        let v = self.catalog.read(|conn| catalog::get_setting(conn, SettingKey::AutoDownloadCloudOnly))?;
        Ok(v.as_bool().unwrap_or(false))
    }

    // -----------------------------------------------------------------------------------------
    // Cloud download job (Deferred decision #5: explicit button, never automatic)
    // -----------------------------------------------------------------------------------------

    /// Reads every cloud-only file of the root (OneDrive hydrates it on read; nothing is pinned)
    /// and generates its thumbnails.
    pub fn start_download_cloud_only(
        self: &Arc<Self>,
        req: DownloadCloudOnlyReq,
        sink: Arc<dyn EventSink>,
    ) -> Result<JobStarted, AppError> {
        self.catalog.read(|conn| catalog::get_root(conn, req.root_id))?;
        let (job_id, cancel) = self.jobs.begin(JobKind::CloudDownload)?;
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name(format!("ss-cloud-{job_id}"))
            .spawn(move || {
                let _ = me.run_cloud_download(job_id, req.root_id, sink.as_ref(), &cancel);
                me.jobs.end(job_id);
            })
            .map_err(|e| crate::errors::internal("cannot start the download job").with_detail(e.to_string()))?;
        Ok(JobStarted { job_id })
    }

    pub fn download_cloud_only_blocking(&self, root_id: i64, sink: &dyn EventSink) -> Result<ThumbReport, AppError> {
        let (job_id, cancel) = self.jobs.begin(JobKind::CloudDownload)?;
        let r = self.run_cloud_download(job_id, root_id, sink, &cancel);
        self.jobs.end(job_id);
        r
    }

    fn run_cloud_download(&self, job_id: i64, root_id: i64, sink: &dyn EventSink, cancel: &AtomicBool) -> Result<ThumbReport, AppError> {
        let reporter = ProgressReporter::new(sink, job_id, JobKind::CloudDownload);
        let result = (|| {
            self.catalog.read(|conn| catalog::get_root(conn, root_id))?;
            thumbs::ensure_format(&self.catalog)?;
            // `error` too: a cloud-only file whose read failed before phase 2.1 was written `error`.
            self.catalog.write(move |tx| {
                tx.execute(
                    "UPDATE image SET thumb_state = 'pending'
                     WHERE root_id = ?1 AND missing = 0 AND cloud_state = 'cloudOnly' AND thumb_state IN ('offline', 'error')",
                    [root_id],
                )?;
                Ok(())
            })?;
            self.thumbs_for(Some(root_id), true, sink, &reporter, cancel)
        })();
        match &result {
            Ok(r) => reporter.finish("done", r.generated, r.generated + r.failed, None),
            Err(e) => reporter.finish("done", 0, 0, Some(e.clone())),
        }
        result
    }
}

/// File extensions the scanner indexes.
pub fn extensions() -> Vec<String> {
    scan::EXTENSIONS.iter().map(|s| (*s).to_owned()).collect()
}

fn add_scan(acc: &mut ScanReport, s: &ScanReport) {
    acc.seen += s.seen;
    acc.added += s.added;
    acc.updated += s.updated;
    acc.unchanged += s.unchanged;
    acc.missing += s.missing;
    acc.restored += s.restored;
    acc.cloud_only += s.cloud_only;
    acc.walk_errors += s.walk_errors;
}

fn add_thumbs(acc: &mut ThumbReport, t: &ThumbReport) {
    acc.generated += t.generated;
    acc.failed += t.failed;
    acc.offline += t.offline;
}
