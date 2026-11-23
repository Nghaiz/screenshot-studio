//! Phase 1.2 tests: indexer, catalog, thumbnails, `query_images` paging, `ssimg`, cloud-only
//! handling, settings, library roots and the `ssctl` CLI. Every test builds its own fixture
//! library and data dir under the temp dir (README D10).

mod common {
    pub mod fixtures;
}

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{File, Metadata};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use common::fixtures::{self, Fixture};
use serde_json::{Value, json};
use ss_core::contract::ipc::*;
use ss_core::contract::{Caller, ErrorKind};
use ss_core::fsaccess::{FileAccess, OsFileAccess};
use ss_core::jobs::{EventSink, NullSink};
use ss_core::scan::patterns;
use ss_core::{AppCore, IndexOptions, paths, query};
use tempfile::TempDir;

/// A core over a fresh data dir plus a fixture library root. Field order = drop order: the core
/// (and its SQLite handles) closes before the temp dirs are removed.
struct Env {
    core: Arc<AppCore>,
    root: LibraryRoot,
    fixtures: Vec<Fixture>,
    lib: TempDir,
    _data: TempDir,
}

impl Env {
    fn new(n: usize, fa: Arc<dyn FileAccess>) -> Self {
        let data = tempfile::tempdir().expect("data dir");
        let lib = tempfile::tempdir().expect("library dir");
        let fixtures = fixtures::write_library(lib.path(), n);
        let core = AppCore::open(data.path(), fa).expect("open core");
        let root = core
            .add_library_root(Caller::Cli, AddLibraryRootReq { path: lib.path().to_string_lossy().into_owned() })
            .expect("add root");
        Env { core, root, fixtures, lib, _data: data }
    }

    fn index(&self, thumbs: bool) -> ss_core::IndexReport {
        self.core.index_blocking(None, IndexOptions { thumbs }, &NullSink).expect("index")
    }

    fn all(&self, include_missing: bool) -> Vec<ImageSummary> {
        let mut q = query::default_query(500);
        q.include_missing = include_missing;
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

    fn sql_i64(&self, sql: &'static str) -> i64 {
        self.core.catalog().read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).expect("sql")
    }
}

