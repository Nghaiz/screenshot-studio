//! Catalog-only organization: tags, albums, favourites, ratings, notes, smart filters. None of these
//! touches a file, so both callers may use them. Image-level changes are journaled (one batch per
//! call, one row per image that actually changed); container deletions and renames of tags/albums
//! are not (contract rule 7).

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value as Json, json};

use crate::catalog::Catalog;
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller, ErrorKind};
use crate::errors::{internal, invalid, not_found, now_ms};

use super::journal::{AlbumPayload, Entry, OldValue, TagPayload, write_batch};
use super::{load_rows, not_found_failure, unique_ids};

const NAME_MAX: usize = 120;
const NOTE_MAX: usize = 10_000;

/// A JSON scalar as an SQL parameter (bool -> 0/1).
pub(crate) fn json_param(v: &Json) -> Value {
    match v {
        Json::Bool(b) => Value::Integer(*b as i64),
        Json::Number(n) => n.as_i64().map(Value::Integer).unwrap_or_else(|| Value::Real(n.as_f64().unwrap_or_default())),
        Json::String(s) => Value::Text(s.clone()),
        _ => Value::Null,
    }
}

/// Sets one column of the FTS row of `id` (inserting the row when it is missing).
pub(crate) fn fts_set(tx: &Transaction<'_>, id: i64, column: &str, value: &str) -> Result<(), AppError> {
    let n = tx.execute(&format!("UPDATE text_fts SET {column} = ?2 WHERE rowid = ?1"), params![id, value])?;
    if n == 0 {
        let ocr: Option<String> =
            tx.query_row("SELECT text FROM ocr_result WHERE image_id = ?1", [id], |r| r.get(0)).optional()?;
        tx.execute(
            "INSERT INTO text_fts (rowid, file_name, ocr_text, note)
             SELECT i.id, i.file_name, ?2, COALESCE(i.note, '') FROM image i WHERE i.id = ?1",
            params![id, crate::filter::fts_ocr_text(&ocr.unwrap_or_default())],
        )?;
        tx.execute(&format!("UPDATE text_fts SET {column} = ?2 WHERE rowid = ?1"), params![id, value])?;
    }
    Ok(())
}

fn clean_name(name: &str, what: &str) -> Result<String, AppError> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > NAME_MAX {
        return Err(invalid(format!("{what} name must be 1..{NAME_MAX} characters")));
    }
    Ok(n.to_owned())
}

fn clean_color(color: &str) -> Result<String, AppError> {
    crate::filter::parse_rgb(color).ok_or_else(|| invalid(format!("color must be #RRGGBB, got `{color}`")))?;
    Ok(color.to_ascii_uppercase())
}

fn unique_violation(e: AppError, what: &str) -> AppError {
    if e.detail.as_deref().is_some_and(|d| d.contains("UNIQUE")) {
        AppError::new(ErrorKind::Conflict, format!("{what} already exists"))
    } else {
        e
    }
}

// --- tags ------------------------------------------------------------------------------------

const TAG_SELECT: &str = "SELECT t.id, t.name, t.color, (SELECT count(*) FROM image_tag x WHERE x.tag_id = t.id) FROM tag t";

fn tag_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, count: r.get(3)? })
}

pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare_cached(&format!("{TAG_SELECT} ORDER BY t.name COLLATE NOCASE"))?;
    Ok(stmt.query_map([], tag_row)?.collect::<Result<_, _>>()?)
}

fn get_tag(conn: &Connection, id: i64) -> Result<Tag, AppError> {
    conn.query_row(&format!("{TAG_SELECT} WHERE t.id = ?1"), [id], tag_row)
        .optional()?
        .ok_or_else(|| not_found(format!("tag {id} not found")))
}

pub fn create_tag(caller: Caller, cat: &Catalog, req: CreateTagReq) -> Result<Tag, AppError> {
    let _ = caller;
    let (name, color) = (clean_name(&req.name, "tag")?, clean_color(&req.color)?);
    let what = format!("tag `{name}`");
    let id = cat
        .write(move |tx| {
            tx.execute("INSERT INTO tag (name, color) VALUES (?1, ?2)", params![name, color])?;
            Ok(tx.last_insert_rowid())
        })
        .map_err(|e| unique_violation(e, &what))?;
    cat.read(|c| get_tag(c, id))
}

pub fn update_tag(caller: Caller, cat: &Catalog, req: UpdateTagReq) -> Result<Tag, AppError> {
    let _ = caller;
    let (name, color, id) = (clean_name(&req.name, "tag")?, clean_color(&req.color)?, req.tag_id);
    let what = format!("tag `{name}`");
    cat.write(move |tx| {
        let n = tx.execute("UPDATE tag SET name = ?2, color = ?3 WHERE id = ?1", params![id, name, color])?;
        if n == 0 { Err(not_found(format!("tag {id} not found"))) } else { Ok(()) }
    })
    .map_err(|e| unique_violation(e, &what))?;
    cat.read(|c| get_tag(c, id))
}

