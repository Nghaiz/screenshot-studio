//! Phase 3.2: the sidecar-free smart features. A planted exact copy and a re-encoded (smaller JPEG) near
//! copy are grouped with the right best image; sessions split at the gap; a blurry image is flagged from
//! its 320 thumbnail (scored in the thumbnail pass and backfilled for older rows); tiny images listed;
//! `get_stats` breakdowns sum to the row counts. Phase 3.4c: `similar` means near-identical copies (a
//! re-encode or a resize of one picture), verified on the grid thumbnails: one changed stroke or region
//! keeps two images apart even when their OCR text and embedding are identical.

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::env::Env;
use image::{Rgb, RgbImage};
use serde_json::json;
use ss_core::contract::ipc::*;
use ss_core::contract::{Caller, ErrorKind};
use ss_core::query;
use ss_core::smart::duplicates::{self, Candidate};

/// Deterministic integer hash for test pixels.
fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

/// A 9x8 grid of flat cell levels (the dHash grid; horizontal neighbours differ by >= 40 levels, so the
/// hash survives a resize + JPEG re-encode). Distinct seeds give unrelated hashes.
fn cells(seed: u32) -> [[i32; 9]; 8] {
    let mut cells = [[0i32; 9]; 8];
    for (r, row) in cells.iter_mut().enumerate() {
        let mut prev = -100;
        for (c, v) in row.iter_mut().enumerate() {
            let mut k = 0u32;
            *v = loop {
                let cand = 40 + (mix(seed ^ ((r as u32) << 8) ^ ((c as u32) << 16) ^ (k << 24)) % 176) as i32;
                if (cand - prev).abs() >= 40 {
                    break cand;
                }
                k += 1;
            };
            prev = *v;
        }
    }
    cells
}

/// [`cells`] under per-pixel grain (sharp: a high Laplacian variance).
fn noise(w: u32, h: u32, seed: u32) -> RgbImage {
    let cells = cells(seed);
    RgbImage::from_fn(w, h, |x, y| {
        let cell = cells[(y * 8 / h) as usize][(x * 9 / w) as usize];
        let grain = (mix(seed.wrapping_mul(31) ^ x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 51) as i32 - 25;
        let v = (cell + grain).clamp(0, 255) as u8;
        Rgb([v, v, v])
    })
}

/// A screenshot-like picture: [`cells`] with rows of dark text-like bars, all scaled to the image so a
/// resize keeps them (sharp edges and flat areas, which a lossy thumbnail keeps; grain it does not).
fn screen(w: u32, h: u32, seed: u32) -> RgbImage {
    let cells = cells(seed);
    RgbImage::from_fn(w, h, |x, y| {
        let cell = cells[(y * 8 / h) as usize][(x * 9 / w) as usize];
        let text = (y * 40 / h) % 4 == 1 && (x * 64 / w) % 8 < 6;
        let v = if text { (cell - 90).max(0) } else { cell };
        Rgb([v as u8; 3])
    })
}

/// A smooth gradient: no edges, so its Laplacian variance is ~0 (blurry).
fn smooth(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| Rgb([(x * 200 / w) as u8 + 20, (y * 200 / h) as u8 + 20, 128]))
}

fn save(img: &RgbImage, dir: &Path, name: &str) {
    img.save(dir.join(name)).unwrap_or_else(|e| panic!("save {name}: {e}"));
}

const A_ORIG: &str = "Screenshot 2026-01-01 101500.png";
const A_COPY: &str = "Screenshot 2026-01-02 101500.png";
const B_PNG: &str = "Screenshot 2026-01-05 080000.png";
const B_JPG: &str = "Screenshot 2026-01-06 080000.jpg";

/// An exact copy (newer by its name) and a re-encoded, downscaled JPEG near copy (newer, smaller).
fn plant_duplicates(env: &Env) {
    let root = env.root_dir();
    save(&screen(300, 200, 1), &root, A_ORIG);
    std::fs::copy(root.join(A_ORIG), root.join(A_COPY)).unwrap();
    let b = screen(300, 200, 2);
    save(&b, &root, B_PNG);
    let small = image::imageops::resize(&b, 240, 160, image::imageops::FilterType::Triangle);
    save(&small, &root, B_JPG);
}

