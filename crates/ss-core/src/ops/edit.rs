//! Edit save, convert and stitch (phase 4.1).
//!
//! - **copy** output: a new file next to the original (`<stem> (edited).<ext>`, `<stem> (stitched).png`,
//!   or `<stem>.<newext>` / `<stem> (converted).<ext>` for a convert), a new catalog row, journal op `copy`
//!   (`convert` for a convert); undo deletes the copy when it is unchanged.
//! - **overwrite** output: the original is copied to `<data>\edit-backups\` and flushed to disk BEFORE the
//!   new bytes replace it (temp file + rename); a new format renames the file to its new extension. The row
//!   keeps its id, tags, albums and notes; its AI rows (OCR, embedding, categories) describe the old pixels,
//!   so they are kept in the journal payload and dropped. Journal op `editOverwrite` / `convert`; undo checks
//!   the edited file is unchanged (blake3), restores the backup and the row, then deletes the backup.
//!
//! Rule 8: every write is guarded with [`guard_file_write`], so `Caller::Cli` (`ssctl edit`) is refused under a
//! protected root or a OneDrive path; the UI may overwrite such an image only through this journaled path.
//! Rule 9: these are explicit user actions on named images, so a cloud-only original may be opened.

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use crate::catalog::{self, Catalog};
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller, ErrorKind};
use crate::errors::{internal, invalid, not_found, now_ms, system_time_ms};
use crate::fsaccess::FileAccess;
use crate::{paths, query, thumbs};

use super::files::{catalog_names, check_file_name, split_ext};
use super::fsio::{self, FileAction};
use super::journal::{AfterCommit, CopyPayload, DbStep, Entry, Step, commit_steps};
use super::organize::fts_set;
use super::{failure, guard_file_write, snapshot, unique_ids};

/// JPEG / lossy WebP quality when the request gives none. WebP at 100 is lossless.
pub const QUALITY_DEFAULT: i64 = 90;
/// Smallest `maxEdge` of a convert.
pub const MAX_EDGE_MIN: i64 = 16;
/// Largest stitch gap, in pixels.
pub const STITCH_GAP_MAX: i64 = 10_000;
/// Most images one stitch takes.
pub const STITCH_MAX_IMAGES: usize = 100;
/// Largest output of an edit, convert or stitch (pixels): 200 MP is ~800 MB as RGBA while composing.
pub const MAX_OUTPUT_PIXELS: u64 = 200_000_000;
const EDITED_LABEL: &str = "edited";
const CONVERTED_LABEL: &str = "converted";
const STITCHED_LABEL: &str = "stitched";
const NAME_SUFFIX_MAX: u32 = 9_999;

/// Decoded `save_edited_image` request (raw PNG body + `x-*` headers).
#[derive(Debug, Clone)]
pub struct SaveEditedImage {
    pub image_id: i64,
    pub mode: EditMode,
    pub format: ImageFormat,
    pub quality: Option<i64>,
    pub png: Vec<u8>,
}

/// A crop rectangle in image pixels (`ssctl edit --crop x,y,w,h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// What an edit or stitch produced: the saved image's row and its journal batch (`ssctl edit` prints it).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOutcome {
    pub image: ImageSummary,
    pub batch_id: Option<i64>,
}

// --- journal payloads ------------------------------------------------------------------------

/// Everything the undo of an overwrite needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverwritePayload {
    pub orig_abs: String,
    pub new_abs: String,
    pub backup: String,
    /// blake3 of the bytes written: undo refuses when the file changed since.
    pub new_hash: String,
    pub old_name: String,
    pub old_width: Option<i64>,
    pub old_height: Option<i64>,
    pub old_dominant_rgb: Option<String>,
    /// The AI rows of the old pixels ([`snapshot::AI_TABLES`]), filled by the DB step.
    pub ai: BTreeMap<String, Vec<Map<String, Json>>>,
}

