//! Phase 2.1 / Deferred decision #29: lossy WebP q80 thumbnails, 320 at index, 1280 on the first
//! `ssimg` request (never from a cloud-only original), the one-time regeneration of lossless
//! thumbnails driven by `<data>\thumbs\FORMAT`, and the cloud-only retry fix (a failed read of a
//! still-cloud-only file ends `offline`, and `download_cloud_only` retries `error` rows).

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use common::env::{CloudMock, Env};
use image::ExtendedColorType;
use ss_core::contract::ipc::*;
use ss_core::jobs::NullSink;
use ss_core::{paths, thumbs};

/// `RIFF....WEBP` + first chunk fourcc (`VP8 ` lossy, `VP8L` lossless, `VP8X` extended).
fn webp_kind(bytes: &[u8]) -> &str {
    assert!(bytes.len() > 16 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP", "not a WebP file");
    std::str::from_utf8(&bytes[12..16]).unwrap()
}

fn thumb_files(data: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(data.join(paths::THUMBS_DIR))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() != thumbs::FORMAT_FILE)
        .map(|e| e.into_path())
        .collect()
}

fn mtime(p: &Path) -> SystemTime {
    std::fs::metadata(p).and_then(|m| m.modified()).unwrap()
}

#[test]
fn index_writes_only_lossy_320_files_and_the_marker() {
    let env = Env::new(52); // fixture 49 is 1500 x 900: both sizes downscale
    let r = env.index(true);
    assert_eq!((r.thumbs.generated, r.thumbs.failed), (52, 0), "{r:?}");
    let files = thumb_files(env.data.path());
    assert_eq!(files.len(), 52, "one file per image");
    for s in env.all() {
        let p = paths::thumb_path(env.data.path(), s.id, 320);
        let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert_eq!(webp_kind(&bytes), "VP8 ", "{} is lossy", p.display());
        let (w, h) = image::image_dimensions(&p).unwrap();
        assert_eq!(w.max(h) as i64, s.width.unwrap().max(s.height.unwrap()).min(320));
        assert!(!paths::thumb_path(env.data.path(), s.id, 1280).exists(), "no 1280 at index");
    }
    let marker = std::fs::read_to_string(env.data.path().join(paths::THUMBS_DIR).join(thumbs::FORMAT_FILE)).unwrap();
    assert_eq!(marker, thumbs::THUMB_FORMAT);
}

#[test]
fn first_1280_request_renders_and_keeps_the_file_cloud_only_answers_202() {
    let mock = Arc::new(CloudMock::default());
    let env = Env::with(50, mock.clone(), "");
    env.index(true);
    let big = env.all().into_iter().find(|s| s.width == Some(1500)).expect("large fixture");
    let p1280 = paths::thumb_path(env.data.path(), big.id, 1280);
    assert!(!p1280.exists());

    let r = env.core.ssimg(&format!("/thumb%2F{}%2F1280?v={}", big.id, big.mtime_ms));
    assert_eq!((r.status, r.header("content-type")), (200, Some("image/webp")));
    assert_eq!(webp_kind(&r.body), "VP8 ");
    assert!(p1280.is_file(), "kept on disk");
    assert_eq!(std::fs::read(&p1280).unwrap(), r.body);
    assert_eq!(image::image_dimensions(&p1280).unwrap(), (1280, 768));
    let opens = mock.opened.lock().unwrap().len();
    let again = env.core.ssimg(&format!("/thumb/{}/1280", big.id));
    assert_eq!((again.status, again.body == r.body), (200, true));
    assert_eq!(mock.opened.lock().unwrap().len(), opens, "second request served from the cache file");

    // A cloud-only original (320 ok from an earlier download) is never opened for its preview.
    let other = env.all().into_iter().find(|s| s.id != big.id).unwrap();
    let abs = PathBuf::from(env.detail(other.id).abs_path);
    mock.set_cloud(&abs, true);
    let opens_before = mock.opens_of(&abs);
    let r = env.core.ssimg(&format!("/thumb/{}/1280", other.id));
    assert_eq!((r.status, r.header(ssimg::THUMB_STATE_HEADER), r.body.len()), (202, Some("offline"), 0));
    assert_eq!(mock.opens_of(&abs), opens_before, "the cloud-only original was not opened");
    assert!(!paths::thumb_path(env.data.path(), other.id, 1280).exists());
    assert_eq!(env.core.ssimg(&format!("/thumb/{}/320", other.id)).status, 200, "its 320 is still served");
}

