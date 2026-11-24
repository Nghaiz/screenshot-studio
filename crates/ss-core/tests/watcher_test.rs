//! Phase 4.1: the live watcher. A file dropped into a watched root becomes a row and a `new-screenshot`
//! within 5 s; a file still being written is not indexed until it is complete; an Explorer rename keeps the
//! row and a delete marks it missing; with `watcherEnabled` off nothing is indexed. Temp roots only (D10).

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::io::{Cursor, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::env::Env;
use common::fixtures;
use ss_core::contract::Caller;
use ss_core::contract::ipc::*;
use ss_core::jobs::EventSink;

/// The phase's bound: dropped file -> row + `new-screenshot`.
const NEW_FILE_MAX: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Rec {
    news: Mutex<Vec<NewScreenshot>>,
    changes: Mutex<Vec<LibraryChanged>>,
}

impl EventSink for Rec {
    fn job_progress(&self, _: &JobProgress) {}
    fn library_changed(&self, c: &LibraryChanged) {
        self.changes.lock().unwrap().push(c.clone());
    }
    fn new_screenshots(&self, items: &[NewScreenshot]) {
        self.news.lock().unwrap().extend_from_slice(items);
    }
}

impl Rec {
    fn announced(&self, name: &str) -> bool {
        self.news.lock().unwrap().iter().any(|n| n.file_name == name)
    }
}

/// An indexed fixture library with the watcher running on it.
fn watched(n: usize) -> (Env, Arc<Rec>) {
    let env = Env::new(n);
    env.index(true);
    let rec = Arc::new(Rec::default());
    env.core.start_watcher(rec.clone()).expect("start watcher");
    assert!(wait(Duration::from_secs(5), || env.core.watched_roots() == 1), "the watcher never watched the root");
    (env, rec)
}

fn wait(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < timeout {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    f()
}

fn row(env: &Env, name: &str) -> Option<ImageSummary> {
    let mut q = ss_core::query::default_query(500);
    q.include_missing = true;
    env.all_of(q).into_iter().find(|s| s.file_name == name)
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 7) as u8, (y * 5) as u8, ((x + y) * 3) as u8]));
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn dropped_file_yields_a_row_and_new_screenshot_within_5s() {
    let (env, rec) = watched(3);
    let t0 = Instant::now();
    // Fixture 102 lands in the root folder itself (102 % 5 == 2).
    let f = fixtures::write_fixture(&env.root_dir(), 102);
    let ok = wait(NEW_FILE_MAX, || row(&env, &f.file_name).is_some() && rec.announced(&f.file_name));
    let took = t0.elapsed();
    assert!(ok, "no row + new-screenshot for {} after {took:?}", f.file_name);
    let r = row(&env, &f.file_name).unwrap();
    assert!(!r.missing);
    assert_eq!((r.width, r.height), (Some(f.width as i64), Some(f.height as i64)));
    assert!(wait(Duration::from_secs(2), || row(&env, &f.file_name).unwrap().thumb_state == ThumbState::Ok), "thumbnail made");
    assert!(
        rec.changes.lock().unwrap().iter().any(|c| c.reason == LibraryChangeReason::Watcher),
        "library-changed (watcher) went out"
    );
    eprintln!("watcher: row + new-screenshot after {took:?}");
    env.core.stop_watcher();
}

#[test]
fn partially_written_file_waits_until_complete() {
    let (env, rec) = watched(2);
    let bytes = png_bytes(400, 300);
    let path = env.root_dir().join("partial.png");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&bytes[..bytes.len() / 2]).unwrap();
    file.flush().unwrap();
    // Well past the debounce: the half file is seen, its size holds still, but its writer still has it open.
    std::thread::sleep(Duration::from_millis(4_000));
    assert!(row(&env, "partial.png").is_none(), "a file still being written was indexed");
    assert!(!rec.announced("partial.png"));

    file.write_all(&bytes[bytes.len() / 2..]).unwrap();
    drop(file);
    assert!(wait(NEW_FILE_MAX, || row(&env, "partial.png").is_some() && rec.announced("partial.png")), "complete file never indexed");
    let r = row(&env, "partial.png").unwrap();
    assert_eq!(r.size_bytes, bytes.len() as i64, "indexed at its full size");
    assert_eq!((r.width, r.height), (Some(400), Some(300)));
    env.core.stop_watcher();
}

#[test]
fn explorer_rename_keeps_the_row_and_delete_marks_it_missing() {
    let (env, rec) = watched(6);
    let root_rows: Vec<ImageSummary> = env.all().into_iter().filter(|s| s.folder.is_empty()).collect();
    let (a, b) = (&root_rows[0], &root_rows[1]);
    let dir = env.root_dir();
    std::fs::rename(dir.join(&a.file_name), dir.join("renamed by explorer.png")).unwrap();
    std::fs::remove_file(dir.join(&b.file_name)).unwrap();

    assert!(
        wait(NEW_FILE_MAX, || row(&env, "renamed by explorer.png").is_some_and(|r| r.id == a.id)),
        "the rename did not move the row"
    );
    assert!(wait(NEW_FILE_MAX, || row(&env, &b.file_name).is_some_and(|r| r.missing)), "the delete did not mark the row missing");
    assert!(row(&env, &a.file_name).is_none(), "no row left at the old name");
    assert!(rec.news.lock().unwrap().is_empty(), "a rename is not a new screenshot");
    env.core.stop_watcher();
}

#[test]
fn nothing_is_indexed_while_paused() {
    let (env, rec) = watched(2);
    env.core
        .set_setting(Caller::Ui, SetSettingReq { key: SettingKey::WatcherEnabled, value: serde_json::Value::Bool(false) })
        .unwrap();
    assert!(wait(Duration::from_secs(5), || env.core.watched_roots() == 0), "pause did not stop watching");
    let f = fixtures::write_fixture(&env.root_dir(), 107);
    std::thread::sleep(Duration::from_millis(3_500));
    assert!(row(&env, &f.file_name).is_none(), "indexed while paused");
    assert!(rec.news.lock().unwrap().is_empty());
    env.core.stop_watcher();
}
