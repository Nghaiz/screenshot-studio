//! ZIP and PDF export (phase 4.1), run as `export` jobs (contract rule 6).
//!
//! - ZIP: every original, stored (PNG/JPEG/WebP are compressed already), flat names, a repeated name gets
//!   ` (2)`, ` (3)`, ... before its extension.
//! - PDF: `perPage` 1 (one cell), 2 (two rows) or 4 (2 x 2) images per page, in request order, each fitted
//!   into its cell keeping its aspect ratio and centred. `a4`: portrait A4, 10 mm margin, 5 mm gap, images
//!   downscaled (never upscaled) to the cell at [`PDF_A4_DPI`]. `fit`: cells as large as the page's largest
//!   image at screen size (1 px = 0.75 pt, 96 dpi), no margin, so a 1-per-page PDF is each screenshot at
//!   its own size. printpdf keeps every image's raw pixels until it saves, so one PDF takes at most
//!   [`PDF_MAX_IMAGES`] images and fit images are capped at [`PDF_FIT_MAX_EDGE`] px.
//!
//! Both write to a temp file beside `destPath` and rename it into place only when complete; a failed or
//! cancelled export leaves nothing behind. An unreadable image fails the job with its path (no silent skip).
//! The written file is a file write: rule 8 applies to `destPath` (`Caller::Cli` refused under a protected root).

use std::collections::HashSet;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use image::{DynamicImage, GenericImageView};
use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, RawImage, RawImageData, RawImageFormat, XObjectTransform};
use serde::Serialize;

use crate::catalog::{self, Catalog};
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller, ErrorKind};
use crate::errors::{internal, invalid, not_found};
use crate::fsaccess::FileAccess;
use crate::jobs::ProgressReporter;
use crate::ops::{self, ImageRow};
use crate::ops::fsio;
use crate::scan::cancelled;

/// Most images one export takes (ZIP).
pub const ZIP_MAX_IMAGES: usize = 100_000;
/// Most images one PDF takes (printpdf holds all of them in memory until it saves).
pub const PDF_MAX_IMAGES: usize = 500;
/// Resolution images are downscaled to for an A4 page.
pub const PDF_A4_DPI: f32 = 150.0;
/// Long edge cap of an image on a `fit` page.
pub const PDF_FIT_MAX_EDGE: u32 = 4096;
const A4_MM: (f32, f32) = (210.0, 297.0);
const A4_MARGIN_MM: f32 = 10.0;
const A4_GAP_MM: f32 = 5.0;
/// Gap between cells of a `fit` page, in points.
const FIT_GAP_PT: f32 = 12.0;
/// Points per screen pixel on a `fit` page (96 dpi).
const FIT_PT_PER_PX: f32 = 0.75;
const PT_PER_MM: f32 = 72.0 / 25.4;

/// What an export wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub files: i64,
    pub bytes: i64,
    /// PDF pages (0 for a ZIP).
    pub pages: i64,
}

/// Pages of a PDF of `n` images at `per_page` images per page.
pub fn pdf_page_count(n: usize, per_page: usize) -> usize {
    n.div_ceil(per_page.max(1))
}

/// Grid of a page: (columns, rows).
fn grid(per_page: usize) -> (usize, usize) {
    match per_page {
        2 => (1, 2),
        4 => (2, 2),
        _ => (1, 1),
    }
}

/// The checked destination: absolute, its folder exists, not a folder itself.
fn dest_path(caller: Caller, cat: &Catalog, raw: &str) -> Result<PathBuf, AppError> {
    let p = PathBuf::from(raw.trim());
    if !p.is_absolute() || p.file_name().is_none() {
        return Err(invalid(format!("destPath must be an absolute file path: {raw}")));
    }
    if p.is_dir() {
        return Err(invalid(format!("destPath is a folder: {}", p.display())));
    }
    match p.parent() {
        Some(dir) if dir.is_dir() => {}
        _ => return Err(not_found(format!("destination folder not found: {}", p.display()))),
    }
    let roots = cat.read(catalog::list_roots)?;
    ops::guard_file_write(caller, &roots, &[&p])?;
    Ok(p)
}