#[test]
fn lossless_thumbnails_without_marker_are_regenerated_once() {
    let env = Env::new(8);
    env.index(true);
    // Make the data dir look like phase 1.4 left it: lossless 320 + 1280 files, no marker.
    let dir = env.data.path().join(paths::THUMBS_DIR);
    std::fs::remove_file(dir.join(thumbs::FORMAT_FILE)).unwrap();
    for s in env.all() {
        for w in [320u32, 1280] {
            let mut out = Vec::new();
            let px = vec![200u8; 16 * 9 * 4];
            image::codecs::webp::WebPEncoder::new_lossless(&mut out).encode(&px, 16, 9, ExtendedColorType::Rgba8).unwrap();
            std::fs::write(paths::thumb_path(env.data.path(), s.id, w as i64), &out).unwrap();
        }
    }
    let stray = dir.join("ab").join("stale_1280.webp");
    std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
    std::fs::write(&stray, b"old").unwrap();
    assert_eq!(thumb_files(env.data.path()).len(), 17);

    let r = env.index(true);
    assert_eq!(r.thumbs.generated, 8, "every ok thumbnail regenerated: {r:?}");
    let files = thumb_files(env.data.path());
    assert_eq!(files.len(), 8, "old 1280 files and strays gone: {files:?}");
    let mut stamps = Vec::new();
    for s in env.all() {
        assert_eq!(s.thumb_state, ThumbState::Ok);
        let p = paths::thumb_path(env.data.path(), s.id, 320);
        assert_eq!(webp_kind(&std::fs::read(&p).unwrap()), "VP8 ");
        stamps.push((p.clone(), mtime(&p)));
    }

    let r = env.index(true);
    assert_eq!(r.thumbs.generated, 0, "not again: {r:?}");
    for (p, t) in stamps {
        assert_eq!(mtime(&p), t, "{} untouched", p.display());
    }
}

#[test]
fn failed_cloud_read_ends_offline_and_error_rows_are_retried() {
    let mock = Arc::new(CloudMock::default());
    let env = Env::with(3, mock.clone(), "");
    let cloud = env.lib.path().join("cloud 2026-11-23 101010.png");
    std::fs::copy(&env.fixtures[0].path, &cloud).unwrap();
    mock.set_cloud(&cloud, true);
    env.index(true);
    let row = env.by_name("cloud 2026-11-23 101010.png");
    assert_eq!((row.cloud_state, row.thumb_state), (CloudState::CloudOnly, ThumbState::Offline));
    assert_eq!(mock.opens_of(&cloud), 0, "background passes never open it");

    // OneDrive client stopped: the explicit download cannot read it, and it stays cloud-only.
    *mock.fail_cloud_reads.lock().unwrap() = true;
    let t = env.core.download_cloud_only_blocking(env.root.id, &NullSink).unwrap();
    assert_eq!(mock.opens_of(&cloud), 1, "the download tried it");
    assert_eq!((t.generated, t.offline, t.failed), (0, 1, 0), "{t:?}");
    assert_eq!(env.detail(row.id).summary.thumb_state, ThumbState::Offline, "offline, not error");

    // A row left in `error` by phase 1.4 is picked up again by the download.
    let id = row.id;
    env.core
        .catalog()
        .write(move |tx| Ok(tx.execute("UPDATE image SET thumb_state = 'error' WHERE id = ?1", [id])?))
        .unwrap();
    env.core.download_cloud_only_blocking(env.root.id, &NullSink).unwrap();
    assert_eq!(mock.opens_of(&cloud), 2, "the error row was retried");
    assert_eq!(env.detail(row.id).summary.thumb_state, ThumbState::Offline);

    // OneDrive back: the next download hydrates and thumbnails it.
    *mock.fail_cloud_reads.lock().unwrap() = false;
    let t = env.core.download_cloud_only_blocking(env.root.id, &NullSink).unwrap();
    assert_eq!(t.generated, 1, "{t:?}");
    let s = env.detail(row.id).summary;
    assert_eq!((s.cloud_state, s.thumb_state), (CloudState::Local, ThumbState::Ok));

    // A local file that cannot be decoded is still `error` (not every failure is a cloud one).
    let junk = env.lib.path().join("broken.png");
    std::fs::write(&junk, b"not a png").unwrap();
    env.index(true);
    assert_eq!(env.by_name("broken.png").thumb_state, ThumbState::Error);
}