/// Not journaled: removes the tag from every image (`affected` = how many had it).
pub fn delete_tag(caller: Caller, cat: &Catalog, req: DeleteTagReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let id = req.tag_id;
    let affected = cat.write(move |tx| {
        let n: i64 = tx.query_row("SELECT count(*) FROM image_tag WHERE tag_id = ?1", [id], |r| r.get(0))?;
        if tx.execute("DELETE FROM tag WHERE id = ?1", [id])? == 0 {
            return Err(not_found(format!("tag {id} not found")));
        }
        Ok(n)
    })?;
    Ok(OpResult { batch_id: None, affected, failed: Vec::new() })
}

pub fn set_tags(caller: Caller, cat: &Catalog, req: SetTagsReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let ids = unique_ids(&req.image_ids);
    let (add, remove) = (unique_ids(&req.add), unique_ids(&req.remove));
    if add.iter().any(|t| remove.contains(t)) {
        return Err(invalid("a tag cannot be in both `add` and `remove`"));
    }
    cat.write(move |tx| {
        for t in add.iter().chain(&remove) {
            get_tag(tx, *t)?;
        }
        let rows = load_rows(tx, &ids)?;
        let mut entries = Vec::new();
        let mut failed = Vec::new();
        let mut touched = std::collections::BTreeSet::new();
        for id in &ids {
            if !rows.contains_key(id) {
                failed.push(not_found_failure(*id));
                continue;
            }
            for t in &add {
                if tx.execute("INSERT OR IGNORE INTO image_tag (image_id, tag_id) VALUES (?1, ?2)", params![id, t])? == 1 {
                    entries.push(Entry::new(JournalOp::Tag, *id, TagPayload { tag_id: *t }));
                    touched.insert(*id);
                }
            }
            for t in &remove {
                if tx.execute("DELETE FROM image_tag WHERE image_id = ?1 AND tag_id = ?2", params![id, t])? == 1 {
                    entries.push(Entry::new(JournalOp::Untag, *id, TagPayload { tag_id: *t }));
                    touched.insert(*id);
                }
            }
        }
        let batch_id = write_batch(tx, &entries)?;
        Ok(OpResult { batch_id, affected: touched.len() as i64, failed })
    })
}

// --- favourite / rating / note ---------------------------------------------------------------

/// Sets `column` of every image to `value`, journaling the old value of those that change.
fn set_column(cat: &Catalog, ids: &[i64], op: JournalOp, column: &'static str, value: Json) -> Result<OpResult, AppError> {
    let ids = unique_ids(ids);
    cat.write(move |tx| {
        let rows = load_rows(tx, &ids)?;
        let mut entries = Vec::new();
        let mut failed = Vec::new();
        for id in &ids {
            let Some(row) = rows.get(id) else {
                failed.push(not_found_failure(*id));
                continue;
            };
            let old = match column {
                "favorite" => json!(row.favorite),
                "rating" => json!(row.rating),
                "note" => json!(row.note),
                _ => return Err(internal(format!("unknown column {column}"))),
            };
            if old == value {
                continue;
            }
            tx.execute(&format!("UPDATE image SET {column} = ?2 WHERE id = ?1"), params![id, json_param(&value)])?;
            if column == "note" {
                fts_set(tx, *id, "note", value.as_str().unwrap_or_default())?;
            }
            entries.push(Entry::new(op, *id, OldValue { old }));
        }
        let batch_id = write_batch(tx, &entries)?;
        Ok(OpResult { batch_id, affected: entries.len() as i64, failed })
    })
}

pub fn set_favorite(caller: Caller, cat: &Catalog, req: SetFavoriteReq) -> Result<OpResult, AppError> {
    let _ = caller;
    set_column(cat, &req.image_ids, JournalOp::Favorite, "favorite", json!(req.value))
}

pub fn set_rating(caller: Caller, cat: &Catalog, req: SetRatingReq) -> Result<OpResult, AppError> {
    let _ = caller;
    if !(0..=5).contains(&req.rating) {
        return Err(invalid(format!("rating must be 0..5, got {}", req.rating)));
    }
    set_column(cat, &req.image_ids, JournalOp::Rating, "rating", json!(req.rating))
}

/// A blank note clears it (stored as NULL).
pub fn set_note(caller: Caller, cat: &Catalog, req: SetNoteReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let note = req.note.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned);
    if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_MAX) {
        return Err(invalid(format!("a note is at most {NOTE_MAX} characters")));
    }
    set_column(cat, &[req.image_id], JournalOp::Note, "note", json!(note))
}

// --- albums ----------------------------------------------------------------------------------