fn local_to_ms(naive: &str) -> i64 {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.query_row("SELECT CAST(strftime('%s', ?1, 'utc') AS INTEGER) * 1000", [naive], |r| r.get(0)).unwrap()
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn mtime(p: &Path) -> SystemTime {
    std::fs::metadata(p).and_then(|m| m.modified()).expect("mtime")
}

// ---------------------------------------------------------------------------------------------

#[test]
fn index_500_fixtures_counts_thumbs_idempotent_reindex_and_missing() {
    let env = Env::new(500, Arc::new(OsFileAccess));
    let r = env.index(true);
    assert_eq!((r.scan.seen, r.scan.added, r.scan.walk_errors), (500, 500, 0), "{r:?}");
    assert_eq!((r.thumbs.generated, r.thumbs.failed, r.thumbs.offline), (500, 0, 0), "{r:?}");

    let c = env.core.get_counts().unwrap();
    assert_eq!((c.total, c.thumb_ok, c.thumb_pending, c.missing, c.cloud_only, c.trashed), (500, 500, 0, 0, 0, 0));
    assert_eq!(c.roots.len(), 1);
    assert_eq!(c.roots[0].image_count, 500);
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), 500);
    assert_eq!(env.sql_i64("SELECT count(*) FROM text_fts"), 500, "one FTS row per image");
    assert_eq!(env.sql_i64("SELECT count(*) FROM image WHERE content_hash IS NULL OR dhash IS NULL OR phash IS NULL"), 0);

    // Rows match the files: dimensions, folder, filename timestamp > created > mtime.
    let rows = env.all(false);
    assert_eq!(rows.len(), 500);
    let by_name: HashMap<(String, String), &ImageSummary> =
        rows.iter().map(|s| ((s.folder.clone(), s.file_name.clone()), s)).collect();
    let mut thumb_mtimes = Vec::new();
    for f in &env.fixtures {
        let s = by_name[&(f.folder.clone(), f.file_name.clone())];
        assert_eq!((s.width, s.height), (Some(f.width as i64), Some(f.height as i64)), "{}", f.file_name);
        assert_eq!(s.thumb_state, ThumbState::Ok);
        assert_eq!(s.cloud_state, CloudState::Local);
        match &f.name_time {
            Some(naive) => assert_eq!(s.taken_ms, local_to_ms(naive), "{} taken from its name", f.file_name),
            None => {
                let d = env.core.get_image(GetImageReq { id: s.id }).unwrap();
                assert_eq!(s.taken_ms, d.created_ms.unwrap_or(s.mtime_ms), "{} falls back to created", f.file_name);
            }
        }
        for w in ssimg::THUMB_WIDTHS {
            let p = paths::thumb_path(env.core.data_dir(), s.id, w);
            let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("thumb {}: {e}", p.display()));
            assert!(is_webp(&bytes), "{} is WebP", p.display());
            let (tw, th) = image::image_dimensions(&p).unwrap();
            let long = (f.width.max(f.height)).min(w as u32);
            assert_eq!(tw.max(th), long, "{} long edge", p.display());
            thumb_mtimes.push((p, mtime(&paths::thumb_path(env.core.data_dir(), s.id, w))));
        }
    }

    // Re-index: no new rows, no updates, no thumbnail regenerated.
    let r2 = env.index(true);
    assert_eq!((r2.scan.added, r2.scan.updated, r2.scan.unchanged, r2.scan.missing), (0, 0, 500, 0), "{r2:?}");
    assert_eq!(r2.thumbs.generated, 0, "{r2:?}");
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), 500, "no duplicate rows");
    assert_eq!(env.sql_i64("SELECT count(*) FROM text_fts"), 500, "no duplicate FTS rows");
    for (p, t) in &thumb_mtimes {
        assert_eq!(mtime(p), *t, "{} rewritten on re-index", p.display());
    }

    // A deleted file becomes `missing`.
    let gone = &env.fixtures[7];
    std::fs::remove_file(&gone.path).unwrap();
    let r3 = env.index(true);
    assert_eq!((r3.scan.missing, r3.scan.added), (1, 0), "{r3:?}");
    let c = env.core.get_counts().unwrap();
    assert_eq!((c.total, c.missing), (499, 1));
    assert_eq!(env.all(false).len(), 499);
    let with_missing = env.all(true);
    assert_eq!(with_missing.len(), 500);
    let row = with_missing.iter().find(|s| s.file_name == gone.file_name && s.folder == gone.folder).unwrap();
    assert!(row.missing);
    assert_eq!(env.core.ssimg(&format!("/full/{}", row.id)).status, 404, "missing original -> 404");
}

