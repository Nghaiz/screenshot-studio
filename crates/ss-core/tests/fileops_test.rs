//! Phase 2.1: move/copy/rename/trash/recycle and the organize ops, each followed by `undo`, which
//! must restore the path, the bytes (blake3) and the catalog row. Plus conflicts, rename preview,
//! the protected-root guard (rule 8) and `list_journal`. Fixture libraries only (D10).

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::env::{Env, blake3_of};
use ss_core::contract::ipc::*;
use ss_core::contract::{Caller, ErrorKind};
use ss_core::fsaccess::OsFileAccess;
use ss_core::paths;

#[derive(Debug, Clone, PartialEq)]
struct State {
    abs: String,
    hash: String,
    summary: ImageSummary,
    tags: Vec<i64>,
    albums: Vec<i64>,
    note: Option<String>,
}

fn state(env: &Env, id: i64) -> State {
    let d = env.detail(id);
    State {
        hash: blake3_of(Path::new(&d.abs_path)),
        abs: d.abs_path,
        summary: d.summary,
        tags: d.tags.iter().map(|t| t.id).collect(),
        albums: d.albums.iter().map(|a| a.id).collect(),
        note: d.note,
    }
}

/// A root-level fixture (folder "") with a ready thumbnail.
fn root_image(env: &Env, nth: usize) -> ImageSummary {
    env.all().into_iter().filter(|s| s.folder.is_empty()).nth(nth).expect("root-level fixture")
}

fn undo(env: &Env, r: &OpResult) -> OpResult {
    let u = env.core.undo(Caller::Ui, UndoReq { batch_id: r.batch_id }).expect("undo");
    assert!(u.failed.is_empty(), "undo failures: {:?}", u.failed);
    u
}

fn mv(env: &Env, ids: Vec<i64>, dest: &Path, mode: MoveMode, on_conflict: OnConflict) -> OpResult {
    env.core
        .move_images(Caller::Ui, MoveImagesReq { image_ids: ids, dest_dir: dest.to_string_lossy().into_owned(), mode, on_conflict })
        .expect("move_images")
}

fn ok(r: &OpResult, n: i64) {
    assert!(r.failed.is_empty(), "failures: {:?}", r.failed);
    assert_eq!(r.affected, n, "{r:?}");
    assert!(r.batch_id.is_some(), "journaled: {r:?}");
}

#[test]
fn move_within_root_then_undo() {
    let env = Env::new(8);
    env.index(true);
    let img = root_image(&env, 0);
    let before = state(&env, img.id);
    let dest = env.root_dir().join("Moved").join("deep");
    std::fs::create_dir_all(&dest).unwrap();

    let r = mv(&env, vec![img.id], &dest, MoveMode::Move, OnConflict::Skip);
    ok(&r, 1);
    let after = state(&env, img.id);
    assert_eq!(after.summary.folder, "Moved/deep");
    assert_eq!(after.hash, before.hash, "same bytes");
    assert!(!Path::new(&before.abs).exists(), "source gone");
    assert!(dest.join(&img.file_name).is_file());
    assert_eq!(env.core.query_images(&ImageQuery { text: Some(img.file_name.replace(".png", "")), ..ss_core::query::default_query(10) }).unwrap().total, 1);

    undo(&env, &r);
    assert_eq!(state(&env, img.id), before);
    assert!(!dest.join(&img.file_name).exists());
    // A second undo of the same batch is refused.
    let again = env.core.undo(Caller::Ui, UndoReq { batch_id: r.batch_id }).unwrap_err();
    assert_eq!(again.kind, ErrorKind::InvalidArgument);
    // Moving into the folder it is already in is a per-item failure, not a silent no-op.
    let same = mv(&env, vec![img.id], &env.root_dir(), MoveMode::Move, OnConflict::Skip);
    assert_eq!((same.affected, same.failed.len()), (0, 1));
}

