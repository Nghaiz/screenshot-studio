//! Snapshot of one image row with every row that hangs off it (tags, albums, OCR, embedding,
//! categories), so a file op that removes the row (move out of the library, Recycle Bin) can be
//! undone with tags, notes and album positions intact. Columns are read from `PRAGMA table_xinfo`
//! (generated columns skipped), so later schema migrations need no change here.
//!
//! Thumbnails of a removed row are parked under `<data>\thumbs\parked\<batch>_<id>_<w>.webp`
//! (never left at the live path: SQLite may give the id to a new image) and moved back on restore.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Transaction, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use crate::contract::ipc::ssimg::{THUMB_GRID, THUMB_WIDTHS};
use crate::contract::{AppError, ErrorKind};
use crate::errors::internal;
use crate::paths;

/// Child tables: (table, optional (fk column, parent table) besides `image_id`).
const CHILDREN: [(&str, Option<(&str, &str)>); 5] = [
    ("ocr_result", None),
    ("embedding", None),
    ("category", None),
    ("image_tag", Some(("tag_id", "tag"))),
    ("album_image", Some(("album_id", "album"))),
];
const PARKED_DIR: &str = "parked";
const BLOB_KEY: &str = "$blob";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub image: Map<String, Json>,
    pub children: BTreeMap<String, Vec<Map<String, Json>>>,
}

impl Snapshot {
    pub fn id(&self) -> Result<i64, AppError> {
        self.image.get("id").and_then(Json::as_i64).ok_or_else(|| internal("snapshot without an id"))
    }
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare_cached("SELECT name FROM pragma_table_xinfo(?1) WHERE hidden = 0 ORDER BY cid")?;
    Ok(stmt.query_map([table], |r| r.get(0))?.collect::<Result<_, _>>()?)
}

fn to_json(v: Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Integer(i) => i.into(),
        Value::Real(f) => serde_json::Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null),
        Value::Text(s) => s.into(),
        Value::Blob(b) => {
            let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
            serde_json::json!({ BLOB_KEY: hex })
        }
    }
}

fn from_json(v: &Json) -> Result<Value, AppError> {
    Ok(match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Integer(*b as i64),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Real(n.as_f64().unwrap_or_default()),
        },
        Json::String(s) => Value::Text(s.clone()),
        Json::Object(o) => {
            let hex = o.get(BLOB_KEY).and_then(Json::as_str).ok_or_else(|| internal("bad snapshot value"))?;
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
                .collect::<Result<Vec<u8>, _>>()
                .map_err(|_| internal("bad snapshot blob"))?;
            Value::Blob(bytes)
        }
        Json::Array(_) => return Err(internal("bad snapshot value")),
    })
}

fn rows(conn: &Connection, table: &str, key: &str, id: i64) -> Result<Vec<Map<String, Json>>, AppError> {
    let cols = columns(conn, table)?;
    let sql = format!("SELECT {} FROM {table} WHERE {key} = ?1", cols.join(", "));
    let mut stmt = conn.prepare(&sql)?;
    let mut out = Vec::new();
    let mut q = stmt.query([id])?;
    while let Some(r) = q.next()? {
        let mut m = Map::new();
        for (i, c) in cols.iter().enumerate() {
            m.insert(c.clone(), to_json(r.get::<_, Value>(i)?));
        }
        out.push(m);
    }
    Ok(out)
}

pub(crate) fn take(conn: &Connection, id: i64) -> Result<Snapshot, AppError> {
    let image = rows(conn, "image", "id", id)?
        .pop()
        .ok_or_else(|| AppError::new(ErrorKind::NotFound, format!("image {id} not found")))?;
    let mut children = BTreeMap::new();
    for (table, _) in CHILDREN {
        children.insert(table.to_owned(), rows(conn, table, "image_id", id)?);
    }
    Ok(Snapshot { image, children })
}

/// Deletes the row (its FTS row first: no triggers; child rows by cascade).
pub(crate) fn remove(tx: &Transaction<'_>, id: i64) -> Result<(), AppError> {
    tx.execute("DELETE FROM text_fts WHERE rowid = ?1", [id])?;
    tx.execute("DELETE FROM image WHERE id = ?1", [id])?;
    Ok(())
}