fn id_set(ids: &[i64]) -> BTreeSet<i64> {
    ids.iter().copied().collect()
}

fn ids_sql(env: &Env, sql: &str) -> Vec<i64> {
    let sql = sql.to_owned();
    env.core
        .catalog()
        .read(move |c| {
            let mut st = c.prepare(&sql)?;
            Ok(st.query_map([], |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?)
        })
        .unwrap()
}

fn dups(env: &Env, mode: DuplicateMode, hamming: Option<i64>) -> DuplicateResult {
    env.core.find_duplicates(FindDuplicatesReq { mode, hamming, root_id: None }).unwrap()
}

/// Stores what an AI pass would for `id` (the rows ss-core `ai` writes): OCR text and an embedding.
fn vouch(env: &Env, id: i64, text: &str, vec: &[f32]) {
    let hex: String = vec.iter().flat_map(|f| f.to_le_bytes()).map(|b| format!("{b:02X}")).collect();
    env.exec(&format!(
        "INSERT INTO ocr_result (image_id, text, lines_json, engine, done_at) VALUES ({id}, '{}', '[]', 'rapidocr', 0)
           ON CONFLICT(image_id) DO UPDATE SET text = excluded.text;
         INSERT INTO embedding (image_id, model, dim, vec, done_at) VALUES ({id}, 'test-model', {}, X'{hex}', 0)
           ON CONFLICT(image_id) DO UPDATE SET dim = excluded.dim, vec = excluded.vec;",
        text.replace('\'', "''"),
        vec.len()
    ));
}

const REPORT: &str = "Quarterly report 2026 - Total revenue 4,810 - Region North - Prepared by finance";
/// REPORT as OCR read it from the smaller copy (`o` read as `0`).
const REPORT_REREAD: &str = "Quarterly rep0rt 2026 - Total revenue 4,810 - Region North - Prepared by finance";

/// The planted re-encoded pair as the AI pass reads it: the same text (OCR misread one letter of the
/// smaller copy) and near-equal embeddings.
fn vouch_reencoded_pair(env: &Env) {
    vouch(env, env.by_name(B_PNG).id, REPORT, &[0.9, 0.3, 0.1]);
    vouch(env, env.by_name(B_JPG).id, REPORT_REREAD, &[0.9, 0.31, 0.1]);
}

/// Groups of `r` as id sets.
fn grouped(r: &DuplicateResult) -> Vec<BTreeSet<i64>> {
    r.groups.iter().map(|g| id_set(&g.image_ids)).collect()
}

// ---------------------------------------------------------------------------------------------
// Duplicates
// ---------------------------------------------------------------------------------------------

#[test]
fn exact_and_reencoded_copies_are_grouped() {
    let env = Env::new(8);
    plant_duplicates(&env);
    env.index(true);
    let id = |n: &str| env.by_name(n).id;
    let (a, a2, b, bj) = (id(A_ORIG), id(A_COPY), id(B_PNG), id(B_JPG));
    let dhash = |i: i64| env.sql_i64(&format!("SELECT dhash FROM image WHERE id = {i}"));
    println!("dHash Hamming PNG vs re-encoded JPEG: {}", (dhash(b) ^ dhash(bj)).count_ones());

    let exact = dups(&env, DuplicateMode::Exact, None);
    assert_eq!(exact.groups.len(), 1, "{exact:?}");
    let g = &exact.groups[0];
    assert_eq!((g.group_id, g.best_id, g.reason), (1, a2, DuplicateReason::Newest), "same pixels: the newer copy wins");
    assert_eq!(g.image_ids, [a2, a], "best first");

    // Before any AI pass the hashes and the thumbnails vouch: both pairs are grouped (Deferred decision #35).
    assert_eq!(grouped(&dups(&env, DuplicateMode::Similar, None)), [id_set(&[a, a2]), id_set(&[b, bj])]);
    vouch_reencoded_pair(&env);
    let similar = dups(&env, DuplicateMode::Similar, None);
    let group_of = |x: i64| similar.groups.iter().find(|g| g.image_ids.contains(&x)).unwrap_or_else(|| panic!("{x} not grouped: {similar:?}"));
    let gb = group_of(b);
    assert_eq!(id_set(&gb.image_ids), id_set(&[b, bj]), "re-encoded JPEG grouped with its original");
    assert_eq!((gb.best_id, gb.reason, gb.image_ids[0]), (b, DuplicateReason::Largest, b), "more pixels wins over newer");
    assert_eq!(id_set(&group_of(a).image_ids), id_set(&[a, a2]));
    let ids: Vec<i64> = similar.groups.iter().map(|g| g.group_id).collect();
    assert_eq!(ids, (1..=similar.groups.len() as i64).collect::<Vec<_>>(), "groupId 1..n");

    // Bounds, root scope, trash.
    let err = |r: FindDuplicatesReq| env.core.find_duplicates(r).unwrap_err().kind;
    assert_eq!(err(FindDuplicatesReq { mode: DuplicateMode::Similar, hamming: Some(65), root_id: None }), ErrorKind::InvalidArgument);
    assert_eq!(err(FindDuplicatesReq { mode: DuplicateMode::Exact, hamming: None, root_id: Some(999) }), ErrorKind::NotFound);
    let scoped = env.core.find_duplicates(FindDuplicatesReq { mode: DuplicateMode::Exact, hamming: None, root_id: Some(env.root.id) }).unwrap();
    assert_eq!(scoped, exact);
    // The dupHamming setting is the default. With the evidence of both pairs agreeing (one text, one
    // embedding) and every pair of hashes in range (64 bits), the thumbnails still keep them apart.
    for x in [a, a2] {
        vouch(&env, x, REPORT, &[0.9, 0.3, 0.1]);
    }
    env.core.set_setting(Caller::Ui, SetSettingReq { key: SettingKey::DupHamming, value: json!(64) }).unwrap();
    let two = dups(&env, DuplicateMode::Similar, None);
    assert_eq!(grouped(&two), [id_set(&[a, a2]), id_set(&[b, bj])], "64 bits: still two pictures");
    env.core.trash_images(Caller::Ui, ImageIdsReq { image_ids: vec![a2] }).unwrap();
    assert!(dups(&env, DuplicateMode::Exact, None).groups.is_empty(), "trashed images are not proposed again");
}

const FLAT_GREY: &str = "Screenshot 2026-02-01 090000.png";
const FLAT_GREY_COPY: &str = "Screenshot 2026-02-02 090000.png";
const FLAT_WHITE: &str = "Screenshot 2026-02-03 090000.png";
const SLIVER: &str = "Screenshot 2026-02-04 090000.png";

/// Near-blank images as the real library has them (README Re-score phase 3.4b): flat screenshots of different
/// sizes and colours, an exact copy of one, and a 1 px high sliver. Their dHashes have 0 set bits.
fn plant_flat(env: &Env) {
    let root = env.root_dir();
    save(&RgbImage::from_pixel(640, 360, Rgb([60, 60, 60])), &root, FLAT_GREY);
    std::fs::copy(root.join(FLAT_GREY), root.join(FLAT_GREY_COPY)).unwrap();
    save(&RgbImage::from_pixel(200, 600, Rgb([250, 250, 250])), &root, FLAT_WHITE);
    save(&RgbImage::from_pixel(14, 1, Rgb([20, 30, 40])), &root, SLIVER);
}

#[test]
fn low_information_images_are_exact_only_in_duplicates_and_stats() {
    let env = Env::new(4);
    plant_duplicates(&env);
    plant_flat(&env);
    env.index(true);
    let id = |n: &str| env.by_name(n).id;
    let flats = [id(FLAT_GREY), id(FLAT_GREY_COPY), id(FLAT_WHITE), id(SLIVER)];
    for f in flats {
        let h = env.sql_i64(&format!("SELECT dhash FROM image WHERE id = {f}"));
        assert!(duplicates::is_low_information(h), "flat fixture {f}: dHash {h:#x} has {} set bits", (h as u64).count_ones());
    }
    let (b, bj) = (id(B_PNG), id(B_JPG));
    vouch_reencoded_pair(&env);

    // Exact: the flat copy is still found, next to the planted exact pair.
    let exact = grouped(&dups(&env, DuplicateMode::Exact, None));
    assert!(exact.contains(&id_set(&flats[..2])), "{exact:?}");
    assert_eq!(exact.len(), 2, "{exact:?}");
    // Similar: no flat image in any group, at the default Hamming and with every pair in range; the
    // re-encoded pair is still grouped.
    for hamming in [None, Some(duplicates::MAX_HAMMING)] {
        let similar = grouped(&dups(&env, DuplicateMode::Similar, hamming));
        assert!(similar.iter().all(|g| flats.iter().all(|f| !g.contains(f))), "hamming {hamming:?}: {similar:?}");
        assert!(similar.iter().any(|g| g.contains(&b) && g.contains(&bj)), "hamming {hamming:?}: {similar:?}");
    }
    // Stats counts with the same rule: 2 exact copies (the planted pair, the flat copy), and similar copies
    // equal to `find_duplicates` similar, which leaves the 4 flat images out (with them: 3 more).
    let s = env.core.get_stats().unwrap();
    let similar = dups(&env, DuplicateMode::Similar, None);
    assert_eq!(s.duplicates_exact, 2);
    assert_eq!(s.duplicates_similar, similar.groups.iter().map(|g| g.image_ids.len() as i64 - 1).sum::<i64>(), "{:?}", similar.groups);
}

#[test]
fn best_image_reasons() {
    let c = |id: i64, pixels: i64, taken_ms: i64, blur: Option<f64>| Candidate {
        id,
        content_hash: Some("h".into()),
        dhash: Some(0),
        phash: Some(0),
        width: pixels,
        height: 1,
        taken_ms,
        mtime_ms: 0,
        blur,
    };
    let best = |cands: Vec<Candidate>| {
        let (i, reason) = duplicates::best_of(&cands, &(0..cands.len()).collect::<Vec<_>>());
        (cands[i].id, reason)
    };
    assert_eq!(best(vec![c(1, 100, 5, None), c(2, 200, 1, None)]), (2, DuplicateReason::Largest));
    assert_eq!(best(vec![c(1, 100, 5, None), c(2, 100, 9, None)]), (2, DuplicateReason::Newest));
    assert_eq!(best(vec![c(1, 100, 5, Some(80.0)), c(2, 100, 5, Some(300.0)), c(3, 100, 5, None)]), (2, DuplicateReason::Sharpest));
    assert_eq!(best(vec![c(4, 100, 5, Some(1.0)), c(3, 100, 5, Some(1.0))]), (3, DuplicateReason::Largest), "full tie: lowest id");
}

const SHOT: &str = "Screenshot 2026-03-01 090000.png";
const SHOT_JPEG: &str = "Screenshot 2026-03-02 090000.jpg";
const SHOT_HALF: &str = "Screenshot 2026-03-03 090000.png";
const SHOT_FOOT: &str = "Screenshot 2026-03-04 090000.png";
const SHOT_REGION: &str = "Screenshot 2026-03-05 090000.png";

/// `img` with three 2 px lines fanning out from one point, ~1.5% of the width long: the crow's foot that
/// turns one ER diagram answer into another.
fn with_crows_foot(img: &RgbImage) -> RgbImage {
    let mut out = img.clone();
    let len = (img.width() * 3 / 200).max(6);
    // In the first cell, between two text rows.
    let (x0, y0) = (img.width() / 30, img.height() * 11 / 40);
    let ink = if img.get_pixel(x0, y0)[0] > 127 { 0 } else { 255 };
    for slope in [-1i64, 0, 1] {
        for t in 0..len {
            for w in 0..2 {
                let y = (i64::from(y0) + slope * i64::from(t) / 2 + w) as u32;
                out.put_pixel(x0 + t, y, Rgb([ink; 3]));
            }
        }
    }
    out
}

/// Deferred decision #35, pinned by identity in both directions on indexed images: a re-encoded JPEG and a
/// 50% resize of one picture are its copies; the same picture plus a crow's-foot-sized stroke, or with one
/// changed region, is not, even when its OCR text and embedding are those of the original (only the
/// thumbnails tell them apart). An image whose thumbnail is not usable is in no group. Stats count the
/// same groups.
#[test]
fn similar_means_one_picture_re_encoded_or_resized() {
    let env = Env::new(4);
    let root = env.root_dir();
    let shot = screen(640, 400, 8);
    save(&shot, &root, SHOT);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 80).encode_image(&shot).unwrap();
    std::fs::write(root.join(SHOT_JPEG), jpeg).unwrap();
    save(&image::imageops::resize(&shot, 320, 200, image::imageops::FilterType::Triangle), &root, SHOT_HALF);
    save(&with_crows_foot(&shot), &root, SHOT_FOOT);
    let mut region = shot.clone();
    for (x, y, p) in region.enumerate_pixels_mut() {
        if (448..576).contains(&x) && (300..360).contains(&y) {
            // A notification-sized box with its own lines.
            *p = Rgb([if (y / 6) % 2 == 0 { 40 } else { 200 }; 3]);
        }
    }
    save(&region, &root, SHOT_REGION);
    env.index(true);
    let id = |n: &str| env.by_name(n).id;
    let (s, jpg, half, foot, reg) = (id(SHOT), id(SHOT_JPEG), id(SHOT_HALF), id(SHOT_FOOT), id(SHOT_REGION));
    let bits = |col: &str, x: i64, y: i64| {
        (env.sql_i64(&format!("SELECT {col} FROM image WHERE id = {x}")) ^ env.sql_i64(&format!("SELECT {col} FROM image WHERE id = {y}"))).count_ones()
    };
    for col in ["dhash", "phash"] {
        for x in [jpg, half, foot] {
            assert!(bits(col, s, x) <= 2, "precondition, {col} of {x}: within the hash-only bound ({})", bits(col, s, x));
        }
    }
    assert!(bits("dhash", s, reg) <= 6, "precondition: the changed region is a candidate ({})", bits("dhash", s, reg));
    let copies = [id_set(&[s, jpg, half])];
    let similar_copies = || dups(&env, DuplicateMode::Similar, None).groups.iter().map(|g| g.image_ids.len() as i64 - 1).sum::<i64>();

    // AI absent: the hashes propose, the thumbnails decide.
    assert_eq!(grouped(&dups(&env, DuplicateMode::Similar, None)), copies, "no evidence");
    // AI present and blind to the stroke and the region: one text and one embedding for all five.
    for (x, text) in [(s, REPORT), (jpg, REPORT), (half, REPORT_REREAD), (foot, REPORT), (reg, REPORT)] {
        vouch(&env, x, text, &[0.2, 1.0, 0.1]);
    }
    for hamming in [None, Some(20), Some(duplicates::MAX_HAMMING)] {
        let r = dups(&env, DuplicateMode::Similar, hamming);
        assert_eq!(grouped(&r), copies, "hamming {hamming:?}: the stroke and the region are other pictures");
        assert_eq!((r.groups[0].best_id, r.groups[0].reason), (jpg, DuplicateReason::Newest), "as many pixels as the PNG, newer");
    }
    assert_eq!(env.core.get_stats().unwrap().duplicates_similar, similar_copies(), "Stats read the same rule");
    assert_eq!(similar_copies(), 2);

    // A thumbnail that is not `ok` (pending regeneration, offline, error) cannot vouch for its image.
    env.exec(&format!("UPDATE image SET thumb_state = 'pending' WHERE id = {half}"));
    assert_eq!(grouped(&dups(&env, DuplicateMode::Similar, None)), [id_set(&[s, jpg])]);
    assert_eq!(env.core.get_stats().unwrap().duplicates_similar, 1);
}