#[test]
fn move_out_of_library_removes_row_and_undo_restores_tags_notes_albums_thumbs() {
    let env = Env::new(6);
    env.index(true);
    let img = root_image(&env, 1);
    let c = &env.core;
    let tag = c.create_tag(Caller::Ui, CreateTagReq { name: "keep".into(), color: "#AABBCC".into() }).unwrap();
    c.set_tags(Caller::Ui, SetTagsReq { image_ids: vec![img.id], add: vec![tag.id], remove: vec![] }).unwrap();
    c.set_note(Caller::Ui, SetNoteReq { image_id: img.id, note: Some("remember me".into()) }).unwrap();
    let album = c.create_album(Caller::Ui, CreateAlbumReq { name: "A".into() }).unwrap();
    c.album_add(Caller::Ui, AlbumImagesReq { album_id: album.id, image_ids: vec![img.id] }).unwrap();
    let before = state(&env, img.id);
    let thumb = paths::thumb_path(env.core.data_dir(), img.id, 320);
    assert!(thumb.is_file());

    let outside = tempfile::tempdir().unwrap();
    let r = mv(&env, vec![img.id], outside.path(), MoveMode::Move, OnConflict::Skip);
    ok(&r, 1);
    assert_eq!(c.get_image(GetImageReq { id: img.id }).unwrap_err().kind, ErrorKind::NotFound, "row removed");
    assert!(outside.path().join(&img.file_name).is_file());
    assert!(!thumb.exists(), "live thumbnail parked");
    assert_eq!(env.sql_i64("SELECT count(*) FROM text_fts WHERE rowid NOT IN (SELECT id FROM image)"), 0, "no orphan FTS row");

    undo(&env, &r);
    assert_eq!(state(&env, img.id), before, "same id, tags, note, album, bytes");
    assert!(thumb.is_file(), "thumbnail back");
    assert_eq!(c.ssimg(&format!("/thumb/{}/320", img.id)).status, 200);
    assert_eq!(c.query_images(&ImageQuery { text: Some("remember".into()), ..ss_core::query::default_query(10) }).unwrap().total, 1);
}

#[test]
fn copy_inside_and_outside_then_undo_removes_copies() {
    let env = Env::new(6);
    env.index(true);
    let img = root_image(&env, 0);
    let before = state(&env, img.id);
    let n0 = env.sql_i64("SELECT count(*) FROM image");

    let dest = env.root_dir().join("Copies");
    std::fs::create_dir_all(&dest).unwrap();
    let r = mv(&env, vec![img.id], &dest, MoveMode::Copy, OnConflict::Skip);
    ok(&r, 1);
    let copy = env.all().into_iter().find(|s| s.folder == "Copies").expect("copy row");
    assert_eq!((copy.file_name.as_str(), copy.thumb_state), (img.file_name.as_str(), ThumbState::Ok));
    assert_eq!(blake3_of(&dest.join(&img.file_name)), before.hash);
    assert_eq!(state(&env, img.id), before, "source untouched");

    let outside = tempfile::tempdir().unwrap();
    let r2 = mv(&env, vec![img.id], outside.path(), MoveMode::Copy, OnConflict::Skip);
    ok(&r2, 1);
    assert!(outside.path().join(&img.file_name).is_file());
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), n0 + 1, "an outside copy has no row");

    undo(&env, &r2);
    assert!(!outside.path().join(&img.file_name).exists());
    undo(&env, &r);
    assert!(!dest.join(&img.file_name).exists());
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), n0);
    assert!(!paths::thumb_path(env.core.data_dir(), copy.id, 320).exists());
    assert_eq!(state(&env, img.id), before);

    // An edited copy is never deleted by undo.
    let r3 = mv(&env, vec![img.id], outside.path(), MoveMode::Copy, OnConflict::Skip);
    std::fs::write(outside.path().join(&img.file_name), b"changed by the user").unwrap();
    let u = env.core.undo(Caller::Ui, UndoReq { batch_id: r3.batch_id }).unwrap();
    assert_eq!((u.affected, u.failed[0].kind), (0, ErrorKind::Conflict));
    assert!(outside.path().join(&img.file_name).is_file());
}

#[test]
fn conflicts_skip_or_rename() {
    let env = Env::new(6);
    env.index(false);
    let img = root_image(&env, 0);
    let skip = mv(&env, vec![img.id], &env.root_dir(), MoveMode::Copy, OnConflict::Skip);
    assert_eq!((skip.affected, skip.batch_id, skip.failed[0].kind), (0, None, ErrorKind::Conflict));
    let ren = mv(&env, vec![img.id], &env.root_dir(), MoveMode::Copy, OnConflict::Rename);
    ok(&ren, 1);
    let (stem, ext) = img.file_name.rsplit_once('.').unwrap();
    let want = format!("{stem} (1).{ext}");
    assert!(env.root_dir().join(&want).is_file());
    env.by_name(&want);
    let missing = env.core.move_images(Caller::Ui, MoveImagesReq {
        image_ids: vec![img.id],
        dest_dir: env.root_dir().join("nope").to_string_lossy().into_owned(),
        mode: MoveMode::Move,
        on_conflict: OnConflict::Skip,
    });
    assert_eq!(missing.unwrap_err().kind, ErrorKind::NotFound, "destination must exist");
    let unknown = mv(&env, vec![999_999], &env.root_dir(), MoveMode::Move, OnConflict::Skip);
    assert_eq!(unknown.failed[0].kind, ErrorKind::NotFound);
}

