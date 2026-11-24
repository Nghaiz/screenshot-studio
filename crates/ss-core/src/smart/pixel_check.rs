//! The pixel stage of `similar` duplicates, their final arbiter (README Deferred decision #35). A
//! candidate pair that passed the cheap checks ([`super::near_copy::plausible`]) is a near-identical copy
//! only when its two pictures agree everywhere, within what re-encoding or resizing leaves behind: a
//! changed stroke (one crow's foot of an ER diagram), a cursor or a changed region fails the pair even
//! when its OCR text and embedding are identical.
//!
//! The pictures are the stored grid thumbnails (`<data>\thumbs`, lossy WebP q80, long edge 320), never
//! the originals: a cloud-only file stays closed, and only the survivors of the cheap checks are read.
//! An image without a usable thumbnail (`thumb_state` other than `ok`, a missing or undecodable file) is
//! in no `similar` group, because its pair cannot be verified.
//!
//! [`block_diff`]: both pictures as luma (Rec. 709), the larger resized (Lanczos3) to the common size (the
//! smaller width and the smaller height), each smoothed with a [1, 2, 1] kernel both ways, then the mean
//! absolute difference over blocks of about [`BLOCK_PX`] px. The score is the largest block mean, in
//! luma levels. Measured basis: README Re-score phase 3.4c (pixel stage).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{PixelType, Resizer};
use rayon::prelude::*;
use rusqlite::Connection;

use crate::contract::AppError;
use crate::contract::ipc::ssimg::THUMB_GRID;
use crate::paths;

/// Side of a [`block_diff`] block, in thumbnail pixels. A crow's foot of a wide diagram is 3-5 px at
/// the thumbnail scale; 4 px blocks keep it from being averaged away. Synthetic re-encodes and resizes
/// (below) reached 16.6 with 3 px blocks; with 8 px blocks the real pairs of different pictures fell to
/// 20.8 and one unverified real pair to 14.5, too close to the noise.
pub const BLOCK_PX: u32 = 4;
/// Largest block mean (luma levels) of a copy. Measured on the stored thumbnails: 120 synthetic
/// re-encodes and resizes of 20 local screenshots (JPEG q85 and q70, 75 / 50 / 33% resizes, 50% resize +
/// JPEG) reached at most 9.8 (p90 7.3); the 9 pixel-identical real pairs 0; the 5 real pairs of
/// different pictures 47.6-151.3. Synthetic changes above it: crow's-foot strokes 19/20, cursors 18/20,
/// notifications 20/20, scrolls 20/20.
pub const BLOCK_DIFF_MAX: f64 = 12.0;
/// Rec. 709 luma weights, in 1/10000.
const LUMA: [u32; 3] = [2126, 7152, 722];
/// Sum of the [1, 2, 1] x [1, 2, 1] kernel: smoothed values are kept at this scale.
const SMOOTH_SCALE: u32 = 16;

/// A decoded thumbnail as luma, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    width: u32,
    height: u32,
    luma: Vec<u8>,
}

impl Picture {
    /// None unless `luma` holds exactly `width * height` (> 0) pixels.
    pub fn from_luma(width: u32, height: u32, luma: Vec<u8>) -> Option<Self> {
        (width > 0 && height > 0 && luma.len() == width as usize * height as usize).then_some(Picture { width, height, luma })
    }

    /// Interleaved RGB (`channels` 3) or RGBA (4, alpha ignored) pixels.
    pub fn from_rgb(width: u32, height: u32, px: &[u8], channels: usize) -> Option<Self> {
        if !(3..=4).contains(&channels) || px.len() != width as usize * height as usize * channels {
            return None;
        }
        let luma = px
            .chunks_exact(channels)
            .map(|p| ((LUMA[0] * u32::from(p[0]) + LUMA[1] * u32::from(p[1]) + LUMA[2] * u32::from(p[2]) + 5000) / 10_000) as u8)
            .collect();
        Picture::from_luma(width, height, luma)
    }

