//! `ssimg` custom scheme handler (contract-tauri-ipc § Image URLs), UI-free so tests call it
//! directly; `src-tauri` serves it off the main thread.
//!
//! - `thumb/<id>/<w>`: `w` must be 320 or 1280 (else 400); unknown id 404; thumbnail `pending` or
//!   `offline` -> 202, empty body, header `x-thumb-state`; `error` (undecodable file) -> 404.
//!   From phase 2.1 the index makes only the 320 file: a `1280` request for an `ok` image whose 1280
//!   file is absent renders it synchronously from the original (lossy WebP q80), keeps it and answers
//!   200; a cloud-only original is never opened for it (202 `offline`, contract rule 9).
//! - `full/<id>`: the original bytes. An explicit user action, so a cloud-only file may be opened
//!   (OneDrive downloads it on demand, as Explorer does); a missing file -> 404.
//! - Anything else -> 400. A 200 carries `Cache-Control: max-age=31536000, immutable` (the UI adds
//!   `?v=<mtimeMs>`, which is ignored here).
//! - The path is percent-decoded first: `convertFileSrc` sends `thumb/12/320` as `/thumb%2F12%2F320`.
//!
//! Cache header review (Deferred decision #31, phase 2.1): `immutable` + one year is kept. The URL
//! carries `?v=<mtimeMs>`, so a changed file gets a new URL and the header never serves stale pixels;
//! what WebView2 holds per thumbnail is its encoded response plus the decoded bitmap, and the encoded
//! part drops ~30x with lossy q80 (MEASURED in 1.4 on 60 real screenshots: ~240 KB lossless 320+1280
//! vs 7.3 KB lossy 320). The decoded bitmaps of off-screen tiles are the UI's side (phase 2.2), and the
//! 500 MB budget is measured by `p2.mjs` in 2.3. A header change would be a contract change (Owner).

use std::io::Read;

use rusqlite::OptionalExtension;

use crate::catalog::Catalog;
use crate::contract::ipc::CloudState;
use crate::contract::ipc::ssimg::{CACHE_CONTROL, THUMB_PREVIEW, THUMB_STATE_HEADER, THUMB_WIDTHS};
use crate::fsaccess::FileAccess;
use crate::{paths, thumbs};

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
        Some(Route::Thumb { id, width }) => thumb(cat, fa, id, width),
        Some(Route::Full { id }) => full(cat, fa, id),
    }
}

fn thumb(cat: &Catalog, fa: &dyn FileAccess, id: i64, width: i64) -> SsimgResponse {
    let row = cat.read(|conn| {
        Ok(conn
            .query_row(
                "SELECT i.thumb_state, r.path, i.folder, i.file_name FROM image i JOIN library_root r ON r.id = i.root_id
                 WHERE i.id = ?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)),
            )
            .optional()?)
    });
    let (state, root, folder, name) = match row {
        Err(e) => {
            eprintln!("ssimg: thumb {id}: {e}");
            return SsimgResponse::empty(500);
        }
        Ok(None) => return SsimgResponse::empty(404),
        Ok(Some(r)) => r,
    };
    match state.as_str() {
        "pending" | "offline" => return accepted(&state),
        "ok" => {}
        _ => return SsimgResponse::empty(404),
    }
    if let Ok(bytes) = std::fs::read(paths::thumb_path(cat.data_dir(), id, width)) {
        return SsimgResponse::ok("image/webp", bytes);
    }
    if width != THUMB_PREVIEW {
        return SsimgResponse::empty(404);
    }
    let abs = paths::image_abs_path(&root, &folder, &name);
    let Ok(meta) = std::fs::symlink_metadata(&abs) else {
        return SsimgResponse::empty(404);
    };
    if fa.cloud_state(&abs, &meta) == CloudState::CloudOnly {
        return accepted("offline");
    }
    match thumbs::render_preview(cat.data_dir(), fa, id, &abs) {
        Ok(bytes) => SsimgResponse::ok("image/webp", bytes),
        Err(e) => {
            eprintln!("ssimg: preview {id}: {e}");
            SsimgResponse::empty(500)
        }
    }
}

fn accepted(state: &str) -> SsimgResponse {
    SsimgResponse { status: 202, headers: vec![(THUMB_STATE_HEADER, state.to_owned())], body: Vec::new() }
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
