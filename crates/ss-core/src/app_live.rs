//! Phase 4.1 surface of [`AppCore`]: app warnings, the live watcher, recent images, edit save / convert /
//! stitch, ZIP and PDF export jobs, the clipboard image and "open this file" (second launch).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::app::AppCore;
use crate::catalog;
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller};
use crate::errors::{internal, invalid};
use crate::export::{self, ExportReport};
use crate::jobs::{EventSink, ProgressReporter};
use crate::ops::edit::{self, CropRect, EditOutcome, SaveEditedImage};
use crate::ops::path_under;
use crate::scan::live;
use crate::{query, thumbs, watcher};

/// Most rows `get_recent` returns.
pub const RECENT_LIMIT_MAX: i64 = 50;

impl AppCore {
    // -----------------------------------------------------------------------------------------
    // Warnings (`AppInfo.warnings`)
    // -----------------------------------------------------------------------------------------

    /// Sets (`Some(message)`) or clears (`None`) the warning of `code`.
    pub fn set_warning(&self, code: AppWarningCode, message: Option<String>) {
        let mut w = self.warnings.lock().expect("app warnings");
        w.retain(|x| x.code != code);
        if let Some(message) = message {
            w.push(AppWarning { code, message });
        }
    }

    pub fn warnings(&self) -> Vec<AppWarning> {
        self.warnings.lock().expect("app warnings").clone()
    }

    // -----------------------------------------------------------------------------------------
    // Live watcher
    // -----------------------------------------------------------------------------------------

    /// Starts the folder watcher (once; a second call is a no-op). It follows `watcherEnabled` and the roots.
    pub fn start_watcher(self: &Arc<Self>, sink: Arc<dyn EventSink>) -> Result<(), AppError> {
        let mut slot = self.watcher.lock().expect("watcher slot");
        if slot.is_none() {
            *slot = Some(watcher::start(self, sink)?);
        }
        Ok(())
    }

    /// Stops the watcher thread and waits for it.
    pub fn stop_watcher(&self) {
        let handle = self.watcher.lock().expect("watcher slot").take();
        drop(handle);
    }

    /// Roots the watcher watches right now (0 when it is not running or paused).
    pub fn watched_roots(&self) -> usize {
        self.watcher.lock().expect("watcher slot").as_ref().map_or(0, |w| w.watched_roots())
    }

    /// Makes a running watcher re-read `watcherEnabled` and the roots now.
    pub fn refresh_watcher(&self) {
        if let Some(w) = self.watcher.lock().expect("watcher slot").as_ref() {
            w.refresh();
        }
    }

    // -----------------------------------------------------------------------------------------
    // Recent, clipboard, open file
    // -----------------------------------------------------------------------------------------

    /// The newest images by taken time (not trashed, not missing): the tray's "Recent" menu.
    pub fn get_recent(&self, req: LimitReq) -> Result<Vec<ImageSummary>, AppError> {
        if !(1..=RECENT_LIMIT_MAX).contains(&req.limit) {
            return Err(invalid(format!("limit must be 1..{RECENT_LIMIT_MAX}, got {}", req.limit)));
        }
        let q = query::default_query(req.limit);
        Ok(self.catalog.read(|c| query::query_images(c, &q))?.items)
    }

    /// The image's pixels as RGBA8 (`copy_image_to_clipboard`); an explicit action, so a cloud-only file is opened.
    pub fn image_rgba(&self, req: ImageIdReq) -> Result<(Vec<u8>, u32, u32), AppError> {
        let d = self.get_image(GetImageReq { id: req.image_id })?;
        if d.summary.missing {
            return Err(crate::errors::not_found(format!("file not found: {}", d.abs_path)));
        }
        let img = edit::decode_file(self.fa.as_ref(), Path::new(&d.abs_path))?.to_rgba8();
        let (w, h) = img.dimensions();
        Ok((img.into_raw(), w, h))
    }

    /// The image id of a file path (second launch with a file argument). A file inside a library root that the
    /// catalog does not know yet is indexed first; a path outside every root is `None`.
    pub fn image_for_path(&self, path: &Path) -> Result<Option<i64>, AppError> {
        let roots = self.catalog.read(catalog::list_roots)?;
        let Some(root) = roots.into_iter().find(|r| path_under(path, Path::new(&r.path))) else {
            return Ok(None);
        };
        let Some((folder, name)) = live::relative(Path::new(&root.path), path) else { return Ok(None) };
        let find = |folder: String, name: String| {
            self.catalog.read(move |c| {
                Ok(c.query_row(
                    "SELECT id FROM image WHERE root_id = ?1 AND folder = ?2 AND file_name = ?3 COLLATE NOCASE",
                    rusqlite::params![root.id, folder, name],
                    |r| r.get::<_, i64>(0),
                )
                .ok())
            })
        };
        if let Some(id) = find(folder.clone(), name.clone())? {
            return Ok(Some(id));
        }
        let r = live::sync_paths(&self.catalog, self.fa.as_ref(), &root, &[path], false)?;
        thumbs::thumb_ids(&self.catalog, self.fa.as_ref(), &r.added_ids, true)?;
        Ok(r.added_ids.first().copied())
    }

    // -----------------------------------------------------------------------------------------
    // Edit, convert, stitch
    // -----------------------------------------------------------------------------------------