/// Rows of `ids` in request order (duplicates dropped); an unknown or missing id fails the export.
fn rows_in_order(cat: &Catalog, ids: &[i64], max: usize) -> Result<Vec<ImageRow>, AppError> {
    let ids = ops::unique_ids(ids);
    if ids.is_empty() || ids.len() > max {
        return Err(invalid(format!("an export takes 1..{max} images, got {}", ids.len())));
    }
    let mut rows = cat.read(|c| ops::load_rows(c, &ids))?;
    ids.iter()
        .map(|id| {
            let row = rows.remove(id).ok_or_else(|| not_found(format!("image {id} not found")))?;
            if row.missing {
                return Err(not_found(format!("file not found: {}", row.abs().display())));
            }
            Ok(row)
        })
        .collect()
}

/// Writes through a temp file beside `dest`, renamed into place only when `write` succeeds.
fn write_atomically(dest: &Path, write: impl FnOnce(&Path) -> Result<(), AppError>) -> Result<i64, AppError> {
    let tmp = fsio::tmp_beside(dest);
    let r = write(&tmp).and_then(|_| {
        std::fs::OpenOptions::new().write(true).open(&tmp)?.sync_all()?;
        std::fs::rename(&tmp, dest)?;
        Ok(std::fs::metadata(dest)?.len() as i64)
    });
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

fn read_err(path: &Path, e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorKind::Io, format!("cannot read {}", path.display())).with_detail(e.to_string())
}

/// `name`, or `stem (k).ext` when a name like it is already in `used` (case-insensitive).
fn unique_entry(name: &str, used: &mut HashSet<String>) -> String {
    let (stem, ext) = ops::files::split_ext(name);
    let mut candidate = name.to_owned();
    let mut k = 2;
    while used.contains(&candidate.to_lowercase()) {
        candidate = if ext.is_empty() { format!("{stem} ({k})") } else { format!("{stem} ({k}).{ext}") };
        k += 1;
    }
    used.insert(candidate.to_lowercase());
    candidate
}

pub fn export_zip(
    caller: Caller,
    cat: &Catalog,
    fa: &dyn FileAccess,
    req: &ExportZipReq,
    reporter: &ProgressReporter<'_>,
    cancel: &AtomicBool,
) -> Result<ExportReport, AppError> {
    let dest = dest_path(caller, cat, &req.dest_path)?;
    let rows = rows_in_order(cat, &req.image_ids, ZIP_MAX_IMAGES)?;
    let total = rows.len() as i64;
    let bytes = write_atomically(&dest, |tmp| {
        let file = std::fs::File::create(tmp)?;
        let mut zip = zip::ZipWriter::new(BufWriter::new(file));
        let mut used = HashSet::new();
        for (i, row) in rows.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            let abs = row.abs();
            reporter.report("zip", i as i64, total, abs.to_str());
            let mut src = fa.open(&abs).map_err(|e| read_err(&abs, e))?;
            let size = src.metadata().map(|m| m.len()).unwrap_or(0);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .large_file(size >= u32::MAX as u64);
            let zip_err = |e: zip::result::ZipError| internal("ZIP write failed").with_detail(e.to_string());
            zip.start_file(unique_entry(&row.file_name, &mut used), options).map_err(zip_err)?;
            std::io::copy(&mut src, &mut zip).map_err(|e| read_err(&abs, e))?;
        }
        let mut out = zip.finish().map_err(|e| internal("ZIP write failed").with_detail(e.to_string()))?;
        out.flush()?;
        Ok(())
    })?;
    Ok(ExportReport { files: total, bytes, pages: 0 })
}

/// One image's raw pixels and the size it is drawn at, in points.
struct Placed {
    raw: RawImage,
    w_pt: f32,
    h_pt: f32,
}

