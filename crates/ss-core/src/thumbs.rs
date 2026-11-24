//! Thumbnail pass: for every `pending` image, ONE read + decode yields blake3, dHash, pHash, the real
//! dimensions, the grid thumbnail (lossy WebP q80, long edge 320, never upscaled) and its sharpness
//! (`blur_score`, phase 3.2, [`crate::smart::blur`]). The 1280
//! preview is made by `ssimg` on its first request ([`render_preview`]) and kept as a cache
//! (contract-sqlite-schema.md § thumbnails, Deferred decision #29). Runs on a rayon
//! pool of `available_parallelism() - 1` threads; results are written in batches of
//! [`WRITE_BATCH`] rows. Resumable: the state lives in `image.thumb_state`, so a cancelled or
//! crashed pass continues with the rows still `pending`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{PixelType, Resizer};
use image::GenericImageView;
use image_hasher::{HashAlg, HasherConfig};
use rayon::prelude::*;
use rusqlite::params;
use serde::Serialize;

use crate::catalog::{Catalog, WRITE_BATCH};
use crate::contract::AppError;
use crate::contract::ipc::CloudState;
use crate::contract::ipc::ssimg::{THUMB_GRID, THUMB_PREVIEW, THUMB_WIDTHS};
use crate::errors::{enum_str, internal};
use crate::fsaccess::FileAccess;
use crate::paths;
use crate::scan::{Progress, cancelled};

/// Perceptual hash side (8 x 8 = 64 bits, stored as signed i64).
const HASH_SIDE: u32 = 8;
/// libwebp lossy quality of every thumbnail.
pub const WEBP_QUALITY: f32 = 80.0;
/// File in `<data>\thumbs` naming the thumbnail format in use.
pub const FORMAT_FILE: &str = "FORMAT";
/// Current thumbnail format; a different or absent marker triggers one regeneration.
pub const THUMB_FORMAT: &str = "webp-q80";

/// The worker pool of background passes: `available_parallelism() - 1` threads, at least 1.
pub fn worker_pool() -> Result<rayon::ThreadPool, AppError> {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).saturating_sub(1).max(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .thread_name(|i| format!("ss-worker-{i}"))
        .build()
        .map_err(|e| internal("cannot start the worker pool").with_detail(e.to_string()))
}

/// Which `pending` rows a pass takes.
#[derive(Debug, Clone, Default)]
pub struct ThumbScope {
    /// Only this root (None = every root).
    pub root_id: Option<i64>,
    /// When false (background passes), cloud-only files are never opened: they become `offline`.
    /// True only for `download_cloud_only` or the `autoDownloadCloudOnly` setting.
    pub allow_cloud: bool,
    /// Only these images (None = every `pending` row of the scope): the watcher and the edit commands.
    pub ids: Option<Vec<i64>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbReport {
    pub generated: i64,
    pub failed: i64,
    pub offline: i64,
}

struct Item {
    id: i64,
    abs: PathBuf,
}

enum Outcome {
    Ok {
        id: i64,
        width: i64,
        height: i64,
        hash: String,
        dhash: Option<i64>,
        phash: Option<i64>,
        cloud: CloudState,
        blur: Option<f64>,
    },
    Offline { id: i64 },
    Error { id: i64 },
}

/// Runs the pass. `on_batch` receives the ids whose thumbnails became `ok` after each batch commit.
pub fn thumb_pass(
    cat: &Catalog,
    fa: &dyn FileAccess,
    scope: &ThumbScope,
    cancel: &AtomicBool,
    progress: Progress<'_>,
    on_batch: &dyn Fn(Vec<i64>),
) -> Result<ThumbReport, AppError> {
    let (root_id, allow_cloud) = (scope.root_id, scope.allow_cloud);
    let items: Vec<Item> = cat.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT i.id, r.path, i.folder, i.file_name FROM image i JOIN library_root r ON r.id = i.root_id
             WHERE i.thumb_state = 'pending' AND i.missing = 0
               AND (?1 IS NULL OR i.root_id = ?1) AND (?2 OR i.cloud_state != 'cloudOnly')
             ORDER BY i.id",
        )?;
        let rows = stmt.query_map(params![root_id, allow_cloud], |r| {
            let (root, folder, name): (String, String, String) = (r.get(1)?, r.get(2)?, r.get(3)?);
            Ok(Item { id: r.get(0)?, abs: paths::image_abs_path(&root, &folder, &name) })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    })?;
    let items: Vec<Item> = match &scope.ids {
        Some(ids) => {
            let want: std::collections::HashSet<i64> = ids.iter().copied().collect();
            items.into_iter().filter(|i| want.contains(&i.id)).collect()
        }
        None => items,
    };