/// Payload of a `convert` journal row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "output", rename_all = "camelCase")]
pub(crate) enum ConvertPayload {
    Copy(CopyPayload),
    Overwrite(OverwritePayload),
}

// --- source rows -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Src {
    id: i64,
    root_id: i64,
    root_path: String,
    folder: String,
    file_name: String,
    taken_ms: i64,
    width: Option<i64>,
    height: Option<i64>,
    dominant_rgb: Option<String>,
    missing: bool,
}

impl Src {
    fn abs(&self) -> PathBuf {
        paths::image_abs_path(&self.root_path, &self.folder, &self.file_name)
    }
    fn dir(&self) -> PathBuf {
        self.abs().parent().map(Path::to_path_buf).unwrap_or_default()
    }
}

fn load_src(cat: &Catalog, id: i64) -> Result<Src, AppError> {
    cat.read(|c| {
        c.query_row(
            "SELECT i.id, i.root_id, r.path, i.folder, i.file_name, i.taken_ms, i.width, i.height, i.dominant_rgb, i.missing
             FROM image i JOIN library_root r ON r.id = i.root_id WHERE i.id = ?1",
            [id],
            |r| {
                Ok(Src {
                    id: r.get(0)?,
                    root_id: r.get(1)?,
                    root_path: r.get(2)?,
                    folder: r.get(3)?,
                    file_name: r.get(4)?,
                    taken_ms: r.get(5)?,
                    width: r.get(6)?,
                    height: r.get(7)?,
                    dominant_rgb: r.get(8)?,
                    missing: r.get::<_, i64>(9)? != 0,
                })
            },
        )
        .optional()?
        .ok_or_else(|| not_found(format!("image {id} not found")))
    })
}

fn summary_of(cat: &Catalog, id: i64) -> Result<ImageSummary, AppError> {
    cat.read(|c| query::summary(c, id))?.ok_or_else(|| not_found(format!("image {id} not found")))
}

// --- decode / encode -------------------------------------------------------------------------

/// An encoded output file.
pub(crate) struct Encoded {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
    pub width: u32,
    pub height: u32,
}

pub fn format_ext(f: ImageFormat) -> &'static str {
    match f {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
    }
}

/// The output format of a file name's extension (GIF and BMP are written as PNG).
pub fn format_of_name(name: &str) -> ImageFormat {
    match split_ext(name).1.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::Webp,
        _ => ImageFormat::Png,
    }
}

fn same_ext(a: &str, b: &str) -> bool {
    let norm = |e: &str| match e.to_ascii_lowercase().as_str() {
        "jpeg" => "jpg".to_owned(),
        other => other.to_owned(),
    };
    norm(a) == norm(b)
}

fn quality(q: Option<i64>) -> Result<u8, AppError> {
    let q = q.unwrap_or(QUALITY_DEFAULT);
    if !(1..=100).contains(&q) {
        return Err(invalid(format!("quality must be 1..100, got {q}")));
    }
    Ok(q as u8)
}

fn check_pixels(w: u64, h: u64) -> Result<(), AppError> {
    if w == 0 || h == 0 || w * h > MAX_OUTPUT_PIXELS {
        return Err(invalid(format!("output size {w}x{h} is outside 1..{MAX_OUTPUT_PIXELS} pixels")));
    }
    Ok(())
}

fn image_err(what: &str, e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorKind::Io, what.to_owned()).with_detail(e.to_string())
}

/// Reads and decodes an original (an explicit user action: may hydrate a cloud-only file, rule 9).
pub(crate) fn decode_file(fa: &dyn FileAccess, path: &Path) -> Result<DynamicImage, AppError> {
    let mut bytes = Vec::new();
    fa.open(path).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(AppError::from)?;
    image::load_from_memory(&bytes).map_err(|e| image_err(&format!("cannot decode {}", path.display()), e))
}

