//! Thumbnail pass: for every `pending` image, ONE read + decode yields blake3, dHash, pHash, the real
//! dimensions and the two WebP thumbnails (long edge 320 and 1280, never upscaled). Runs on a rayon
//! pool of `available_parallelism() - 1` threads; results are written in batches of
//! [`WRITE_BATCH`] rows. Resumable: the state lives in `image.thumb_state`, so a cancelled or
//! crashed pass continues with the rows still `pending`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{PixelType, Resizer};
use image::{ExtendedColorType, GenericImageView};
use image_hasher::{HashAlg, HasherConfig};
use rayon::prelude::*;
use rusqlite::params;
use serde::Serialize;

use crate::catalog::{Catalog, WRITE_BATCH};
use crate::contract::AppError;
use crate::contract::ipc::{CloudState, ssimg::THUMB_WIDTHS};
use crate::errors::{enum_str, internal};
use crate::fsaccess::FileAccess;
use crate::paths;
use crate::scan::{Progress, cancelled};

/// Perceptual hash side (8 x 8 = 64 bits, stored as signed i64).
const HASH_SIDE: u32 = 8;

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
    Ok { id: i64, width: i64, height: i64, hash: String, dhash: Option<i64>, phash: Option<i64>, cloud: CloudState },
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
            Outcome::Ok { id, width, height, hash, dhash, phash, cloud } => {
                tx.prepare_cached(
                    "UPDATE image SET width = ?2, height = ?3, content_hash = ?4, dhash = ?5, phash = ?6,
                        thumb_state = 'ok', cloud_state = ?7 WHERE id = ?1",
                )?
                .execute(params![id, width, height, hash, dhash, phash, enum_str(&cloud)])?;
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
            Outcome::Error { id }
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

    let rgba = img.into_rgba8();
    let src = FirImage::from_vec_u8(w, h, rgba.into_raw(), PixelType::U8x4).map_err(|e| e.to_string())?;
    let mut resizer = Resizer::new();
    for width in THUMB_WIDTHS {
        let (dw, dh) = fit_long_edge(w, h, width as u32);
        let pixels = if (dw, dh) == (w, h) {
            src.buffer().to_vec()
        } else {
            let mut dst = FirImage::new(dw, dh, PixelType::U8x4);
            resizer.resize(&src, &mut dst, None).map_err(|e| e.to_string())?;
            dst.into_vec()
        };
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .encode(&pixels, dw, dh, ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?;
        write_atomic(&paths::thumb_path(data_dir, item.id, width), &webp).map_err(|e| e.to_string())?;
    }

    // Reading a cloud-only file (explicit download) hydrates it: record the state it has now.
    let cloud = std::fs::symlink_metadata(&item.abs).map(|m| fa.cloud_state(&item.abs, &m)).unwrap_or(CloudState::Unknown);
    Ok(Outcome::Ok { id: item.id, width: w as i64, height: h as i64, hash, dhash, phash, cloud })
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

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("webp.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
