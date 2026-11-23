//! Read queries behind `query_images`, `get_image`, `get_timeline` and `get_counts`.
//!
//! Every `ImageQuery` filter is built by [`crate::filter::build`] (phase 2.1; `semantic` text is
//! refused until 3.2). Paging is keyset-based: the opaque cursor carries the last row's sort key and
//! id, so every id is returned exactly once.

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::catalog;
use crate::filter::{self, FilterSql};
use crate::contract::AppError;
use crate::contract::ipc::*;
use crate::errors::{enum_str, invalid, not_found, now_ms, parse_enum};
use crate::paths;

pub const LIMIT_MAX: i64 = 500;

const SUMMARY_COLUMNS: &str = "i.id, i.root_id, i.folder, i.file_name, i.size_bytes, i.taken_ms, i.width, i.height,
    i.aspect, i.cloud_state, i.thumb_state, i.favorite, i.rating,
    CASE WHEN o.image_id IS NULL THEN NULL ELSE length(trim(o.text)) > 0 END,
    i.dominant_rgb, i.trashed_at, i.missing, i.mtime_ms";
const FROM: &str = filter::FROM_IMAGES;

/// A summary row before its enum columns are parsed (parsing errors are `AppError`s, which a
/// rusqlite row closure cannot return).
struct RawSummary {
    summary: ImageSummary,
    aspect: Option<String>,
    cloud: String,
    thumb: String,
}

fn summary_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawSummary> {
    Ok(RawSummary {
        aspect: r.get(8)?,
        cloud: r.get(9)?,
        thumb: r.get(10)?,
        summary: ImageSummary {
            id: r.get(0)?,
            root_id: r.get(1)?,
            folder: r.get(2)?,
            file_name: r.get(3)?,
            size_bytes: r.get(4)?,
            taken_ms: r.get(5)?,
            width: r.get(6)?,
            height: r.get(7)?,
            aspect: None,
            cloud_state: CloudState::Unknown,
            thumb_state: ThumbState::Pending,
            favorite: r.get::<_, i64>(11)? != 0,
            rating: r.get(12)?,
            has_text: r.get::<_, Option<i64>>(13)?.map(|v| v != 0),
            dominant_rgb: r.get(14)?,
            trashed_at: r.get(15)?,
            missing: r.get::<_, i64>(16)? != 0,
            mtime_ms: r.get(17)?,
        },
    })
}

fn finish(raw: RawSummary) -> Result<ImageSummary, AppError> {
    let mut s = raw.summary;
    s.aspect = raw.aspect.as_deref().map(parse_enum).transpose()?;
    s.cloud_state = parse_enum(&raw.cloud)?;
    s.thumb_state = parse_enum(&raw.thumb)?;
    Ok(s)
}

/// One image row by id.
pub fn summary(conn: &Connection, id: i64) -> Result<Option<ImageSummary>, AppError> {
    conn.query_row(&format!("SELECT {SUMMARY_COLUMNS} {FROM} WHERE i.id = ?1"), [id], summary_from_row)
        .optional()?
        .map(finish)
        .transpose()
}

// ---------------------------------------------------------------------------------------------
// query_images
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct Cursor {
    /// sort field + dir, so a cursor cannot be replayed against another sort
    f: SortField,
    d: SortDir,
    /// last sort key (integer or text)
    k: serde_json::Value,
    /// last id
    i: i64,
    /// random seed (sort `random` only)
    s: Option<i64>,
}

fn encode_cursor(c: &Cursor) -> String {
    serde_json::to_vec(c).expect("cursor serializes").iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_cursor(s: &str) -> Result<Cursor, AppError> {
    let bad = || invalid("invalid cursor");
    if s.len() % 2 != 0 || !s.is_ascii() {
        return Err(bad());
    }
    let bytes: Vec<u8> =
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16)).collect::<Result<_, _>>().map_err(|_| bad())?;
    serde_json::from_slice(&bytes).map_err(|_| bad())
}

/// SQL expression of the sort key. Every key is non-NULL so keyset comparisons are total.
fn sort_expr(field: SortField, seed: i64, relevance: Option<&str>) -> Result<String, AppError> {
    Ok(match field {
        SortField::Taken => "i.taken_ms".into(),
        SortField::Name => "lower(i.file_name)".into(),
        SortField::Size => "i.size_bytes".into(),
        SortField::Width => "COALESCE(i.width, -1)".into(),
        SortField::Rating => "i.rating".into(),
        SortField::Random => format!("((i.id * 2654435761 + {seed}) % 4294967291)"),
        SortField::Relevance => match relevance {
            Some(r) => r.to_owned(),
            None => return Err(invalid("sort `relevance` needs a text query")),
        },
    })
}