// ---------------------------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------------------------

#[test]
fn sessions_split_at_the_gap() {
    let env = Env::new(6);
    env.index(false);
    let ids: Vec<i64> = env.all().iter().map(|s| s.id).collect();
    let t0: i64 = 1_767_225_600_000;
    let minutes = [0, 10, 25, 60, 70, 200];
    for (id, m) in ids.iter().zip(minutes) {
        env.exec(&format!("UPDATE image SET taken_ms = {} WHERE id = {id}", t0 + m * 60_000));
    }
    let sessions = |gap: Option<i64>, dir: SortDir| -> Vec<Vec<i64>> {
        let mut q = query::default_query(500);
        q.sort.dir = dir;
        let r = env.core.get_sessions(GetSessionsReq { query: q, gap_minutes: gap }).unwrap();
        for s in &r.sessions {
            assert!(s.start_ms <= s.end_ms);
        }
        r.sessions.into_iter().map(|s| s.image_ids).collect()
    };
    let i = |n: usize| ids[n];
    assert_eq!(sessions(None, SortDir::Asc), vec![vec![i(0), i(1), i(2)], vec![i(3), i(4)], vec![i(5)]], "default gap 30 min");
    assert_eq!(sessions(None, SortDir::Desc), vec![vec![i(5)], vec![i(4), i(3)], vec![i(2), i(1), i(0)]], "newest first");
    assert_eq!(sessions(Some(10), SortDir::Asc), vec![vec![i(0), i(1)], vec![i(2)], vec![i(3), i(4)], vec![i(5)]], "exactly the gap stays together");
    let r = env.core.get_sessions(GetSessionsReq { query: query::default_query(500), gap_minutes: Some(30) }).unwrap();
    let first = r.sessions.last().unwrap();
    assert_eq!((first.start_ms, first.end_ms), (t0, t0 + 25 * 60_000));
    env.core.set_setting(Caller::Ui, SetSettingReq { key: SettingKey::SessionGapMinutes, value: json!(5) }).unwrap();
    assert_eq!(sessions(None, SortDir::Asc).len(), 6, "the sessionGapMinutes setting is the default");
    let bad = env.core.get_sessions(GetSessionsReq { query: query::default_query(500), gap_minutes: Some(0) });
    assert_eq!(bad.unwrap_err().kind, ErrorKind::InvalidArgument);

    // The query's filters apply.
    let mut q = query::default_query(500);
    q.date_from = Some(t0 + 30 * 60_000);
    q.sort.dir = SortDir::Asc;
    let r = env.core.get_sessions(GetSessionsReq { query: q, gap_minutes: Some(30) }).unwrap();
    assert_eq!(r.sessions.into_iter().map(|s| s.image_ids).collect::<Vec<_>>(), vec![vec![i(3), i(4)], vec![i(5)]]);
}

