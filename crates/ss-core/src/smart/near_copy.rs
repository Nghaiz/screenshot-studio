//! The cheap checks of `similar` duplicates (README Deferred decision #35). `similar` means near-identical
//! copies: a re-encode or a resize of one picture. It does not mean "same layout": a 64-bit dHash of a
//! mostly white screenshot cannot tell apart two images that share one (one diagram layout with other
//! labels, one app's different dialogs), so the dHash only proposes candidate pairs. This module drops the
//! candidates that the evidence the catalog already holds rules out, so that only the survivors have their
//! thumbnails decoded by the final arbiter, [`super::pixel_check`]. Thresholds: README Re-score phase 3.4c
//! (real library and synthetic copies of 20 local screenshots, counts only).
//!
//! A candidate pair is [`plausible`] when every check that applies passes:
//! 1. Shape, always ([`shape_fits`]): the aspect ratios agree within the rounding of a resize.
//! 2. Text, when both images have an OCR result and either has text: [`text_similarity`] >= [`TEXT_MIN`].
//! 3. Image features, when both have an embedding of the same model and size: cosine >= [`COSINE_MIN`].
//! 4. Neither 2 nor 3 applied (AI absent, or no text and no embedding): [`hash_only_fits`], both dHash and
//!    pHash within [`HASH_ONLY_MAX`] bits.

use std::cell::OnceCell;
use std::collections::HashMap;

use rusqlite::Connection;

use super::duplicates::Candidate;
use crate::contract::AppError;
use crate::sidecar::vector;

/// Aspect-ratio slack of [`shape_fits`], in pixels of the smaller image: a resize rounds each side to a
/// whole pixel. Synthetic 75% / 50% resizes reached 0.27% / 0.80% aspect difference, all inside the
/// bound; of the 875 real candidate pairs within Hamming 6, 59 fit (154 were within a flat 1%).
pub const SHAPE_SLACK_PX: f64 = 1.0;
/// Least [`text_similarity`] of a copy. Synthetic copies at or above it (where the text check applies):
/// cursor 18/20, JPEG 17/19, 75% resize 16/20, 50% resize 13/20, scroll 18/19, notification 12/20; 4
/// labelled real copies 0.961-0.996. 11 labelled real pairs of one window whose content changed
/// reached at most 0.847.
pub const TEXT_MIN: f64 = 0.90;
/// Least embedding cosine of a copy. Synthetic: every JPEG (min 0.973) and 75% resize (min 0.989) at or
/// above it, 50% resize 17/20, cursor 19/20, scroll 18/20, notification 15/20. Two labelled real pairs of
/// one template with different content (two exercises' statements, one dialog for two logins) passed the
/// text check at 0.923 and 0.966; their cosines were 0.955 and 0.908.
pub const COSINE_MIN: f32 = 0.96;
/// Most dHash and most pHash bits a copy may differ by when no OCR text or embedding can rule it out. It
/// bounds how many thumbnails the pixel stage decodes without AI evidence, whatever the slider says.
/// Synthetic re-encodes and resizes within it: JPEG q85 17/20, q70 14/20, 75% 17/20, 50% 17/20, 33% 14/20.
pub const HASH_ONLY_MAX: u32 = 2;

/// What the catalog knows about one image besides its row: the evidence of the cheap checks.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// None without an `ocr_result` row.
    pub text: Option<OcrText>,
    /// None without an `embedding` row (or with a malformed one).
    pub embedding: Option<Embedding>,
}

#[derive(Debug, Clone)]
pub struct Embedding {
    pub model: String,
    unit: Vec<f32>,
}

impl Embedding {
    /// Normalizes `v` to unit length (a zero vector stays zero, so it matches nothing).
    pub fn new(model: impl Into<String>, mut v: Vec<f32>) -> Self {
        vector::normalize(&mut v);
        Embedding { model: model.into(), unit: v }
    }

    /// Cosine with `other`, or None when the two cannot be compared (other model or size).
    pub fn cosine(&self, other: &Embedding) -> Option<f32> {
        (self.model == other.model && self.unit.len() == other.unit.len()).then(|| vector::dot(&self.unit, &other.unit))
    }
}