fn sql_to_json(v: Value) -> serde_json::Value {
    match v {
        Value::Integer(i) => i.into(),
        Value::Text(s) => s.into(),
        Value::Real(f) => serde_json::Number::from_f64(f).map(Into::into).unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    }
}

fn json_to_sql(v: &serde_json::Value) -> Result<Value, AppError> {
    match v {
        serde_json::Value::Number(n) if n.is_i64() => Ok(Value::Integer(n.as_i64().unwrap_or_default())),
        serde_json::Value::Number(n) => Ok(Value::Real(n.as_f64().unwrap_or_default())),
        serde_json::Value::String(s) => Ok(Value::Text(s.clone())),
        _ => Err(invalid("invalid cursor")),
    }
}

pub fn query_images(conn: &Connection, q: &ImageQuery) -> Result<ImagePage, AppError> {
    if !(1..=LIMIT_MAX).contains(&q.limit) {
        return Err(invalid(format!("limit must be 1..{LIMIT_MAX}, got {}", q.limit)));
    }
    let FilterSql { from, filter, params: filter_params, relevance } = filter::build(q)?;
    let cursor = q.cursor.as_deref().map(decode_cursor).transpose()?;
    if let Some(c) = &cursor {
        if c.f != q.sort.field || c.d != q.sort.dir {
            return Err(invalid("cursor does not match the sort of this query"));
        }
    }
    let seed = cursor.as_ref().and_then(|c| c.s).unwrap_or_else(|| now_ms().rem_euclid(1_000_003));
    let expr = sort_expr(q.sort.field, seed, relevance.as_deref())?;

    let total: i64 = conn.query_row(&format!("SELECT count(*) {from} WHERE {filter}"), params_from_iter(&filter_params), |r| {
        r.get(0)
    })?;

    let (cmp, dir) = match q.sort.dir {
        SortDir::Asc => (">", "ASC"),
        SortDir::Desc => ("<", "DESC"),
    };
    let mut page_params = filter_params.clone();
    let mut sql = format!("SELECT {SUMMARY_COLUMNS}, {expr} AS sort_key {from} WHERE {filter}");
    if let Some(c) = &cursor {
        sql.push_str(&format!(" AND ({expr}, i.id) {cmp} (?, ?)"));
        page_params.push(json_to_sql(&c.k)?);
        page_params.push(Value::Integer(c.i));
    }
    sql.push_str(&format!(" ORDER BY sort_key {dir}, i.id {dir} LIMIT ?"));
    page_params.push(Value::Integer(q.limit + 1));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&page_params), |r| Ok((summary_from_row(r)?, r.get::<_, Value>(18)?)))?;
    let mut items = Vec::new();
    let mut keys = Vec::new();
    for row in rows {
        let (summary_row, key) = row?;
        items.push(finish(summary_row)?);
        keys.push(key);
    }
    let next_cursor = if items.len() as i64 > q.limit {
        items.truncate(q.limit as usize);
        let last = items.last().expect("limit >= 1");
        let key = keys.swap_remove(q.limit as usize - 1);
        Some(encode_cursor(&Cursor {
            f: q.sort.field,
            d: q.sort.dir,
            k: sql_to_json(key),
            i: last.id,
            s: (q.sort.field == SortField::Random).then_some(seed),
        }))
    } else {
        None
    };
    Ok(ImagePage { items, total, next_cursor })
}

// ---------------------------------------------------------------------------------------------
// get_image
// ---------------------------------------------------------------------------------------------