// ---------------------------------------------------------------------------------------------
// Blurry and tiny
// ---------------------------------------------------------------------------------------------

#[test]
fn blurry_fixture_flagged_and_tiny_listed() {
    let env = Env::new(10);
    let root = env.root_dir();
    save(&smooth(800, 600), &root, "blurry.png");
    save(&noise(400, 300, 7), &root, "sharp.png");
    save(&noise(100, 100, 8), &root, "tiny.png");
    env.index(true);
    assert_eq!(env.sql_i64("SELECT count(*) FROM image WHERE thumb_state = 'ok' AND blur_score IS NULL"), 0, "scored in the thumbnail pass");
    let id = |n: &str| env.by_name(n).id;
    let (blurry, sharp, tiny) = (id("blurry.png"), id("sharp.png"), id("tiny.png"));
    let score = |i: i64| -> f64 {
        env.core.catalog().read(move |c| Ok(c.query_row("SELECT blur_score FROM image WHERE id = ?1", [i], |r| r.get(0))?)).unwrap()
    };
    println!("blur scores: blurry {:.2}, sharp {:.2}, tiny {:.2}", score(blurry), score(sharp), score(tiny));
    assert!(score(blurry) < 60.0 && score(sharp) > 60.0);

    let r = env.core.detect_issues(DetectIssuesReq { root_id: None }).unwrap();
    assert!(r.blurry.contains(&blurry) && !r.blurry.contains(&sharp), "{r:?}");
    assert_eq!(r.blurry, ids_sql(&env, "SELECT id FROM image WHERE missing = 0 AND blur_score < 60 ORDER BY id"));
    assert!(r.tiny.contains(&tiny) && !r.tiny.contains(&sharp) && !r.tiny.contains(&blurry), "{r:?}");
    assert_eq!(r.tiny, ids_sql(&env, "SELECT id FROM image WHERE missing = 0 AND width * height < 40000 ORDER BY id"));

    // Rows indexed before 3.2 have no score: detect_issues and the next index fill it from the thumbnail.
    let scores = || ids_sql(&env, "SELECT id FROM image ORDER BY id").into_iter().map(|i| (i, score(i).to_bits())).collect::<Vec<_>>();
    let before = scores();
    env.exec("UPDATE image SET blur_score = NULL");
    assert_eq!(env.core.detect_issues(DetectIssuesReq { root_id: Some(env.root.id) }).unwrap(), r);
    assert_eq!(scores(), before, "the backfill scores exactly what the thumbnail pass did");
    env.exec("UPDATE image SET blur_score = NULL");
    env.index(true);
    assert_eq!(scores(), before, "the index backfills too");

    // Settings are the thresholds; trashed images are not listed.
    env.core.set_setting(Caller::Ui, SetSettingReq { key: SettingKey::BlurThreshold, value: json!(0) }).unwrap();
    env.core.set_setting(Caller::Ui, SetSettingReq { key: SettingKey::TinyMaxPixels, value: json!(0) }).unwrap();
    let none = env.core.detect_issues(DetectIssuesReq { root_id: None }).unwrap();
    assert!(none.blurry.is_empty() && none.tiny.is_empty(), "{none:?}");
    env.core.set_setting(Caller::Ui, SetSettingReq { key: SettingKey::TinyMaxPixels, value: json!(40000) }).unwrap();
    env.core.trash_images(Caller::Ui, ImageIdsReq { image_ids: vec![tiny] }).unwrap();
    assert!(!env.core.detect_issues(DetectIssuesReq { root_id: None }).unwrap().tiny.contains(&tiny));
    assert_eq!(env.core.detect_issues(DetectIssuesReq { root_id: Some(999) }).unwrap_err().kind, ErrorKind::NotFound);
}