const ALBUM_SELECT: &str = "SELECT a.id, a.name, a.created_at,
    (SELECT count(*) FROM album_image x WHERE x.album_id = a.id),
    (SELECT x.image_id FROM album_image x WHERE x.album_id = a.id ORDER BY x.position, x.image_id LIMIT 1)
    FROM album a";

fn album_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Album> {
    Ok(Album { id: r.get(0)?, name: r.get(1)?, created_at: r.get(2)?, count: r.get(3)?, cover_image_id: r.get(4)? })
}

pub fn list_albums(conn: &Connection) -> Result<Vec<Album>, AppError> {
    let mut stmt = conn.prepare_cached(&format!("{ALBUM_SELECT} ORDER BY a.name COLLATE NOCASE, a.id"))?;
    Ok(stmt.query_map([], album_row)?.collect::<Result<_, _>>()?)
}

fn get_album(conn: &Connection, id: i64) -> Result<Album, AppError> {
    conn.query_row(&format!("{ALBUM_SELECT} WHERE a.id = ?1"), [id], album_row)
        .optional()?
        .ok_or_else(|| not_found(format!("album {id} not found")))
}

pub fn create_album(caller: Caller, cat: &Catalog, req: CreateAlbumReq) -> Result<Album, AppError> {
    let _ = caller;
    let name = clean_name(&req.name, "album")?;
    let id = cat.write(move |tx| {
        tx.execute("INSERT INTO album (name, created_at) VALUES (?1, ?2)", params![name, now_ms()])?;
        Ok(tx.last_insert_rowid())
    })?;
    cat.read(|c| get_album(c, id))
}

pub fn rename_album(caller: Caller, cat: &Catalog, req: RenameAlbumReq) -> Result<Album, AppError> {
    let _ = caller;
    let (name, id) = (clean_name(&req.name, "album")?, req.album_id);
    cat.write(move |tx| {
        if tx.execute("UPDATE album SET name = ?2 WHERE id = ?1", params![id, name])? == 0 {
            return Err(not_found(format!("album {id} not found")));
        }
        Ok(())
    })?;
    cat.read(|c| get_album(c, id))
}

/// Not journaled; the images stay (`affected` = how many were in it).
pub fn delete_album(caller: Caller, cat: &Catalog, req: DeleteAlbumReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let id = req.album_id;
    let affected = cat.write(move |tx| {
        let n: i64 = tx.query_row("SELECT count(*) FROM album_image WHERE album_id = ?1", [id], |r| r.get(0))?;
        if tx.execute("DELETE FROM album WHERE id = ?1", [id])? == 0 {
            return Err(not_found(format!("album {id} not found")));
        }
        Ok(n)
    })?;
    Ok(OpResult { batch_id: None, affected, failed: Vec::new() })
}

fn position_of(tx: &Transaction<'_>, album: i64, image: i64) -> Result<Option<i64>, AppError> {
    Ok(tx
        .query_row("SELECT position FROM album_image WHERE album_id = ?1 AND image_id = ?2", params![album, image], |r| r.get(0))
        .optional()?)
}

/// Appends images (in request order) after the album's last position.
pub fn album_add(caller: Caller, cat: &Catalog, req: AlbumImagesReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let (album, ids) = (req.album_id, unique_ids(&req.image_ids));
    cat.write(move |tx| {
        get_album(tx, album)?;
        let rows = load_rows(tx, &ids)?;
        let mut next: i64 =
            tx.query_row("SELECT COALESCE(max(position), -1) + 1 FROM album_image WHERE album_id = ?1", [album], |r| r.get(0))?;
        let (mut entries, mut failed) = (Vec::new(), Vec::new());
        for id in &ids {
            if !rows.contains_key(id) {
                failed.push(not_found_failure(*id));
                continue;
            }
            if position_of(tx, album, *id)?.is_some() {
                continue;
            }
            tx.execute("INSERT INTO album_image (album_id, image_id, position) VALUES (?1, ?2, ?3)", params![album, id, next])?;
            next += 1;
            entries.push(Entry::new(JournalOp::AlbumAdd, *id, AlbumPayload { album_id: album, position: None }));
        }
        let batch_id = write_batch(tx, &entries)?;
        Ok(OpResult { batch_id, affected: entries.len() as i64, failed })
    })
}

pub fn album_remove(caller: Caller, cat: &Catalog, req: AlbumImagesReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let (album, ids) = (req.album_id, unique_ids(&req.image_ids));
    cat.write(move |tx| {
        get_album(tx, album)?;
        let (mut entries, mut failed) = (Vec::new(), Vec::new());
        for id in &ids {
            match position_of(tx, album, *id)? {
                None => failed.push(super::failure(*id, &not_found(format!("image {id} is not in album {album}")))),
                Some(pos) => {
                    tx.execute("DELETE FROM album_image WHERE album_id = ?1 AND image_id = ?2", params![album, id])?;
                    entries.push(Entry::new(JournalOp::AlbumRemove, *id, AlbumPayload { album_id: album, position: Some(pos) }));
                }
            }
        }
        let batch_id = write_batch(tx, &entries)?;
        Ok(OpResult { batch_id, affected: entries.len() as i64, failed })
    })
}