#[test]
fn query_images_cursor_paging_returns_every_id_exactly_once() {
    let env = Env::new(120, Arc::new(OsFileAccess));
    env.index(false);
    let all: BTreeSet<i64> = env.all(false).iter().map(|s| s.id).collect();
    assert_eq!(all.len(), 120);

    let sorts = [
        (SortField::Taken, SortDir::Desc),
        (SortField::Taken, SortDir::Asc),
        (SortField::Name, SortDir::Asc),
        (SortField::Size, SortDir::Desc),
        (SortField::Width, SortDir::Asc),
        (SortField::Rating, SortDir::Desc),
        (SortField::Random, SortDir::Asc),
    ];
    for (field, dir) in sorts {
        let mut q = query::default_query(7);
        q.sort = SortSpec { field, dir };
        let mut seen = Vec::new();
        let mut pages = 0;
        loop {
            let page = env.core.query_images(&q).unwrap();
            assert_eq!(page.total, 120);
            assert!(page.items.len() <= 7);
            seen.extend(page.items.iter().map(|s| (s.id, s.taken_ms)));
            pages += 1;
            assert!(pages < 100, "paging does not terminate for {field:?}");
            match page.next_cursor {
                Some(c) => q.cursor = Some(c),
                None => break,
            }
        }
        let ids: Vec<i64> = seen.iter().map(|x| x.0).collect();
        let unique: BTreeSet<i64> = ids.iter().copied().collect();
        assert_eq!(ids.len(), 120, "{field:?} {dir:?}: every id once (got {} rows)", ids.len());
        assert_eq!(unique, all, "{field:?} {dir:?}: same id set");
        if (field, dir) == (SortField::Taken, SortDir::Desc) {
            assert!(seen.windows(2).all(|w| w[0].1 >= w[1].1), "taken desc order");
        }
    }

    // Contract errors.
    let mut bad = query::default_query(0);
    assert_eq!(env.core.query_images(&bad).unwrap_err().kind, ErrorKind::InvalidArgument);
    bad.limit = 501;
    assert_eq!(env.core.query_images(&bad).unwrap_err().kind, ErrorKind::InvalidArgument);
    let mut q = query::default_query(5);
    let cursor = env.core.query_images(&q).unwrap().next_cursor.unwrap();
    q.sort.field = SortField::Name;
    q.cursor = Some(cursor);
    assert_eq!(env.core.query_images(&q).unwrap_err().kind, ErrorKind::InvalidArgument, "cursor of another sort");
    let mut q = query::default_query(5);
    q.cursor = Some("zz".into());
    assert_eq!(env.core.query_images(&q).unwrap_err().kind, ErrorKind::InvalidArgument, "garbage cursor");
    let mut q = query::default_query(5);
    q.favorite_only = true;
    assert_eq!(env.core.query_images(&q).unwrap_err().kind, ErrorKind::InvalidArgument, "2.1 filter rejected");

    // Trash filter and date range.
    let victim = *all.iter().next().unwrap();
    env.core
        .catalog()
        .write(move |tx| Ok(tx.execute("UPDATE image SET trashed_at = 1 WHERE id = ?1", [victim])?))
        .unwrap();
    let mut q = query::default_query(500);
    assert_eq!(env.core.query_images(&q).unwrap().total, 119);
    q.trash = TrashFilter::Only;
    assert_eq!(env.core.query_images(&q).unwrap().items.iter().map(|s| s.id).collect::<Vec<_>>(), vec![victim]);
    q.trash = TrashFilter::Include;
    assert_eq!(env.core.query_images(&q).unwrap().total, 120);
    let rows = env.all(false);
    let pivot = rows[60].taken_ms;
    let mut q = query::default_query(500);
    q.date_from = Some(pivot);
    let from = env.core.query_images(&q).unwrap();
    assert!(from.items.iter().all(|s| s.taken_ms >= pivot) && from.total > 0);

    // get_image + get_timeline.
    let d = env.core.get_image(GetImageReq { id: rows[0].id }).unwrap();
    assert!(Path::new(&d.abs_path).is_file(), "absPath {}", d.abs_path);
    assert_eq!(d.summary.file_name, rows[0].file_name);
    assert_eq!(env.core.get_image(GetImageReq { id: 999_999 }).unwrap_err().kind, ErrorKind::NotFound);
    for bucket in [TimelineBucketSize::Day, TimelineBucketSize::Month] {
        let t = env.core.get_timeline(&GetTimelineReq { query: query::default_query(1), bucket }).unwrap();
        assert_eq!(t.buckets.iter().map(|b| b.count).sum::<i64>(), 119, "{bucket:?}");
        assert!(t.buckets.windows(2).all(|w| w[0].start_ms < w[1].start_ms));
    }
}

// ---------------------------------------------------------------------------------------------
// Cloud-only mock (contract rule 9)
// ---------------------------------------------------------------------------------------------

/// Reports files whose name contains `cloud` as cloud-only until opened (opening hydrates, as
/// OneDrive does), and records every open.
#[derive(Default)]
struct CloudMock {
    opened: Mutex<Vec<PathBuf>>,
    hydrated: Mutex<HashSet<PathBuf>>,
}

fn is_cloud_name(p: &Path) -> bool {
    p.file_name().is_some_and(|n| n.to_string_lossy().contains("cloud"))
}

impl CloudMock {
    fn opened_cloud(&self) -> Vec<PathBuf> {
        self.opened.lock().unwrap().iter().filter(|p| is_cloud_name(p)).cloned().collect()
    }
}

impl FileAccess for CloudMock {
    fn cloud_state(&self, path: &Path, meta: &Metadata) -> CloudState {
        if is_cloud_name(path) && !self.hydrated.lock().unwrap().contains(path) {
            CloudState::CloudOnly
        } else {
            OsFileAccess.cloud_state(path, meta)
        }
    }

    fn open(&self, path: &Path) -> std::io::Result<File> {
        self.opened.lock().unwrap().push(path.to_path_buf());
        if is_cloud_name(path) {
            self.hydrated.lock().unwrap().insert(path.to_path_buf());
        }
        File::open(path)
    }
}

fn add_cloud_file(env: &Env) -> PathBuf {
    let src = &env.fixtures[1].path;
    let dst = env.lib.path().join("cloud 2026-11-23 101010.png");
    std::fs::copy(src, &dst).unwrap();
    dst
}

