//! Phase 4.1: edit save (copy / overwrite with backup + undo), convert and stitch. An overwrite followed by
//! undo must give back the identical bytes (blake3), the row and its AI rows; a copy is a new row; a stitch
//! is as large as the images plus the gaps; `ssctl`-style CLI edits are refused under a protected root
//! (rule 8). Fixture libraries only (D10).

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::env::{Env, blake3_of};
use ss_core::contract::ipc::*;
use ss_core::contract::{Caller, ErrorKind};
use ss_core::fsaccess::OsFileAccess;
use ss_core::ops::{CropRect, SaveEditedImage};
use ss_core::paths;

/// A PNG of `w` x `h` with a pattern of its own.
fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x as u8) ^ seed, (y as u8).wrapping_mul(3), seed, 255]));
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).expect("encode png");
    out.into_inner()
}

/// Root-level fixtures with a ready thumbnail.
fn root_images(env: &Env) -> Vec<ImageSummary> {
    env.all().into_iter().filter(|s| s.folder.is_empty() && s.thumb_state == ThumbState::Ok).collect()
}

fn abs(env: &Env, id: i64) -> PathBuf {
    PathBuf::from(env.detail(id).abs_path)
}

fn backups(env: &Env) -> usize {
    std::fs::read_dir(paths::edit_backups_dir(env.data.path())).map(|d| d.count()).unwrap_or(0)
}

fn undo(env: &Env, batch: Option<i64>) -> OpResult {
    let r = env.core.undo(Caller::Ui, UndoReq { batch_id: batch }).expect("undo");
    assert!(r.failed.is_empty(), "undo failures: {:?}", r.failed);
    r
}

fn latest_op(env: &Env) -> JournalOp {
    env.core.list_journal(LimitReq { limit: 1 }).expect("journal").items[0].op
}

fn save(env: &Env, id: i64, mode: EditMode, format: ImageFormat, bytes: Vec<u8>) -> ss_core::ops::EditOutcome {
    env.core
        .save_edited_image(Caller::Ui, SaveEditedImage { image_id: id, mode, format, quality: None, png: bytes })
        .expect("save_edited_image")
}

#[test]
fn overwrite_then_undo_restores_identical_bytes() {
    let env = Env::new(6);
    env.index(true);
    let img = root_images(&env).remove(0);
    let path = abs(&env, img.id);
    let (h0, before) = (blake3_of(&path), env.detail(img.id));
    // Give the image AI rows: an overwrite must drop them (they describe the old pixels), undo must bring them back.
    env.exec(&format!(
        "INSERT INTO ocr_result (image_id, text, lines_json, engine, done_at) VALUES ({}, 'hoa don 123', '[]', 'test', 1);",
        img.id
    ));
    assert_eq!(env.detail(img.id).ocr_text.as_deref(), Some("hoa don 123"));

    let out = save(&env, img.id, EditMode::Overwrite, ImageFormat::Png, png(50, 40, 7));
    assert_eq!(out.image.id, img.id, "an overwrite keeps the row");
    assert_eq!((out.image.width, out.image.height), (Some(50), Some(40)));
    assert_eq!(out.image.thumb_state, ThumbState::Ok, "thumbnail made again after the overwrite");
    assert_ne!(blake3_of(&path), h0, "the file was overwritten");
    assert_eq!(backups(&env), 1, "the original is kept as a backup");
    assert_eq!(latest_op(&env), JournalOp::EditOverwrite);
    assert_eq!(env.detail(img.id).ocr_text, None, "OCR of the old pixels is dropped");

    let u = undo(&env, out.batch_id);
    assert_eq!(u.affected, 1);
    assert_eq!(blake3_of(&path), h0, "undo restores the identical bytes");
    let after = env.detail(img.id);
    assert_eq!((after.summary.width, after.summary.height), (before.summary.width, before.summary.height));
    assert_eq!(after.summary.file_name, before.summary.file_name);
    assert_eq!(after.summary.size_bytes, before.summary.size_bytes);
    assert_eq!(after.summary.thumb_state, ThumbState::Ok);
    assert_eq!(after.content_hash.as_deref(), Some(h0.as_str()), "hash recomputed from the restored file");
    assert_eq!(after.ocr_text.as_deref(), Some("hoa don 123"), "undo brings the AI rows back");
    assert_eq!(backups(&env), 0, "the spent backup is removed");
    // A second undo of the same batch is refused, not repeated.
    assert!(env.core.undo(Caller::Ui, UndoReq { batch_id: out.batch_id }).is_err());
}