/// Alpha composited over white (JPEG has no alpha channel).
fn flatten_on_white(img: &DynamicImage) -> image::RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let Rgba([r, g, b, a]) = *rgba.get_pixel(x, y);
        let mix = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32) + 127) / 255) as u8;
        image::Rgb([mix(r), mix(g), mix(b)])
    })
}

pub(crate) fn encode(img: &DynamicImage, format: ImageFormat, q: Option<i64>) -> Result<Encoded, AppError> {
    let (width, height) = img.dimensions();
    check_pixels(width as u64, height as u64)?;
    let bytes = match format {
        ImageFormat::Png => {
            let mut buf = Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| image_err("PNG encode failed", e))?;
            buf.into_inner()
        }
        ImageFormat::Jpeg => {
            let q = quality(q)?;
            let mut out = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q)
                .encode_image(&flatten_on_white(img))
                .map_err(|e| image_err("JPEG encode failed", e))?;
            out
        }
        ImageFormat::Webp => {
            let q = quality(q)?;
            let rgba = img.to_rgba8();
            webp::Encoder::from_rgba(&rgba, width, height)
                .encode_simple(q == 100, q as f32)
                .map_err(|e| image_err("WebP encode failed", format!("{e:?}")))?
                .to_vec()
        }
    };
    Ok(Encoded { bytes, ext: format_ext(format), width, height })
}

// --- names and rows --------------------------------------------------------------------------

/// A free name in `dir` (on disk and in the catalog's `taken`, lower case), reserved in `taken`:
/// `<stem>.<ext>` first when `plain_first`, then `<stem> (<label>).<ext>`, `<stem> (<label> 2).<ext>`, ...
fn pick_name(dir: &Path, taken: &mut HashSet<String>, stem: &str, label: &str, ext: &str, plain_first: bool) -> Result<String, AppError> {
    let plain = plain_first.then(|| format!("{stem}.{ext}"));
    let labelled = std::iter::once(format!("{stem} ({label}).{ext}"))
        .chain((2..=NAME_SUFFIX_MAX).map(|k| format!("{stem} ({label} {k}).{ext}")));
    let name = plain
        .into_iter()
        .chain(labelled)
        .find(|n| !taken.contains(&n.to_lowercase()) && !dir.join(n).exists())
        .ok_or_else(|| AppError::new(ErrorKind::Conflict, format!("no free name for `{stem}` in {}", dir.display())))?;
    check_file_name(&name)?;
    taken.insert(name.to_lowercase());
    Ok(name)
}

/// Inserts the row of a file this module wrote next to `src` (its taken time is the source's, so the
/// result sorts beside it); thumbnail `pending` until [`thumbs::thumb_ids`] runs after the commit.
fn insert_new_row(tx: &Transaction<'_>, src: &Src, name: &str, file: &FileFacts, enc: (u32, u32)) -> Result<i64, AppError> {
    tx.execute(
        "INSERT INTO image (root_id, folder, file_name, size_bytes, mtime_ms, created_ms, taken_ms, width, height,
                            thumb_state, cloud_state, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', 'local', ?10)",
        params![src.root_id, src.folder, name, file.size, file.mtime_ms, file.created_ms, src.taken_ms, enc.0, enc.1, now_ms()],
    )?;
    let id = tx.last_insert_rowid();
    catalog::fts_insert(tx, id, name)?;
    Ok(id)
}

struct FileFacts {
    size: i64,
    mtime_ms: i64,
    created_ms: Option<i64>,
}

fn facts(path: &Path) -> Result<FileFacts, AppError> {
    let m = std::fs::metadata(path)?;
    Ok(FileFacts {
        size: m.len() as i64,
        mtime_ms: m.modified().ok().and_then(system_time_ms).unwrap_or(0),
        created_ms: m.created().ok().and_then(system_time_ms),
    })
}

fn backup_path(data_dir: &Path, id: i64, ext: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    paths::edit_backups_dir(data_dir).join(format!("{id}-{}-{}-{seq}.{ext}", now_ms(), std::process::id()))
}