/// An image's OCR text; its [`TextProfile`] is built on first use, so only pairs that pass the cheaper
/// checks pay for it (at the slider's maximum, 2,321 shape-fit pairs of the real library).
#[derive(Debug, Clone)]
pub struct OcrText {
    raw: String,
    profile: OnceCell<TextProfile>,
}

impl OcrText {
    pub fn new(raw: impl Into<String>) -> Self {
        OcrText { raw: raw.into(), profile: OnceCell::new() }
    }

    pub fn profile(&self) -> &TextProfile {
        self.profile.get_or_init(|| TextProfile::new(&self.raw))
    }
}

/// OCR text as compared: lower-case words (letters, digits, `_`) with diacritics folded ([`fold`]),
/// joined by one space, and the multiset of its character trigrams (each packed into a u64, sorted).
#[derive(Debug, Clone)]
pub struct TextProfile {
    key: String,
    grams: Vec<u64>,
}

impl TextProfile {
    pub fn new(text: &str) -> Self {
        let mut key = String::with_capacity(text.len());
        let mut in_word = false;
        for c in text.chars().flat_map(char::to_lowercase).filter_map(fold) {
            if c.is_alphanumeric() || c == '_' {
                if !in_word && !key.is_empty() {
                    key.push(' ');
                }
                key.push(c);
                in_word = true;
            } else {
                in_word = false;
            }
        }
        let chars: Vec<u64> = key.chars().map(u64::from).collect();
        // A char is at most 21 bits, so three fit in one u64.
        let mut grams: Vec<u64> = chars.windows(3).map(|w| w[0] << 42 | w[1] << 21 | w[2]).collect();
        grams.sort_unstable();
        TextProfile { key, grams }
    }

    /// No word at all (the OCR found no text).
    pub fn is_empty(&self) -> bool {
        self.key.is_empty()
    }
}