    /// A WebP thumbnail; None when it does not decode.
    pub fn decode_webp(bytes: &[u8]) -> Option<Self> {
        let img = webp::Decoder::new(bytes).decode()?;
        Picture::from_rgb(img.width(), img.height(), &img, if img.is_alpha() { 4 } else { 3 })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// The pair score (module doc): the largest block mean of the smoothed absolute difference, in luma
/// levels. Infinite when the pictures cannot be brought to a common size (the pair is not a copy).
pub fn block_diff(a: &Picture, b: &Picture) -> f64 {
    let (w, h) = (a.width.min(b.width), a.height.min(b.height));
    let (Some(la), Some(lb)) = (fitted(a, w, h), fitted(b, w, h)) else {
        return f64::INFINITY;
    };
    let (sa, sb) = (smoothed(&la, w as usize, h as usize), smoothed(&lb, w as usize, h as usize));
    let (w, h) = (w as usize, h as usize);
    let edges = |len: usize| {
        let n = (len / BLOCK_PX as usize).max(1);
        (0..=n).map(move |k| k * len / n).collect::<Vec<_>>()
    };
    let (ys, xs) = (edges(h), edges(w));
    let mut worst = 0.0f64;
    for yb in ys.windows(2) {
        for xb in xs.windows(2) {
            let mut sum = 0u64;
            for y in yb[0]..yb[1] {
                let row = y * w;
                sum += (row + xb[0]..row + xb[1]).map(|i| u64::from(sa[i].abs_diff(sb[i]))).sum::<u64>();
            }
            let n = ((yb[1] - yb[0]) * (xb[1] - xb[0])) as f64;
            worst = worst.max(sum as f64 / (n * f64::from(SMOOTH_SCALE)));
        }
    }
    worst
}

/// True when the pair's [`block_diff`] is within [`BLOCK_DIFF_MAX`].
pub fn same_picture(a: &Picture, b: &Picture) -> bool {
    block_diff(a, b) <= BLOCK_DIFF_MAX
}

/// `p`'s luma at `w` x `h` (Lanczos3 when the size differs); None when the resize fails.
fn fitted(p: &Picture, w: u32, h: u32) -> Option<Vec<u8>> {
    if (p.width, p.height) == (w, h) {
        return Some(p.luma.clone());
    }
    let src = FirImage::from_vec_u8(p.width, p.height, p.luma.clone(), PixelType::U8).ok()?;
    let mut dst = FirImage::new(w, h, PixelType::U8);
    Resizer::new().resize(&src, &mut dst, None).ok()?;
    Some(dst.into_vec())
}

/// The [1, 2, 1] kernel both ways (edges clamped), at [`SMOOTH_SCALE`] times the luma.
fn smoothed(px: &[u8], w: usize, h: usize) -> Vec<u16> {
    let at = |v: usize, len: usize, d: isize| (v as isize + d).clamp(0, len as isize - 1) as usize;
    let mut across = vec![0u16; px.len()];
    for y in 0..h {
        for x in 0..w {
            let p = |dx| u16::from(px[y * w + at(x, w, dx)]);
            across[y * w + x] = p(-1) + 2 * p(0) + p(1);
        }
    }
    let mut out = vec![0u16; px.len()];
    for y in 0..h {
        for x in 0..w {
            let p = |dy| across[at(y, h, dy) * w + x];
            out[y * w + x] = p(-1) + 2 * p(0) + p(1);
        }
    }
    out
}

/// The grid thumbnails of `ids` whose `thumb_state` is `ok` and whose file decodes, read and decoded in
/// parallel. An id missing from the result has no usable picture.
pub fn load(conn: &Connection, data_dir: &Path, ids: &[i64]) -> Result<HashMap<i64, Arc<Picture>>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let list = super::near_copy::id_list(ids);
    let mut stmt = conn.prepare_cached("SELECT id FROM image WHERE thumb_state = 'ok' AND id IN (SELECT value FROM json_each(?1))")?;
    let ok: Vec<i64> = stmt.query_map([list], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(ok
        .par_iter()
        .filter_map(|&id| {
            let bytes = std::fs::read(paths::thumb_path(data_dir, id, THUMB_GRID)).ok()?;
            Picture::decode_webp(&bytes).map(|p| (id, Arc::new(p)))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A screenshot-like picture: white, with dark "text" bars in rows and one framed box.
    fn screen(w: u32, h: u32) -> Picture {
        let luma = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let text = y % 12 < 3 && x % 40 < 28 && y > 8;
                let frame = (x == w / 2 || x == w * 3 / 4) && (h / 4..h * 3 / 4).contains(&y);
                if text || frame { 30 } else { 250 }
            })
            .collect();
        Picture::from_luma(w, h, luma).unwrap()
    }

    fn resized(p: &Picture, w: u32, h: u32) -> Picture {
        Picture::from_luma(w, h, fitted(p, w, h).unwrap()).unwrap()
    }

    fn painted(p: &Picture, f: impl Fn(u32, u32) -> Option<u8>) -> Picture {
        let mut q = p.clone();
        for y in 0..p.height {
            for x in 0..p.width {
                if let Some(v) = f(x, y) {
                    q.luma[(y * p.width + x) as usize] = v;
                }
            }
        }
        q
    }

    #[test]
    fn a_copy_and_its_resize_agree() {
        let a = screen(320, 200);
        assert_eq!(block_diff(&a, &a), 0.0);
        // A thumbnail one pixel shorter (the rounding of a resized copy) and a 50% copy.
        for (w, h) in [(320, 199), (160, 100)] {
            let d = block_diff(&a, &resized(&a, w, h));
            assert!(d <= BLOCK_DIFF_MAX, "{w}x{h}: {d}");
        }
    }

    #[test]
    fn a_stroke_or_a_changed_region_is_another_picture() {
        let a = screen(320, 200);
        // A crow's foot ~1.5% of the width: three 1 px lines fanning out from one point, in a white area.
        let (x0, y0) = (100.0f64, 160.0f64);
        let foot = painted(&a, |x, y| {
            let (dx, dy) = (x as f64 - x0, y as f64 - y0);
            ((0.0..=5.0).contains(&dx) && [-0.6, 0.0, 0.6].iter().any(|k: &f64| (dy - k * dx).abs() < 0.5)).then_some(30)
        });
        assert!(block_diff(&a, &foot) > BLOCK_DIFF_MAX, "stroke: {}", block_diff(&a, &foot));
        let region = painted(&a, |x, y| ((200..232).contains(&x) && (20..40).contains(&y)).then_some(120));
        assert!(block_diff(&a, &region) > BLOCK_DIFF_MAX);
        // Symmetric, and the stroke still shows when one side is a resize.
        assert_eq!(block_diff(&a, &foot), block_diff(&foot, &a));
        assert!(!same_picture(&resized(&a, 320, 199), &foot));
    }

    #[test]
    fn pictures_decode_from_webp_and_reject_bad_input() {
        let rgb: Vec<u8> = (0..16 * 8).flat_map(|i| [i as u8, 255 - i as u8, 7]).collect();
        let mem = webp::Encoder::from_rgb(&rgb, 16, 8).encode_simple(false, 80.0).unwrap();
        let p = Picture::decode_webp(&mem).unwrap();
        assert_eq!(p.size(), (16, 8));
        assert!(Picture::decode_webp(b"not a webp").is_none());
        assert!(Picture::from_rgb(2, 2, &[0; 11], 3).is_none());
        assert!(Picture::from_luma(0, 0, vec![]).is_none());
        // Rec. 709: pure green weighs most.
        assert_eq!(Picture::from_rgb(1, 1, &[0, 255, 0], 3).unwrap().luma, [182]);
    }
}