/// Ids whose thumbnails must be (re)generated after the commit.
type Touched = Arc<Mutex<Vec<i64>>>;

/// Everything a batch of edit steps shares.
struct Ctx<'a> {
    caller: Caller,
    roots: Vec<LibraryRoot>,
    data_dir: PathBuf,
    cat: &'a Catalog,
    touched: Touched,
    /// Lower-case names reserved per (root, folder) while this batch picks names.
    taken: std::collections::HashMap<(i64, String), HashSet<String>>,
}

impl<'a> Ctx<'a> {
    fn new(caller: Caller, cat: &'a Catalog) -> Result<Self, AppError> {
        Ok(Self {
            caller,
            roots: cat.read(catalog::list_roots)?,
            data_dir: cat.data_dir().to_path_buf(),
            cat,
            touched: Arc::default(),
            taken: Default::default(),
        })
    }

    fn taken_for(&mut self, src: &Src) -> Result<&mut HashSet<String>, AppError> {
        let key = (src.root_id, src.folder.clone());
        if !self.taken.contains_key(&key) {
            let names = catalog_names(self.cat, src.root_id, &src.folder)?;
            self.taken.insert(key.clone(), names);
        }
        Ok(self.taken.get_mut(&key).expect("inserted above"))
    }

    fn touched_ids(&self) -> Vec<i64> {
        self.touched.lock().expect("touched ids").clone()
    }
}

/// Writes `enc` as a new file next to `src` and returns its step (journal `op`: `copy` or `convert`).
fn copy_step(ctx: &mut Ctx<'_>, src: Src, enc: Encoded, label: &str, plain_first: bool, op: JournalOp) -> Result<Step, AppError> {
    let dir = src.dir();
    let (stem, _) = split_ext(&src.file_name);
    let stem = stem.to_owned();
    let name = pick_name(&dir, ctx.taken_for(&src)?, &stem, label, enc.ext, plain_first)?;
    let to = dir.join(&name);
    guard_file_write(ctx.caller, &ctx.roots, &[&src.abs(), &to])?;
    fsio::create_new_with(&to, &enc.bytes)?;
    let file = match facts(&to) {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(&to);
            return Err(e);
        }
    };
    let touched = ctx.touched.clone();
    let id = src.id;
    let dims = (enc.width, enc.height);
    let to_abs = to.to_string_lossy().into_owned();
    let db: DbStep = Box::new(move |tx: &Transaction<'_>| {
        let new_id = insert_new_row(tx, &src, &name, &file, dims)?;
        touched.lock().expect("touched ids").push(new_id);
        let p = CopyPayload { to_abs, new_image_id: Some(new_id), size_bytes: file.size, mtime_ms: file.mtime_ms };
        let entry = match op {
            JournalOp::Convert => Entry::new(op, id, ConvertPayload::Copy(p)),
            _ => Entry::new(JournalOp::Copy, id, p),
        };
        Ok((entry, None))
    });
    Ok(Step { image_id: id, action: FileAction::Copied { to }, db })
}

