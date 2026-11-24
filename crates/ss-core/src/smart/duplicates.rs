//! Duplicate groups (works without the sidecar, Deferred decision #18).
//!
//! - `exact`: same blake3 `content_hash`.
//! - `similar`: near-identical copies (Deferred decision #35). Candidate stage: dHash (64-bit gradient
//!   hash) Hamming distance <= `hamming` (the user's slider) and an aspect ratio a copy could have.
//!   Confirm stage ([`near_copy::confirm`]): OCR text, embeddings or, without either, a stricter hash rule
//!   must agree that the pair is a copy and not just the same layout. Groups are seeded, not chained:
//!   images are taken in id order, and each not-yet-grouped image collects every not-yet-grouped candidate
//!   of ITSELF that the confirm stage accepts, so a chain of small differences never merges two unrelated
//!   screenshots. Low-information hashes ([`is_low_information`]) take no part: a near-flat image has
//!   almost no gradient bits, so any two of them are within `hamming` however different they are
//!   (Deferred decision #33). They stay eligible for `exact`.
//!
//! Population: present (`missing = 0`), not in the in-app trash, optionally one root. Best image of a
//! group: most pixels, then newest (`taken_ms`, then `mtime_ms`), then sharpest (`blur_score`), then
//! lowest id. `reason` names the first criterion that set the best apart from the runner-up (`largest`
//! when the group is a tie on all three, as exact copies usually are).
//!
//! [`groups`] is the one rule: `find_duplicates` (the duplicates view, its "keep the best" button, the
//! cleanup wizard's similar step) and `get_stats` (`duplicatesSimilar`) all read it.

use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::{Connection, params};

use super::near_copy::{self, Evidence};
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
    pub phash: Option<i64>,
    /// 0 while unknown (no thumbnail pass yet).
    pub width: i64,
    pub height: i64,
    pub taken_ms: i64,
    pub mtime_ms: i64,
    pub blur: Option<f64>,
}

impl Candidate {
    pub fn pixels(&self) -> i64 {
        self.width * self.height
    }
}

