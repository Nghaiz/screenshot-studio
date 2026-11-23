//! Zero-shot category labels (data). The sidecar scores each image against every label
//! (`classify.zeroShot`, prompt template in `sidecar/ss_sidecar/config.py`); the stored `category.label`
//! is the label string itself. Display names (vi / en) belong to the UI's i18n dictionaries.

/// Every label, in display order.
pub const LABELS: [&str; 12] =
    ["chat", "code", "game", "browser", "document", "map", "receipt", "social", "photo", "settings", "error dialog", "meme"];

/// [`LABELS`] as owned strings (the `classify.zeroShot` params).
pub fn labels() -> Vec<String> {
    LABELS.iter().map(|s| (*s).to_owned()).collect()
}