    let total = items.len() as i64;
    let done = AtomicI64::new(0);
    let data_dir = cat.data_dir().to_path_buf();
    let pool = worker_pool()?;
    let mut report = ThumbReport::default();
    for chunk in items.chunks(WRITE_BATCH) {
        let outcomes: Vec<Outcome> = pool.install(|| {
            chunk
                .par_iter()
                .filter(|_| !cancel.load(Ordering::Relaxed))
                .map(|item| {
                    let out = process(&data_dir, fa, item, allow_cloud);
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    progress(n, total, item.abs.to_str());
                    out
                })
                .collect()
        });
        let ok_ids: Vec<i64> =
            outcomes.iter().filter_map(|o| if let Outcome::Ok { id, .. } = o { Some(*id) } else { None }).collect();
        for o in &outcomes {
            match o {
                Outcome::Ok { .. } => report.generated += 1,
                Outcome::Offline { .. } => report.offline += 1,
                Outcome::Error { .. } => report.failed += 1,
            }
        }
        cat.write(move |tx| write_outcomes(tx, outcomes))?;
        if !ok_ids.is_empty() {
            on_batch(ok_ids);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
    }
    Ok(report)
}

fn write_outcomes(tx: &rusqlite::Transaction<'_>, outcomes: Vec<Outcome>) -> Result<(), AppError> {
    for o in outcomes {
        match o {
            Outcome::Ok { id, width, height, hash, dhash, phash, cloud, blur } => {
                tx.prepare_cached(
                    "UPDATE image SET width = ?2, height = ?3, content_hash = ?4, dhash = ?5, phash = ?6,
                        thumb_state = 'ok', cloud_state = ?7, blur_score = ?8 WHERE id = ?1",
                )?
                .execute(params![id, width, height, hash, dhash, phash, enum_str(&cloud), blur])?;
            }
            Outcome::Offline { id } => {
                tx.prepare_cached("UPDATE image SET thumb_state = 'offline', cloud_state = 'cloudOnly' WHERE id = ?1")?
                    .execute([id])?;
            }
            Outcome::Error { id } => {
                tx.prepare_cached("UPDATE image SET thumb_state = 'error' WHERE id = ?1")?.execute([id])?;
            }
        }
    }
    Ok(())
}

fn process(data_dir: &Path, fa: &dyn FileAccess, item: &Item, allow_cloud: bool) -> Outcome {
    let id = item.id;
    // Attribute check before any open: a background pass never opens a cloud-only file.
    let meta = match std::fs::symlink_metadata(&item.abs) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ss-core thumbs: {}: {e}", item.abs.display());
            return Outcome::Error { id };
        }
    };
    if !allow_cloud && fa.cloud_state(&item.abs, &meta) == CloudState::CloudOnly {
        return Outcome::Offline { id };
    }
    match render(data_dir, fa, item) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("ss-core thumbs: {}: {e}", item.abs.display());
            // A cloud-only file that could not be read (OneDrive client stopped, offline) is still
            // cloud-only: `offline` keeps it eligible for `download_cloud_only`; `error` would not.
            let still_cloud =
                std::fs::symlink_metadata(&item.abs).is_ok_and(|m| fa.cloud_state(&item.abs, &m) == CloudState::CloudOnly);
            if still_cloud { Outcome::Offline { id } } else { Outcome::Error { id } }
        }
    }
}

fn render(data_dir: &Path, fa: &dyn FileAccess, item: &Item) -> Result<Outcome, String> {
    let mut bytes = Vec::new();
    fa.open(&item.abs).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(|e| e.to_string())?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    let (w, h) = img.dimensions();

    let dhash = hash_i64(&HasherConfig::new().hash_alg(HashAlg::Gradient).hash_size(HASH_SIDE, HASH_SIDE).to_hasher(), &img);
    let phash =
        hash_i64(&HasherConfig::new().hash_alg(HashAlg::Mean).hash_size(HASH_SIDE, HASH_SIDE).preproc_dct().to_hasher(), &img);

    let webp = encode_thumb(img.into_rgb8(), THUMB_GRID as u32)?;
    let blur = crate::smart::blur::blur_of_thumbnail(&webp);
    write_atomic(&paths::thumb_path(data_dir, item.id, THUMB_GRID), &webp).map_err(|e| e.to_string())?;
    // A new grid thumbnail means the file changed: a cached preview is stale.
    let _ = std::fs::remove_file(paths::thumb_path(data_dir, item.id, THUMB_PREVIEW));

    // Reading a cloud-only file (explicit download) hydrates it: record the state it has now.
    let cloud = std::fs::symlink_metadata(&item.abs).map(|m| fa.cloud_state(&item.abs, &m)).unwrap_or(CloudState::Unknown);
    Ok(Outcome::Ok { id: item.id, width: w as i64, height: h as i64, hash, dhash, phash, cloud, blur })
}

fn hash_i64(hasher: &image_hasher::Hasher, img: &image::DynamicImage) -> Option<i64> {
    let bytes: [u8; 8] = hasher.hash_image(img).as_bytes().try_into().ok()?;
    Some(i64::from_be_bytes(bytes))
}