/// Backs the original up (flushed to disk), then replaces it with `enc` (a new extension renames the file);
/// returns its step (journal `op`: `editOverwrite` or `convert`).
fn overwrite_step(ctx: &mut Ctx<'_>, src: Src, enc: Encoded, op: JournalOp) -> Result<Step, AppError> {
    let orig = src.abs();
    if src.missing || !orig.is_file() {
        return Err(not_found(format!("file not found: {}", orig.display())));
    }
    let (stem, old_ext) = split_ext(&src.file_name);
    let (stem, old_ext) = (stem.to_owned(), old_ext.to_owned());
    let new_name = if same_ext(&old_ext, enc.ext) {
        src.file_name.clone()
    } else {
        let dir = src.dir();
        pick_name(&dir, ctx.taken_for(&src)?, &stem, CONVERTED_LABEL, enc.ext, true)?
    };
    let new = src.dir().join(&new_name);
    guard_file_write(ctx.caller, &ctx.roots, &[&orig, &new])?;
    let backup = backup_path(&ctx.data_dir, src.id, if old_ext.is_empty() { "bin" } else { &old_ext });
    fsio::backup_copy(&orig, &backup)?;
    let written = if new == orig {
        fsio::replace_with(&orig, &enc.bytes)
    } else {
        fsio::create_new_with(&new, &enc.bytes).and_then(|_| {
            std::fs::remove_file(&orig).inspect_err(|_| {
                let _ = std::fs::remove_file(&new);
            })
        })
    };
    if let Err(e) = written {
        let _ = std::fs::remove_file(&backup);
        return Err(e.into());
    }
    let action = FileAction::Overwritten { orig: orig.clone(), new: new.clone(), backup: backup.clone() };
    let file = match facts(&new) {
        Ok(f) => f,
        Err(e) => {
            action.rollback();
            return Err(e);
        }
    };
    let mut payload = OverwritePayload {
        orig_abs: orig.to_string_lossy().into_owned(),
        new_abs: new.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        new_hash: blake3::hash(&enc.bytes).to_hex().to_string(),
        old_name: src.file_name.clone(),
        old_width: src.width,
        old_height: src.height,
        old_dominant_rgb: src.dominant_rgb.clone(),
        ai: BTreeMap::new(),
    };
    let (id, data_dir, touched) = (src.id, ctx.data_dir.clone(), ctx.touched.clone());
    let dims = (enc.width, enc.height);
    let db: DbStep = Box::new(move |tx: &Transaction<'_>| {
        payload.ai = snapshot::take_ai(tx, id)?;
        tx.execute(
            "UPDATE image SET file_name = ?2, size_bytes = ?3, mtime_ms = ?4, width = ?5, height = ?6,
                content_hash = NULL, dhash = NULL, phash = NULL, blur_score = NULL, dominant_rgb = NULL,
                thumb_state = 'pending', cloud_state = 'local', missing = 0
             WHERE id = ?1",
            params![id, new_name, file.size, file.mtime_ms, dims.0, dims.1],
        )?;
        for table in snapshot::AI_TABLES {
            tx.execute(&format!("DELETE FROM {table} WHERE image_id = ?1"), [id])?;
        }
        fts_set(tx, id, "file_name", &new_name)?;
        fts_set(tx, id, "ocr_text", "")?;
        touched.lock().expect("touched ids").push(id);
        let entry = match op {
            JournalOp::Convert => Entry::new(op, id, ConvertPayload::Overwrite(payload)),
            _ => Entry::new(JournalOp::EditOverwrite, id, payload),
        };
        let after: AfterCommit = Box::new(move |_| thumbs::remove_thumbs(&data_dir, id));
        Ok((entry, Some(after)))
    });
    Ok(Step { image_id: id, action, db })
}

/// Commits `steps`, then makes the thumbnails (and hashes) of every new or overwritten row.
fn commit_and_thumb(ctx: &Ctx<'_>, fa: &dyn FileAccess, steps: Vec<Step>, failed: Vec<OpFailure>) -> Result<OpResult, AppError> {
    let (r, _) = commit_steps(ctx.cat, steps, failed)?;
    let ids = ctx.touched_ids();
    if let Err(e) = thumbs::thumb_ids(ctx.cat, fa, &ids, true) {
        eprintln!("ss-core: thumbnails after an edit: {}: {}", e.message, e.detail.unwrap_or_default());
    }
    Ok(r)
}

/// One-image edits: commit, then the saved row (the copy, or the overwritten original).
fn finish_single(ctx: &Ctx<'_>, fa: &dyn FileAccess, step: Step) -> Result<EditOutcome, AppError> {
    let r = commit_and_thumb(ctx, fa, vec![step], Vec::new())?;
    if let Some(f) = r.failed.first() {
        return Err(AppError::new(f.kind, f.message.clone()));
    }
    let id = *ctx.touched_ids().last().ok_or_else(|| internal("the edit saved no row"))?;
    Ok(EditOutcome { image: summary_of(ctx.cat, id)?, batch_id: r.batch_id })
}

