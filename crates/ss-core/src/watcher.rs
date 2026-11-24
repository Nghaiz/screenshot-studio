//! Live folder watcher (phase 4.1): `notify-debouncer-full` ([`WATCHER_DEBOUNCE_MS`]) on every library root
//! while the `watcherEnabled` setting is true. Roots and the setting are re-read every
//! [`WATCHER_ROOT_POLL_S`] seconds (roots added by `ssctl` in another process are picked up) and at once on
//! [`WatcherHandle::refresh`].
//!
//! - A new or changed image file waits in `pending` until it is complete: its size and mtime held still for
//!   [`STABLE_MS`] (or its mtime is that old) and no other process holds it open for writing (an exclusive
//!   open succeeds; never tried on a cloud-only file, which a background pass never opens, rule 9). After
//!   [`PENDING_MAX_S`] it is indexed anyway. Then [`live::sync_paths`] gives it a row, the thumbnail pass
//!   runs for it, and `library-changed` (`watcher`) plus `new-screenshot` go out through the sink.
//! - A rename inside a root keeps the row (tags, notes, AI results) via [`live::apply_rename`]; a deleted
//!   or moved-out file is marked `missing`; a folder created, deleted or moved in/out rescans its root.
//! - Attribute-only changes (OneDrive hydration, pin state, indexer) and access events are ignored.
//!
//! Files the app writes itself (edits, converts, stitches) already have their row when the debounced event
//! arrives, so they sync as unchanged and are never announced as new screenshots.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::app::AppCore;
use crate::catalog;
use crate::contract::AppError;
use crate::contract::ipc::*;
use crate::errors::internal;
use crate::jobs::EventSink;
use crate::ops::path_under;
use crate::scan::{self, has_image_extension, live};
use crate::{query, thumbs};

/// Debounce timeout of file events.
pub const WATCHER_DEBOUNCE_MS: u64 = 2_000;
/// How often the roots and `watcherEnabled` are re-read.
pub const WATCHER_ROOT_POLL_S: u64 = 10;
/// How often pending files are re-checked.
pub const WATCHER_TICK_MS: u64 = 250;
/// A pending file is complete once its size and mtime held still this long.
pub const STABLE_MS: u64 = 500;
/// A file still held open by its writer after this long is indexed anyway.
pub const PENDING_MAX_S: u64 = 300;

enum Msg {
    Events(DebounceEventResult),
    Refresh,
    Stop,
}

/// The running watcher thread. Dropping it stops the thread.
pub struct WatcherHandle {
    tx: mpsc::Sender<Msg>,
    thread: Option<JoinHandle<()>>,
    watched: Arc<AtomicUsize>,
}

impl WatcherHandle {
    /// Roots being watched right now (0 while paused or before the first refresh).
    pub fn watched_roots(&self) -> usize {
        self.watched.load(Ordering::SeqCst)
    }

    /// Re-reads the roots and `watcherEnabled` now instead of at the next poll.
    pub fn refresh(&self) {
        let _ = self.tx.send(Msg::Refresh);
    }
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            // The thread may hold the last `Arc<AppCore>` and so run this drop itself: never join yourself.
            if t.thread().id() != std::thread::current().id() {
                let _ = t.join();
            }
        }
    }
}

/// Starts the watcher thread for `core` (held weakly: the thread ends when the core is dropped).
pub(crate) fn start(core: &Arc<AppCore>, sink: Arc<dyn EventSink>) -> Result<WatcherHandle, AppError> {
    let (tx, rx) = mpsc::channel();
    let weak = Arc::downgrade(core);
    let events_tx = tx.clone();
    let watched = Arc::new(AtomicUsize::new(0));
    let published = watched.clone();
    let thread = std::thread::Builder::new()
        .name("ss-watcher".into())
        .spawn(move || run(weak, sink, rx, events_tx, published))
        .map_err(|e| internal("cannot start the folder watcher").with_detail(e.to_string()))?;
    Ok(WatcherHandle { tx, thread: Some(thread), watched })
}

struct Pending {
    root_id: i64,
    first_seen: Instant,
    size: u64,
    mtime: Option<SystemTime>,
    still_since: Instant,
}

/// Work collected from one batch of events, done in [`State::flush`].
#[derive(Default)]
struct Work {
    /// Roots to rescan whole (folder created, deleted or moved; lost events).
    rescan: BTreeSet<i64>,
    /// Files gone (deleted or moved away): synced at once, they cannot be "incomplete".
    gone: Vec<(i64, PathBuf)>,
    /// Rows moved by renames.
    moved: Vec<i64>,
}

