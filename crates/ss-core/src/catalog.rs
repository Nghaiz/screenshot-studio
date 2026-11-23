//! SQLite catalog: open + migrate, WAL, ONE writer thread (every write is a closure run inside a
//! transaction on that thread) and a small pool of reader connections. FTS rows for `file_name` are
//! maintained here, never by triggers (contract-sqlite-schema.md).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};
use std::thread::JoinHandle;

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value as JsonValue;

use crate::contract::ipc::{LibraryRoot, SettingKey, SettingsMap};
use crate::contract::{AppError, MIGRATION_0001_INIT};
use crate::errors::{enum_str, internal, not_found};
use crate::paths;

/// Rows per write transaction in bulk passes (scan upserts, thumbnail results).
pub const WRITE_BATCH: usize = 500;
/// Idle reader connections kept for reuse.
const READER_POOL_MAX: usize = 8;
const BUSY_TIMEOUT_MS: u32 = 10_000;

type WriteJob = Box<dyn FnOnce(&mut Connection) + Send>;

pub struct Catalog {
    data_dir: PathBuf,
    db_path: PathBuf,
    writer_tx: Option<mpsc::Sender<WriteJob>>,
    writer: Option<JoinHandle<()>>,
    readers: Mutex<Vec<Connection>>,
}

impl Catalog {
    /// Opens (creating the data dir and DB if needed) and applies migration 0001 on an empty DB.
    pub fn open(data_dir: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = paths::catalog_path(data_dir);
        let mut conn = open_conn(&db_path)?;
        migrate(&mut conn)?;
        let (tx, rx) = mpsc::channel::<WriteJob>();
        let writer = std::thread::Builder::new()
            .name("ss-catalog-writer".into())
            .spawn(move || {
                let mut conn = conn;
                for job in rx {
                    job(&mut conn);
                }
            })
            .map_err(|e| internal("cannot start the catalog writer thread").with_detail(e.to_string()))?;
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            db_path,
            writer_tx: Some(tx),
            writer: Some(writer),
            readers: Mutex::new(Vec::new()),
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Runs `f` inside one transaction on the writer thread; commits when it returns `Ok`.
    pub fn write<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> Result<T, AppError> + Send + 'static,
    {
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let job: WriteJob = Box::new(move |conn| {
            let result = (|| {
                let tx = conn.transaction()?;
                let value = f(&tx)?;
                tx.commit()?;
                Ok(value)
            })();
            let _ = done_tx.send(result);
        });
        let sender = self.writer_tx.as_ref().ok_or_else(|| internal("catalog is closed"))?;
        sender.send(job).map_err(|_| internal("catalog writer thread stopped"))?;
        done_rx.recv().map_err(|_| internal("catalog writer thread stopped"))?
    }

    /// Runs `f` on a pooled reader connection.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T, AppError>) -> Result<T, AppError> {
        let conn = match self.readers.lock().expect("reader pool").pop() {
            Some(c) => c,
            None => open_conn(&self.db_path)?,
        };
        let result = f(&conn);
        let mut pool = self.readers.lock().expect("reader pool");
        if pool.len() < READER_POOL_MAX {
            pool.push(conn);
        }
        result
    }
}

impl Drop for Catalog {
    fn drop(&mut self) {
        // Closing the channel ends the writer loop; joining closes its connection before the
        // data dir can be removed (Windows keeps open files locked).
        self.writer_tx.take();
        if let Some(h) = self.writer.take() {
            let _ = h.join();
        }
    }
}