pub fn get_image(conn: &Connection, id: i64) -> Result<ImageDetail, AppError> {
    let summary = summary(conn, id)?.ok_or_else(|| not_found(format!("image {id} not found")))?;
    let (root, created_ms, content_hash, note, blur_score): (String, Option<i64>, Option<String>, Option<String>, Option<f64>) =
        conn.query_row(
            "SELECT r.path, i.created_ms, i.content_hash, i.note, i.blur_score
             FROM image i JOIN library_root r ON r.id = i.root_id WHERE i.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
    let tags = {
        let mut stmt = conn.prepare_cached(
            "SELECT t.id, t.name, t.color, (SELECT count(*) FROM image_tag x WHERE x.tag_id = t.id)
             FROM tag t JOIN image_tag it ON it.tag_id = t.id WHERE it.image_id = ?1 ORDER BY t.name",
        )?;
        stmt.query_map([id], |r| Ok(Tag { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, count: r.get(3)? }))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let albums = {
        let mut stmt = conn.prepare_cached(
            "SELECT a.id, a.name FROM album a JOIN album_image ai ON ai.album_id = a.id WHERE ai.image_id = ?1 ORDER BY a.name",
        )?;
        stmt.query_map([id], |r| Ok(AlbumRef { id: r.get(0)?, name: r.get(1)? }))?.collect::<Result<Vec<_>, _>>()?
    };
    let ocr_text: Option<String> =
        conn.query_row("SELECT text FROM ocr_result WHERE image_id = ?1", [id], |r| r.get(0)).optional()?;
    let categories = {
        let mut stmt = conn.prepare_cached("SELECT label, score FROM category WHERE image_id = ?1 ORDER BY score DESC")?;
        stmt.query_map([id], |r| Ok(CategoryScore { label: r.get(0)?, score: r.get(1)? }))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let abs_path = paths::image_abs_path(&root, &summary.folder, &summary.file_name).to_string_lossy().into_owned();
    Ok(ImageDetail { summary, abs_path, created_ms, content_hash, note, tags, albums, ocr_text, categories, blur_score })
}

// ---------------------------------------------------------------------------------------------
// get_timeline
// ---------------------------------------------------------------------------------------------

/// Buckets by local calendar day or month of `taken_ms`; `startMs` is the bucket's local midnight.
pub fn get_timeline(conn: &Connection, req: &GetTimelineReq) -> Result<TimelineResult, AppError> {
    let fmt = match req.bucket {
        TimelineBucketSize::Day => "%Y-%m-%d",
        TimelineBucketSize::Month => "%Y-%m-01",
    };
    let FilterSql { from, filter, params, .. } = filter::build(&req.query)?;
    let sql = format!(
        "SELECT CAST(strftime('%s', k, 'utc') AS INTEGER) * 1000, n FROM (
           SELECT strftime('{fmt}', i.taken_ms / 1000, 'unixepoch', 'localtime') AS k, count(*) AS n
           {from} WHERE {filter} GROUP BY k) ORDER BY k"
    );
    let mut stmt = conn.prepare(&sql)?;
    let buckets = stmt
        .query_map(params_from_iter(&params), |r| Ok(TimelineBucket { start_ms: r.get(0)?, count: r.get(1)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TimelineResult { buckets })
}

// ---------------------------------------------------------------------------------------------
// get_counts
// ---------------------------------------------------------------------------------------------

/// `total`, `totalBytes`, `thumb*` and `cloudOnly` count present files (missing = 0), trashed
/// included; `missing` counts vanished rows; `trashed` counts present rows in the in-app trash.
pub fn get_counts(conn: &Connection) -> Result<CatalogCounts, AppError> {
    let (ok, pending, offline) = (enum_str(&ThumbState::Ok), enum_str(&ThumbState::Pending), enum_str(&CloudState::CloudOnly));
    let mut counts = conn.query_row(
        "SELECT
           count(*) FILTER (WHERE missing = 0),
           COALESCE(sum(size_bytes) FILTER (WHERE missing = 0), 0),
           count(*) FILTER (WHERE missing = 0 AND thumb_state = ?1),
           count(*) FILTER (WHERE missing = 0 AND thumb_state = ?2),
           count(*) FILTER (WHERE missing = 0 AND cloud_state = ?3),
           count(*) FILTER (WHERE missing = 1),
           count(*) FILTER (WHERE missing = 0 AND trashed_at IS NOT NULL)
         FROM image",
        [ok, pending, offline],
        |r| {
            Ok(CatalogCounts {
                total: r.get(0)?,
                total_bytes: r.get(1)?,
                thumb_ok: r.get(2)?,
                thumb_pending: r.get(3)?,
                cloud_only: r.get(4)?,
                missing: r.get(5)?,
                trashed: r.get(6)?,
                roots: Vec::new(),
            })
        },
    )?;
    counts.roots = catalog::list_roots(conn)?;
    Ok(counts)
}

/// The `ImageQuery` with every filter at its "no constraint" value (used by `ssctl list`).
pub fn default_query(limit: i64) -> ImageQuery {
    ImageQuery {
        text: None,
        text_mode: TextMode::Any,
        date_from: None,
        date_to: None,
        min_width: None,
        max_width: None,
        min_height: None,
        max_height: None,
        aspects: Vec::new(),
        min_size: None,
        max_size: None,
        folders: Vec::new(),
        tag_ids: Vec::new(),
        tag_match: TagMatch::Any,
        album_id: None,
        color: None,
        color_tolerance: 0,
        categories: Vec::new(),
        favorite_only: false,
        min_rating: 0,
        has_text: None,
        trash: TrashFilter::Exclude,
        include_missing: false,
        sort: SortSpec { field: SortField::Taken, dir: SortDir::Desc },
        cursor: None,
        limit,
    }
}
