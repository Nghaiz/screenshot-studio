//! Blurry and tiny images (`detect_issues`) and the blur-score backfill for rows indexed before the
//! thumbnail pass computed it (phase 3.2). Reads only the 320 px thumbnails in the data dir, never an
//! original, so it never touches a cloud-only file.

use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use rusqlite::{Connection, params};

use super::blur;
use crate::catalog::{Catalog, WRITE_BATCH};
use crate::contract::AppError;
use crate::contract::ipc::IssuesResult;
use crate::contract::ipc::ssimg::THUMB_GRID;
use crate::paths;
use crate::scan::cancelled;
use crate::thumbs::worker_pool;

/// Computes `blur_score` for every row with an `ok` thumbnail and no score yet. Returns how many
/// were scored. A thumbnail file that is gone or does not decode is skipped (its row stays NULL).
pub fn backfill_blur(cat: &Catalog, root_id: Option<i64>, cancel: &AtomicBool) -> Result<i64, AppError> {
    let ids: Vec<i64> = cat.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id FROM image WHERE thumb_state = 'ok' AND blur_score IS NULL AND missing = 0
               AND (?1 IS NULL OR root_id = ?1) ORDER BY id",
        )?;
        let rows = stmt.query_map(params![root_id], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    })?;
    if ids.is_empty() {
        return Ok(0);
    }
    let data_dir = cat.data_dir().to_path_buf();
    let pool = worker_pool()?;
    let mut scored = 0;
    for chunk in ids.chunks(WRITE_BATCH) {
        let scores: Vec<(i64, f64)> = pool.install(|| {
            chunk
                .par_iter()
                .filter(|_| !cancel.load(Ordering::Relaxed))
                .filter_map(|&id| {
                    let bytes = std::fs::read(paths::thumb_path(&data_dir, id, THUMB_GRID)).ok()?;
                    blur::blur_of_thumbnail(&bytes).map(|s| (id, s))
                })
                .collect()
        });
        scored += scores.len() as i64;
        cat.write(move |tx| {
            let mut stmt = tx.prepare_cached("UPDATE image SET blur_score = ?2 WHERE id = ?1 AND blur_score IS NULL")?;
            for (id, s) in scores {
                stmt.execute(params![id, s])?;
            }
            Ok(())
        })?;
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
    }
    Ok(scored)
}

/// `blurry`: scored below `blur_threshold`; `tiny`: fewer than `tiny_max_pixels` pixels. Present,
/// not in the in-app trash, optionally one root; ids ascending.
pub fn detect(conn: &Connection, root_id: Option<i64>, blur_threshold: f64, tiny_max_pixels: i64) -> Result<IssuesResult, AppError> {
    let ids = |sql: &str, p: rusqlite::types::Value| -> Result<Vec<i64>, AppError> {
        let mut stmt = conn.prepare(&format!(
            "SELECT id FROM image WHERE missing = 0 AND trashed_at IS NULL AND (?1 IS NULL OR root_id = ?1) AND {sql} ORDER BY id"
        ))?;
        let rows = stmt.query_map(params![root_id, p], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    };
    Ok(IssuesResult {
        blurry: ids("blur_score IS NOT NULL AND blur_score < ?2", blur_threshold.into())?,
        tiny: ids("width IS NOT NULL AND height IS NOT NULL AND width * height < ?2", tiny_max_pixels.into())?,
    })
}
