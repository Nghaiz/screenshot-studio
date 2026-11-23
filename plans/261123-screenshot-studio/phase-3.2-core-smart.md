# Phase 3.2: Core sidecar supervisor, AI jobs, semantic search, smart features (lane core)

Lane: core
Depends-on: 2.3
Steps: lane-decides
Effort: L
Owns: `crates/**`, `src-tauri/**` (except the frozen contract paths in README D2)
Creates: `crates/ss-core/tests/sidecar_client_test.rs`, `crates/ss-core/tests/smart_test.rs`, `crates/ss-core/tests/fake_sidecar.py`
Accept: `node scripts/accept/lane-tests.mjs crates/ss-core/tests/sidecar_client_test.rs crates/ss-core/tests/smart_test.rs`

## Goal

Rust side of P3: supervise the optional Python sidecar per the contract lifecycle, run resumable AI jobs that fill
`ocr_result`, `text_fts.ocr_text`, `embedding`, `category`, `dominant_rgb`; semantic search; and the sidecar-free smart
features (duplicates, sessions, blur/tiny, stats). The app must behave identically minus AI when the venv is absent.

## Scope

- `ss-core::sidecar`: process spawn (`tokio::process`, D6), NDJSON framing, id matching, max 2 in flight, timeouts and
  restart policy from `sidecar/config.rs` constants, `SidecarStatus` + `sidecar-status` events, `restart_sidecar`.
- AI job (`start_ai`): batches of 16/32 per contract, only images missing results for the current model, resumable,
  `job-progress` events; cloud-only files skipped unless downloaded.
- Semantic search: `embed.text` for the query, cosine over all `embedding` rows in memory (cache invalidated on writes),
  merged into `query_images` when `textMode` is `semantic` (sort `relevance`) or `any` (FTS first, then semantic, deduped;
  semantic arm only when `ready`, bounded by `SEMANTIC_QUERY_TIMEOUT_MS`, contract-sidecar § Degradation).
- Duplicates: exact by `content_hash`, similar by dHash Hamming <= `dupHamming` (BK-tree or sorted buckets); best =
  largest pixels, then newest, then sharpest. Sessions by `sessionGapMinutes`. Blur (Laplacian variance on the 320
  thumb, computed in the thumbnail pass if missing) and tiny. `get_stats`. `detect_pii` forwards OCR lines to `pii.detect`.
- `ssctl` rows of phase 3.2 in `design-intent.md` (`ai`, `dupes`, `status`).
- Zero-shot categories: labels list as data (`ss-core::smart::categories`: chat, code, game, browser, document, map,
  receipt, social, photo, settings, error dialog, meme), vi/en display names in the UI.

## Tests

`sidecar_client_test.rs` uses `fake_sidecar.py` (stdlib-only Python script speaking the contract; replies from
`contracts/fixtures/sidecar/`): ready path; absent venv -> `absent` and `start_ai` -> `sidecarUnavailable`; crash ->
restart with backoff then `error` after 3; timeout -> `$/cancel`; garbage on stdout -> error state, app keeps serving
queries. If no Python is on PATH the test FAILS with a message (it never silently skips). `smart_test.rs`: a planted
exact copy and a re-encoded near copy are grouped; sessions split at the gap; blurry fixture flagged; stats totals equal
row counts.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Supervisor deadlock on a full stdout pipe | medium | high | dedicated reader task per stream; bounded channels; test with a chatty fake |
| Semantic merge slows `query_images` | low | medium | embeddings cached in memory (~17 MB); timing printed in tests |
| Sidecar absent breaks a non-AI path | low | high | named before this phase: absent-venv test runs every command except AI ones |