#[test]
fn cloud_only_file_is_counted_listed_never_opened_until_download() {
    let mock = Arc::new(CloudMock::default());
    let env = Env::new(6, mock.clone());
    let cloud_path = add_cloud_file(&env);
    let r = env.index(true);
    assert_eq!((r.scan.seen, r.scan.cloud_only), (7, 1), "{r:?}");
    assert_eq!((r.thumbs.generated, r.thumbs.failed), (6, 0), "{r:?}");
    assert!(mock.opened_cloud().is_empty(), "background passes opened {:?}", mock.opened_cloud());

    let c = env.core.get_counts().unwrap();
    assert_eq!((c.total, c.cloud_only, c.thumb_ok), (7, 1, 6));
    let row = env.all(false).into_iter().find(|s| s.file_name.starts_with("cloud")).expect("listed");
    assert_eq!((row.cloud_state, row.thumb_state, row.width, row.height), (CloudState::CloudOnly, ThumbState::Offline, None, None));
    assert_eq!(row.taken_ms, local_to_ms("2026-11-23 10:10:10"));
    for w in ssimg::THUMB_WIDTHS {
        assert!(!paths::thumb_path(env.core.data_dir(), row.id, w).exists());
    }

    // Re-index and a scan-only pass still never open it.
    env.index(true);
    env.index(false);
    assert!(mock.opened_cloud().is_empty());

    // autoDownloadCloudOnly=true lets the background pass open it.
    let settings = env.core.get_settings().unwrap();
    assert_eq!(settings.0["autoDownloadCloudOnly"], json!(false));

    // The explicit download reads it (hydrates), then thumbnails it.
    let t = env.core.download_cloud_only_blocking(env.root.id, &NullSink).unwrap();
    assert_eq!((t.generated, t.failed), (1, 0), "{t:?}");
    assert_eq!(mock.opened_cloud(), vec![cloud_path.clone()]);
    let row = env.core.get_image(GetImageReq { id: row.id }).unwrap().summary;
    assert_eq!((row.cloud_state, row.thumb_state), (CloudState::Local, ThumbState::Ok));
    assert!(row.width.is_some());
}

#[test]
fn auto_download_setting_allows_background_open_of_cloud_only() {
    let mock = Arc::new(CloudMock::default());
    let env = Env::new(2, mock.clone());
    add_cloud_file(&env);
    env.core
        .set_setting(Caller::Ui, SetSettingReq { key: SettingKey::AutoDownloadCloudOnly, value: json!(true) })
        .unwrap();
    let r = env.index(true);
    assert_eq!(r.thumbs.generated, 3, "{r:?}");
    assert_eq!(mock.opened_cloud().len(), 1);
}

// ---------------------------------------------------------------------------------------------

