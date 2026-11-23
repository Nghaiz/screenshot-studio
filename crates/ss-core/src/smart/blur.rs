//! Sharpness score: variance of the Laplacian (3 x 3, 4-neighbour) of the luma of the 320 px grid
//! thumbnail (contract-sqlite-schema.md `image.blur_score`). Low = blurry; the `blurThreshold`
//! setting (default 60) is the cut. Always computed from the encoded WebP thumbnail, both in the
//! thumbnail pass and in the backfill of older rows, so the two give identical scores.

use image::GrayImage;

/// Variance of the Laplacian over the interior pixels; 0 for images under 3 x 3.
pub fn laplacian_variance(luma: &GrayImage) -> f64 {
    let (w, h) = luma.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let px = |x: u32, y: u32| f64::from(luma.get_pixel(x, y).0[0]);
    let (mut sum, mut sum_sq, mut n) = (0.0, 0.0, 0.0);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let lap = px(x - 1, y) + px(x + 1, y) + px(x, y - 1) + px(x, y + 1) - 4.0 * px(x, y);
            sum += lap;
            sum_sq += lap * lap;
            n += 1.0;
        }
    }
    let mean = sum / n;
    (sum_sq / n - mean * mean).max(0.0)
}

/// Score of an encoded thumbnail (WebP bytes). None when it does not decode.
pub fn blur_of_thumbnail(bytes: &[u8]) -> Option<f64> {
    let img = image::load_from_memory(bytes).ok()?;
    Some(laplacian_variance(&img.into_luma8()))
}