/// Dice coefficient of the two trigram multisets (1.0 for the same words, including short ones).
pub fn text_similarity(a: &TextProfile, b: &TextProfile) -> f64 {
    if a.key == b.key {
        return 1.0;
    }
    let total = a.grams.len() + b.grams.len();
    if total == 0 {
        return 0.0;
    }
    // Multiset intersection of two sorted lists.
    let (mut i, mut j, mut common) = (0, 0, 0usize);
    while i < a.grams.len() && j < b.grams.len() {
        match a.grams[i].cmp(&b.grams[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
        }
    }
    2.0 * common as f64 / total as f64
}

/// A lower-case letter without its diacritics; None for a combining mark (U+0300-U+036F). Covers the
/// Vietnamese letters (OCR misreads their tone marks first in a downscaled copy) and Latin-1.
fn fold(c: char) -> Option<char> {
    Some(match c {
        '\u{0300}'..='\u{036F}' => return None,
        'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ' | 'ấ' | 'ẩ' | 'ẫ' | 'ậ' | 'ä' | 'å' => 'a',
        'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' | 'ë' => 'e',
        'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ' | 'ớ' | 'ở' | 'ỡ' | 'ợ' | 'ö' => 'o',
        'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' | 'û' | 'ü' => 'u',
        'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' | 'ÿ' => 'y',
        'đ' => 'd',
        'ç' => 'c',
        'ñ' => 'n',
        other => other,
    })
}

/// The aspect ratios agree within [`SHAPE_SLACK_PX`] of rounding on each side of the smaller image (a
/// re-encode or a same-size change has the same size; a resize keeps the aspect up to that rounding).
pub fn shape_fits(a: &Candidate, b: &Candidate) -> bool {
    let (small, large) = if a.pixels() <= b.pixels() { (a, b) } else { (b, a) };
    if small.width <= 0 || small.height <= 0 || large.width <= 0 || large.height <= 0 {
        return false;
    }
    let ratio = |c: &Candidate| c.width as f64 / c.height as f64;
    let (rs, rl) = (ratio(small), ratio(large));
    (rs - rl).abs() / rs.max(rl) <= SHAPE_SLACK_PX * (1.0 / small.width as f64 + 1.0 / small.height as f64)
}

/// The rule when neither text nor embeddings apply: both hashes within [`HASH_ONLY_MAX`] bits.
pub fn hash_only_fits(a: &Candidate, b: &Candidate) -> bool {
    let close = |x: Option<i64>, y: Option<i64>| matches!((x, y), (Some(x), Some(y)) if (x ^ y).count_ones() <= HASH_ONLY_MAX);
    close(a.dhash, b.dhash) && close(a.phash, b.phash)
}

/// True when the evidence of `a` and `b` (a dHash candidate pair) does not rule out a copy (the module
/// doc's checks). Only the pixel stage can confirm one.
pub fn plausible(a: &Candidate, ea: &Evidence, b: &Candidate, eb: &Evidence) -> bool {
    if !shape_fits(a, b) {
        return false;
    }
    // Cheapest first: the result does not depend on the order, only the cost does.
    let mut checked = false;
    if let (Some(xa), Some(xb)) = (&ea.embedding, &eb.embedding)
        && let Some(cos) = xa.cosine(xb)
    {
        if !(cos >= COSINE_MIN) {
            return false; // NaN fails too
        }
        checked = true;
    }
    if let (Some(ta), Some(tb)) = (&ea.text, &eb.text) {
        let (pa, pb) = (ta.profile(), tb.profile());
        if !(pa.is_empty() && pb.is_empty()) {
            if text_similarity(pa, pb) < TEXT_MIN {
                return false;
            }
            checked = true;
        }
    }
    checked || hash_only_fits(a, b)
}

/// `ids` as a JSON array, for `json_each`.
pub(super) fn id_list(ids: &[i64]) -> String {
    format!("[{}]", ids.iter().map(i64::to_string).collect::<Vec<_>>().join(","))
}

/// Evidence of `ids` (images without OCR or embedding rows are absent from the map).
pub fn load(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, Evidence>, AppError> {
    let mut out: HashMap<i64, Evidence> = HashMap::with_capacity(ids.len());
    if ids.is_empty() {
        return Ok(out);
    }
    let list = id_list(ids);
    let mut st = conn.prepare_cached("SELECT image_id, text FROM ocr_result WHERE image_id IN (SELECT value FROM json_each(?1))")?;
    for row in st.query_map([&list], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, text) = row?;
        out.entry(id).or_default().text = Some(OcrText::new(text));
    }
    let mut st =
        conn.prepare_cached("SELECT image_id, model, dim, vec FROM embedding WHERE image_id IN (SELECT value FROM json_each(?1))")?;
    for row in st.query_map([&list], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Vec<u8>>(3)?)))? {
        let (id, model, dim, blob) = row?;
        // A blob that does not hold `dim` floats is no evidence (the semantic index skips it the same way).
        if let Some(v) = usize::try_from(dim).ok().and_then(|d| vector::floats_from_le(&blob, d)) {
            out.entry(id).or_default().embedding = Some(Embedding::new(model, v));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32 set bits: an ordinary screenshot's dHash.
    const HASH: i64 = 0x0F0F_0F0F_0F0F_0F0F;

    fn img(id: i64, w: i64, h: i64) -> Candidate {
        Candidate { id, content_hash: None, dhash: Some(HASH), phash: Some(HASH), width: w, height: h, taken_ms: 0, mtime_ms: 0, blur: None }
    }
    fn ev(text: Option<&str>, vec: Option<&[f32]>) -> Evidence {
        Evidence { text: text.map(OcrText::new), embedding: vec.map(|v| Embedding::new("siglip", v.to_vec())) }
    }
    fn sim(a: &str, b: &str) -> f64 {
        text_similarity(&TextProfile::new(a), &TextProfile::new(b))
    }

    // Generated texts (no owner data). The ER pair is one box-diamond-box drawing with other names; the copy
    // is an invoice read twice by OCR (tone marks lost, `0` read as `O`); the terminal gained output lines.
    const ER_A: &str = "Student\ntakes\nCourse";
    const ER_B: &str = "Author\nwrites\nBook";
    const INVOICE: &str = "Hoá đơn tháng 11\nTổng cộng: 2.450.000 đ\nMã khách hàng KH-0042\nHạn thanh toán 30/11/2026";
    const INVOICE_REREAD: &str = "Hoa đơn thang 11\nTổng công: 2.450.000 d\nMa khách hảng KH-O042\nHạn thanh toán 30/11/2026";
    const TERM_A: &str = "PS D:/work> cargo build\nCompiling ss-core v0.1.0\nFinished release in 42s\nPS D:/work> cargo test\nrunning 12 tests";
    const TERM_B: &str = "Compiling ss-core v0.1.0\nFinished release in 42s\nPS D:/work> cargo test\nrunning 12 tests\ntest result: ok. 12 passed\nPS D:/work> git status";
    /// Two embeddings at cosine ~0.995 (one layout) and one at ~0.90 from the first.
    const V1: [f32; 3] = [1.0, 0.10, 0.0];
    const V1_NEAR: [f32; 3] = [1.0, 0.0, 0.0];
    const V1_FAR: [f32; 3] = [1.0, 0.60, 0.0];

    #[test]
    fn a_same_layout_pair_with_different_text_is_not_a_copy() {
        // Same size, same hashes, embeddings that see one layout: only the text tells them apart.
        let (a, b) = (img(1, 1830, 280), img(2, 1830, 280));
        assert!(sim(ER_A, ER_B) < 0.1, "{}", sim(ER_A, ER_B));
        assert!(!plausible(&a, &ev(Some(ER_A), Some(&V1)), &b, &ev(Some(ER_B), Some(&V1_NEAR))));
        assert!(!plausible(&a, &ev(Some(ER_A), None), &b, &ev(Some(ER_B), None)), "text alone refuses it too");
        // One window whose content changed is not a copy either, however close its embedding.
        assert!(sim(TERM_A, TERM_B) < TEXT_MIN, "{}", sim(TERM_A, TERM_B));
        assert!(!plausible(&img(3, 2560, 1600), &ev(Some(TERM_A), Some(&V1)), &img(4, 2560, 1600), &ev(Some(TERM_B), Some(&V1))));
    }

    #[test]
    fn a_reencoded_or_resized_copy_is_a_copy() {
        assert!(sim(INVOICE, INVOICE_REREAD) >= TEXT_MIN, "{}", sim(INVOICE, INVOICE_REREAD));
        let (orig, e1) = (img(1, 1830, 280), ev(Some(INVOICE), Some(&V1)));
        let e2 = ev(Some(INVOICE_REREAD), Some(&V1_NEAR));
        assert!(plausible(&orig, &e1, &img(2, 1830, 280), &e2), "re-encode: same size");
        assert!(plausible(&orig, &e1, &img(3, 915, 140), &e2), "50% resize");
        assert!(plausible(&img(3, 915, 140), &e2, &orig, &e1), "symmetric");
        // The same ER drawing captured twice (1 px apart) with the same names passes: only pixels can tell.
        assert!(plausible(&img(4, 1833, 287), &ev(Some(ER_A), Some(&V1)), &img(5, 1834, 287), &ev(Some(ER_A), Some(&V1_NEAR))));
    }

    #[test]
    fn every_applicable_check_must_agree() {
        let (a, b) = (img(1, 800, 600), img(2, 800, 600));
        let text = |t: &str, v: &[f32]| ev(Some(t), Some(v));
        assert!(!plausible(&a, &text(INVOICE, &V1), &b, &text(INVOICE, &V1_FAR)), "same text, the embedding disagrees");
        assert!(!plausible(&a, &ev(Some(INVOICE), Some(&V1)), &b, &ev(Some(""), Some(&V1))), "text on one side only");
        assert!(!plausible(&a, &ev(Some(INVOICE), Some(&V1)), &b, &ev(Some(INVOICE), Some(&[0.0, 0.0, 0.0]))), "a zero vector matches nothing");
        // No text on either side (OCR found none): the embedding decides.
        assert!(plausible(&a, &ev(Some(""), Some(&V1)), &b, &ev(Some(" · "), Some(&V1_NEAR))));
        assert!(!plausible(&a, &ev(Some(""), Some(&V1)), &b, &ev(Some(""), Some(&V1_FAR))));
        // Embeddings of different models (or sizes) are not compared; the text still is.
        let other = Evidence { text: Some(OcrText::new(INVOICE)), embedding: Some(Embedding::new("clip", V1_FAR.to_vec())) };
        assert!(plausible(&a, &text(INVOICE, &V1), &b, &other));
        let short = Evidence { text: Some(OcrText::new(INVOICE)), embedding: Some(Embedding::new("siglip", vec![1.0])) };
        assert!(plausible(&a, &text(INVOICE, &V1), &b, &short));
        // The shape check always applies.
        assert!(!plausible(&a, &text(INVOICE, &V1), &img(2, 800, 500), &text(INVOICE, &V1)));
    }

    #[test]
    fn without_text_or_embeddings_both_hashes_must_be_close() {
        let none = Evidence::default();
        let with = |id, w, h, d: i64, p: i64| Candidate { dhash: Some(HASH ^ d), phash: Some(HASH ^ p), ..img(id, w, h) };
        let a = img(1, 800, 600);
        assert!(plausible(&a, &none, &with(2, 800, 600, 0b11, 0b11), &none), "2 + 2 bits, same size");
        assert!(plausible(&a, &none, &with(2, 400, 300, 0b1, 0b1), &none), "a resize: the pixel stage decides");
        assert!(!plausible(&a, &none, &with(2, 800, 600, 0b111, 0), &none), "3 dHash bits");
        assert!(!plausible(&a, &none, &with(2, 800, 600, 0, 0b111), &none), "3 pHash bits");
        assert!(!plausible(&a, &none, &Candidate { phash: None, ..img(2, 800, 600) }, &none), "no pHash, no proof");
        // Evidence on one side only is no evidence: the hash rule decides.
        let one = ev(Some(INVOICE), Some(&V1));
        assert!(plausible(&a, &one, &with(2, 800, 600, 1, 1), &none));
        assert!(!plausible(&a, &one, &with(2, 400, 300, 0b111, 0), &none));
    }

    #[test]
    fn shape_allows_the_rounding_of_a_resize_and_nothing_more() {
        let fits = |a: (i64, i64), b: (i64, i64)| shape_fits(&img(1, a.0, a.1), &img(2, b.0, b.1));
        assert!(fits((1830, 280), (1830, 280)));
        assert!(fits((1831, 283), (916, 142)) && fits((1831, 283), (915, 141)), "50% resize rounded either way");
        assert!(fits((1833, 287), (1834, 287)), "1 px");
        assert!(!fits((1826, 291), (1790, 291)), "a re-crop 36 px narrower");
        assert!(!fits((2560, 1600), (1920, 1080)), "16:10 vs 16:9");
        assert!(!fits((0, 0), (0, 0)) && !fits((100, 0), (100, 0)), "unknown size");
        assert_eq!(fits((640, 400), (1280, 801)), fits((1280, 801), (640, 400)));
    }

    #[test]
    fn text_is_compared_as_folded_words() {
        assert_eq!(sim("Hoá đơn 2026 INVOICE-7731", "HOA DON 2026 invoice 7731"), 1.0, "case, tone marks, đ, punctuation");
        assert_eq!(sim("Ho\u{0301}a", "hoa"), 1.0, "a decomposed tone mark folds too");
        assert_eq!(sim("ab", "ab"), 1.0, "short texts compare equal");
        assert_eq!(sim("ab", "cd"), 0.0);
        assert_eq!(sim("", INVOICE), 0.0);
        assert!(TextProfile::new(" \n·— ").is_empty() && !TextProfile::new("x").is_empty());
        let s = sim("Customer places Order", "Customer places Order 1 N");
        assert!(s > 0.9 && s < 1.0, "{s}");
    }
}
