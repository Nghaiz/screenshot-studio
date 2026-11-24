//! `get_stats`: [`CatalogCounts`] plus breakdowns, computed in SQL (the dashboard never counts on the
//! client).
//!
//! Populations: `byMonth`, `byAspect` and `byFolder` count the same rows as `total` (present files,
//! `missing = 0`, in-app trash included), so each of them sums to `total`. `byMonth` keys are the local
//! `YYYY-MM` of `taken_ms` (like `get_timeline`); `byAspect` keys are the `AspectClass` strings plus
//! `unknown` for rows without dimensions yet; `byFolder` keys are the folder relative to its root
//! (`''` = a root itself; the same folder name under two roots is one key, as the `folders` filter
//! treats it). `byCategory`: images with a category row of that label; `topTags`: the
//! [`TOP_TAGS`] most used tags (present images). `duplicatesExact` / `duplicatesSimilar`: redundant
//! copies (sum over groups of size - 1) of `find_duplicates` over the whole library.

use std::path::Path;

use rusqlite::Connection;

use super::duplicates::{self, Candidate};
use crate::contract::AppError;
use crate::contract::ipc::{CountBy, DuplicateMode, Stats};
use crate::query;

/// Tags listed in `topTags`.
pub const TOP_TAGS: i64 = 10;
/// `byAspect` key of rows without dimensions.
pub const ASPECT_UNKNOWN: &str = "unknown";

/// `data_dir` holds the grid thumbnails the `similar` count compares (the same rule as `find_duplicates`).
pub fn get_stats(conn: &Connection, data_dir: &Path, hamming: u32) -> Result<Stats, AppError> {
    let counts = query::get_counts(conn)?;
    let by_month = count_by(
        conn,
        "SELECT strftime('%Y-%m', taken_ms / 1000, 'unixepoch', 'localtime') AS k, count(*) FROM image
         WHERE missing = 0 GROUP BY k ORDER BY k",
        [],
    )?;
    let by_aspect = count_by(
        conn,
        "SELECT COALESCE(aspect, ?1) AS k, count(*) AS n FROM image WHERE missing = 0 GROUP BY k ORDER BY n DESC, k",
        [ASPECT_UNKNOWN],
    )?;
    let by_folder =
        count_by(conn, "SELECT folder AS k, count(*) FROM image WHERE missing = 0 GROUP BY k ORDER BY k", [])?;
    let by_category = count_by(
        conn,
        "SELECT c.label AS k, count(DISTINCT c.image_id) AS n FROM category c JOIN image i ON i.id = c.image_id
         WHERE i.missing = 0 GROUP BY k ORDER BY n DESC, k",
        [],
    )?;
    let top_tags = count_by(
        conn,
        "SELECT t.name AS k, count(*) AS n FROM tag t JOIN image_tag it ON it.tag_id = t.id JOIN image i ON i.id = it.image_id
         WHERE i.missing = 0 GROUP BY t.id ORDER BY n DESC, k LIMIT ?1",
        [TOP_TAGS],
    )?;
    let cands: Vec<Candidate> = duplicates::load(conn, None)?;
    let duplicates_exact = duplicates::redundant(&duplicates::groups(conn, data_dir, &cands, DuplicateMode::Exact, 0)?);
    let duplicates_similar = duplicates::redundant(&duplicates::groups(conn, data_dir, &cands, DuplicateMode::Similar, hamming)?);
    Ok(Stats { counts, by_month, by_aspect, by_folder, by_category, top_tags, duplicates_exact, duplicates_similar })
}

fn count_by<P: rusqlite::Params>(conn: &Connection, sql: &str, p: P) -> Result<Vec<CountBy>, AppError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(p, |r| Ok(CountBy { key: r.get::<_, Option<String>>(0)?.unwrap_or_default(), count: r.get(1)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}