#[test]
fn rename_preview_and_undo() {
    let env = Env::new(8);
    env.index(false);
    let rows: Vec<ImageSummary> = env.all().into_iter().filter(|s| s.folder.is_empty()).take(2).collect();
    let ids: Vec<i64> = rows.iter().map(|s| s.id).collect();
    let before: Vec<State> = ids.iter().map(|id| state(&env, *id)).collect();

    let p = env.core.preview_rename(PreviewRenameReq { image_ids: ids.clone(), pattern: "shot_{n:03}_{w}x{h}".into() }).unwrap();
    assert_eq!(p.items.len(), 2);
    for (i, it) in p.items.iter().enumerate() {
        let s = &rows[i];
        assert_eq!(it.new_name, format!("shot_{:03}_{}x{}.png", i + 1, s.width.unwrap(), s.height.unwrap()));
        assert!(!it.conflict);
    }
    let dated = env.core.preview_rename(PreviewRenameReq { image_ids: vec![ids[0]], pattern: "{date:YYYY-MM-DD}_{time:HHmmss}.{ext}".into() }).unwrap();
    assert!(dated.items[0].new_name.len() == "2020-01-01_000000.png".len(), "{:?}", dated.items[0]);
    let same = env.core.preview_rename(PreviewRenameReq { image_ids: ids.clone(), pattern: "same".into() }).unwrap();
    assert!(same.items.iter().any(|i| i.conflict), "two images to one name");
    let bad = env.core.preview_rename(PreviewRenameReq { image_ids: ids.clone(), pattern: "{nope}".into() });
    assert_eq!(bad.unwrap_err().kind, ErrorKind::InvalidArgument);

    let items = p.items.iter().map(|i| RenameItem { image_id: i.image_id, new_name: i.new_name.clone() }).collect();
    let r = env.core.rename_images(Caller::Ui, RenameImagesReq { items }).unwrap();
    ok(&r, 2);
    for (it, b) in p.items.iter().zip(&before) {
        let s = state(&env, it.image_id);
        assert_eq!((s.summary.file_name.as_str(), s.hash.as_str()), (it.new_name.as_str(), b.hash.as_str()));
        assert!(!Path::new(&b.abs).exists());
    }
    assert_eq!(env.core.query_images(&ImageQuery { text: Some("shot".into()), ..ss_core::query::default_query(10) }).unwrap().total, 2);
    undo(&env, &r);
    for (id, b) in ids.iter().zip(&before) {
        assert_eq!(&state(&env, *id), b);
    }

    // Invalid and colliding names are per-item failures.
    let r = env
        .core
        .rename_images(Caller::Ui, RenameImagesReq {
            items: vec![
                RenameItem { image_id: ids[0], new_name: "bad:name.png".into() },
                RenameItem { image_id: ids[1], new_name: rows[0].file_name.clone() },
            ],
        })
        .unwrap();
    assert_eq!(r.affected, 0);
    assert_eq!(r.failed.iter().map(|f| f.kind).collect::<Vec<_>>(), vec![ErrorKind::InvalidArgument, ErrorKind::Conflict]);
}

#[test]
fn trash_restore_then_undo() {
    let env = Env::new(5);
    env.index(false);
    let img = root_image(&env, 0);
    let before = state(&env, img.id);
    let r = env.core.trash_images(Caller::Ui, ImageIdsReq { image_ids: vec![img.id] }).unwrap();
    ok(&r, 1);
    assert!(state(&env, img.id).summary.trashed_at.is_some());
    assert_eq!(state(&env, img.id).hash, before.hash, "in-app trash never touches the file");
    undo(&env, &r);
    assert_eq!(state(&env, img.id), before);

    env.core.trash_images(Caller::Ui, ImageIdsReq { image_ids: vec![img.id] }).unwrap();
    let trashed = state(&env, img.id);
    let r = env.core.restore_images(Caller::Ui, ImageIdsReq { image_ids: vec![img.id] }).unwrap();
    ok(&r, 1);
    assert_eq!(state(&env, img.id).summary.trashed_at, None);
    undo(&env, &r);
    assert_eq!(state(&env, img.id), trashed);
}

