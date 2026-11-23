//! `ssimg` custom scheme handler (contract-tauri-ipc § Image URLs), UI-free so tests call it
//! directly; `src-tauri` serves it off the main thread.
//!
//! - `thumb/<id>/<w>`: `w` must be 320 or 1280 (else 400); unknown id 404; thumbnail `pending` or
//!   `offline` -> 202, empty body, header `x-thumb-state`; `error` (undecodable file) -> 404.
//! - `full/<id>`: the original bytes. An explicit user action, so a cloud-only file may be opened
//!   (OneDrive downloads it on demand, as Explorer does); a missing file -> 404.
//! - Anything else -> 400. A 200 carries `Cache-Control: max-age=31536000, immutable` (the UI adds
//!   `?v=<mtimeMs>`, which is ignored here).
//! - The path is percent-decoded first: `convertFileSrc` sends `thumb/12/320` as `/thumb%2F12%2F320`.

use std::io::Read;

use rusqlite::OptionalExtension;

use crate::catalog::Catalog;
use crate::contract::ipc::ssimg::{CACHE_CONTROL, THUMB_STATE_HEADER, THUMB_WIDTHS};
use crate::fsaccess::FileAccess;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsimgResponse {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl SsimgResponse {
    fn empty(status: u16) -> Self {
        Self { status, headers: Vec::new(), body: Vec::new() }
    }

    fn ok(content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type", content_type.to_owned()), ("Cache-Control", CACHE_CONTROL.to_owned())],
            body,
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

enum Route {
    Thumb { id: i64, width: i64 },
    Full { id: i64 },
}

/// Decodes `%XX` escapes. The UI builds every URL with Tauri's `convertFileSrc`, which applies
/// `encodeURIComponent` to the whole path, so `thumb/12/320` arrives as `/thumb%2F12%2F320`.
/// `None` on a malformed escape or a non-UTF-8 result.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn parse(path: &str) -> Option<Route> {
    let raw = path.split(['?', '#']).next().unwrap_or_default();
    let decoded = percent_decode(raw)?;
    let path = decoded.trim_start_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    let int = |s: &str| s.parse::<i64>().ok().filter(|v| *v > 0);
    match parts.as_slice() {
        ["thumb", id, w] => {
            let width = w.parse::<i64>().ok().filter(|w| THUMB_WIDTHS.contains(w))?;
            Some(Route::Thumb { id: int(id)?, width })
        }
        ["full", id] => Some(Route::Full { id: int(id)? }),
        _ => None,
    }
}

/// `path` is the URI path, e.g. `/thumb/12/320` (a query string is ignored).
pub fn handle(cat: &Catalog, fa: &dyn FileAccess, path: &str) -> SsimgResponse {
    match parse(path) {
        None => SsimgResponse::empty(400),
        Some(Route::Thumb { id, width }) => thumb(cat, id, width),
        Some(Route::Full { id }) => full(cat, fa, id),
    }
}

fn thumb(cat: &Catalog, id: i64, width: i64) -> SsimgResponse {
    let state = cat.read(|conn| {
        Ok(conn.query_row("SELECT thumb_state FROM image WHERE id = ?1", [id], |r| r.get::<_, String>(0)).optional()?)
    });
    match state.as_ref().map(|s| s.as_deref()) {
        Err(e) => {
            eprintln!("ssimg: thumb {id}: {e}");
            SsimgResponse::empty(500)
        }
        Ok(None) | Ok(Some("error")) => SsimgResponse::empty(404),
        Ok(Some(s @ ("pending" | "offline"))) => {
            SsimgResponse { status: 202, headers: vec![(THUMB_STATE_HEADER, s.to_owned())], body: Vec::new() }
        }
        Ok(Some(_)) => match std::fs::read(paths::thumb_path(cat.data_dir(), id, width)) {
            Ok(bytes) => SsimgResponse::ok("image/webp", bytes),
            Err(_) => SsimgResponse::empty(404),
        },
    }
}

fn full(cat: &Catalog, fa: &dyn FileAccess, id: i64) -> SsimgResponse {
    let row = cat.read(|conn| {
        Ok(conn
            .query_row(
                "SELECT r.path, i.folder, i.file_name, i.missing FROM image i JOIN library_root r ON r.id = i.root_id
                 WHERE i.id = ?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)),
            )
            .optional()?)
    });
    let (root, folder, name) = match row {
        Err(e) => {
            eprintln!("ssimg: full {id}: {e}");
            return SsimgResponse::empty(500);
        }
        Ok(None) | Ok(Some((_, _, _, 1))) => return SsimgResponse::empty(404),
        Ok(Some((root, folder, name, _))) => (root, folder, name),
    };
    let abs = paths::image_abs_path(&root, &folder, &name);
    let mut bytes = Vec::new();
    match fa.open(&abs).and_then(|mut f| f.read_to_end(&mut bytes)) {
        Ok(_) => SsimgResponse::ok(content_type(&name), bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SsimgResponse::empty(404),
        Err(e) => {
            eprintln!("ssimg: full {id}: {e}");
            SsimgResponse::empty(500)
        }
    }
}

fn content_type(file_name: &str) -> &'static str {
    let ext = file_name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}
