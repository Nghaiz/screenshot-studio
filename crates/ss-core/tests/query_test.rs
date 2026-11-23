//! Phase 2.1: every `ImageQuery` filter alone and combined, FTS text search (Vietnamese diacritics
//! folded), relevance paging, timeline under filters, and the 5,600-row timing target.

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::collections::BTreeSet;
use std::time::Instant;

use common::env::Env;
use rusqlite::params;
use ss_core::contract::ipc::*;
use ss_core::contract::{Caller, ErrorKind};
use ss_core::{catalog, query};

fn q() -> ImageQuery {
    query::default_query(500)
}

fn ids(rows: &[ImageSummary]) -> BTreeSet<i64> {
    rows.iter().map(|s| s.id).collect()
}

/// Ids matching `qq`, checked against the ids of `all` that satisfy `pred`.
fn check(env: &Env, what: &str, qq: ImageQuery, all: &[ImageSummary], pred: impl Fn(&ImageSummary) -> bool) -> BTreeSet<i64> {
    let got = ids(&env.all_of(qq.clone()));
    let want: BTreeSet<i64> = all.iter().filter(|s| pred(s)).map(|s| s.id).collect();
    assert_eq!(got, want, "{what}");
    assert_eq!(env.core.query_images(&qq).unwrap().total, want.len() as i64, "{what}: total");
    got
}

/// A library of 60 fixtures with tags, an album, favourites, ratings, a note, colours, OCR rows and
/// categories set on known images.
struct Lib {
    env: Env,
    all: Vec<ImageSummary>,
    tag_a: i64,
    tag_b: i64,
    album: i64,
}

fn lib() -> Lib {
    let env = Env::new(60);
    env.index(false);
    let all = env.all();
    assert_eq!(all.len(), 60);
    let id = |i: usize| all[i].id;
    let c = &env.core;
    let tag_a = c.create_tag(Caller::Ui, CreateTagReq { name: "work".into(), color: "#112233".into() }).unwrap().id;
    let tag_b = c.create_tag(Caller::Ui, CreateTagReq { name: "Bug".into(), color: "#445566".into() }).unwrap().id;
    c.set_tags(Caller::Ui, SetTagsReq { image_ids: (0..10).map(id).collect(), add: vec![tag_a], remove: vec![] }).unwrap();
    c.set_tags(Caller::Ui, SetTagsReq { image_ids: (5..15).map(id).collect(), add: vec![tag_b], remove: vec![] }).unwrap();
    let album = c.create_album(Caller::Ui, CreateAlbumReq { name: "Trip".into() }).unwrap().id;
    c.album_add(Caller::Ui, AlbumImagesReq { album_id: album, image_ids: vec![id(3), id(20), id(40)] }).unwrap();
    c.set_favorite(Caller::Ui, SetFavoriteReq { image_ids: (0..60).step_by(4).map(id).collect(), value: true }).unwrap();
    for (i, r) in [(1, 1), (2, 3), (7, 5), (30, 4)] {
        c.set_rating(Caller::Ui, SetRatingReq { image_ids: vec![id(i)], rating: r }).unwrap();
    }
    c.set_note(Caller::Ui, SetNoteReq { image_id: id(11), note: Some("hóa đơn tháng mười".into()) }).unwrap();
    // AI columns are filled by phase 3.2; write them directly here.
    let (a, b, d, e, f) = (id(12), id(13), id(14), id(15), id(16));
    c.catalog()
        .write(move |tx| {
            tx.execute("UPDATE image SET dominant_rgb = '#FF0000' WHERE id = ?1", [a])?;
            tx.execute("UPDATE image SET dominant_rgb = '#f00a0a' WHERE id = ?1", [b])?;
            tx.execute("UPDATE image SET dominant_rgb = '#0000FF' WHERE id = ?1", [d])?;
            for (img, text) in [(e, "Xin chào thế giới invoice 42"), (f, "   ")] {
                tx.execute(
                    "INSERT INTO ocr_result (image_id, text, lines_json, engine, done_at) VALUES (?1, ?2, '[]', 'test', 0)",
                    params![img, text],
                )?;
                tx.execute("UPDATE text_fts SET ocr_text = ?2 WHERE rowid = ?1", params![img, text])?;
            }
            for (img, label) in [(a, "code"), (b, "chat"), (e, "code")] {
                tx.execute(
                    "INSERT INTO category (image_id, label, score, model) VALUES (?1, ?2, 0.9, 'test')",
                    params![img, label],
                )?;
            }
            Ok(())
        })
        .unwrap();
    let all = env.all();
    Lib { env, all, tag_a, tag_b, album }
}