fn decode_rgb(fa: &dyn FileAccess, abs: &Path) -> Result<DynamicImage, AppError> {
    let mut bytes = Vec::new();
    fa.open(abs).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(|e| read_err(abs, e))?;
    let img = image::load_from_memory(&bytes).map_err(|e| read_err(abs, e))?;
    drop(bytes);
    // Alpha over white: printpdf's RGB8 path needs no soft mask, and paper is white.
    let rgba = img.to_rgba8();
    let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let image::Rgba([r, g, b, a]) = *rgba.get_pixel(x, y);
        let mix = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32) + 127) / 255) as u8;
        image::Rgb([mix(r), mix(g), mix(b)])
    });
    Ok(DynamicImage::ImageRgb8(rgb))
}

/// Shrinks `img` so its long side is at most `max_w` x `max_h` pixels (never enlarges).
fn shrink_to(img: DynamicImage, max_w: f32, max_h: f32) -> DynamicImage {
    let (w, h) = img.dimensions();
    let s = (max_w / w as f32).min(max_h / h as f32);
    if s >= 1.0 {
        return img;
    }
    let (dw, dh) = (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1));
    img.resize_exact(dw, dh, image::imageops::FilterType::Lanczos3)
}

fn raw_of(img: DynamicImage) -> RawImage {
    let (w, h) = img.dimensions();
    RawImage {
        pixels: RawImageData::U8(img.into_rgb8().into_raw()),
        width: w as usize,
        height: h as usize,
        data_format: RawImageFormat::RGB8,
        tag: Vec::new(),
    }
}

/// Page layout: page size and cell size in points, margin and gap in points.
struct Layout {
    page_w: f32,
    page_h: f32,
    cell_w: f32,
    cell_h: f32,
    margin: f32,
    gap: f32,
}

fn layout(page_size: PdfPageSize, per_page: usize, images: &[(u32, u32)]) -> Layout {
    let (cols, rows) = grid(per_page);
    match page_size {
        PdfPageSize::A4 => {
            let (pw, ph) = (A4_MM.0 * PT_PER_MM, A4_MM.1 * PT_PER_MM);
            let (margin, gap) = (A4_MARGIN_MM * PT_PER_MM, A4_GAP_MM * PT_PER_MM);
            let cell_w = (pw - 2.0 * margin - gap * (cols - 1) as f32) / cols as f32;
            let cell_h = (ph - 2.0 * margin - gap * (rows - 1) as f32) / rows as f32;
            Layout { page_w: pw, page_h: ph, cell_w, cell_h, margin, gap }
        }
        PdfPageSize::Fit => {
            let cell_w = images.iter().map(|d| d.0 as f32 * FIT_PT_PER_PX).fold(1.0, f32::max);
            let cell_h = images.iter().map(|d| d.1 as f32 * FIT_PT_PER_PX).fold(1.0, f32::max);
            // A page holding fewer images than `per_page` (the last one) keeps only the rows it fills.
            let used_rows = images.len().div_ceil(cols).clamp(1, rows);
            let page_w = cell_w * cols.min(images.len().max(1)) as f32 + FIT_GAP_PT * (cols.min(images.len().max(1)) - 1) as f32;
            let page_h = cell_h * used_rows as f32 + FIT_GAP_PT * (used_rows - 1) as f32;
            Layout { page_w, page_h, cell_w, cell_h, margin: 0.0, gap: FIT_GAP_PT }
        }
    }
}

/// Largest enlargement on an A4 page, in points per pixel (2x screen size): a tiny screenshot is not
/// blown up to fill a whole page.
const A4_MAX_PT_PER_PX: f32 = 1.5;