// --- public operations -----------------------------------------------------------------------

/// `save_edited_image`: the UI's PNG bytes saved as a copy or over the original.
pub fn save_edited_image(caller: Caller, cat: &Catalog, fa: &dyn FileAccess, req: SaveEditedImage) -> Result<EditOutcome, AppError> {
    let img = image::load_from_memory_with_format(&req.png, image::ImageFormat::Png)
        .map_err(|e| invalid("the edited image is not a PNG").with_detail(e.to_string()))?;
    let enc = match req.format {
        // The UI's own PNG bytes are kept as they are.
        ImageFormat::Png => {
            let (width, height) = img.dimensions();
            check_pixels(width as u64, height as u64)?;
            Encoded { bytes: req.png, ext: "png", width, height }
        }
        f => encode(&img, f, req.quality)?,
    };
    save_encoded(caller, cat, fa, req.image_id, enc, req.mode)
}

/// `ssctl edit <id> --crop x,y,w,h`: crops the original and saves it in its own format.
pub fn crop_image(caller: Caller, cat: &Catalog, fa: &dyn FileAccess, image_id: i64, crop: CropRect, mode: EditMode) -> Result<EditOutcome, AppError> {
    let src = load_src(cat, image_id)?;
    // Guard before opening anything: a refused CLI edit never reads the original either.
    let roots = cat.read(catalog::list_roots)?;
    guard_file_write(caller, &roots, &[&src.abs()])?;
    let img = decode_file(fa, &src.abs())?;
    let (w, h) = img.dimensions();
    let CropRect { x, y, width, height } = crop;
    let inside = x >= 0 && y >= 0 && width > 0 && height > 0 && x + width <= w as i64 && y + height <= h as i64;
    if !inside {
        return Err(invalid(format!("crop {x},{y},{width},{height} is not inside the {w}x{h} image")));
    }
    let cropped = img.crop_imm(x as u32, y as u32, width as u32, height as u32);
    let enc = encode(&cropped, format_of_name(&src.file_name), None)?;
    save_encoded(caller, cat, fa, image_id, enc, mode)
}

fn save_encoded(caller: Caller, cat: &Catalog, fa: &dyn FileAccess, image_id: i64, enc: Encoded, mode: EditMode) -> Result<EditOutcome, AppError> {
    let src = load_src(cat, image_id)?;
    let mut ctx = Ctx::new(caller, cat)?;
    let step = match mode {
        EditMode::Copy => {
            guard_file_write(caller, &ctx.roots, &[&src.abs()])?;
            copy_step(&mut ctx, src, enc, EDITED_LABEL, false, JournalOp::Copy)?
        }
        EditMode::Overwrite => overwrite_step(&mut ctx, src, enc, JournalOp::EditOverwrite)?,
    };
    finish_single(&ctx, fa, step)
}