#[test]
fn each_filter_alone() {
    let Lib { env, all, tag_a, tag_b, album } = lib();
    let detail = |id: i64| env.detail(id);

    // Text: Vietnamese diacritics folded, words ANDed, prefix match.
    let vi = check(&env, "anh chup", ImageQuery { text: Some("anh chup".into()), ..q() }, &all, |s| s.file_name.starts_with("Ảnh chụp"));
    assert!(!vi.is_empty());
    check(&env, "diacritics in the query", ImageQuery { text: Some("ẢNH CHỤP".into()), ..q() }, &all, |s| s.file_name.starts_with("Ảnh chụp"));
    check(&env, "prefix", ImageQuery { text: Some("scree".into()), ..q() }, &all, |s| s.file_name.starts_with("Screenshot"));
    check(&env, "filename mode", ImageQuery { text: Some("anh chup".into()), text_mode: TextMode::Filename, ..q() }, &all, |s| {
        s.file_name.starts_with("Ảnh chụp")
    });
    check(&env, "note via any", ImageQuery { text: Some("hoa thang".into()), ..q() }, &all, |s| s.id == all[11].id);
    check(&env, "note not in filename mode", ImageQuery { text: Some("hoa thang".into()), text_mode: TextMode::Filename, ..q() }, &all, |_| false);
    check(&env, "ocr mode", ImageQuery { text: Some("chao invoice".into()), text_mode: TextMode::Ocr, ..q() }, &all, |s| s.id == all[15].id);
    check(&env, "FTS syntax is inert", ImageQuery { text: Some("\"anh\" OR NEAR(".into()), ..q() }, &all, |_| false);
    check(&env, "only punctuation", ImageQuery { text: Some(" - * ".into()), ..q() }, &all, |_| false);
    check(&env, "blank text = no constraint", ImageQuery { text: Some("   ".into()), ..q() }, &all, |_| true);
    let e = env.core.query_images(&ImageQuery { text: Some("cat".into()), text_mode: TextMode::Semantic, ..q() }).unwrap_err();
    assert_eq!(e.kind, ErrorKind::SidecarUnavailable);

    // Dimensions and size.
    let w = |s: &ImageSummary| s.width.unwrap();
    let h = |s: &ImageSummary| s.height.unwrap();
    check(&env, "minWidth", ImageQuery { min_width: Some(200), ..q() }, &all, |s| w(s) >= 200);
    check(&env, "maxWidth", ImageQuery { max_width: Some(200), ..q() }, &all, |s| w(s) <= 200);
    check(&env, "minHeight", ImageQuery { min_height: Some(170), ..q() }, &all, |s| h(s) >= 170);
    check(&env, "maxHeight", ImageQuery { max_height: Some(170), ..q() }, &all, |s| h(s) <= 170);
    let mut sizes: Vec<i64> = all.iter().map(|s| s.size_bytes).collect();
    sizes.sort_unstable();
    let mid = sizes[sizes.len() / 2];
    check(&env, "minSize", ImageQuery { min_size: Some(mid), ..q() }, &all, |s| s.size_bytes >= mid);
    check(&env, "maxSize", ImageQuery { max_size: Some(mid), ..q() }, &all, |s| s.size_bytes <= mid);
    for a in [AspectClass::Phone, AspectClass::Pc, AspectClass::Square, AspectClass::Wide, AspectClass::Other] {
        let got = check(&env, "aspect", ImageQuery { aspects: vec![a], ..q() }, &all, |s| s.aspect == Some(a));
        assert!(!got.is_empty(), "{a:?} has fixtures");
    }

    // Folders: in or below; "" = the root folder itself.
    check(&env, "folder 4ERD", ImageQuery { folders: vec!["4ERD".into()], ..q() }, &all, |s| s.folder.starts_with("4ERD"));
    check(&env, "folder 4erd/sub", ImageQuery { folders: vec!["4erd/sub".into()], ..q() }, &all, |s| s.folder == "4ERD/sub");
    check(&env, "root folder", ImageQuery { folders: vec!["".into()], ..q() }, &all, |s| s.folder.is_empty());
    check(&env, "two folders", ImageQuery { folders: vec!["1ERD".into(), "".into()], ..q() }, &all, |s| {
        s.folder.is_empty() || s.folder == "1ERD"
    });

    // Tags, album, favourites, rating, text presence, colour, categories, trash.
    let has = |s: &ImageSummary, t: i64| detail(s.id).tags.iter().any(|x| x.id == t);
    check(&env, "tag any", ImageQuery { tag_ids: vec![tag_a, tag_b], ..q() }, &all, |s| has(s, tag_a) || has(s, tag_b));
    let both = check(&env, "tag all", ImageQuery { tag_ids: vec![tag_a, tag_b], tag_match: TagMatch::All, ..q() }, &all, |s| {
        has(s, tag_a) && has(s, tag_b)
    });
    assert_eq!(both.len(), 5);
    check(&env, "album", ImageQuery { album_id: Some(album), ..q() }, &all, |s| [3, 20, 40].iter().any(|i| all[*i].id == s.id));
    check(&env, "favoriteOnly", ImageQuery { favorite_only: true, ..q() }, &all, |s| s.favorite);
    check(&env, "minRating", ImageQuery { min_rating: 4, ..q() }, &all, |s| s.rating >= 4);
    check(&env, "hasText true", ImageQuery { has_text: Some(true), ..q() }, &all, |s| s.has_text == Some(true));
    check(&env, "hasText false", ImageQuery { has_text: Some(false), ..q() }, &all, |s| s.has_text == Some(false));
    let red = |tol| ImageQuery { color: Some("#FF0000".into()), color_tolerance: tol, ..q() };
    check(&env, "colour exact", red(0), &all, |s| s.id == all[12].id);
    check(&env, "colour near", red(5), &all, |s| s.id == all[12].id || s.id == all[13].id);
    check(&env, "colour any", red(100), &all, |s| s.dominant_rgb.is_some());
    check(&env, "categories", ImageQuery { categories: vec!["code".into()], ..q() }, &all, |s| s.id == all[12].id || s.id == all[15].id);
    env.core.trash_images(Caller::Ui, ImageIdsReq { image_ids: vec![all[50].id] }).unwrap();
    check(&env, "trash only", ImageQuery { trash: TrashFilter::Only, ..q() }, &all, |s| s.id == all[50].id);
    check(&env, "trash excluded", q(), &all, |s| s.id != all[50].id);

    // Invalid inputs are refused, never ignored.
    for (what, bad) in [
        ("colour", ImageQuery { color: Some("red".into()), ..q() }),
        ("tolerance", ImageQuery { color_tolerance: 101, ..q() }),
        ("rating", ImageQuery { min_rating: 6, ..q() }),
        ("relevance without text", ImageQuery { sort: SortSpec { field: SortField::Relevance, dir: SortDir::Desc }, ..q() }),
    ] {
        assert_eq!(env.core.query_images(&bad).unwrap_err().kind, ErrorKind::InvalidArgument, "{what}");
    }
}