#[test]
fn recycle_bin_then_undo_restores_file_and_row() {
    let env = Env::new(5);
    env.index(true);
    let img = root_image(&env, 0);
    let tag = env.core.create_tag(Caller::Ui, CreateTagReq { name: "rb".into(), color: "#010203".into() }).unwrap();
    env.core.set_tags(Caller::Ui, SetTagsReq { image_ids: vec![img.id], add: vec![tag.id], remove: vec![] }).unwrap();
    let before = state(&env, img.id);

    let r = env.core.recycle_images(Caller::Ui, ImageIdsReq { image_ids: vec![img.id] }).unwrap();
    ok(&r, 1);
    assert!(!Path::new(&before.abs).exists(), "file in the Recycle Bin");
    assert_eq!(env.core.get_image(GetImageReq { id: img.id }).unwrap_err().kind, ErrorKind::NotFound);

    undo(&env, &r);
    assert_eq!(state(&env, img.id), before, "file restored from the Recycle Bin, row with its tag");
}

#[test]
fn organize_ops_then_undo() {
    let env = Env::new(6);
    env.index(false);
    let all = env.all();
    let (a, b, c3) = (all[0].id, all[1].id, all[2].id);
    let before: Vec<State> = [a, b, c3].iter().map(|i| state(&env, *i)).collect();
    let back = |env: &Env| {
        for s in &before {
            assert_eq!(&state(env, s.summary.id), s);
        }
    };
    let c = &env.core;
    let t = c.create_tag(Caller::Ui, CreateTagReq { name: "t".into(), color: "#123456".into() }).unwrap();
    let album = c.create_album(Caller::Ui, CreateAlbumReq { name: "al".into() }).unwrap();

    let r = c.set_favorite(Caller::Ui, SetFavoriteReq { image_ids: vec![a, b], value: true }).unwrap();
    ok(&r, 2);
    undo(&env, &r);
    back(&env);
    let r = c.set_rating(Caller::Ui, SetRatingReq { image_ids: vec![a], rating: 4 }).unwrap();
    ok(&r, 1);
    undo(&env, &r);
    back(&env);
    assert_eq!(c.set_rating(Caller::Ui, SetRatingReq { image_ids: vec![a], rating: 7 }).unwrap_err().kind, ErrorKind::InvalidArgument);
    let r = c.set_note(Caller::Ui, SetNoteReq { image_id: a, note: Some("n".into()) }).unwrap();
    ok(&r, 1);
    undo(&env, &r);
    back(&env);
    let r = c.set_tags(Caller::Ui, SetTagsReq { image_ids: vec![a, b], add: vec![t.id], remove: vec![] }).unwrap();
    ok(&r, 2);
    assert_eq!(c.list_tags().unwrap()[0].count, 2);
    undo(&env, &r);
    back(&env);

    // Album add, reorder, remove: each undone in turn.
    let add = c.album_add(Caller::Ui, AlbumImagesReq { album_id: album.id, image_ids: vec![a, b, c3] }).unwrap();
    ok(&add, 3);
    let order = |env: &Env| {
        let q = ImageQuery { album_id: Some(album.id), ..ss_core::query::default_query(10) };
        env.core.query_images(&q).unwrap().total
    };
    assert_eq!(order(&env), 3);
    assert_eq!(c.list_albums().unwrap()[0].cover_image_id, Some(a));
    let re = c.album_reorder(Caller::Ui, AlbumImagesReq { album_id: album.id, image_ids: vec![c3, a, b] }).unwrap();
    ok(&re, 3);
    assert_eq!(c.list_albums().unwrap()[0].cover_image_id, Some(c3));
    undo(&env, &re);
    assert_eq!(c.list_albums().unwrap()[0].cover_image_id, Some(a));
    let bad = c.album_reorder(Caller::Ui, AlbumImagesReq { album_id: album.id, image_ids: vec![a] });
    assert_eq!(bad.unwrap_err().kind, ErrorKind::InvalidArgument);
    let rm = c.album_remove(Caller::Ui, AlbumImagesReq { album_id: album.id, image_ids: vec![b] }).unwrap();
    ok(&rm, 1);
    assert_eq!(order(&env), 2);
    undo(&env, &rm);
    assert_eq!(order(&env), 3);
    undo(&env, &add);
    assert_eq!(order(&env), 0);
    back(&env);

    // Container ops are catalog-only, not journaled.
    let dt = c.delete_tag(Caller::Ui, DeleteTagReq { tag_id: t.id }).unwrap();
    assert_eq!(dt.batch_id, None);
    assert_eq!(c.create_tag(Caller::Ui, CreateTagReq { name: "T2".into(), color: "#zzzzzz".into() }).unwrap_err().kind, ErrorKind::InvalidArgument);
    c.create_tag(Caller::Ui, CreateTagReq { name: "dup".into(), color: "#000000".into() }).unwrap();
    assert_eq!(c.create_tag(Caller::Ui, CreateTagReq { name: "DUP".into(), color: "#000000".into() }).unwrap_err().kind, ErrorKind::Conflict);

    // Latest-undoable undo (batchId null) takes the newest batch.
    let fav = c.set_favorite(Caller::Ui, SetFavoriteReq { image_ids: vec![c3], value: true }).unwrap();
    let u = c.undo(Caller::Ui, UndoReq { batch_id: None }).unwrap();
    assert_eq!(u.batch_id, fav.batch_id);
    back(&env);

    let j = c.list_journal(LimitReq { limit: 50 }).unwrap();
    assert_eq!(j.items[0].batch_id, fav.batch_id.unwrap());
    assert_eq!((j.items[0].op, j.items[0].count, j.items[0].undoable), (JournalOp::Favorite, 1, true));
    assert!(j.items.iter().all(|e| e.undone_at.is_some()), "every batch above was undone: {:?}", j.items);
    assert!(j.items.iter().any(|e| e.op == JournalOp::AlbumReorder));
    assert_eq!(c.list_journal(LimitReq { limit: 0 }).unwrap_err().kind, ErrorKind::InvalidArgument);
}