    /// An edit changed pixels (or put old ones back): AI caches of the old pixels are stale.
    pub(crate) fn after_content_change(&self, rethumb: &[i64]) {
        if rethumb.is_empty() {
            return;
        }
        if let Err(e) = thumbs::thumb_ids(&self.catalog, self.fa.as_ref(), rethumb, true) {
            eprintln!("ss-core: thumbnails after undo: {}: {}", e.message, e.detail.unwrap_or_default());
        }
        self.semantic.invalidate();
    }

    pub fn save_edited_image(&self, caller: Caller, req: SaveEditedImage) -> Result<EditOutcome, AppError> {
        let overwrite = req.mode == EditMode::Overwrite;
        let r = edit::save_edited_image(caller, &self.catalog, self.fa.as_ref(), req)?;
        if overwrite {
            self.semantic.invalidate();
        }
        Ok(r)
    }

    /// `ssctl edit <id> --crop x,y,w,h --mode copy|overwrite`.
    pub fn crop_image(&self, caller: Caller, image_id: i64, crop: CropRect, mode: EditMode) -> Result<EditOutcome, AppError> {
        let r = edit::crop_image(caller, &self.catalog, self.fa.as_ref(), image_id, crop, mode)?;
        if mode == EditMode::Overwrite {
            self.semantic.invalidate();
        }
        Ok(r)
    }

    pub fn convert_images(&self, caller: Caller, req: ConvertImagesReq) -> Result<OpResult, AppError> {
        let overwrite = req.output == EditMode::Overwrite;
        let r = edit::convert_images(caller, &self.catalog, self.fa.as_ref(), req)?;
        if overwrite && r.affected > 0 {
            self.semantic.invalidate();
        }
        Ok(r)
    }

    pub fn stitch_images(&self, caller: Caller, req: StitchImagesReq) -> Result<EditOutcome, AppError> {
        edit::stitch_images(caller, &self.catalog, self.fa.as_ref(), req)
    }

    // -----------------------------------------------------------------------------------------
    // Export jobs
    // -----------------------------------------------------------------------------------------

    /// Starts a ZIP export on a background thread (`job-progress` kind `export`).
    pub fn start_export_zip(self: &Arc<Self>, caller: Caller, req: ExportZipReq, sink: Arc<dyn EventSink>) -> Result<JobStarted, AppError> {
        self.spawn_export(sink, move |me, sink, job_id, cancel| me.run_export(sink, job_id, cancel, |rep, c| {
            export::export_zip(caller, &me.catalog, me.fa.as_ref(), &req, rep, c)
        }))
    }

    /// Starts a PDF export on a background thread (`job-progress` kind `export`).
    pub fn start_export_pdf(self: &Arc<Self>, caller: Caller, req: ExportPdfReq, sink: Arc<dyn EventSink>) -> Result<JobStarted, AppError> {
        self.spawn_export(sink, move |me, sink, job_id, cancel| me.run_export(sink, job_id, cancel, |rep, c| {
            export::export_pdf(caller, &me.catalog, me.fa.as_ref(), &req, rep, c)
        }))
    }

    /// Runs a ZIP export to completion on the calling thread (tests).
    pub fn export_zip_blocking(&self, caller: Caller, req: ExportZipReq, sink: &dyn EventSink) -> Result<ExportReport, AppError> {
        let (job_id, cancel) = self.jobs.begin(JobKind::Export)?;
        let r = self.run_export(sink, job_id, &cancel, |rep, c| export::export_zip(caller, &self.catalog, self.fa.as_ref(), &req, rep, c));
        self.jobs.end(job_id);
        r
    }

    /// Runs a PDF export to completion on the calling thread (tests).
    pub fn export_pdf_blocking(&self, caller: Caller, req: ExportPdfReq, sink: &dyn EventSink) -> Result<ExportReport, AppError> {
        let (job_id, cancel) = self.jobs.begin(JobKind::Export)?;
        let r = self.run_export(sink, job_id, &cancel, |rep, c| export::export_pdf(caller, &self.catalog, self.fa.as_ref(), &req, rep, c));
        self.jobs.end(job_id);
        r
    }

    fn spawn_export<F>(self: &Arc<Self>, sink: Arc<dyn EventSink>, f: F) -> Result<JobStarted, AppError>
    where
        F: FnOnce(&AppCore, &dyn EventSink, i64, &AtomicBool) -> Result<ExportReport, AppError> + Send + 'static,
    {
        let (job_id, cancel) = self.jobs.begin(JobKind::Export)?;
        let me = Arc::clone(self);
        let spawned = std::thread::Builder::new().name(format!("ss-export-{job_id}")).spawn(move || {
            let _ = f(&me, sink.as_ref(), job_id, &cancel);
            me.jobs.end(job_id);
        });
        if let Err(e) = spawned {
            self.jobs.end(job_id);
            return Err(internal("cannot start the export job").with_detail(e.to_string()));
        }
        Ok(JobStarted { job_id })
    }

    fn run_export(
        &self,
        sink: &dyn EventSink,
        job_id: i64,
        cancel: &AtomicBool,
        f: impl FnOnce(&ProgressReporter<'_>, &AtomicBool) -> Result<ExportReport, AppError>,
    ) -> Result<ExportReport, AppError> {
        let reporter = ProgressReporter::new(sink, job_id, JobKind::Export);
        let r = f(&reporter, cancel);
        match &r {
            Ok(rep) => reporter.finish("done", rep.files, rep.files, None),
            Err(e) => reporter.finish("done", 0, 0, Some(e.clone())),
        }
        r
    }
}
