//! `ImageQuery -> (FROM, WHERE, params)`: the ONE place a query's filters become SQL (phase 2.1).
//! Used by `query_images`, `get_timeline` and saved smart filters (validation).
//!
//! Filter semantics (an empty array / `null` means "no constraint"):
//! - `text` (non-blank) + `textMode`: FTS5 `MATCH` over `text_fts`. Each whitespace-separated word
//!   becomes a quoted FTS phrase with a prefix star (`"chup"*`), so FTS syntax in the input is inert;
//!   words are ANDed. Diacritics are folded by the tokenizer (`unicode61 remove_diacritics 2`), so
//!   `anh chup` finds `Ảnh chụp.png` (note: `đ` is a separate letter, not a decorated `d`).
//!   `any` = file name, OCR text and note; `filename` / `ocr` = that column only; `semantic` needs
//!   the AI sidecar (phase 3.2) and is refused with `sidecarUnavailable`. A text with no letter or
//!   digit matches nothing.
//! - `folders`: an image matches a folder when it is in it or below it; `""` = the root folder itself.
//! - `tagIds` + `tagMatch`: `any` = at least one of the tags, `all` = every one of them.
//! - `color` + `colorTolerance` (0..100): Euclidean RGB distance of `dominant_rgb` <= tolerance% of
//!   the largest possible distance; images without a dominant colour never match.
//! - `hasText`: `true` / `false` match the summary's `hasText` (an OCR row exists and its text is
//!   non-empty / empty); images without an OCR row match neither.
//! - `categories`: at least one category row with one of the labels.

use rusqlite::types::Value;

use crate::contract::ipc::*;
use crate::contract::{AppError, ErrorKind};
use crate::errors::invalid;

/// Base `FROM` of every image query (the OCR join feeds `hasText`).
pub const FROM_IMAGES: &str = "FROM image i LEFT JOIN ocr_result o ON o.image_id = i.id";
/// The largest RGB distance, `sqrt(3 * 255^2)`.
const MAX_RGB_DISTANCE: f64 = 441.672_955_930_063_7;
const MAX_TOLERANCE: i64 = 100;
const MAX_RATING: i64 = 5;

/// A query's filters as SQL.
#[derive(Debug, Clone)]
pub struct FilterSql {
    /// `FROM ...` including the FTS join when a text search is active.
    pub from: String,
    /// `WHERE` body (never empty).
    pub filter: String,
    pub params: Vec<Value>,
    /// Relevance sort key (higher = more relevant) when an FTS text search is active.
    pub relevance: Option<String>,
}