struct State {
    sink: Arc<dyn EventSink>,
    events_tx: mpsc::Sender<Msg>,
    debouncer: Option<Debouncer<notify::RecommendedWatcher, RecommendedCache>>,
    watched: HashMap<i64, LibraryRoot>,
    pending: HashMap<PathBuf, Pending>,
}

fn run(core: Weak<AppCore>, sink: Arc<dyn EventSink>, rx: mpsc::Receiver<Msg>, events_tx: mpsc::Sender<Msg>, published: Arc<AtomicUsize>) {
    let mut st = State { sink, events_tx, debouncer: None, watched: HashMap::new(), pending: HashMap::new() };
    let mut last_poll: Option<Instant> = None;
    loop {
        let poll_due = last_poll.is_none_or(|t| t.elapsed() >= Duration::from_secs(WATCHER_ROOT_POLL_S));
        let msg = if poll_due { Ok(Msg::Refresh) } else { rx.recv_timeout(Duration::from_millis(WATCHER_TICK_MS)) };
        let Some(core) = core.upgrade() else { break };
        let mut work = Work::default();
        match msg {
            Ok(Msg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Msg::Refresh) => {
                st.refresh(&core);
                published.store(st.watched.len(), Ordering::SeqCst);
                last_poll = Some(Instant::now());
            }
            Ok(Msg::Events(Ok(events))) => {
                for e in &events {
                    st.on_event(&core, &e.event, &mut work);
                }
            }
            Ok(Msg::Events(Err(errors))) => {
                for e in &errors {
                    eprintln!("ss-core watcher: {e}");
                }
                // Events may be lost: rescan everything watched.
                work.rescan.extend(st.watched.keys().copied());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        st.flush(&core, work);
    }
    // Dropping the debouncer stops its threads.
    st.debouncer = None;
    published.store(0, Ordering::SeqCst);
}

fn log_err(what: &str, e: &AppError) {
    eprintln!("ss-core watcher: {what}: {}: {}", e.message, e.detail.as_deref().unwrap_or(""));
}

/// True when no other process holds the file open for writing (an exclusive open succeeds).
#[cfg(windows)]
fn unlocked(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().read(true).share_mode(0).open(path).is_ok()
}

#[cfg(not(windows))]
fn unlocked(_path: &Path) -> bool {
    true
}

impl State {
    fn enabled(core: &AppCore) -> bool {
        core.catalog.read(|c| catalog::get_setting(c, SettingKey::WatcherEnabled)).map(|v| v.as_bool().unwrap_or(true)).unwrap_or(true)
    }

    /// Re-reads `watcherEnabled` and the roots; (un)watches to match.
    fn refresh(&mut self, core: &AppCore) {
        if !Self::enabled(core) {
            if self.debouncer.take().is_some() {
                eprintln!("ss-core watcher: paused (watcherEnabled = false)");
            }
            self.watched.clear();
            self.pending.clear();
            core.set_warning(AppWarningCode::WatcherFailed, None);
            return;
        }
        let roots = match core.catalog.read(catalog::list_roots) {
            Ok(r) => r,
            Err(e) => return log_err("cannot read the library roots", &e),
        };
        if self.debouncer.is_none() {
            let tx = self.events_tx.clone();
            match new_debouncer(Duration::from_millis(WATCHER_DEBOUNCE_MS), None, move |r: DebounceEventResult| {
                let _ = tx.send(Msg::Events(r));
            }) {
                Ok(d) => self.debouncer = Some(d),
                Err(e) => {
                    let msg = format!("the folder watcher could not start: {e}");
                    eprintln!("ss-core watcher: {msg}");
                    core.set_warning(AppWarningCode::WatcherFailed, Some(msg));
                    return;
                }
            }
        }
        let debouncer = self.debouncer.as_mut().expect("created above");
        let wanted: HashMap<i64, LibraryRoot> = roots.into_iter().map(|r| (r.id, r)).collect();
        let stale: Vec<i64> = self.watched.keys().filter(|id| !wanted.contains_key(id)).copied().collect();
        for id in stale {
            if let Some(r) = self.watched.remove(&id) {
                let _ = debouncer.unwatch(&r.path);
            }
            self.pending.retain(|_, p| p.root_id != id);
        }
        let mut failures = Vec::new();
        for (id, root) in wanted {
            if self.watched.contains_key(&id) {
                continue;
            }
            match debouncer.watch(Path::new(&root.path), RecursiveMode::Recursive) {
                Ok(()) => {
                    self.watched.insert(id, root);
                }
                Err(e) => failures.push(format!("{}: {e}", root.path)),
            }
        }
        if failures.is_empty() {
            core.set_warning(AppWarningCode::WatcherFailed, None);
        } else {
            let msg = format!("cannot watch {}", failures.join("; "));
            eprintln!("ss-core watcher: {msg}");
            core.set_warning(AppWarningCode::WatcherFailed, Some(msg));
        }
    }

    fn root_of(&self, path: &Path) -> Option<&LibraryRoot> {
        self.watched.values().find(|r| path_under(path, Path::new(&r.path)) && !same_path(path, Path::new(&r.path)))
    }

    fn on_event(&mut self, core: &AppCore, ev: &Event, work: &mut Work) {
        if ev.need_rescan() {
            match ev.paths.first().and_then(|p| self.root_of(p)) {
                Some(r) => {
                    work.rescan.insert(r.id);
                }
                None => work.rescan.extend(self.watched.keys().copied()),
            }
        }
        match &ev.kind {
            // Attribute-only (OneDrive hydration and pin state, the indexer) and reads: nothing to do.
            EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_)) => {}
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if ev.paths.len() == 2 => {
                self.renamed(core, &ev.paths[0], &ev.paths[1], work);
            }
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                for p in &ev.paths {
                    self.gone(p, work);
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)) => {
                for p in &ev.paths {
                    if p.exists() { self.appeared(p, work) } else { self.gone(p, work) }
                }
            }
            _ => {
                for p in &ev.paths {
                    self.appeared(p, work);
                }
            }
        }
    }

    fn appeared(&mut self, path: &Path, work: &mut Work) {
        let Some(root_id) = self.root_of(path).map(|r| r.id) else { return };
        if path.is_dir() {
            work.rescan.insert(root_id);
            return;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !has_image_extension(&name) {
            return;
        }
        let now = Instant::now();
        let p = self.pending.entry(path.to_path_buf()).or_insert(Pending {
            root_id,
            first_seen: now,
            size: u64::MAX,
            mtime: None,
            still_since: now,
        });
        p.still_since = now;
    }

    fn gone(&mut self, path: &Path, work: &mut Work) {
        let Some(root_id) = self.root_of(path).map(|r| r.id) else { return };
        self.pending.remove(path);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if has_image_extension(&name) {
            work.gone.push((root_id, path.to_path_buf()));
        } else {
            // A folder (or something we cannot tell): its rows are found by a rescan.
            work.rescan.insert(root_id);
        }
    }

    fn renamed(&mut self, core: &AppCore, from: &Path, to: &Path, work: &mut Work) {
        let (rf, rt) = (self.root_of(from).cloned(), self.root_of(to).cloned());
        if let (Some(a), Some(b)) = (&rf, &rt)
            && a.id == b.id
        {
            match live::apply_rename(&core.catalog, a, from, to) {
                Ok(ids) => work.moved.extend(ids),
                Err(e) => log_err("rename", &e),
            }
            self.pending.remove(from);
            if to.is_dir() {
                return;
            }
        }
        self.gone(from, work);
        self.appeared(to, work);
    }

    /// Paths of `pending` that are complete now.
    fn ready(&mut self, core: &AppCore) -> Vec<(i64, PathBuf)> {
        let now = Instant::now();
        let stable = Duration::from_millis(STABLE_MS);
        let mut ready = Vec::new();
        let mut vanished = Vec::new();
        for (path, p) in self.pending.iter_mut() {
            let Ok(meta) = std::fs::metadata(path) else {
                vanished.push(path.clone());
                continue;
            };
            let mtime = meta.modified().ok();
            let just_changed = meta.len() != p.size || mtime != p.mtime;
            if just_changed {
                p.size = meta.len();
                p.mtime = mtime;
                p.still_since = now;
            }
            // Complete when it held still for STABLE_MS, or when two looks in a row agree and its last
            // write is already that old (the debounce delay usually guarantees it).
            let old_mtime = mtime.and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= stable);
            let held_still = now.duration_since(p.still_since) >= stable;
            if !(held_still || (old_mtime && !just_changed)) {
                continue;
            }
            let cloud_only = core.fa.cloud_state(path, &meta) == CloudState::CloudOnly;
            let timed_out = now.duration_since(p.first_seen) >= Duration::from_secs(PENDING_MAX_S);
            if cloud_only || timed_out || unlocked(path) {
                ready.push((p.root_id, path.clone()));
            }
        }
        for path in vanished {
            if let Some(p) = self.pending.remove(&path) {
                ready.push((p.root_id, path));
            }
        }
        for (_, path) in &ready {
            self.pending.remove(path);
        }
        ready
    }

    fn flush(&mut self, core: &AppCore, mut work: Work) {
        let mut by_root: HashMap<i64, Vec<PathBuf>> = HashMap::new();
        for (root, path) in self.ready(core).into_iter().chain(std::mem::take(&mut work.gone)) {
            by_root.entry(root).or_default().push(path);
        }
        if by_root.is_empty() && work.rescan.is_empty() && work.moved.is_empty() {
            return;
        }
        let auto_cloud = core
            .catalog
            .read(|c| catalog::get_setting(c, SettingKey::AutoDownloadCloudOnly))
            .map(|v| v.as_bool().unwrap_or(false))
            .unwrap_or(false);
        let mut added = Vec::new();
        let mut changed = work.moved;
        let mut refetch_all = false;
        for (root_id, paths) in by_root {
            if work.rescan.contains(&root_id) {
                continue; // the rescan covers them
            }
            let Some(root) = self.watched.get(&root_id) else { continue };
            let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
            match live::sync_paths(&core.catalog, core.fa.as_ref(), root, &refs, auto_cloud) {
                Ok(r) => {
                    added.extend(r.added_ids);
                    changed.extend(r.changed_ids);
                }
                Err(e) => log_err(&format!("sync {}", root.path), &e),
            }
        }
        for root_id in &work.rescan {
            let Some(root) = self.watched.get(root_id).cloned() else { continue };
            match rescan(core, &root, auto_cloud) {
                Ok(ids) => added.extend(ids),
                Err(e) => log_err(&format!("rescan {}", root.path), &e),
            }
            refetch_all = true;
        }
        let mut thumb_ids = added.clone();
        thumb_ids.extend(&changed);
        if let Err(e) = thumbs::thumb_ids(&core.catalog, core.fa.as_ref(), &thumb_ids, auto_cloud) {
            log_err("thumbnails", &e);
        }
        if refetch_all || !thumb_ids.is_empty() {
            let image_ids = if refetch_all { None } else { Some(thumb_ids) };
            self.sink.library_changed(&LibraryChanged { reason: LibraryChangeReason::Watcher, image_ids });
        }
        let news: Vec<NewScreenshot> = added
            .iter()
            .filter_map(|id| core.catalog.read(|c| query::summary(c, *id)).ok().flatten())
            .filter(|s| !s.missing)
            .map(|s| NewScreenshot { image_id: s.id, file_name: s.file_name })
            .collect();
        if !news.is_empty() {
            self.sink.new_screenshots(&news);
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    crate::ops::norm_path(a) == crate::ops::norm_path(b)
}

/// A full scan of one root; returns the ids of the rows it added.
fn rescan(core: &AppCore, root: &LibraryRoot, auto_cloud: bool) -> Result<Vec<i64>, AppError> {
    let before: i64 = core.catalog.read(|c| Ok(c.query_row("SELECT COALESCE(max(id), 0) FROM image", [], |r| r.get(0))?))?;
    let cancel = AtomicBool::new(false);
    scan::scan_root(&core.catalog, core.fa.as_ref(), root, auto_cloud, &cancel, &|_, _, _| {})?;
    let root_id = root.id;
    let ids: Vec<i64> = core.catalog.read(move |c| {
        let mut stmt = c.prepare("SELECT id FROM image WHERE root_id = ?1 AND id > ?2 AND missing = 0")?;
        let ids = stmt.query_map(rusqlite::params![root_id, before], |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?;
        Ok(ids)
    })?;
    let scope = thumbs::ThumbScope { root_id: Some(root_id), allow_cloud: auto_cloud, ids: None };
    thumbs::thumb_pass(&core.catalog, core.fa.as_ref(), &scope, &cancel, &|_, _, _| {}, &|_| {})?;
    Ok(ids)
}
