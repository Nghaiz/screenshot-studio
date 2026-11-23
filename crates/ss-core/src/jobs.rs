//! Long jobs (contract rule 6): ids, cancel flags, and `job-progress` reporting throttled to
//! [`JOB_PROGRESS_MAX_PER_SEC`] per job, the final event always sent with `finished: true`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::contract::ipc::events::JOB_PROGRESS_MAX_PER_SEC;
use crate::contract::ipc::{JobKind, JobProgress, LibraryChanged};
use crate::contract::{AppError, ErrorKind};
use crate::errors::not_found;

/// Where job events go: the Tauri app emits them to the webview; `ssctl` logs to stderr.
pub trait EventSink: Send + Sync {
    fn job_progress(&self, p: &JobProgress);
    fn library_changed(&self, c: &LibraryChanged);
}

/// Drops every event.
pub struct NullSink;

impl EventSink for NullSink {
    fn job_progress(&self, _: &JobProgress) {}
    fn library_changed(&self, _: &LibraryChanged) {}
}

#[derive(Default)]
pub struct JobRegistry {
    next_id: AtomicI64,
    running: Mutex<HashMap<i64, (JobKind, Arc<AtomicBool>)>>,
}

impl JobRegistry {
    /// Registers a job. Only one job of each kind runs at a time (`conflict` otherwise).
    pub fn begin(&self, kind: JobKind) -> Result<(i64, Arc<AtomicBool>), AppError> {
        let mut running = self.running.lock().expect("job registry");
        if let Some((id, _)) = running.iter().find(|(_, (k, _))| *k == kind) {
            return Err(AppError::new(ErrorKind::Conflict, format!("a {kind:?} job is already running")).with_detail(format!("jobId {id}")));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        running.insert(id, (kind, cancel.clone()));
        Ok((id, cancel))
    }

    pub fn end(&self, job_id: i64) {
        self.running.lock().expect("job registry").remove(&job_id);
    }

    pub fn cancel(&self, job_id: i64) -> Result<(), AppError> {
        match self.running.lock().expect("job registry").get(&job_id) {
            Some((_, flag)) => {
                flag.store(true, Ordering::Relaxed);
                Ok(())
            }
            None => Err(not_found(format!("job {job_id} is not running"))),
        }
    }
}

/// Builds `job-progress` events for one job.
pub struct ProgressReporter<'a> {
    sink: &'a dyn EventSink,
    job_id: i64,
    kind: JobKind,
    last: Mutex<Option<Instant>>,
}

impl<'a> ProgressReporter<'a> {
    pub fn new(sink: &'a dyn EventSink, job_id: i64, kind: JobKind) -> Self {
        Self { sink, job_id, kind, last: Mutex::new(None) }
    }

    /// Emits unless an event of this job went out less than `1 s / JOB_PROGRESS_MAX_PER_SEC` ago.
    pub fn report(&self, stage: &str, done: i64, total: i64, current_path: Option<&str>) {
        let min_gap = Duration::from_millis(1000 / u64::from(JOB_PROGRESS_MAX_PER_SEC));
        {
            let mut last = self.last.lock().expect("progress throttle");
            if last.is_some_and(|t| t.elapsed() < min_gap) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.emit(stage, done, total, current_path, false, None);
    }

    /// The final event (never throttled).
    pub fn finish(&self, stage: &str, done: i64, total: i64, error: Option<AppError>) {
        self.emit(stage, done, total, None, true, error);
    }

    fn emit(&self, stage: &str, done: i64, total: i64, current_path: Option<&str>, finished: bool, error: Option<AppError>) {
        self.sink.job_progress(&JobProgress {
            job_id: self.job_id,
            kind: self.kind,
            stage: stage.to_owned(),
            done,
            total,
            current_path: current_path.map(str::to_owned),
            finished,
            error,
        });
    }
}