// ---------------------------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------------------------

#[test]
fn stats_totals_equal_row_counts() {
    let env = Env::new(25);
    plant_duplicates(&env);
    env.index(true);
    let c = &env.core;
    let all = env.all();
    let id = |i: usize| all[i].id;
    let t1 = c.create_tag(Caller::Ui, CreateTagReq { name: "work".into(), color: "#112233".into() }).unwrap();
    let t2 = c.create_tag(Caller::Ui, CreateTagReq { name: "bug".into(), color: "#445566".into() }).unwrap();
    c.set_tags(Caller::Ui, SetTagsReq { image_ids: (0..6).map(id).collect(), add: vec![t1.id], remove: vec![] }).unwrap();
    c.set_tags(Caller::Ui, SetTagsReq { image_ids: (0..2).map(id).collect(), add: vec![t2.id], remove: vec![] }).unwrap();
    env.exec(&format!(
        "INSERT INTO category (image_id, label, score, model) VALUES ({0}, 'code', 0.9, 'm'), ({0}, 'chat', 0.3, 'm'),
         ({1}, 'code', 0.8, 'm'), ({2}, 'meme', 0.7, 'm')",
        id(3),
        id(4),
        id(5)
    ));
    c.trash_images(Caller::Ui, ImageIdsReq { image_ids: vec![id(7)] }).unwrap();
    env.exec(&format!("UPDATE image SET width = NULL WHERE id = {}", id(8)));
    let gone = env.fixtures.iter().find(|f| !all[..9].iter().any(|s| s.file_name == f.file_name)).unwrap();
    std::fs::remove_file(&gone.path).unwrap();
    env.index(false);
    assert_eq!(env.sql_i64("SELECT count(*) FROM image WHERE missing = 1"), 1);

    let s = c.get_stats().unwrap();
    let total = env.sql_i64("SELECT count(*) FROM image WHERE missing = 0");
    assert_eq!(s.counts, c.get_counts().unwrap());
    assert_eq!((s.counts.total, s.counts.trashed, s.counts.missing), (total, 1, 1));
    let sum = |v: &[CountBy]| v.iter().map(|x| x.count).sum::<i64>();
    assert_eq!((sum(&s.by_month), sum(&s.by_aspect), sum(&s.by_folder)), (total, total, total), "each breakdown sums to total");
    assert!(s.by_month.iter().all(|m| m.key.len() == 7 && m.key.as_bytes()[4] == b'-'), "{:?}", s.by_month);
    assert!(s.by_month.windows(2).all(|w| w[0].key < w[1].key));
    let folders: Vec<String> = {
        let rows = env.core.catalog().read(|c| {
            let mut st = c.prepare("SELECT DISTINCT folder FROM image WHERE missing = 0 ORDER BY folder")?;
            Ok(st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?)
        });
        rows.unwrap()
    };
    assert_eq!(s.by_folder.iter().map(|f| f.key.clone()).collect::<Vec<_>>(), folders);
    assert_eq!(s.by_folder.iter().find(|f| f.key.is_empty()).map(|f| f.count), Some(env.sql_i64("SELECT count(*) FROM image WHERE missing = 0 AND folder = ''")));
    assert_eq!(s.by_aspect.iter().find(|a| a.key == "unknown").map(|a| a.count), Some(1), "{:?}", s.by_aspect);
    let cats: BTreeMap<String, i64> = s.by_category.iter().map(|x| (x.key.clone(), x.count)).collect();
    assert_eq!(cats, BTreeMap::from([("code".into(), 2), ("chat".into(), 1), ("meme".into(), 1)]));
    assert_eq!(s.by_category[0].key, "code", "most common first");
    let tags: Vec<(String, i64)> = s.top_tags.iter().map(|x| (x.key.clone(), x.count)).collect();
    assert_eq!(tags, [("work".to_owned(), 6), ("bug".to_owned(), 2)]);
    assert_eq!(s.duplicates_exact, 1, "one redundant exact copy");
    let similar = dups(&env, DuplicateMode::Similar, None);
    assert_eq!(s.duplicates_similar, similar.groups.iter().map(|g| g.image_ids.len() as i64 - 1).sum::<i64>());
    // Neither planted pair is left (the steps above trashed B_PNG and emptied A_COPY's width), and the
    // generated fixtures, one drawing routine at several sizes, are no copies of each other.
    assert_eq!(s.duplicates_similar, 0);

    // The wire shape the UI sidebar reads: `get_stats()` (no args) -> `byFolder[].key/count`, counts flattened.
    let wire = serde_json::to_value(&s).unwrap();
    assert_eq!(wire["total"], json!(total));
    let by_folder = wire["byFolder"].as_array().unwrap();
    assert_eq!(by_folder.len(), s.by_folder.len());
    assert!(by_folder.iter().all(|f| f["key"].is_string() && f["count"].is_i64()), "{by_folder:?}");
}