/// `convert_images`: re-encode (and shrink to `maxEdge`) each image; one `convert` batch.
pub fn convert_images(caller: Caller, cat: &Catalog, fa: &dyn FileAccess, req: ConvertImagesReq) -> Result<OpResult, AppError> {
    if let Some(m) = req.max_edge
        && m < MAX_EDGE_MIN
    {
        return Err(invalid(format!("maxEdge must be >= {MAX_EDGE_MIN}, got {m}")));
    }
    quality(req.quality)?;
    let mut ctx = Ctx::new(caller, cat)?;
    let (mut steps, mut failed) = (Vec::new(), Vec::new());
    for id in unique_ids(&req.image_ids) {
        let one = (|| -> Result<Step, AppError> {
            let src = load_src(cat, id)?;
            guard_file_write(caller, &ctx.roots, &[&src.abs()])?;
            if src.missing {
                return Err(not_found(format!("file not found: {}", src.abs().display())));
            }
            let img = decode_file(fa, &src.abs())?;
            let img = match req.max_edge {
                Some(m) => {
                    let (w, h) = img.dimensions();
                    let (dw, dh) = thumbs::fit_long_edge(w, h, m.min(u32::MAX as i64) as u32);
                    if (dw, dh) == (w, h) { img } else { img.resize_exact(dw, dh, image::imageops::FilterType::Lanczos3) }
                }
                None => img,
            };
            let enc = encode(&img, req.format, req.quality)?;
            match req.output {
                EditMode::Copy => copy_step(&mut ctx, src, enc, CONVERTED_LABEL, true, JournalOp::Convert),
                EditMode::Overwrite => overwrite_step(&mut ctx, src, enc, JournalOp::Convert),
            }
        })();
        match one {
            Ok(s) => steps.push(s),
            Err(e) => failed.push(failure(id, &e)),
        }
    }
    commit_and_thumb(&ctx, fa, steps, failed)
}

/// Parses `#RRGGBB`, `#RRGGBBAA` or `transparent`.
fn parse_color(s: &str) -> Result<Rgba<u8>, AppError> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("transparent") {
        return Ok(Rgba([0, 0, 0, 0]));
    }
    let hex = t.strip_prefix('#').filter(|h| (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit()));
    let hex = hex.ok_or_else(|| invalid(format!("background must be #RRGGBB, #RRGGBBAA or transparent, got `{s}`")))?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("checked hex");
    Ok(Rgba([byte(0), byte(2), byte(4), if hex.len() == 8 { byte(6) } else { 255 }]))
}

/// Size of a stitch of images of `dims` (order kept): the long axis sums the images plus the gaps, the
/// other axis is the largest image.
pub fn stitch_size(dims: &[(u32, u32)], direction: StitchDirection, gap: u32) -> (u64, u64) {
    let gaps = gap as u64 * dims.len().saturating_sub(1) as u64;
    let (sum_w, sum_h) = dims.iter().fold((0u64, 0u64), |(w, h), d| (w + d.0 as u64, h + d.1 as u64));
    let (max_w, max_h) = dims.iter().fold((0u64, 0u64), |(w, h), d| (w.max(d.0 as u64), h.max(d.1 as u64)));
    match direction {
        StitchDirection::Vertical => (max_w, sum_h + gaps),
        StitchDirection::Horizontal => (sum_w + gaps, max_h),
    }
}

/// `stitch_images`: the images in request order, centred on the cross axis over `background`, saved as a
/// PNG next to the first image (`<stem> (stitched).png`); journal op `copy` (undo deletes it).
pub fn stitch_images(caller: Caller, cat: &Catalog, fa: &dyn FileAccess, req: StitchImagesReq) -> Result<EditOutcome, AppError> {
    let ids = unique_ids(&req.image_ids);
    if ids.len() < 2 || ids.len() > STITCH_MAX_IMAGES {
        return Err(invalid(format!("stitch needs 2..{STITCH_MAX_IMAGES} different images, got {}", ids.len())));
    }
    if !(0..=STITCH_GAP_MAX).contains(&req.gap) {
        return Err(invalid(format!("gap must be 0..{STITCH_GAP_MAX}, got {}", req.gap)));
    }
    let background = parse_color(&req.background)?;
    let srcs = ids.iter().map(|id| load_src(cat, *id)).collect::<Result<Vec<_>, _>>()?;
    let roots = cat.read(catalog::list_roots)?;
    guard_file_write(caller, &roots, &[&srcs[0].abs()])?;
    let images = srcs.iter().map(|s| decode_file(fa, &s.abs()).map(|i| i.to_rgba8())).collect::<Result<Vec<_>, _>>()?;
    let dims: Vec<(u32, u32)> = images.iter().map(|i| i.dimensions()).collect();
    let gap = req.gap as u32;
    let (w, h) = stitch_size(&dims, req.direction, gap);
    check_pixels(w, h)?;
    let mut canvas = RgbaImage::from_pixel(w as u32, h as u32, background);
    let mut offset = 0u32;
    for img in &images {
        let (iw, ih) = img.dimensions();
        let (x, y) = match req.direction {
            StitchDirection::Vertical => ((w as u32 - iw) / 2, offset),
            StitchDirection::Horizontal => (offset, (h as u32 - ih) / 2),
        };
        image::imageops::overlay(&mut canvas, img, x as i64, y as i64);
        offset += gap + if req.direction == StitchDirection::Vertical { ih } else { iw };
    }
    drop(images);
    let enc = encode(&DynamicImage::ImageRgba8(canvas), ImageFormat::Png, None)?;
    let mut ctx = Ctx::new(caller, cat)?;
    let first = srcs.into_iter().next().expect("two or more images");
    let step = copy_step(&mut ctx, first, enc, STITCHED_LABEL, false, JournalOp::Copy)?;
    finish_single(&ctx, fa, step)
}