fn insert(tx: &Transaction<'_>, table: &str, row: &Map<String, Json>) -> Result<(), AppError> {
    let cols: Vec<&String> = row.keys().collect();
    let values = row.values().map(from_json).collect::<Result<Vec<_>, _>>()?;
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({})",
        cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "),
        vec!["?"; cols.len()].join(", ")
    );
    tx.execute(&sql, params_from_iter(values))?;
    Ok(())
}

/// Re-inserts the row (same id when still free, else a new one) and its children; child rows whose
/// tag/album was deleted meanwhile are dropped. The thumbnail state is kept only when the parked 320
/// file of `batch` exists. Returns the row's id.
pub(crate) fn restore(tx: &Transaction<'_>, snap: &Snapshot, data_dir: &Path, batch: i64) -> Result<i64, AppError> {
    let old_id = snap.id()?;
    let mut image = snap.image.clone();
    let taken: bool = tx.query_row("SELECT count(*) > 0 FROM image WHERE id = ?1", [old_id], |r| r.get(0))?;
    if taken {
        image.remove("id");
    }
    let thumb_ok = image.get("thumb_state").and_then(Json::as_str) == Some("ok");
    if thumb_ok && !parked_path(data_dir, batch, old_id, THUMB_GRID).is_file() {
        let cloud_only = image.get("cloud_state").and_then(Json::as_str) == Some("cloudOnly");
        image.insert("thumb_state".into(), Json::from(if cloud_only { "offline" } else { "pending" }));
    }
    insert(tx, "image", &image).map_err(|e| match e.detail.as_deref() {
        Some(d) if d.contains("UNIQUE") => AppError::new(ErrorKind::Conflict, "the catalog has another image at that path"),
        Some(d) if d.contains("FOREIGN KEY") => AppError::new(ErrorKind::NotFound, "its library root was removed"),
        _ => e,
    })?;
    let id = tx.last_insert_rowid();
    for (table, fk) in CHILDREN {
        for row in snap.children.get(table).into_iter().flatten() {
            if let Some((col, parent)) = fk {
                let parent_id = row.get(col).and_then(Json::as_i64);
                let alive: bool = tx
                    .query_row(&format!("SELECT count(*) > 0 FROM {parent} WHERE id = ?1"), [parent_id], |r| r.get(0))?;
                if !alive {
                    continue;
                }
            }
            let mut row = row.clone();
            row.insert("image_id".into(), Json::from(id));
            insert(tx, table, &row)?;
        }
    }
    let ocr: Option<String> =
        tx.query_row("SELECT text FROM ocr_result WHERE image_id = ?1", [id], |r| r.get(0)).optional()?;
    tx.execute(
        "INSERT INTO text_fts (rowid, file_name, ocr_text, note) SELECT id, file_name, ?2, COALESCE(note, '') FROM image WHERE id = ?1",
        rusqlite::params![id, crate::filter::fts_ocr_text(&ocr.unwrap_or_default())],
    )?;
    Ok(id)
}

fn parked_path(data_dir: &Path, batch: i64, id: i64, width: i64) -> PathBuf {
    data_dir.join(paths::THUMBS_DIR).join(PARKED_DIR).join(format!("{batch}_{id}_{width}.webp"))
}

/// Moves the live thumbnails of `id` aside for `batch` (best effort: they are a cache).
pub(crate) fn park_thumbs(data_dir: &Path, batch: i64, id: i64) {
    for w in THUMB_WIDTHS {
        let (from, to) = (paths::thumb_path(data_dir, id, w), parked_path(data_dir, batch, id, w));
        if !from.is_file() {
            continue;
        }
        let r = to.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::rename(&from, &to));
        if let Err(e) = r {
            eprintln!("ss-core: could not park thumbnail {}: {e}", from.display());
            let _ = std::fs::remove_file(&from);
        }
    }
}

/// Moves parked thumbnails back to the live path of `new_id`.
pub(crate) fn unpark_thumbs(data_dir: &Path, batch: i64, old_id: i64, new_id: i64) {
    for w in THUMB_WIDTHS {
        let (from, to) = (parked_path(data_dir, batch, old_id, w), paths::thumb_path(data_dir, new_id, w));
        if !from.is_file() {
            continue;
        }
        let r = to.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::rename(&from, &to));
        if let Err(e) = r {
            eprintln!("ss-core: could not restore thumbnail {}: {e}", to.display());
        }
    }
}
