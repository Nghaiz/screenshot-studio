//! Capture sessions: images whose `taken_ms` are no more than the gap apart, chained.

use crate::contract::ipc::{ImageSession, SortDir};

/// The `sessionGapMinutes` range (same bounds as the setting).
pub const GAP_MINUTES_MAX: i64 = 7 * 24 * 60;

/// Splits `(id, taken_ms)` rows (any order) into sessions wherever two neighbours in time are more
/// than `gap_ms` apart. `Asc`: oldest session first and oldest image first; `Desc`: newest first.
pub fn split(mut rows: Vec<(i64, i64)>, gap_ms: i64, dir: SortDir) -> Vec<ImageSession> {
    rows.sort_by_key(|&(id, t)| (t, id));
    let mut sessions: Vec<ImageSession> = Vec::new();
    for (id, t) in rows {
        match sessions.last_mut() {
            Some(s) if t - s.end_ms <= gap_ms => {
                s.end_ms = t;
                s.image_ids.push(id);
            }
            _ => sessions.push(ImageSession { start_ms: t, end_ms: t, image_ids: vec![id] }),
        }
    }
    if dir == SortDir::Desc {
        sessions.reverse();
        sessions.iter_mut().for_each(|s| s.image_ids.reverse());
    }
    sessions
}