// --- undo (called by `journal::undo`) --------------------------------------------------------

fn conflict(msg: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::Conflict, msg)
}

/// File step of undoing an overwrite: the edited file must be the one written, then the backup goes back.
pub(super) fn overwrite_file_undo(caller: Caller, roots: &[LibraryRoot], p: &OverwritePayload) -> Result<FileAction, AppError> {
    let (orig, new, backup) = (PathBuf::from(&p.orig_abs), PathBuf::from(&p.new_abs), PathBuf::from(&p.backup));
    guard_file_write(caller, roots, &[&orig, &new])?;
    if !backup.is_file() {
        return Err(not_found(format!("the backup of the original is gone: {}", backup.display())));
    }
    match std::fs::read(&new) {
        Ok(bytes) if blake3::hash(&bytes).to_hex().as_str() == p.new_hash => {}
        Ok(_) => return Err(conflict(format!("{} changed since the edit; not restored", new.display()))),
        Err(e) => return Err(conflict(format!("the edited file {} is gone; not restored", new.display())).with_detail(e.to_string())),
    }
    let renamed = super::norm_path(&orig) != super::norm_path(&new);
    if renamed && orig.exists() {
        return Err(conflict(format!("cannot restore: {} exists again", orig.display())));
    }
    fsio::restore_from_backup(&backup, &orig)?;
    if renamed && let Err(e) = std::fs::remove_file(&new) {
        let _ = std::fs::remove_file(&orig);
        return Err(e.into());
    }
    Ok(FileAction::None)
}

/// DB step of undoing an overwrite: the row's old name, size, dimensions and AI rows.
pub(super) fn overwrite_db_undo(tx: &Transaction<'_>, id: i64, p: &OverwritePayload) -> Result<(), AppError> {
    let file = facts(Path::new(&p.orig_abs))?;
    let n = tx.execute(
        "UPDATE image SET file_name = ?2, size_bytes = ?3, mtime_ms = ?4, width = ?5, height = ?6, dominant_rgb = ?7,
            content_hash = NULL, dhash = NULL, phash = NULL, blur_score = NULL, thumb_state = 'pending', missing = 0
         WHERE id = ?1",
        params![id, p.old_name, file.size, file.mtime_ms, p.old_width, p.old_height, p.old_dominant_rgb],
    )?;
    if n == 0 {
        return Err(not_found(format!("image {id} is no longer in the catalog")));
    }
    snapshot::restore_ai(tx, id, &p.ai)?;
    fts_set(tx, id, "file_name", &p.old_name)?;
    let ocr: Option<String> = tx.query_row("SELECT text FROM ocr_result WHERE image_id = ?1", [id], |r| r.get(0)).optional()?;
    fts_set(tx, id, "ocr_text", &crate::filter::fts_ocr_text(&ocr.unwrap_or_default()))?;
    Ok(())
}