/// Size whose long edge is `target`, keeping the aspect ratio; never larger than the source.
pub fn fit_long_edge(w: u32, h: u32, target: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= target || long == 0 {
        return (w.max(1), h.max(1));
    }
    let scale = |v: u32| ((v as u64 * target as u64 + long as u64 / 2) / long as u64).max(1) as u32;
    (scale(w), scale(h))
}

/// Lossy WebP q80 of `rgb`, long edge `target` (never upscaled).
fn encode_thumb(rgb: image::RgbImage, target: u32) -> Result<Vec<u8>, String> {
    let (w, h) = rgb.dimensions();
    let (dw, dh) = fit_long_edge(w, h, target);
    let pixels = if (dw, dh) == (w, h) {
        rgb.into_raw()
    } else {
        let src = FirImage::from_vec_u8(w, h, rgb.into_raw(), PixelType::U8x3).map_err(|e| e.to_string())?;
        let mut dst = FirImage::new(dw, dh, PixelType::U8x3);
        Resizer::new().resize(&src, &mut dst, None).map_err(|e| e.to_string())?;
        dst.into_vec()
    };
    let mem = webp::Encoder::from_rgb(&pixels, dw, dh)
        .encode_simple(false, WEBP_QUALITY)
        .map_err(|e| format!("webp encode: {e:?}"))?;
    Ok(mem.to_vec())
}

/// The 1280 preview of one image, written to its cache path and returned (`ssimg`, first request).
/// The caller decides whether the original may be opened (never for a cloud-only file).
pub fn render_preview(data_dir: &Path, fa: &dyn FileAccess, id: i64, abs: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fa.open(abs).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    let webp = encode_thumb(img.into_rgb8(), THUMB_PREVIEW as u32)?;
    write_atomic(&paths::thumb_path(data_dir, id, THUMB_PREVIEW), &webp).map_err(|e| e.to_string())?;
    Ok(webp)
}

/// Thumbnails (and hashes, dimensions, blur score) of the `pending` rows among `ids`, on the calling thread.
/// `allow_cloud`: an explicit user action on these images may open a cloud-only file (rule 9).
pub fn thumb_ids(cat: &Catalog, fa: &dyn FileAccess, ids: &[i64], allow_cloud: bool) -> Result<ThumbReport, AppError> {
    if ids.is_empty() {
        return Ok(ThumbReport::default());
    }
    let scope = ThumbScope { root_id: None, allow_cloud, ids: Some(ids.to_vec()) };
    thumb_pass(cat, fa, &scope, &AtomicBool::new(false), &|_, _, _| {}, &|_| {})
}

/// Deletes both cached thumbnails of an image (best effort: they are a cache).
pub fn remove_thumbs(data_dir: &Path, id: i64) {
    for w in THUMB_WIDTHS {
        let p = paths::thumb_path(data_dir, id, w);
        if let Err(e) = std::fs::remove_file(&p) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("ss-core: could not remove thumbnail {}: {e}", p.display());
            }
        }
    }
}

/// Copies the cached thumbnails of `src` to `dst` (a copied file). True when the 320 one was copied.
pub fn copy_thumbs(data_dir: &Path, src: i64, dst: i64) -> bool {
    let mut grid = false;
    for w in THUMB_WIDTHS {
        let (from, to) = (paths::thumb_path(data_dir, src, w), paths::thumb_path(data_dir, dst, w));
        let _ = std::fs::remove_file(&to);
        if !from.is_file() {
            continue;
        }
        let ok = to.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::copy(&from, &to)).is_ok();
        grid |= ok && w == THUMB_GRID;
    }
    grid
}

/// Format migration (Deferred decision #29): when `<data>\thumbs\FORMAT` is absent or names another
/// format, deletes every thumbnail file, marks every `ok` thumbnail `pending`, then writes the
/// marker, so the next thumbnail pass regenerates them once. Returns whether it migrated.
pub fn ensure_format(cat: &Catalog) -> Result<bool, AppError> {
    let dir = cat.data_dir().join(paths::THUMBS_DIR);
    let marker = dir.join(FORMAT_FILE);
    if std::fs::read_to_string(&marker).is_ok_and(|s| s.trim() == THUMB_FORMAT) {
        return Ok(false);
    }
    if dir.is_dir() {
        for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() && entry.file_name() != FORMAT_FILE {
                if let Err(e) = std::fs::remove_file(entry.path()) {
                    eprintln!("ss-core: could not remove old thumbnail {}: {e}", entry.path().display());
                }
            }
        }
    }
    cat.write(|tx| {
        tx.execute("UPDATE image SET thumb_state = 'pending' WHERE thumb_state = 'ok'", [])?;
        Ok(())
    })?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&marker, THUMB_FORMAT)?;
    Ok(true)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static TMP_SEQ: AtomicI64 = AtomicI64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}-{}.tmp", std::process::id(), TMP_SEQ.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
