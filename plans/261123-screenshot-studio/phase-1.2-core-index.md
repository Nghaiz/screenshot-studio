# Phase 1.2: Core indexer, catalog, thumbnails, `ssimg`, `ssctl` (lane core)

Lane: core
Depends-on: 1.1
Steps: lane-decides
Effort: L
Owns: `crates/**`, `src-tauri/**` (except the frozen contract paths in README D2)
Creates: `crates/ss-core/tests/index_test.rs`, `crates/ss-core/tests/common/fixtures.rs`
AX: AX-01 `ssctl add-root <dir> --json`, `ssctl index <rootId> --wait --json`, `ssctl list --json`, `ssctl stats --json` print one JSON document (contract types per design-intent) on stdout, logs on stderr, exit 0 ok / 1 error / 2 usage; `ssctl --help` lists them
Accept: `node scripts/accept/lane-tests.mjs crates/ss-core/tests/index_test.rs`

## Goal

Rust side of P1: given a library root, scan it, store every PNG (and jpg/jpeg/webp/gif/bmp) in the SQLite catalog,
generate WebP thumbnails in the background, serve them via `ssimg`, and implement the P1 commands of
`contract-tauri-ipc.md` (column "Phase" = 1.2). The UI lane is developing against mocks at the same time.

## Scope (lane decides the step order)

- `ss-core::paths` (data dir constant, `SS_DATA_DIR` override), `ss-core::catalog` (open/migrate DB, WAL, one writer
  thread + pooled readers; FTS rows for file_name kept in sync), `ss-core::scan` (walkdir, extension filter, upsert by
  `(root, folder, file_name)`, mark `missing`, `taken_ms` from filename patterns like `Screenshot 2026-11-23 095012.png`
  / `Screenshot (123).png` > created > mtime).
- Cloud state: Windows file attributes `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (0x400000) / `FILE_ATTRIBUTE_OFFLINE`
  from metadata only -> `cloudOnly`; such files are counted and listed but NOT opened unless the setting
  `autoDownloadCloudOnly` is true or `download_cloud_only` runs (Deferred decision #5). Never change attributes, never pin.
- Header-only dimension read (`image::ImageReader::into_dimensions`) for local files; blake3 + dHash/pHash computed in
  the thumbnail pass (one decode per file), rayon pool sized `available_parallelism() - 1`, resumable (state in `thumb_state`).
- Thumbnails 320 and 1280 WebP per `contract-sqlite-schema.md` path rule; `fast_image_resize`.
- Jobs: `start_index`, `download_cloud_only`, `cancel_job` with `job-progress` events (<= 10/s) and `library-changed`.
- `ssimg` async URI scheme per contract (§ Image URLs), served off the main thread.
- Commands: `get_app_info`, `add_library_root` (sets `protectedRoot` when the path is under `%OneDrive%` or contains `\OneDrive\`),
  `remove_library_root`, `start_index`, `download_cloud_only`, `cancel_job`, `query_images` (sort, limit, cursor, trash,
  includeMissing only), `get_image`, `get_timeline`, `get_counts`, `get_settings`, `set_setting`. Write APIs take `Caller` (rule 8).
- Core library is UI-free; `src-tauri` commands are thin wrappers; `ssctl` uses the same library and implements the 1.2 rows of the `ssctl` surface in `design-intent.md`.
- Fixture generator `tests/common/fixtures.rs` helper (a `common/` module, so cargo does not build it as its own test crate): writes N synthetic PNGs (varied sizes, aspect ratios, a text
  label drawn, timestamps in names) into a temp dir. Tests never touch the OneDrive folder (D10).

## Tests (in `crates/ss-core/tests/index_test.rs`)

Index 500 generated fixtures -> count == 500; re-index is idempotent (no duplicate rows, no thumb regen when mtime
unchanged); a deleted file becomes `missing`; thumbnails exist for both widths; `query_images` cursor paging returns
every id exactly once; `ssimg` handler returns 200/202/400/404 per contract; a simulated cloud-only file (attribute
mock behind a trait) is not opened.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| SQLite writer contention during the thumb pass | medium | medium | single writer thread with batched transactions (500 rows) |
| Opening a cloud-only file triggers hydration | medium | medium | attribute check before any open; test with the mock |
| Filename date patterns vary (vi/en Windows locales) | medium | low | pattern table as data in `ss-core::scan::patterns`, fallback to created/mtime |
