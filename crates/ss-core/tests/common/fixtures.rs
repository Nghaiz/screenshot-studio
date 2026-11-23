//! Synthetic screenshot library for `ss-core` tests. Writes PNGs with varied sizes and aspect
//! ratios, a drawn text label, and timestamps in their names, into a temp dir (README D10: tests
//! never touch the OneDrive folder). Lives in `tests/common/` so cargo does not build it as its
//! own test crate.

#![allow(dead_code)] // each test crate uses a different subset

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};

#[derive(Debug, Clone)]
pub struct Fixture {
    pub path: PathBuf,
    /// `/`-separated folder relative to the library root ('' = the root itself).
    pub folder: String,
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    /// Local wall-clock time encoded in the name (`YYYY-MM-DD HH:MM:SS`), if any.
    pub name_time: Option<String>,
}

/// Base sizes, one per aspect class of the schema: phone, pc, square, wide, other.
const SHAPES: [(u32, u32); 5] = [(90, 200), (320, 180), (160, 160), (420, 150), (200, 170)];
/// Every this-many fixtures one is larger than the 1280 preview, so both thumbnails downscale.
const LARGE_EVERY: usize = 50;
const LARGE: (u32, u32) = (1500, 900);

/// Writes `n` fixtures under `dir` and returns them in creation order.
pub fn write_library(dir: &Path, n: usize) -> Vec<Fixture> {
    (0..n).map(|i| write_fixture(dir, i)).collect()
}

/// Writes fixture number `i` (deterministic name, size and pixels).
pub fn write_fixture(dir: &Path, i: usize) -> Fixture {
    let (width, height) = size_of(i);
    let (year, month, day) = (2020 + (i / 336) as u32, 1 + ((i / 28) % 12) as u32, 1 + (i % 28) as u32);
    let (h, m, s) = ((i % 24) as u32, ((i * 7) % 60) as u32, ((i * 13) % 60) as u32);
    let date = format!("{year:04}-{month:02}-{day:02}");
    let time = format!("{h:02}{m:02}{s:02}");
    let iso = format!("{date} {h:02}:{m:02}:{s:02}");
    let (file_name, name_time) = match i % 4 {
        0 => (format!("Screenshot {date} {time}.png"), Some(iso)),
        1 => (format!("Screenshot ({i}).png"), None),
        2 => (format!("Ảnh chụp màn hình {date} {time}.png"), Some(iso)),
        _ => (format!("Screenshot_{year:04}{month:02}{day:02}_{time}.png"), Some(iso)),
    };
    let folder = match i % 5 {
        0 => "1ERD",
        1 => "4ERD/sub",
        _ => "",
    }
    .to_owned();
    let mut path = dir.to_path_buf();
    for part in folder.split('/').filter(|p| !p.is_empty()) {
        path.push(part);
    }
    std::fs::create_dir_all(&path).expect("fixture dir");
    path.push(&file_name);
    draw(width, height, i).save(&path).expect("write fixture png");
    Fixture { path, folder, file_name, width, height, name_time }
}

fn size_of(i: usize) -> (u32, u32) {
    if i % LARGE_EVERY == LARGE_EVERY - 1 {
        return LARGE;
    }
    let (w, h) = SHAPES[i % SHAPES.len()];
    let k = (i % 7) as u32;
    match i % SHAPES.len() {
        2 => (w + k * 3, h + k * 3), // square stays square
        _ => (w + k * 3, h),
    }
}

/// Gradient background unique per index, with `#<i>` drawn in a 3x5 pixel font.
fn draw(width: u32, height: u32, i: usize) -> RgbaImage {
    let seed = (i * 37 % 256) as u8;
    let mut img = RgbaImage::from_fn(width, height, |x, y| {
        let r = (x * 255 / width.max(1)) as u8 ^ seed;
        let g = (y * 255 / height.max(1)) as u8;
        let b = seed.wrapping_add((i % 13) as u8 * 17);
        Rgba([r, g, b, 255])
    });
    let label = format!("#{i}");
    let scale = 2;
    let mut cx = 4;
    for ch in label.chars() {
        let rows = glyph(ch);
        for (ry, row) in rows.iter().enumerate() {
            for rx in 0..3u32 {
                if row & (0b100 >> rx) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let (x, y) = (cx + rx * scale + dx, 4 + ry as u32 * scale + dy);
                            if x < width && y < height {
                                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                            }
                        }
                    }
                }
            }
        }
        cx += 4 * scale;
    }
    img
}

/// 3x5 glyphs, one byte per row (bit 2 = left column).
fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '#' => [0b101, 0b111, 0b101, 0b111, 0b101],
        _ => [0; 5],
    }
}