/// `imageIds` must be exactly the album's images, in their new order.
pub fn album_reorder(caller: Caller, cat: &Catalog, req: AlbumImagesReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let (album, ids) = (req.album_id, req.image_ids);
    cat.write(move |tx| {
        get_album(tx, album)?;
        let mut current: Vec<(i64, i64)> = {
            let mut stmt = tx.prepare("SELECT image_id, position FROM album_image WHERE album_id = ?1")?;
            stmt.query_map([album], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
        };
        current.sort_unstable();
        let mut wanted = ids.clone();
        wanted.sort_unstable();
        if wanted != current.iter().map(|c| c.0).collect::<Vec<_>>() {
            return Err(invalid("imageIds must list every image of the album exactly once"));
        }
        let mut entries = Vec::new();
        for (pos, id) in ids.iter().enumerate() {
            let old = current.iter().find(|c| c.0 == *id).map(|c| c.1).unwrap_or_default();
            if old == pos as i64 {
                continue;
            }
            tx.execute("UPDATE album_image SET position = ?3 WHERE album_id = ?1 AND image_id = ?2", params![album, id, pos as i64])?;
            entries.push(Entry::new(JournalOp::AlbumReorder, *id, AlbumPayload { album_id: album, position: Some(old) }));
        }
        let batch_id = write_batch(tx, &entries)?;
        Ok(OpResult { batch_id, affected: entries.len() as i64, failed: Vec::new() })
    })
}

// --- smart filters ---------------------------------------------------------------------------

fn filter_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, String, String, i64)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn to_filter((id, name, query_json, created_at): (i64, String, String, i64)) -> Result<SmartFilter, AppError> {
    let query = serde_json::from_str(&query_json)
        .map_err(|e| internal(format!("smart filter {id} holds an unreadable query")).with_detail(e.to_string()))?;
    Ok(SmartFilter { id, name, query, created_at })
}

pub fn list_smart_filters(conn: &Connection) -> Result<Vec<SmartFilter>, AppError> {
    let mut stmt = conn.prepare_cached("SELECT id, name, query_json, created_at FROM smart_filter ORDER BY name COLLATE NOCASE")?;
    let rows = stmt.query_map([], filter_row)?.collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(to_filter).collect()
}

/// Creates (`id: null`) or replaces a filter. The query is validated as `query_images` would, and
/// stored without its paging cursor.
pub fn save_smart_filter(caller: Caller, cat: &Catalog, req: SaveSmartFilterReq) -> Result<SmartFilter, AppError> {
    let _ = caller;
    let name = clean_name(&req.name, "smart filter")?;
    let mut query = req.query;
    query.cursor = None;
    crate::filter::validate(&query)?;
    if !(1..=crate::query::LIMIT_MAX).contains(&query.limit) {
        return Err(invalid(format!("limit must be 1..{}, got {}", crate::query::LIMIT_MAX, query.limit)));
    }
    let query_json = serde_json::to_string(&query).map_err(|e| internal("serialize query").with_detail(e.to_string()))?;
    let what = format!("smart filter `{name}`");
    let given = req.id;
    let id = cat
        .write(move |tx| match given {
            None => {
                tx.execute(
                    "INSERT INTO smart_filter (name, query_json, created_at) VALUES (?1, ?2, ?3)",
                    params![name, query_json, now_ms()],
                )?;
                Ok(tx.last_insert_rowid())
            }
            Some(id) => {
                let n = tx.execute("UPDATE smart_filter SET name = ?2, query_json = ?3 WHERE id = ?1", params![id, name, query_json])?;
                if n == 0 { Err(not_found(format!("smart filter {id} not found"))) } else { Ok(id) }
            }
        })
        .map_err(|e| unique_violation(e, &what))?;
    cat.read(|c| {
        let row = c.query_row("SELECT id, name, query_json, created_at FROM smart_filter WHERE id = ?1", [id], filter_row)?;
        to_filter(row)
    })
}

pub fn delete_smart_filter(caller: Caller, cat: &Catalog, req: DeleteSmartFilterReq) -> Result<OpResult, AppError> {
    let _ = caller;
    let id = req.id;
    cat.write(move |tx| {
        if tx.execute("DELETE FROM smart_filter WHERE id = ?1", [id])? == 0 {
            return Err(not_found(format!("smart filter {id} not found")));
        }
        Ok(OpResult { batch_id: None, affected: 1, failed: Vec::new() })
    })
}