#[test]
fn filters_combine_and_relevance_pages_once() {
    let Lib { env, all, tag_a, .. } = lib();
    let has_a = |s: &ImageSummary| env.detail(s.id).tags.iter().any(|t| t.id == tag_a);
    let combined = ImageQuery {
        text: Some("screenshot".into()),
        folders: vec!["".into(), "1ERD".into()],
        min_width: Some(100),
        tag_ids: vec![tag_a],
        ..q()
    };
    check(&env, "text + folders + width + tag", combined, &all, |s| {
        s.file_name.starts_with("Screenshot") && (s.folder.is_empty() || s.folder == "1ERD") && s.width.unwrap() >= 100 && has_a(s)
    });
    check(&env, "favourite + aspect + rating", ImageQuery { favorite_only: true, aspects: vec![AspectClass::Phone], min_rating: 0, ..q() }, &all, |s| {
        s.favorite && s.aspect == Some(AspectClass::Phone)
    });

    // Relevance sort, 3 per page: every match exactly once, best first.
    let mut rq = ImageQuery {
        text: Some("screenshot".into()),
        sort: SortSpec { field: SortField::Relevance, dir: SortDir::Desc },
        limit: 3,
        ..q()
    };
    let mut seen = Vec::new();
    loop {
        let page = env.core.query_images(&rq).unwrap();
        seen.extend(page.items.iter().map(|s| s.id));
        match page.next_cursor {
            Some(c) => rq.cursor = Some(c),
            None => break,
        }
    }
    let unique: BTreeSet<i64> = seen.iter().copied().collect();
    assert_eq!(unique.len(), seen.len(), "no id twice");
    let want: BTreeSet<i64> = all.iter().filter(|s| s.file_name.starts_with("Screenshot")).map(|s| s.id).collect();
    assert_eq!(unique, want);

    // Timeline honours the same filters.
    let tq = ImageQuery { folders: vec!["4ERD".into()], ..q() };
    let total = env.core.query_images(&tq).unwrap().total;
    let t = env.core.get_timeline(&GetTimelineReq { query: tq, bucket: TimelineBucketSize::Month }).unwrap();
    assert_eq!(t.buckets.iter().map(|b| b.count).sum::<i64>(), total);

    // Saved smart filters keep the query (cursor dropped) and reject invalid ones.
    let saved = env
        .core
        .save_smart_filter(Caller::Ui, SaveSmartFilterReq { id: None, name: "Tagged".into(), query: ImageQuery { tag_ids: vec![tag_a], cursor: Some("x".into()), ..q() } })
        .unwrap();
    assert_eq!(saved.query.cursor, None);
    assert_eq!(env.core.list_smart_filters().unwrap().len(), 1);
    let dup = env.core.save_smart_filter(Caller::Ui, SaveSmartFilterReq { id: None, name: "Tagged".into(), query: q() });
    assert_eq!(dup.unwrap_err().kind, ErrorKind::Conflict);
    let bad = env.core.save_smart_filter(Caller::Ui, SaveSmartFilterReq { id: None, name: "Bad".into(), query: ImageQuery { min_rating: 9, ..q() } });
    assert_eq!(bad.unwrap_err().kind, ErrorKind::InvalidArgument);
    let renamed = env.core.save_smart_filter(Caller::Ui, SaveSmartFilterReq { id: Some(saved.id), name: "Tagged A".into(), query: saved.query.clone() }).unwrap();
    assert_eq!((renamed.id, renamed.name.as_str()), (saved.id, "Tagged A"));
    env.core.delete_smart_filter(Caller::Ui, DeleteSmartFilterReq { id: saved.id }).unwrap();
    assert!(env.core.list_smart_filters().unwrap().is_empty());
}

