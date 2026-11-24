//! Duplicate groups (works without the sidecar, Deferred decision #18).
//!
//! - `exact`: same blake3 `content_hash`.
//! - `similar`: dHash (64-bit gradient hash) Hamming distance <= `hamming`. Groups are seeded, not
//!   chained: images are taken in id order, and each not-yet-grouped image collects every not-yet-
//!   grouped image within `hamming` of ITSELF (a BK-tree query), so a chain of small differences never
//!   merges two unrelated screenshots. Low-information hashes ([`is_low_information`]) take no part:
//!   a near-flat image has almost no gradient bits, so any two of them are within `hamming` however
//!   different they are (Deferred decision #33). They stay eligible for `exact`.
//!
//! Population: present (`missing = 0`), not in the in-app trash, optionally one root. Best image of a
//! group: most pixels, then newest (`taken_ms`, then `mtime_ms`), then sharpest (`blur_score`), then
//! lowest id. `reason` names the first criterion that set the best apart from the runner-up (`largest`
//! when the group is a tie on all three, as exact copies usually are).

use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::contract::AppError;
use crate::contract::ipc::{DuplicateGroup, DuplicateMode, DuplicateReason, DuplicateResult};

/// The largest meaningful Hamming distance of a 64-bit hash.
pub const MAX_HAMMING: i64 = 64;

/// A dHash with fewer set bits than this, or fewer clear bits, carries too little signal for `similar`
/// (Deferred decision #33). Basis, real library 2026-11-24 (5,662 hashes, counts in README Re-score
/// phase 3.4b): of the 241 pairs within Hamming 6 whose lower hash has 0-9 set bits, 0-22% per bucket
/// had matching aspect ratios (within 15%), against 62-94% for the 870 pairs from 10 set bits up; every
/// hash with 0-6 set bits had such a partner (the 13 with 0 bits are all 1-14 px slivers). The 4 hashes
/// with 58-64 set bits formed 2 pairs, both of different aspects.
pub const DUP_MIN_INFO_BITS: u32 = 10;

/// True when `dhash` has fewer than [`DUP_MIN_INFO_BITS`] set bits or fewer than that many clear bits:
/// a flat (or one-way gradient) image whose hash collides with every other flat image.
pub fn is_low_information(dhash: i64) -> bool {
    let ones = (dhash as u64).count_ones();
    ones.min(64 - ones) < DUP_MIN_INFO_BITS
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: i64,
    pub content_hash: Option<String>,
    pub dhash: Option<i64>,
    pub pixels: i64,
    pub taken_ms: i64,
    pub mtime_ms: i64,
    pub blur: Option<f64>,
}