#[test]
fn overwrite_to_another_format_renames_and_undo_restores_the_original() {
    let env = Env::new(6);
    env.index(true);
    let img = root_images(&env).remove(0);
    let path = abs(&env, img.id);
    let h0 = blake3_of(&path);
    let out = save(&env, img.id, EditMode::Overwrite, ImageFormat::Jpeg, png(64, 48, 3));
    let jpg = path.with_extension("jpg");
    assert_eq!(out.image.file_name, jpg.file_name().unwrap().to_string_lossy());
    assert!(jpg.is_file() && !path.exists(), "the .png became a .jpg");
    assert_eq!(&std::fs::read(&jpg).unwrap()[..2], &[0xFF, 0xD8], "JPEG bytes");

    undo(&env, out.batch_id);
    assert!(path.is_file() && !jpg.exists());
    assert_eq!(blake3_of(&path), h0);
    assert_eq!(env.detail(img.id).summary.file_name, img.file_name);
}

#[test]
fn undo_refuses_when_the_edited_file_changed_since() {
    let env = Env::new(4);
    env.index(true);
    let img = root_images(&env).remove(0);
    let path = abs(&env, img.id);
    let out = save(&env, img.id, EditMode::Overwrite, ImageFormat::Png, png(30, 30, 1));
    std::fs::write(&path, png(31, 31, 2)).unwrap();
    let r = env.core.undo(Caller::Ui, UndoReq { batch_id: out.batch_id }).expect("undo runs");
    assert_eq!(r.affected, 0);
    assert_eq!(r.failed.first().map(|f| f.kind), Some(ErrorKind::Conflict), "{r:?}");
    assert_eq!(backups(&env), 1, "the backup is kept when undo did not run");
}

#[test]
fn copy_creates_a_new_row_and_undo_removes_it() {
    let env = Env::new(6);
    env.index(true);
    let img = root_images(&env).remove(0);
    let path = abs(&env, img.id);
    let (h0, n0) = (blake3_of(&path), env.all().len());

    let out = save(&env, img.id, EditMode::Copy, ImageFormat::Png, png(80, 60, 9));
    let stem = Path::new(&img.file_name).file_stem().unwrap().to_string_lossy().into_owned();
    assert_ne!(out.image.id, img.id);
    assert_eq!(out.image.file_name, format!("{stem} (edited).png"));
    assert_eq!((out.image.width, out.image.height), (Some(80), Some(60)));
    assert_eq!(out.image.thumb_state, ThumbState::Ok);
    assert_eq!(out.image.taken_ms, img.taken_ms, "the copy sorts beside its original");
    let copy = abs(&env, out.image.id);
    assert!(copy.is_file());
    assert_eq!(blake3_of(&path), h0, "the original is untouched");
    assert_eq!(env.all().len(), n0 + 1, "one new row");
    assert_eq!(latest_op(&env), JournalOp::Copy);

    // A second copy takes the next free name.
    let again = save(&env, img.id, EditMode::Copy, ImageFormat::Webp, png(10, 10, 4));
    assert_eq!(again.image.file_name, format!("{stem} (edited).webp"));
    let third = save(&env, img.id, EditMode::Copy, ImageFormat::Png, png(10, 10, 5));
    assert_eq!(third.image.file_name, format!("{stem} (edited 2).png"));

    undo(&env, out.batch_id);
    assert!(!copy.exists(), "undo deletes the copy");
    assert!(env.all().iter().all(|s| s.id != out.image.id), "and its row");
}