#[test]
fn ssimg_returns_200_202_400_404_per_contract() {
    let mock = Arc::new(CloudMock::default());
    let env = Env::new(4, mock.clone());
    add_cloud_file(&env);
    env.index(true);
    let late = fixtures::write_fixture(env.lib.path(), 900);
    env.index(false); // the new file stays `pending`

    let rows = env.all(false);
    let ok = rows.iter().find(|s| s.thumb_state == ThumbState::Ok).unwrap();
    let pending = rows.iter().find(|s| s.file_name == late.file_name).unwrap();
    let offline = rows.iter().find(|s| s.cloud_state == CloudState::CloudOnly).unwrap();
    assert_eq!(pending.thumb_state, ThumbState::Pending);

    for w in [320, 1280] {
        let r = env.core.ssimg(&format!("/thumb/{}/{w}?v={}", ok.id, ok.mtime_ms));
        assert_eq!(r.status, 200, "thumb {w}");
        assert_eq!(r.header("content-type"), Some("image/webp"));
        assert_eq!(r.header("cache-control"), Some(ssimg::CACHE_CONTROL));
        assert!(is_webp(&r.body));
    }
    // What the webview really sends: `convertFileSrc` percent-encodes the whole path (phase 1.4 integration bug).
    let r = env.core.ssimg(&format!("/thumb%2F{}%2F320?v={}", ok.id, ok.mtime_ms));
    assert_eq!(r.status, 200, "percent-encoded thumb path");
    assert!(is_webp(&r.body));
    assert_eq!(env.core.ssimg(&format!("/full%2F{}", ok.id)).status, 200, "percent-encoded full path");
    let r = env.core.ssimg(&format!("/thumb/{}/320", pending.id));
    assert_eq!((r.status, r.header(ssimg::THUMB_STATE_HEADER), r.body.len()), (202, Some("pending"), 0));
    let r = env.core.ssimg(&format!("/thumb/{}/320", offline.id));
    assert_eq!((r.status, r.header(ssimg::THUMB_STATE_HEADER), r.body.len()), (202, Some("offline"), 0));

    for bad in [
        format!("/thumb/{}/500", ok.id),
        format!("/thumb/{}/0", ok.id),
        "/thumb/abc/320".to_owned(),
        format!("/thumb/{}", ok.id),
        "/nope/1".to_owned(),
        "/full/-3".to_owned(),
        "/".to_owned(),
        "/thumb%2".to_owned(),
        "/thumb%ZZ1%2F320".to_owned(),
    ] {
        assert_eq!(env.core.ssimg(&bad).status, 400, "{bad}");
    }
    assert_eq!(env.core.ssimg("/thumb/999999/320").status, 404);
    assert_eq!(env.core.ssimg("/full/999999").status, 404);

    let full = env.core.ssimg(&format!("/full/{}", ok.id));
    assert_eq!(full.status, 200);
    assert_eq!(full.header("content-type"), Some("image/png"));
    let d = env.core.get_image(GetImageReq { id: ok.id }).unwrap();
    assert_eq!(full.body, std::fs::read(&d.abs_path).unwrap());
    assert!(mock.opened_cloud().is_empty(), "no ssimg call above touched the cloud-only file");
}

#[test]
fn filename_timestamp_patterns() {
    let cases = [
        ("Screenshot 2026-11-23 095012.png", Some("2026-11-23 09:50:12")),
        ("Ảnh chụp màn hình 2025-01-31 235959.png", Some("2025-01-31 23:59:59")),
        ("Screenshot_20240229_070809.jpg", Some("2024-02-29 07:08:09")),
        ("Screenshot 2026-11-23 at 09.50.12.png", Some("2026-11-23 09:50:12")),
        ("Snipaste_2026-11-23_09-50-12.png", Some("2026-11-23 09:50:12")),
        ("Screenshot 2026-11-23.png", Some("2026-11-23 00:00:00")),
        ("Screenshot (123).png", None),
        ("photo.png", None),
        ("Screenshot 2026-13-01 000000.png", None),
        ("x12026-11-23 095012.png", None),
        ("Screenshot_20230229_000000.png", None),
    ];
    for (name, want) in cases {
        let got = patterns::parse_file_name(name).map(|t| t.to_sqlite());
        assert_eq!(got.as_deref(), want, "{name}");
    }
    assert!(!patterns::PATTERNS.is_empty());
}

#[test]
fn library_roots_validation_and_protected_detection() {
    assert!(paths::is_protected_path(r"C:\Users\Nghaiz\OneDrive\Pictures\Screenshots"));
    assert!(paths::is_protected_path(r"c:/users/x/onedrive/pictures"));
    assert!(paths::is_protected_path(r"C:\Users\x\OneDrive - Contoso\Docs"));
    assert!(!paths::is_protected_path(r"D:\Screenshot_Picture\fixtures"));
    assert!(!paths::is_protected_path(r"D:\OneDriveBackup\x"));

    let env = Env::new(3, Arc::new(OsFileAccess));
    assert!(!env.root.protected_root);
    let err = |path: &str| env.core.add_library_root(Caller::Ui, AddLibraryRootReq { path: path.into() }).unwrap_err().kind;
    assert_eq!(err(&env.root.path), ErrorKind::Conflict, "same root twice");
    assert_eq!(err(&env.lib.path().join("1ERD").to_string_lossy()), ErrorKind::Conflict, "nested root");
    assert_eq!(err(&env.lib.path().join("does-not-exist").to_string_lossy()), ErrorKind::NotFound);
    assert_eq!(err("relative\\dir"), ErrorKind::InvalidArgument);
    assert_eq!(err("   "), ErrorKind::InvalidArgument);

    let info = env.core.get_app_info().unwrap();
    assert_eq!(info.contract_version, ss_core::contract::CONTRACT_VERSION);
    assert_eq!(info.library_roots.len(), 1);
    assert_eq!(info.sidecar.state, SidecarState::Absent);

    env.index(true);
    let ids: Vec<i64> = env.all(false).iter().map(|s| s.id).collect();
    let r = env.core.remove_library_root(Caller::Ui, RemoveLibraryRootReq { root_id: env.root.id }).unwrap();
    assert_eq!((r.affected, r.batch_id, r.failed.len()), (3, None, 0));
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), 0);
    assert_eq!(env.sql_i64("SELECT count(*) FROM text_fts"), 0, "FTS rows removed with the root");
    for id in ids {
        assert!(!paths::thumb_path(env.core.data_dir(), id, 320).exists());
    }
    assert!(env.fixtures.iter().all(|f| f.path.is_file()), "removing a root never touches files");
    let again = env.core.remove_library_root(Caller::Ui, RemoveLibraryRootReq { root_id: env.root.id });
    assert_eq!(again.unwrap_err().kind, ErrorKind::NotFound);
}