pub fn load(conn: &Connection, root_id: Option<i64>) -> Result<Vec<Candidate>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, content_hash, dhash, COALESCE(width, 0) * COALESCE(height, 0), taken_ms, mtime_ms, blur_score
         FROM image WHERE missing = 0 AND trashed_at IS NULL AND (?1 IS NULL OR root_id = ?1) ORDER BY id",
    )?;
    let rows = stmt.query_map(params![root_id], |r| {
        Ok(Candidate {
            id: r.get(0)?,
            content_hash: r.get(1)?,
            dhash: r.get(2)?,
            pixels: r.get(3)?,
            taken_ms: r.get(4)?,
            mtime_ms: r.get(5)?,
            blur: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Index lists (into `c`) of the groups of `mode`, each with at least two members.
pub fn groups(c: &[Candidate], mode: DuplicateMode, hamming: u32) -> Vec<Vec<usize>> {
    match mode {
        DuplicateMode::Exact => exact_groups(c),
        DuplicateMode::Similar => similar_groups(c, hamming),
    }
}

fn exact_groups(c: &[Candidate]) -> Vec<Vec<usize>> {
    let mut by_hash: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, cand) in c.iter().enumerate() {
        if let Some(h) = cand.content_hash.as_deref() {
            by_hash.entry(h).or_default().push(i);
        }
    }
    let mut out: Vec<Vec<usize>> = by_hash.into_values().filter(|g| g.len() > 1).collect();
    out.sort_by_key(|g| g[0]);
    out
}

/// The hash `similar` may compare: present and not low-information.
fn informative(cand: &Candidate) -> Option<i64> {
    cand.dhash.filter(|&h| !is_low_information(h))
}

fn similar_groups(c: &[Candidate], hamming: u32) -> Vec<Vec<usize>> {
    let mut tree = BkTree::default();
    for (i, cand) in c.iter().enumerate() {
        if let Some(h) = informative(cand) {
            tree.insert(h as u64, i);
        }
    }
    let mut taken = vec![false; c.len()];
    let mut out = Vec::new();
    for (i, cand) in c.iter().enumerate() {
        let Some(h) = informative(cand) else { continue };
        if taken[i] {
            continue;
        }
        let mut group: Vec<usize> = tree.within(h as u64, hamming).into_iter().filter(|&j| !taken[j]).collect();
        if group.len() < 2 {
            continue;
        }
        group.sort_unstable();
        for &j in &group {
            taken[j] = true;
        }
        out.push(group);
    }
    out
}

/// Ranks best first (see the module doc).
fn rank(a: &Candidate, b: &Candidate) -> Ordering {
    b.pixels
        .cmp(&a.pixels)
        .then((b.taken_ms, b.mtime_ms).cmp(&(a.taken_ms, a.mtime_ms)))
        .then(b.blur.unwrap_or(f64::MIN).total_cmp(&a.blur.unwrap_or(f64::MIN)))
        .then(a.id.cmp(&b.id))
}

/// Best member and why it won.
pub fn best_of(c: &[Candidate], group: &[usize]) -> (usize, DuplicateReason) {
    let mut sorted = group.to_vec();
    sorted.sort_by(|&a, &b| rank(&c[a], &c[b]));
    let (best, second) = (&c[sorted[0]], &c[sorted[1]]);
    let reason = if best.pixels != second.pixels {
        DuplicateReason::Largest
    } else if (best.taken_ms, best.mtime_ms) != (second.taken_ms, second.mtime_ms) {
        DuplicateReason::Newest
    } else if best.blur.unwrap_or(f64::MIN) != second.blur.unwrap_or(f64::MIN) {
        DuplicateReason::Sharpest
    } else {
        DuplicateReason::Largest
    };
    (sorted[0], reason)
}

/// `find_duplicates`: groups with the best image first, then the others by id; groups ordered by
/// size (largest first), then by their best id. `groupId` is 1-based in that order.
pub fn find(c: &[Candidate], mode: DuplicateMode, hamming: u32) -> DuplicateResult {
    let mut groups: Vec<(usize, DuplicateReason, Vec<usize>)> = groups(c, mode, hamming)
        .into_iter()
        .map(|g| {
            let (best, reason) = best_of(c, &g);
            (best, reason, g)
        })
        .collect();
    groups.sort_by(|a, b| b.2.len().cmp(&a.2.len()).then(c[a.0].id.cmp(&c[b.0].id)));
    let groups = groups
        .into_iter()
        .enumerate()
        .map(|(n, (best, reason, members))| {
            let mut image_ids = vec![c[best].id];
            image_ids.extend(members.iter().filter(|&&m| m != best).map(|&m| c[m].id));
            DuplicateGroup { group_id: n as i64 + 1, image_ids, best_id: c[best].id, reason }
        })
        .collect();
    DuplicateResult { groups }
}

/// Images that are redundant copies: the sum over groups of (size - 1).
pub fn redundant(groups: &[Vec<usize>]) -> i64 {
    groups.iter().map(|g| g.len() as i64 - 1).sum()
}

/// BK-tree over 64-bit hashes with the Hamming metric.
#[derive(Default)]
struct BkTree {
    nodes: Vec<BkNode>,
}

struct BkNode {
    hash: u64,
    items: Vec<usize>,
    children: Vec<(u32, usize)>,
}

impl BkTree {
    fn insert(&mut self, hash: u64, item: usize) {
        if self.nodes.is_empty() {
            self.nodes.push(BkNode { hash, items: vec![item], children: Vec::new() });
            return;
        }
        let mut at = 0;
        loop {
            let d = (self.nodes[at].hash ^ hash).count_ones();
            if d == 0 {
                self.nodes[at].items.push(item);
                return;
            }
            match self.nodes[at].children.iter().find(|(cd, _)| *cd == d) {
                Some(&(_, next)) => at = next,
                None => {
                    let next = self.nodes.len();
                    self.nodes.push(BkNode { hash, items: vec![item], children: Vec::new() });
                    self.nodes[at].children.push((d, next));
                    return;
                }
            }
        }
    }

    fn within(&self, hash: u64, max: u32) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = if self.nodes.is_empty() { Vec::new() } else { vec![0] };
        while let Some(at) = stack.pop() {
            let node = &self.nodes[at];
            let d = (node.hash ^ hash).count_ones();
            if d <= max {
                out.extend_from_slice(&node.items);
            }
            for &(cd, child) in &node.children {
                if cd + max >= d && cd <= d + max {
                    stack.push(child);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: i64, dhash: i64) -> Candidate {
        Candidate { id, content_hash: None, dhash: Some(dhash), pixels: 100, taken_ms: id, mtime_ms: 0, blur: None }
    }

    #[test]
    fn bk_tree_matches_brute_force_and_groups_do_not_chain() {
        let hashes: Vec<i64> = (0..300).map(|i: i64| i.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64) ^ (i << 7)).collect();
        let mut tree = BkTree::default();
        for (i, h) in hashes.iter().enumerate() {
            tree.insert(*h as u64, i);
        }
        for (i, h) in hashes.iter().enumerate().step_by(7) {
            for max in [0, 3, 12, 30] {
                let mut got = tree.within(*h as u64, max);
                got.sort_unstable();
                let want: Vec<usize> = (0..hashes.len()).filter(|&j| ((hashes[j] ^ h) as u64).count_ones() <= max).collect();
                assert_eq!(got, want, "item {i} max {max}");
            }
        }
        // a - b - c each 4 bits apart, a - c 8 bits: with hamming 4 the seed a takes b only.
        let chain = vec![cand(1, BALANCED), cand(2, BALANCED ^ 0xF), cand(3, BALANCED ^ 0xFF)];
        assert_eq!(similar_groups(&chain, 4), vec![vec![0, 1]]);
    }

    /// 32 set bits, 32 clear: an ordinary screenshot's hash.
    const BALANCED: i64 = 0x0F0F_0F0F_0F0F_0F0F;
    /// A hash with exactly `ones` set bits (the low ones).
    fn bits(ones: u32) -> i64 {
        if ones == 64 { -1 } else { ((1u64 << ones) - 1) as i64 }
    }
    fn ids(c: &[Candidate], groups: &[Vec<usize>]) -> Vec<Vec<i64>> {
        groups.iter().map(|g| g.iter().map(|&i| c[i].id).collect()).collect()
    }

    #[test]
    fn low_information_threshold_is_symmetric() {
        for ones in 0..=64 {
            let low = !(DUP_MIN_INFO_BITS..=64 - DUP_MIN_INFO_BITS).contains(&ones);
            assert_eq!(is_low_information(bits(ones)), low, "{ones} set bits");
        }
        assert!(!is_low_information(BALANCED));
    }

    #[test]
    fn low_information_images_never_form_or_join_a_similar_group() {
        // Real-library shape (README Re-score phase 3.4b): flat slivers (0 bits), near-blank screenshots
        // (2-9 bits) and one-way gradients (55+ bits), every pair within the default Hamming 6 or close.
        let c = vec![
            cand(1, 0),
            cand(2, 0),
            cand(3, bits(3)),
            cand(4, bits(9)),
            cand(5, bits(9) << 3),
            cand(6, bits(55)),
            cand(7, -1),
            // Informative pair at the boundary (exactly 10 set bits / 10 clear bits) and an ordinary pair.
            cand(8, bits(10)),
            cand(9, bits(10) << 2),
            cand(10, !bits(10)),
            cand(11, !(bits(10) << 2)),
            cand(12, BALANCED),
            cand(13, BALANCED ^ 0b101),
        ];
        let got = ids(&c, &similar_groups(&c, 6));
        assert_eq!(got, vec![vec![8, 9], vec![10, 11], vec![12, 13]], "low-information hashes (ids 1-7) are in no group");
        // With every pair in range (Hamming 64) only the informative ones gather.
        assert_eq!(ids(&c, &similar_groups(&c, 64)), vec![vec![8, 9, 10, 11, 12, 13]]);
        // A low-information image within range of an informative one does not join it either.
        let mixed = vec![cand(1, bits(12)), cand(2, bits(9)), cand(3, bits(12) << 1)];
        assert_eq!(ids(&mixed, &similar_groups(&mixed, 4)), vec![vec![1, 3]]);
    }

    #[test]
    fn low_information_copies_stay_eligible_for_exact() {
        let flat = |id: i64, hash: &str| Candidate { content_hash: Some(hash.into()), ..cand(id, 0) };
        let c = vec![flat(1, "a"), flat(2, "a"), flat(3, "b")];
        assert_eq!(ids(&c, &groups(&c, DuplicateMode::Exact, 0)), vec![vec![1, 2]]);
        assert!(groups(&c, DuplicateMode::Similar, MAX_HAMMING as u32).is_empty());
        assert_eq!(redundant(&groups(&c, DuplicateMode::Exact, 0)), 1);
    }
}