#[test]
fn protected_root_file_writes_are_ui_only_and_journaled() {
    let env = Env::with(4, Arc::new(OsFileAccess), "OneDrive/Pictures/Screenshots");
    assert!(env.root.protected_root);
    env.index(false);
    let img = root_image(&env, 0);
    let dest: PathBuf = env.root_dir().join("sorted");
    std::fs::create_dir_all(&dest).unwrap();
    let req = || MoveImagesReq {
        image_ids: vec![img.id],
        dest_dir: dest.to_string_lossy().into_owned(),
        mode: MoveMode::Move,
        on_conflict: OnConflict::Skip,
    };
    let cli = env.core.move_images(Caller::Cli, req()).unwrap();
    assert_eq!((cli.affected, cli.failed[0].kind), (0, ErrorKind::ProtectedRoot));
    assert!(Path::new(&env.detail(img.id).abs_path).is_file(), "nothing moved");
    let cli_rename = env.core.rename_images(Caller::Cli, RenameImagesReq { items: vec![RenameItem { image_id: img.id, new_name: "x.png".into() }] }).unwrap();
    assert_eq!(cli_rename.failed[0].kind, ErrorKind::ProtectedRoot);
    let cli_recycle = env.core.recycle_images(Caller::Cli, ImageIdsReq { image_ids: vec![img.id] }).unwrap();
    assert_eq!(cli_recycle.failed[0].kind, ErrorKind::ProtectedRoot);

    // A destination under a OneDrive path is protected too, whatever the source.
    let plain = Env::new(4); // fixtures 0 and 1 live in sub folders
    plain.index(false);
    let p_img = root_image(&plain, 0);
    let cli_dest = plain.core.move_images(Caller::Cli, MoveImagesReq { image_ids: vec![p_img.id], ..req() }).unwrap();
    assert_eq!(cli_dest.failed[0].kind, ErrorKind::ProtectedRoot);

    let ui = env.core.move_images(Caller::Ui, req()).unwrap();
    ok(&ui, 1);
    let j = env.core.list_journal(LimitReq { limit: 5 }).unwrap();
    assert_eq!((j.items[0].op, j.items[0].batch_id), (JournalOp::Move, ui.batch_id.unwrap()));
    // ...and its undo is UI-only as well.
    let u = env.core.undo(Caller::Cli, UndoReq { batch_id: ui.batch_id }).unwrap();
    assert_eq!((u.affected, u.failed[0].kind), (0, ErrorKind::ProtectedRoot));
    undo(&env, &ui);
}