#[test]
fn settings_defaults_validation_and_roundtrip() {
    let env = Env::new(0, Arc::new(OsFileAccess));
    let s = env.core.get_settings().unwrap();
    assert_eq!(s.0.len(), SettingKey::ALL.len());
    for key in SettingKey::ALL {
        let name = serde_json::to_value(key).unwrap().as_str().unwrap().to_owned();
        assert_eq!(s.0[&name], key.default_value(), "{name}");
    }
    let set = |key, value: Value| env.core.set_setting(Caller::Cli, SetSettingReq { key, value });
    set(SettingKey::DupHamming, json!(9)).unwrap();
    set(SettingKey::AiDevice, json!("cpu")).unwrap();
    set(SettingKey::SidecarVenv, json!(r"D:\venv")).unwrap();
    for (key, value) in [
        (SettingKey::DupHamming, json!("9")),
        (SettingKey::DupHamming, json!(65)),
        (SettingKey::AiDevice, json!("gpu")),
        (SettingKey::WatcherEnabled, json!(1)),
        (SettingKey::Hotkey, json!("")),
        (SettingKey::SessionGapMinutes, json!(0)),
    ] {
        assert_eq!(set(key, value.clone()).unwrap_err().kind, ErrorKind::InvalidArgument, "{key:?} = {value}");
    }
    let s = env.core.get_settings().unwrap();
    assert_eq!((&s.0["dupHamming"], &s.0["aiDevice"], &s.0["sidecarVenv"]), (&json!(9), &json!("cpu"), &json!(r"D:\venv")));
}

/// Records events; used to check `start_index` runs in the background and reports progress.
#[derive(Default)]
struct Recorder {
    progress: Mutex<Vec<(Instant, JobProgress)>>,
    changed: Mutex<Vec<LibraryChanged>>,
}

impl EventSink for Recorder {
    fn job_progress(&self, p: &JobProgress) {
        self.progress.lock().unwrap().push((Instant::now(), p.clone()));
    }
    fn library_changed(&self, c: &LibraryChanged) {
        self.changed.lock().unwrap().push(c.clone());
    }
}

#[test]
fn start_index_job_reports_throttled_progress_and_library_changed() {
    let env = Env::new(60, Arc::new(OsFileAccess));
    let rec = Arc::new(Recorder::default());
    let started = env.core.start_index(StartIndexReq { root_id: Some(env.root.id) }, rec.clone()).unwrap();
    let second = env.core.start_index(StartIndexReq { root_id: None }, rec.clone());
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if rec.progress.lock().unwrap().iter().any(|(_, p)| p.finished) {
            break;
        }
        assert!(Instant::now() < deadline, "index job did not finish");
        std::thread::sleep(Duration::from_millis(50));
    }
    let progress = rec.progress.lock().unwrap();
    assert!(progress.iter().all(|(_, p)| p.job_id == started.job_id && p.kind == JobKind::Index));
    let last = &progress.last().unwrap().1;
    assert!(last.finished && last.error.is_none(), "{last:?}");
    assert_eq!(progress.iter().filter(|(_, p)| p.finished).count(), 1);
    // <= 10 events per second: consecutive non-final events at least ~100 ms apart.
    let non_final: Vec<Instant> = progress.iter().filter(|(_, p)| !p.finished).map(|(t, _)| *t).collect();
    assert!(non_final.windows(2).all(|w| w[1] - w[0] >= Duration::from_millis(95)), "throttle");
    let changed = rec.changed.lock().unwrap();
    assert!(changed.iter().any(|c| c.image_ids.is_none()), "refetch-all after the scan");
    let thumbed: usize = changed.iter().filter_map(|c| c.image_ids.as_ref()).map(Vec::len).sum();
    assert_eq!(thumbed, 60, "every thumbnailed id announced");
    match second {
        Err(e) => assert_eq!(e.kind, ErrorKind::Conflict),
        Ok(_) => {} // the first job may already have finished
    }
    assert_eq!(env.core.cancel_job(CancelJobReq { job_id: 424_242 }).unwrap_err().kind, ErrorKind::NotFound);
    assert_eq!(env.core.get_counts().unwrap().thumb_ok, 60);
}

