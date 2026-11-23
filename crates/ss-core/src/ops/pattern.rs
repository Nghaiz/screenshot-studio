//! Rename patterns (contract `preview_rename`): `{name}` (stem), `{n}` / `{n:03}` (1-based index,
//! zero-padded to the given width), `{date:<fmt>}` / `{time:<fmt>}` (local `taken` time; `<fmt>`
//! tokens `YYYY YY MM DD HH mm ss`, anything else copied), `{w}` `{h}` (pixels, `0` when unknown),
//! `{ext}` (original extension, no dot). A pattern without `{ext}` gets `.<ext>` appended.

use std::collections::HashMap;

use rusqlite::{Connection, params_from_iter};

use crate::contract::AppError;
use crate::errors::invalid;

use super::ImageRow;

/// Local date-time parts of an image's `taken_ms`: year, month, day, hour, minute, second.
pub(crate) type LocalTime = [String; 6];

const PAD_MAX: usize = 9;
const FORMAT_TOKENS: [(&str, usize); 7] = [("YYYY", 0), ("YY", 0), ("MM", 1), ("DD", 2), ("HH", 3), ("mm", 4), ("ss", 5)];

enum Part<'a> {
    Lit(&'a str),
    Token(&'a str),
}

fn parts(pattern: &str) -> Result<Vec<Part<'_>>, AppError> {
    let mut out = Vec::new();
    let mut rest = pattern;
    while !rest.is_empty() {
        match (rest.find('{'), rest.find('}')) {
            (Some(o), c) if c.is_none_or(|c| c > o) => {
                if o > 0 {
                    out.push(Part::Lit(&rest[..o]));
                }
                let close = rest[o..].find('}').ok_or_else(|| invalid("unclosed `{` in the pattern"))? + o;
                out.push(Part::Token(&rest[o + 1..close]));
                rest = &rest[close + 1..];
            }
            (_, Some(_)) => return Err(invalid("unmatched `}` in the pattern")),
            (None, None) => {
                out.push(Part::Lit(rest));
                rest = "";
            }
            _ => unreachable!(),
        }
    }
    Ok(out)
}

fn check_token(t: &str) -> Result<(), AppError> {
    let ok = match t.split_once(':') {
        None => matches!(t, "name" | "n" | "w" | "h" | "ext"),
        Some(("n", pad)) => pad.len() <= PAD_MAX && !pad.is_empty() && pad.bytes().all(|b| b.is_ascii_digit()),
        Some(("date" | "time", fmt)) => !fmt.is_empty(),
        _ => false,
    };
    if ok { Ok(()) } else { Err(invalid(format!("unknown pattern token `{{{t}}}`"))) }
}

pub(crate) fn validate(pattern: &str) -> Result<(), AppError> {
    if pattern.trim().is_empty() {
        return Err(invalid("the pattern is empty"));
    }
    for p in parts(pattern)? {
        if let Part::Token(t) = p {
            check_token(t)?;
        }
    }
    Ok(())
}

pub(crate) fn local_times(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, LocalTime>, AppError> {
    let mut out = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT id, strftime('%Y|%m|%d|%H|%M|%S', taken_ms / 1000, 'unixepoch', 'localtime') FROM image WHERE id IN ({})",
            vec!["?"; chunk.len()].join(", ")
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(chunk), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, s) = row?;
            let v: Vec<String> = s.split('|').map(str::to_owned).collect();
            if let Ok(t) = <[String; 6]>::try_from(v) {
                out.insert(id, t);
            }
        }
    }
    Ok(out)
}

fn format_time(fmt: &str, t: &LocalTime) -> String {
    let mut out = String::new();
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for (tok, idx) in FORMAT_TOKENS {
            if let Some(after) = rest.strip_prefix(tok) {
                let v = &t[idx];
                out.push_str(if tok == "YY" { &v[v.len().saturating_sub(2)..] } else { v });
                rest = after;
                continue 'outer;
            }
        }
        let c = rest.chars().next().expect("non-empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub(crate) fn render(pattern: &str, row: &ImageRow, n: i64, time: Option<&LocalTime>) -> Result<String, AppError> {
    let (stem, ext) = match row.file_name.rfind('.') {
        Some(i) if i > 0 => (&row.file_name[..i], &row.file_name[i + 1..]),
        _ => (row.file_name.as_str(), ""),
    };
    let zero = || std::array::from_fn::<String, 6, _>(|_| "00".to_owned());
    let mut out = String::new();
    let mut has_ext = false;
    for p in parts(pattern)? {
        match p {
            Part::Lit(s) => out.push_str(s),
            Part::Token(t) => {
                check_token(t)?;
                match t.split_once(':') {
                    None => match t {
                        "name" => out.push_str(stem),
                        "n" => out.push_str(&n.to_string()),
                        "w" => out.push_str(&row.width.unwrap_or(0).to_string()),
                        "h" => out.push_str(&row.height.unwrap_or(0).to_string()),
                        _ => {
                            has_ext = true;
                            out.push_str(ext);
                        }
                    },
                    Some(("n", pad)) => out.push_str(&format!("{n:0width$}", width = pad.parse::<usize>().unwrap_or(0))),
                    Some((_, fmt)) => out.push_str(&format_time(fmt, time.cloned().as_ref().unwrap_or(&zero()))),
                }
            }
        }
    }
    if !has_ext && !ext.is_empty() {
        out.push('.');
        out.push_str(ext);
    }
    Ok(out.trim().to_owned())
}