pub fn load(conn: &Connection, root_id: Option<i64>) -> Result<Vec<Candidate>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, content_hash, dhash, phash, COALESCE(width, 0), COALESCE(height, 0), taken_ms, mtime_ms, blur_score
         FROM image WHERE missing = 0 AND trashed_at IS NULL AND (?1 IS NULL OR root_id = ?1) ORDER BY id",
    )?;
    let rows = stmt.query_map(params![root_id], |r| {
        Ok(Candidate {
            id: r.get(0)?,
            content_hash: r.get(1)?,
            dhash: r.get(2)?,
            phash: r.get(3)?,
            width: r.get(4)?,
            height: r.get(5)?,
            taken_ms: r.get(6)?,
            mtime_ms: r.get(7)?,
            blur: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Index lists (into `c`) of the groups of `mode`, each with at least two members. `similar` reads the
/// confirm stage's evidence of its candidate pairs from `conn`.
pub fn groups(conn: &Connection, c: &[Candidate], mode: DuplicateMode, hamming: u32) -> Result<Vec<Vec<usize>>, AppError> {
    match mode {
        DuplicateMode::Exact => Ok(exact_groups(c)),
        DuplicateMode::Similar => similar_groups(c, hamming, |ids| near_copy::load(conn, ids)),
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

/// Candidate stage: for each image, the other images (ascending index) within `hamming` dHash bits whose
/// shape fits a copy. A flat scan of all 15.7M pairs of the real library costs the same at any radius;
/// the BK-tree it replaced (phase 3.4c) needed ~73 ms at radius 6, one query per seed, against ~19 ms at
/// radius 20, where the first seeds took hundreds of images and left few to query. The whole `similar`
/// call now takes ~20 ms at 6 (ssctl, real library, process start subtracted).
fn candidates(c: &[Candidate], hamming: u32) -> Vec<Vec<usize>> {
    let (idx, hashes): (Vec<usize>, Vec<u64>) =
        c.iter().enumerate().filter_map(|(i, cand)| informative(cand).map(|h| (i, h as u64))).unzip();
    let mut near = vec![Vec::new(); c.len()];
    let mut dist: Vec<u8> = Vec::with_capacity(hashes.len());
    for (a, &ha) in hashes.iter().enumerate() {
        dist.clear();
        // Distances first, as bytes in a tight loop (no branch), then the rare hits.
        dist.extend(hashes[a + 1..].iter().map(|&hb| (ha ^ hb).count_ones() as u8));
        for (off, &d) in dist.iter().enumerate() {
            let (i, j) = (idx[a], idx[a + 1 + off]);
            if u32::from(d) <= hamming && near_copy::shape_fits(&c[i], &c[j]) {
                near[i].push(j);
                near[j].push(i);
            }
        }
    }
    near
}

/// Seeded groups over the candidate pairs that the confirm stage accepts. `evidence` is asked once, for
/// the ids of every image in a candidate pair.
fn similar_groups(
    c: &[Candidate],
    hamming: u32,
    evidence: impl FnOnce(&[i64]) -> Result<HashMap<i64, Evidence>, AppError>,
) -> Result<Vec<Vec<usize>>, AppError> {
    let near = candidates(c, hamming);
    let ids: Vec<i64> = near.iter().zip(c).filter(|(n, _)| !n.is_empty()).map(|(_, cand)| cand.id).collect();
    let ev = evidence(&ids)?;
    let none = Evidence::default();
    let ev_of = |i: usize| ev.get(&c[i].id).unwrap_or(&none);
    let mut taken = vec![false; c.len()];
    let mut out = Vec::new();
    for i in 0..c.len() {
        if taken[i] || near[i].is_empty() {
            continue;
        }
        let mut group: Vec<usize> =
            near[i].iter().copied().filter(|&j| !taken[j] && near_copy::confirm(&c[i], ev_of(i), &c[j], ev_of(j))).collect();
        if group.is_empty() {
            continue;
        }
        group.push(i);
        group.sort_unstable();
        for &j in &group {
            taken[j] = true;
        }
        out.push(group);
    }
    Ok(out)
}

/// Ranks best first (see the module doc).
fn rank(a: &Candidate, b: &Candidate) -> Ordering {
    b.pixels()
        .cmp(&a.pixels())
        .then((b.taken_ms, b.mtime_ms).cmp(&(a.taken_ms, a.mtime_ms)))
        .then(b.blur.unwrap_or(f64::MIN).total_cmp(&a.blur.unwrap_or(f64::MIN)))
        .then(a.id.cmp(&b.id))
}

/// Best member and why it won.
pub fn best_of(c: &[Candidate], group: &[usize]) -> (usize, DuplicateReason) {
    let mut sorted = group.to_vec();
    sorted.sort_by(|&a, &b| rank(&c[a], &c[b]));
    let (best, second) = (&c[sorted[0]], &c[sorted[1]]);
    let reason = if best.pixels() != second.pixels() {
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
pub fn find(conn: &Connection, c: &[Candidate], mode: DuplicateMode, hamming: u32) -> Result<DuplicateResult, AppError> {
    let mut groups: Vec<(usize, DuplicateReason, Vec<usize>)> = groups(conn, c, mode, hamming)?
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
    Ok(DuplicateResult { groups })
}

/// Images that are redundant copies: the sum over groups of (size - 1).
pub fn redundant(groups: &[Vec<usize>]) -> i64 {
    groups.iter().map(|g| g.len() as i64 - 1).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart::near_copy::Embedding;

    fn cand(id: i64, dhash: i64) -> Candidate {
        Candidate {
            id,
            content_hash: None,
            dhash: Some(dhash),
            phash: Some(0),
            width: 100,
            height: 100,
            taken_ms: id,
            mtime_ms: 0,
            blur: None,
        }
    }

    /// Evidence that agrees for every image (one embedding for all, no OCR): only the candidate stage
    /// decides.
    fn agreeing(ids: &[i64]) -> Result<HashMap<i64, Evidence>, AppError> {
        Ok(ids.iter().map(|&id| (id, Evidence { text: None, embedding: Some(Embedding::new("m", vec![1.0, 0.0])) })).collect())
    }

    fn similar(c: &[Candidate], hamming: u32) -> Vec<Vec<i64>> {
        ids(c, &similar_groups(c, hamming, agreeing).unwrap())
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
    fn candidate_stage_matches_brute_force_and_groups_do_not_chain() {
        let c: Vec<Candidate> =
            (0..300).map(|i: i64| cand(i, i.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64) ^ (i << 7))).collect();
        for max in [0, 3, 12, 30, 64] {
            let near = candidates(&c, max);
            for (i, a) in c.iter().enumerate().step_by(7) {
                let want: Vec<usize> = (0..c.len())
                    .filter(|&j| {
                        j != i && [a, &c[j]].iter().all(|x| informative(x).is_some()) && ((a.dhash.unwrap() ^ c[j].dhash.unwrap()) as u64).count_ones() <= max
                    })
                    .collect();
                assert_eq!(near[i], want, "item {i} max {max}");
            }
        }
        // a - b - c each 4 bits apart, a - c 8 bits: with hamming 4 the seed a takes b only.
        let chain = vec![cand(1, BALANCED), cand(2, BALANCED ^ 0xF), cand(3, BALANCED ^ 0xFF)];
        assert_eq!(similar(&chain, 4), vec![vec![1, 2]]);
    }

    #[test]
    fn the_candidate_stage_needs_a_shape_a_copy_could_have() {
        let sized = |id: i64, w: i64, h: i64| Candidate { width: w, height: h, ..cand(id, BALANCED) };
        // A 50% resize (rounded) fits; a crop to another aspect does not, nor does an unknown size.
        let c = vec![sized(1, 1830, 280), sized(2, 915, 140), sized(3, 1830, 300), sized(4, 0, 0)];
        assert_eq!(similar(&c, 6), vec![vec![1, 2]]);
        assert!(candidates(&c, 6)[2].is_empty() && candidates(&c, 6)[3].is_empty());
    }

    #[test]
    fn evidence_is_asked_once_for_the_candidate_images_only() {
        let c = vec![cand(1, BALANCED), cand(2, !BALANCED), cand(3, BALANCED ^ 1), cand(4, 0)];
        let mut asked = Vec::new();
        let got = similar_groups(&c, 6, |ids| {
            asked.push(ids.to_vec());
            agreeing(ids)
        })
        .unwrap();
        assert_eq!(asked, vec![vec![1, 3]], "only the pair within range; the far and the flat image are not read");
        assert_eq!(ids(&c, &got), vec![vec![1, 3]]);
        // The confirm stage can refuse every candidate: no evidence + hashes 1 bit apart but pHash far apart.
        let far = vec![cand(1, BALANCED), Candidate { phash: Some(0xFF), ..cand(2, BALANCED ^ 1) }];
        assert!(similar_groups(&far, 6, |_| Ok(HashMap::new())).unwrap().is_empty());
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
        assert_eq!(similar(&c, 6), vec![vec![8, 9], vec![10, 11], vec![12, 13]], "low-information hashes (ids 1-7) are in no group");
        // With every pair in range (Hamming 64) only the informative ones gather.
        assert_eq!(similar(&c, 64), vec![vec![8, 9, 10, 11, 12, 13]]);
        // A low-information image within range of an informative one does not join it either.
        let mixed = vec![cand(1, bits(12)), cand(2, bits(9)), cand(3, bits(12) << 1)];
        assert_eq!(similar(&mixed, 4), vec![vec![1, 3]]);
    }

    #[test]
    fn low_information_copies_stay_eligible_for_exact() {
        let flat = |id: i64, hash: &str| Candidate { content_hash: Some(hash.into()), ..cand(id, 0) };
        let c = vec![flat(1, "a"), flat(2, "a"), flat(3, "b")];
        assert_eq!(ids(&c, &exact_groups(&c)), vec![vec![1, 2]]);
        assert!(similar_groups(&c, MAX_HAMMING as u32, agreeing).unwrap().is_empty());
        assert_eq!(redundant(&exact_groups(&c)), 1);
    }
}