/// Validates every field of `q` that is not paging and builds its SQL.
pub fn build(q: &ImageQuery) -> Result<FilterSql, AppError> {
    let mut parts = vec!["1 = 1".to_owned()];
    let mut params: Vec<Value> = Vec::new();
    let mut from = FROM_IMAGES.to_owned();
    let mut relevance = None;

    match q.trash {
        TrashFilter::Exclude => parts.push("i.trashed_at IS NULL".into()),
        TrashFilter::Only => parts.push("i.trashed_at IS NOT NULL".into()),
        TrashFilter::Include => {}
    }
    if !q.include_missing {
        parts.push("i.missing = 0".into());
    }
    let mut cmp = |col: &str, op: &str, v: Option<i64>| {
        if let Some(v) = v {
            parts.push(format!("{col} {op} ?"));
            params.push(Value::Integer(v));
        }
    };
    cmp("i.taken_ms", ">=", q.date_from);
    cmp("i.taken_ms", "<=", q.date_to);
    cmp("i.width", ">=", q.min_width);
    cmp("i.width", "<=", q.max_width);
    cmp("i.height", ">=", q.min_height);
    cmp("i.height", "<=", q.max_height);
    cmp("i.size_bytes", ">=", q.min_size);
    cmp("i.size_bytes", "<=", q.max_size);

    if !q.aspects.is_empty() {
        parts.push(format!("i.aspect IN ({})", placeholders(q.aspects.len())));
        params.extend(q.aspects.iter().map(|a| Value::Text(crate::errors::enum_str(a))));
    }
    if !q.folders.is_empty() {
        let mut ors = Vec::new();
        for f in &q.folders {
            let f = f.trim().trim_matches('/').replace('\\', "/");
            if f.is_empty() {
                ors.push("i.folder = ''".to_owned());
            } else {
                ors.push("(i.folder = ? COLLATE NOCASE OR i.folder LIKE ? ESCAPE '\\')".to_owned());
                params.push(Value::Text(f.clone()));
                params.push(Value::Text(format!("{}/%", like_escape(&f))));
            }
        }
        parts.push(format!("({})", ors.join(" OR ")));
    }
    if !q.tag_ids.is_empty() {
        let mut ids = q.tag_ids.clone();
        ids.sort_unstable();
        ids.dedup();
        let list = placeholders(ids.len());
        match q.tag_match {
            TagMatch::Any => parts.push(format!(
                "EXISTS (SELECT 1 FROM image_tag t WHERE t.image_id = i.id AND t.tag_id IN ({list}))"
            )),
            TagMatch::All => {
                parts.push(format!(
                    "(SELECT count(*) FROM image_tag t WHERE t.image_id = i.id AND t.tag_id IN ({list})) = {}",
                    ids.len()
                ));
            }
        }
        params.extend(ids.into_iter().map(Value::Integer));
    }
    if let Some(album) = q.album_id {
        parts.push("EXISTS (SELECT 1 FROM album_image a WHERE a.image_id = i.id AND a.album_id = ?)".into());
        params.push(Value::Integer(album));
    }
    if !(0..=MAX_TOLERANCE).contains(&q.color_tolerance) {
        return Err(invalid(format!("colorTolerance must be 0..{MAX_TOLERANCE}, got {}", q.color_tolerance)));
    }
    if let Some(color) = q.color.as_deref() {
        let (r, g, b) = parse_rgb(color).ok_or_else(|| invalid(format!("color must be #RRGGBB, got `{color}`")))?;
        let max = MAX_RGB_DISTANCE * q.color_tolerance as f64 / MAX_TOLERANCE as f64;
        let d = |pos: usize| format!("({} - ?)", hex_channel_sql(pos));
        parts.push(format!(
            "(i.dominant_rgb IS NOT NULL AND length(i.dominant_rgb) = 7 AND
              {r_} * {r_} + {g_} * {g_} + {b_} * {b_} <= ?)",
            r_ = d(2),
            g_ = d(4),
            b_ = d(6)
        ));
        for c in [r, r, g, g, b, b] {
            params.push(Value::Integer(c));
        }
        params.push(Value::Real(max * max));
    }
    if !q.categories.is_empty() {
        parts.push(format!(
            "EXISTS (SELECT 1 FROM category c WHERE c.image_id = i.id AND c.label IN ({}))",
            placeholders(q.categories.len())
        ));
        params.extend(q.categories.iter().map(|c| Value::Text(c.clone())));
    }
    if q.favorite_only {
        parts.push("i.favorite = 1".into());
    }
    if !(0..=MAX_RATING).contains(&q.min_rating) {
        return Err(invalid(format!("minRating must be 0..{MAX_RATING}, got {}", q.min_rating)));
    }
    if q.min_rating > 0 {
        parts.push("i.rating >= ?".into());
        params.push(Value::Integer(q.min_rating));
    }
    match q.has_text {
        Some(true) => parts.push("(o.image_id IS NOT NULL AND length(trim(o.text)) > 0)".into()),
        Some(false) => parts.push("(o.image_id IS NOT NULL AND length(trim(o.text)) = 0)".into()),
        None => {}
    }

    if let Some(text) = q.text.as_deref().filter(|t| !t.trim().is_empty()) {
        let column = match q.text_mode {
            TextMode::Any => None,
            TextMode::Filename => Some("file_name"),
            TextMode::Ocr => Some("ocr_text"),
            TextMode::Semantic => {
                return Err(AppError::new(ErrorKind::SidecarUnavailable, "semantic search needs the AI sidecar")
                    .with_detail("semantic search lands in phase 3.2"));
            }
        };
        match fts_expression(text, column) {
            Some(expr) => {
                from.push_str(" JOIN text_fts ON text_fts.rowid = i.id");
                parts.push("text_fts MATCH ?".into());
                params.push(Value::Text(expr));
                relevance = Some("(-bm25(text_fts))".to_owned());
            }
            None => {
                parts.push("0 = 1".into());
                relevance = Some("0".to_owned());
            }
        }
    }
    Ok(FilterSql { from, filter: parts.join(" AND "), params, relevance })
}

/// FTS5 expression for the user's text: every word a quoted prefix phrase (quotes doubled), ANDed,
/// optionally restricted to one column. `None` when no word has a letter or digit.
pub fn fts_expression(text: &str, column: Option<&str>) -> Option<String> {
    let words: Vec<String> = text
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .map(|w| {
            let phrase = format!("\"{}\"*", w.replace('"', "\"\""));
            match column {
                Some(c) => format!("{c} : {phrase}"),
                None => phrase,
            }
        })
        .collect();
    (!words.is_empty()).then(|| words.join(" AND "))
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// `#RRGGBB` -> channels.
pub fn parse_rgb(s: &str) -> Option<(i64, i64, i64)> {
    let hex = s.strip_prefix('#').filter(|h| h.len() == 6 && h.is_ascii())?;
    let ch = |i: usize| i64::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((ch(0)?, ch(2)?, ch(4)?))
}

/// SQL for one 0..255 channel of `i.dominant_rgb` (`#RRGGBB`), the two hex digits at 1-based `pos`.
fn hex_channel_sql(pos: usize) -> String {
    let digit = |p: usize| format!("(instr('0123456789ABCDEF', upper(substr(i.dominant_rgb, {p}, 1))) - 1)");
    format!("({} * 16 + {})", digit(pos), digit(pos + 1))
}