fn open_conn(db_path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS.into()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<(), AppError> {
    let has_schema: bool = conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
        [],
        |r| r.get(0),
    )?;
    if !has_schema {
        conn.execute_batch(MIGRATION_0001_INIT)?;
    }
    let version: i64 = conn.query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))?;
    if version != 1 {
        return Err(internal(format!("catalog schema version {version} is not supported (expected 1)")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Library roots
// ---------------------------------------------------------------------------------------------

const ROOT_SELECT: &str = "SELECT r.id, r.path, r.added_at, r.protected_root,
    (SELECT count(*) FROM image i WHERE i.root_id = r.id AND i.missing = 0) FROM library_root r";

fn root_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryRoot> {
    Ok(LibraryRoot {
        id: r.get(0)?,
        path: r.get(1)?,
        added_at: r.get(2)?,
        protected_root: r.get::<_, i64>(3)? != 0,
        image_count: r.get(4)?,
    })
}

pub fn list_roots(conn: &Connection) -> Result<Vec<LibraryRoot>, AppError> {
    let mut stmt = conn.prepare_cached(&format!("{ROOT_SELECT} ORDER BY r.id"))?;
    let rows = stmt.query_map([], root_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_root(conn: &Connection, root_id: i64) -> Result<LibraryRoot, AppError> {
    conn.query_row(&format!("{ROOT_SELECT} WHERE r.id = ?1"), [root_id], root_from_row)
        .optional()?
        .ok_or_else(|| not_found(format!("library root {root_id} not found")))
}

pub fn find_root_by_path(conn: &Connection, path: &str) -> Result<Option<i64>, AppError> {
    Ok(conn
        .query_row("SELECT id FROM library_root WHERE path = ?1 COLLATE NOCASE", [path], |r| r.get(0))
        .optional()?)
}

pub fn insert_root(tx: &Transaction<'_>, path: &str, protected: bool, added_at: i64) -> Result<i64, AppError> {
    tx.execute(
        "INSERT INTO library_root (path, protected_root, added_at) VALUES (?1, ?2, ?3)",
        params![path, protected as i64, added_at],
    )?;
    Ok(tx.last_insert_rowid())
}

/// Removes a root and its image rows (FTS rows first: no triggers). Returns the removed image ids.
pub fn delete_root(tx: &Transaction<'_>, root_id: i64) -> Result<Vec<i64>, AppError> {
    let exists: bool = tx.query_row("SELECT count(*) > 0 FROM library_root WHERE id = ?1", [root_id], |r| r.get(0))?;
    if !exists {
        return Err(not_found(format!("library root {root_id} not found")));
    }
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT id FROM image WHERE root_id = ?1")?;
        stmt.query_map([root_id], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    tx.execute("DELETE FROM text_fts WHERE rowid IN (SELECT id FROM image WHERE root_id = ?1)", [root_id])?;
    tx.execute("DELETE FROM library_root WHERE id = ?1", [root_id])?;
    Ok(ids)
}

// ---------------------------------------------------------------------------------------------
// FTS
// ---------------------------------------------------------------------------------------------

/// Inserts the FTS row of a new image (rowid = image.id).
pub fn fts_insert(tx: &Transaction<'_>, image_id: i64, file_name: &str) -> Result<(), AppError> {
    tx.prepare_cached("INSERT INTO text_fts (rowid, file_name, ocr_text, note) VALUES (?1, ?2, '', '')")?
        .execute(params![image_id, file_name])?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------------

/// Every setting key with its stored value, or its default when not stored.
pub fn get_settings(conn: &Connection) -> Result<SettingsMap, AppError> {
    let mut stored = BTreeMap::new();
    {
        let mut stmt = conn.prepare_cached("SELECT key, value_json FROM setting")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (k, v) = row?;
            let value = serde_json::from_str(&v)
                .map_err(|e| internal(format!("setting `{k}` holds invalid JSON")).with_detail(e.to_string()))?;
            stored.insert(k, value);
        }
    }
    let mut out = BTreeMap::new();
    for key in SettingKey::ALL {
        let name = enum_str(&key);
        let value = stored.get(&name).cloned().unwrap_or_else(|| key.default_value());
        out.insert(name, value);
    }
    Ok(SettingsMap(out))
}

pub fn get_setting(conn: &Connection, key: SettingKey) -> Result<JsonValue, AppError> {
    let stored: Option<String> = conn
        .query_row("SELECT value_json FROM setting WHERE key = ?1", [enum_str(&key)], |r| r.get(0))
        .optional()?;
    match stored {
        Some(v) => serde_json::from_str(&v).map_err(|e| internal("setting holds invalid JSON").with_detail(e.to_string())),
        None => Ok(key.default_value()),
    }
}

pub fn put_setting(tx: &Transaction<'_>, key: SettingKey, value: &JsonValue) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO setting (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![enum_str(&key), value.to_string()],
    )?;
    Ok(())
}
