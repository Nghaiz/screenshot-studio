//! Every sidecar tunable (contract-sidecar-jsonrpc.md § Lifecycle rule 5: constants live here, never
//! inlined elsewhere). [`SidecarTimings`] carries the time-based ones so tests can shrink them; the
//! production values are its `Default`.

use std::time::Duration;

/// `initialize` must answer within this (model load; the real sidecar needs ~11-14 s on CUDA).
pub const SIDECAR_INIT_TIMEOUT_S: u64 = 120;
/// Per-request timeout; on expiry Rust sends `$/cancel`.
pub const SIDECAR_REQUEST_TIMEOUT_S: u64 = 300;
/// After a `$/cancel`, the sidecar has this long to answer (-32004) before it is killed and restarted.
pub const SIDECAR_CANCEL_GRACE_S: u64 = 10;
/// App exit: after `shutdown`, the process has this long to exit before it is killed.
pub const SIDECAR_SHUTDOWN_WAIT_S: u64 = 3;
/// Restart delays after a crash, by consecutive crash (the last one repeats).
pub const SIDECAR_RESTART_BACKOFF_S: [u64; 3] = [1, 5, 30];
/// This many crashes inside [`SIDECAR_CRASH_WINDOW_S`] -> state `error` until `restart_sidecar`.
pub const SIDECAR_CRASH_LIMIT: usize = 3;
pub const SIDECAR_CRASH_WINDOW_S: u64 = 600;
/// Outstanding requests Rust keeps at most (contract § Transport; the sidecar runs 2 workers).
pub const SIDECAR_MAX_IN_FLIGHT: usize = 2;
/// After stdout closes, how long the process gets to report its exit code before it is killed.
pub const SIDECAR_EXIT_GRACE_MS: u64 = 2_000;

/// `textMode: "any"` waits at most this long for `embed.text` (contract § Degradation).
pub const SEMANTIC_QUERY_TIMEOUT_MS: u64 = 1_500;
/// `textMode: "semantic"` (the user asked for it explicitly) waits at most this long.
pub const SEMANTIC_ONLY_TIMEOUT_MS: u64 = 15_000;
/// `detect_pii` (one image, OCR inside the sidecar when no `ocr_result` row exists).
pub const DETECT_PII_TIMEOUT_S: u64 = 60;

/// `textMode: "semantic"`: the most similar images returned, best first.
pub const SEMANTIC_TOP_K: usize = 200;
/// `textMode: "any"`: images the semantic arm may add after the FTS hits.
pub const SEMANTIC_ANY_TOP_K: usize = 40;
/// `textMode: "any"`: a semantic-only hit needs at least this cosine. Basis (phase 3.1, SigLIP i18n on
/// generated images): "cat" vs a drawn cat 0.102, vs a text screenshot -0.015; "con mèo" 0.083 vs 0.016.
pub const SEMANTIC_ANY_MIN_SCORE: f32 = 0.05;
/// Query-text embeddings kept in memory (paging re-runs the same query once per page).
pub const SEMANTIC_TEXT_CACHE: usize = 32;

/// `sidecarVenv: null` means this venv (contract-tauri-ipc.md § Setting keys).
pub const DEFAULT_SIDECAR_VENV: &str = r"D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv";
/// Model cache handed to the sidecar when the app's environment has no `HF_HOME` (design-intent.md).
pub const DEFAULT_HF_HOME: &str = r"D:\DevCache\huggingface";
/// Embedding model asked for in `initialize` (README Deferred decision #19).
pub const EMBED_MODEL: &str = "ViT-B-16-SigLIP-i18n-256";
/// OCR languages asked for in `initialize`.
pub const LANGUAGES: [&str; 2] = ["vi", "en"];

/// AI job batch sizes (contract maxima: `ocr.recognize` 16, `embed.images` 32).
pub const AI_BATCH_OCR: usize = crate::contract::sidecar::OCR_MAX_ITEMS;
pub const AI_BATCH_EMBED: usize = crate::contract::sidecar::EMBED_IMAGES_MAX_ITEMS;
pub const AI_BATCH_CLASSIFY: usize = 16;
pub const AI_BATCH_COLOR: usize = 32;
/// Zero-shot: the best label is always kept; other labels only at or above this softmax score.
pub const CATEGORY_KEEP_MIN_SCORE: f64 = 0.2;

/// A stdout line longer than this is a protocol violation (the largest real line, an `embed.images`
/// result of 32 x 768 floats, is ~140 KB).
pub const MAX_LINE_BYTES: usize = 32 * 1024 * 1024;
/// Last stderr lines kept for error details.
pub const STDERR_TAIL_LINES: usize = 20;
/// A longer stderr line is cut to this many bytes in the log and the tail.
pub const STDERR_LINE_MAX: usize = 4_096;

/// The time-based tunables. `Default` = the constants above; tests pass shorter ones.
#[derive(Debug, Clone)]
pub struct SidecarTimings {
    pub init_timeout: Duration,
    pub request_timeout: Duration,
    pub cancel_grace: Duration,
    pub shutdown_wait: Duration,
    pub restart_backoff: Vec<Duration>,
    pub crash_limit: usize,
    pub crash_window: Duration,
    pub exit_grace: Duration,
}

impl Default for SidecarTimings {
    fn default() -> Self {
        Self {
            init_timeout: Duration::from_secs(SIDECAR_INIT_TIMEOUT_S),
            request_timeout: Duration::from_secs(SIDECAR_REQUEST_TIMEOUT_S),
            cancel_grace: Duration::from_secs(SIDECAR_CANCEL_GRACE_S),
            shutdown_wait: Duration::from_secs(SIDECAR_SHUTDOWN_WAIT_S),
            restart_backoff: SIDECAR_RESTART_BACKOFF_S.iter().map(|s| Duration::from_secs(*s)).collect(),
            crash_limit: SIDECAR_CRASH_LIMIT,
            crash_window: Duration::from_secs(SIDECAR_CRASH_WINDOW_S),
            exit_grace: Duration::from_millis(SIDECAR_EXIT_GRACE_MS),
        }
    }
}

impl SidecarTimings {
    /// Delay before restart number `consecutive` (1-based) after a crash.
    pub fn backoff(&self, consecutive: usize) -> Duration {
        let last = self.restart_backoff.len().saturating_sub(1);
        self.restart_backoff.get(consecutive.saturating_sub(1).min(last)).copied().unwrap_or(Duration::ZERO)
    }

    /// How long a caller may have to wait for `ready` after a crash: one init plus the longest backoff.
    pub fn recovery_budget(&self) -> Duration {
        self.init_timeout + self.restart_backoff.iter().copied().max().unwrap_or(Duration::ZERO) + self.exit_grace
    }
}