#[test]
fn ssctl_prints_one_json_document_per_subcommand() {
    let data = tempfile::tempdir().unwrap();
    let lib = tempfile::tempdir().unwrap();
    fixtures::write_library(lib.path(), 5);
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_ssctl"))
            .arg("--data-dir")
            .arg(data.path())
            .args(args)
            .env_remove(paths::DATA_DIR_ENV)
            .output()
            .expect("run ssctl");
        let stdout = String::from_utf8(out.stdout).unwrap();
        (out.status.code(), stdout, String::from_utf8_lossy(&out.stderr).into_owned())
    };
    let doc = |stdout: &str| -> Value {
        assert_eq!(stdout.trim_end().lines().count(), 1, "one compact JSON line: {stdout}");
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"))
    };

    let (code, out, _) = run(&["--help"]);
    assert_eq!(code, Some(0));
    for sub in ["add-root", "remove-root", "index", "list", "stats", "extensions", "get", "set"] {
        assert!(out.contains(sub), "--help lists {sub}");
    }

    let lib_path = lib.path().to_string_lossy().into_owned();
    let (code, out, _) = run(&["add-root", &lib_path, "--json"]);
    assert_eq!(code, Some(0));
    let root: LibraryRoot = serde_json::from_value(doc(&out)).unwrap();
    assert_eq!(root.path, lib_path);

    let (code, out, err) = run(&["index", &root.id.to_string(), "--wait", "--json"]);
    assert_eq!(code, Some(0), "{err}");
    let counts: CatalogCounts = serde_json::from_value(doc(&out)).unwrap();
    assert_eq!((counts.total, counts.thumb_ok), (5, 5));
    assert!(!err.is_empty(), "progress logs go to stderr");

    let (code, out, _) = run(&["list", "--limit", "2", "--json"]);
    assert_eq!(code, Some(0));
    let page: ImagePage = serde_json::from_value(doc(&out)).unwrap();
    assert_eq!((page.items.len(), page.total, page.next_cursor.is_some()), (2, 5, true));
    let (_, out, _) = run(&["list", "--since", "4102444800000", "--json"]);
    assert_eq!(serde_json::from_value::<ImagePage>(doc(&out)).unwrap().total, 0);

    let (code, out, _) = run(&["stats", "--json"]);
    assert_eq!(code, Some(0));
    assert_eq!(serde_json::from_value::<CatalogCounts>(doc(&out)).unwrap().total, 5);

    let (_, out, _) = run(&["extensions", "--json"]);
    assert_eq!(doc(&out), json!(["png", "jpg", "jpeg", "webp", "gif", "bmp"]));

    assert_eq!(run(&["set", "dupHamming", "7", "--json"]).0, Some(0));
    let (_, out, _) = run(&["get", "dupHamming", "--json"]);
    assert_eq!(doc(&out), json!(7));

    // Errors: exit 1 with the AppError envelope on stdout; usage: exit 2.
    let (code, out, _) = run(&["remove-root", "999", "--json"]);
    assert_eq!(code, Some(1));
    assert_eq!(doc(&out)["kind"], json!("notFound"));
    let (code, out, _) = run(&["set", "dupHamming", "\"x\"", "--json"]);
    assert_eq!((code, doc(&out)["kind"].clone()), (Some(1), json!("invalidArgument")));
    assert_eq!(run(&["bogus"]).0, Some(2));
    assert_eq!(run(&[]).0, Some(2));
    assert_eq!(run(&["list", "--limit"]).0, Some(2));

    let (code, out, _) = run(&["remove-root", &root.id.to_string(), "--json"]);
    assert_eq!(code, Some(0));
    assert_eq!(doc(&out)["affected"], json!(5));
}