/// The drawn size (from the original pixel size) and the pixels kept (downscaled for memory).
fn place(img: DynamicImage, page_size: PdfPageSize, l: &Layout) -> Placed {
    let (ow, oh) = img.dimensions();
    let (ow, oh) = (ow as f32, oh as f32);
    let (w_pt, h_pt) = match page_size {
        PdfPageSize::A4 => {
            let s = (l.cell_w / ow).min(l.cell_h / oh).min(A4_MAX_PT_PER_PX);
            (ow * s, oh * s)
        }
        // The cell is the page's largest image at screen size, so every image fits at its own size.
        PdfPageSize::Fit => (ow * FIT_PT_PER_PX, oh * FIT_PT_PER_PX),
    };
    let img = match page_size {
        PdfPageSize::A4 => {
            let px_per_pt = PDF_A4_DPI / 72.0;
            shrink_to(img, w_pt * px_per_pt, h_pt * px_per_pt)
        }
        PdfPageSize::Fit => shrink_to(img, PDF_FIT_MAX_EDGE as f32, PDF_FIT_MAX_EDGE as f32),
    };
    Placed { raw: raw_of(img), w_pt, h_pt }
}

pub fn export_pdf(
    caller: Caller,
    cat: &Catalog,
    fa: &dyn FileAccess,
    req: &ExportPdfReq,
    reporter: &ProgressReporter<'_>,
    cancel: &AtomicBool,
) -> Result<ExportReport, AppError> {
    let per_page = match req.per_page {
        1 | 2 | 4 => req.per_page as usize,
        n => return Err(invalid(format!("perPage must be 1, 2 or 4, got {n}"))),
    };
    let dest = dest_path(caller, cat, &req.dest_path)?;
    let rows = rows_in_order(cat, &req.image_ids, PDF_MAX_IMAGES)?;
    let total = rows.len() as i64;
    let (cols, _) = grid(per_page);
    let mut doc = PdfDocument::new("Screenshot Studio export");
    let mut pages = Vec::new();
    let mut done = 0i64;
    for chunk in rows.chunks(per_page) {
        let mut decoded = Vec::with_capacity(chunk.len());
        for row in chunk {
            if cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            let abs = row.abs();
            reporter.report("pdf", done, total, abs.to_str());
            decoded.push(decode_rgb(fa, &abs)?);
            done += 1;
        }
        let dims: Vec<(u32, u32)> = decoded.iter().map(|i| i.dimensions()).collect();
        let l = layout(req.page_size, per_page, &dims);
        let mut ops_list = Vec::new();
        for (k, img) in decoded.into_iter().enumerate() {
            let p = place(img, req.page_size, &l);
            let (col, row) = (k % cols, k / cols);
            let cell_x = l.margin + col as f32 * (l.cell_w + l.gap);
            let cell_top = l.page_h - l.margin - row as f32 * (l.cell_h + l.gap);
            let x = cell_x + (l.cell_w - p.w_pt) / 2.0;
            let y = cell_top - l.cell_h + (l.cell_h - p.h_pt) / 2.0;
            // Size through the dpi: printpdf draws an image `px * 72 / dpi` points wide.
            let dpi = p.raw.width as f32 * 72.0 / p.w_pt.max(0.01);
            let id = doc.add_image(&p.raw);
            ops_list.push(Op::UseXobject {
                id,
                transform: XObjectTransform { translate_x: Some(Pt(x)), translate_y: Some(Pt(y)), dpi: Some(dpi), ..Default::default() },
            });
        }
        pages.push(PdfPage::new(Mm(l.page_w / PT_PER_MM), Mm(l.page_h / PT_PER_MM), ops_list));
    }
    let page_count = pages.len() as i64;
    reporter.report("pdf-save", total, total, None);
    let bytes = write_atomically(&dest, |tmp| {
        let mut warnings = Vec::new();
        let out = doc.with_pages(pages).save(&PdfSaveOptions::default(), &mut warnings);
        if out.is_empty() {
            return Err(internal("PDF save produced no bytes"));
        }
        std::fs::write(tmp, out)?;
        Ok(())
    })?;
    Ok(ExportReport { files: total, bytes, pages: page_count })
}