const TIMING_ROWS: i64 = 5_600;

#[test]
fn query_5600_rows_under_target() {
    let env = Env::new(0);
    let root = env.root.id;
    env.core
        .catalog()
        .write(move |tx| {
            tx.execute("INSERT INTO tag (id, name, color) VALUES (1, 'a', '#000000'), (2, 'b', '#000000')", [])?;
            for i in 0..TIMING_ROWS {
                let name = match i % 3 {
                    0 => format!("Ảnh chụp màn hình 2026-01-{:02} {i}.png", i % 28 + 1),
                    1 => format!("Screenshot 2025-{:02}-01 {i}.png", i % 12 + 1),
                    _ => format!("Screenshot ({i}).png"),
                };
                let folder = ["", "1ERD", "4ERD/sub"][(i % 3) as usize];
                let (w, h) = [(1080, 2400), (1920, 1080), (800, 800), (3440, 1440)][(i % 4) as usize];
                let rgb = format!("#{:02X}{:02X}{:02X}", (i * 7) % 256, (i * 13) % 256, (i * 29) % 256);
                tx.execute(
                    "INSERT INTO image (root_id, folder, file_name, size_bytes, mtime_ms, taken_ms, width, height,
                                        dominant_rgb, thumb_state, cloud_state, favorite, rating, indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, 'ok', 'local', ?9, ?10, 0)",
                    params![root, folder, name, 100_000 + i * 37, 1_700_000_000_000 + i * 60_000, w, h, rgb, (i % 5 == 0) as i64, i % 6],
                )?;
                let id = tx.last_insert_rowid();
                catalog::fts_insert(tx, id, &name)?;
                tx.execute("INSERT INTO image_tag (image_id, tag_id) VALUES (?1, ?2)", params![id, 1 + (i % 7) % 2])?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), TIMING_ROWS);

    let queries = [
        ("no filter", q()),
        (
            "text + relevance",
            ImageQuery { text: Some("anh chup".into()), sort: SortSpec { field: SortField::Relevance, dir: SortDir::Desc }, limit: 200, ..q() },
        ),
        (
            "everything",
            ImageQuery {
                text: Some("screenshot".into()),
                folders: vec!["1ERD".into(), "4ERD".into()],
                aspects: vec![AspectClass::Pc, AspectClass::Wide],
                min_width: Some(1000),
                tag_ids: vec![1, 2],
                color: Some("#808080".into()),
                color_tolerance: 60,
                min_rating: 1,
                limit: 200,
                ..q()
            },
        ),
    ];
    let limit_ms = if cfg!(debug_assertions) { 1_000 } else { 200 };
    for (what, qq) in queries {
        env.core.query_images(&qq).unwrap(); // warm the statement cache and pages
        let t = Instant::now();
        let page = env.core.query_images(&qq).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("query_5600 {what}: {ms:.1} ms, total {}, page {}", page.total, page.items.len());
        assert!(page.total > 0, "{what} matches rows");
        assert!(ms < limit_ms as f64, "{what}: {ms:.1} ms >= {limit_ms} ms");
    }
}