#[test]
fn stitch_size_is_the_sum_plus_gaps() {
    let env = Env::new(8);
    env.index(true);
    let imgs: Vec<ImageSummary> = root_images(&env).into_iter().take(3).collect();
    assert_eq!(imgs.len(), 3);
    let dims: Vec<(i64, i64)> = imgs.iter().map(|s| (s.width.unwrap(), s.height.unwrap())).collect();
    let ids: Vec<i64> = imgs.iter().map(|s| s.id).collect();
    let gap = 10;

    let v = env
        .core
        .stitch_images(Caller::Ui, StitchImagesReq { image_ids: ids.clone(), direction: StitchDirection::Vertical, gap, background: "#112233".into() })
        .expect("stitch vertical");
    let want_v = (dims.iter().map(|d| d.0).max().unwrap(), dims.iter().map(|d| d.1).sum::<i64>() + 2 * gap);
    assert_eq!((v.image.width.unwrap(), v.image.height.unwrap()), want_v);
    let file_dims = image::image_dimensions(abs(&env, v.image.id)).unwrap();
    assert_eq!((file_dims.0 as i64, file_dims.1 as i64), want_v, "the PNG on disk has the same size");
    let stem = Path::new(&imgs[0].file_name).file_stem().unwrap().to_string_lossy().into_owned();
    assert_eq!(v.image.file_name, format!("{stem} (stitched).png"));
    assert_eq!(v.image.folder, imgs[0].folder, "next to the first image");

    let h = env
        .core
        .stitch_images(Caller::Ui, StitchImagesReq { image_ids: ids, direction: StitchDirection::Horizontal, gap: 0, background: "transparent".into() })
        .expect("stitch horizontal");
    let want_h = (dims.iter().map(|d| d.0).sum::<i64>(), dims.iter().map(|d| d.1).max().unwrap());
    assert_eq!((h.image.width.unwrap(), h.image.height.unwrap()), want_h);

    let one = env.core.stitch_images(Caller::Ui, StitchImagesReq { image_ids: vec![imgs[0].id], direction: StitchDirection::Vertical, gap: 0, background: "#fff".into() });
    assert_eq!(one.unwrap_err().kind, ErrorKind::InvalidArgument, "one image or a bad colour is refused");
}

#[test]
fn convert_overwrite_and_copy_then_undo() {
    let env = Env::new(8);
    env.index(true);
    let imgs: Vec<ImageSummary> = root_images(&env).into_iter().take(2).collect();
    let ids: Vec<i64> = imgs.iter().map(|s| s.id).collect();
    let hashes: Vec<String> = ids.iter().map(|id| blake3_of(&abs(&env, *id))).collect();

    let r = env
        .core
        .convert_images(Caller::Ui, ConvertImagesReq { image_ids: ids.clone(), format: ImageFormat::Webp, quality: Some(80), max_edge: Some(64), output: EditMode::Overwrite })
        .expect("convert overwrite");
    assert_eq!((r.affected, r.failed.len()), (2, 0), "{r:?}");
    assert_eq!(latest_op(&env), JournalOp::Convert);
    for id in &ids {
        let d = env.detail(*id);
        assert!(d.summary.file_name.ends_with(".webp"));
        assert!(d.summary.width.unwrap().max(d.summary.height.unwrap()) <= 64, "shrunk to maxEdge");
    }
    undo(&env, r.batch_id);
    for (id, h) in ids.iter().zip(&hashes) {
        assert_eq!(&blake3_of(&abs(&env, *id)), h, "undo restores every original");
    }

    let n0 = env.all().len();
    let c = env
        .core
        .convert_images(Caller::Ui, ConvertImagesReq { image_ids: ids, format: ImageFormat::Jpeg, quality: None, max_edge: None, output: EditMode::Copy })
        .expect("convert copy");
    assert_eq!(c.affected, 2);
    assert_eq!(env.all().len(), n0 + 2, "a new row per copy");
    undo(&env, c.batch_id);
    assert_eq!(env.all().len(), n0);
}

#[test]
fn cli_edit_under_a_protected_root_is_refused() {
    let env = Env::with(4, Arc::new(OsFileAccess), "OneDrive/Pictures");
    env.index(true);
    let img = root_images(&env).remove(0);
    let path = abs(&env, img.id);
    let h0 = blake3_of(&path);
    let crop = CropRect { x: 0, y: 0, width: 10, height: 10 };
    for mode in [EditMode::Copy, EditMode::Overwrite] {
        let e = env.core.crop_image(Caller::Cli, img.id, crop, mode).unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProtectedRoot, "{e:?}");
    }
    assert_eq!(blake3_of(&path), h0);
    assert_eq!(backups(&env), 0);
    // The UI may, through the journaled path.
    let out = env.core.crop_image(Caller::Ui, img.id, crop, EditMode::Overwrite).expect("UI crop");
    assert_eq!((out.image.width, out.image.height), (Some(10), Some(10)));
    undo(&env, out.batch_id);
    assert_eq!(blake3_of(&path), h0);
}
