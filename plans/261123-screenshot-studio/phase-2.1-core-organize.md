# Phase 2.1: Core search, organize, file ops, undo, delete paths (lane core)

Lane: core
Depends-on: 1.4
Steps: lane-decides
Effort: L
Owns: `crates/**`, `src-tauri/**` (except the frozen contract paths in README D2)
Creates: `crates/ss-core/tests/query_test.rs`, `crates/ss-core/tests/fileops_test.rs`, `crates/ss-core/tests/delete_safety_test.rs`, `crates/ss-core/tests/thumbs_format_test.rs`
Accept: `node scripts/accept/lane-tests.mjs crates/ss-core/tests/query_test.rs crates/ss-core/tests/fileops_test.rs crates/ss-core/tests/delete_safety_test.rs crates/ss-core/tests/thumbs_format_test.rs`

## Goal

Implement every command marked phase 2.1 in `contract-tauri-ipc.md`: full `ImageQuery` filtering + FTS text search,
tags, albums, favourites, ratings, notes, smart filters, move/copy, rename + pattern preview, in-app trash/restore,
Recycle Bin with undo, the two-step permanent delete, the undo journal.

## Scope

- Query builder: one function `ImageQuery -> (SQL, params)` with every filter of the contract; FTS5 MATCH for
  `textMode` `any|filename|ocr` (input escaped as FTS phrase tokens, diacritics folded by the tokenizer); colour filter
  on `dominant_rgb` with tolerance (simple RGB distance; empty until 3.2 fills it). Target < 200 ms at 5,600 rows (design metric).
- Journal: every mutation in one SQLite transaction with its `op_journal` batch; `undo` replays the inverse
  (move back, rename back, restore from Recycle Bin via `trash::os_limited::restore_all` matched by original path + deletion time,
  untrash, restore old values). File op first, DB second; on DB failure the file op is reversed.
- Move/copy: destination must be an existing directory; conflicts per `onConflict`; moving out of a library root removes
  the row (journaled), into a root adds it.
- Protected roots (rule 8, path-based): file writes whose source or destination is under a protected root or a `\OneDrive\` path need `Caller::Ui` and always journal;
  `ssctl` write subcommands (`edit`, `undo`) refuse images of a `protectedRoot` root; `index`/`ai` write catalog rows only, never files. Implement the 2.1 rows of the `ssctl` surface in `design-intent.md`.
- Thumbnails (README Deferred decision #29, contract-sqlite-schema.md § thumbnails): switch `ss-core::thumbs` to lossy
  WebP q80 with crate `webp` (`webp::Encoder::from_rgb(&rgb, w, h).encode(80.0)`, already in the manifests since 1.4);
  the index makes the 320 file only; `ssimg` makes the 1280 file on first request and keeps it; a missing or different
  `<data>\thumbs\FORMAT` marker makes the next index delete old thumbnail files and regenerate them once (phase 1.4 indexed the real
  library with lossless files, ~1.3 GB; target after regeneration: ~41 MB for 5,683 images).
- Cloud-only retry (bug found in 1.4, owner deferred it here): a read of a cloud-only file that fails while the
  file is still cloud-only (OneDrive client not running, offline) is written as `error` by `thumbs::write_outcomes`,
  and `run_cloud_download` only resets `offline` rows, so the image can never be retried. Fix: after a failed read,
  re-check the cloud attributes and write `offline` when the file is still cloud-only; `run_cloud_download` also
  resets `cloudOnly` rows in `error`. The real catalog has 22 such rows from 1.4 (OneDrive was stopped): after the
  fix, the owner presses "Download cloud-only files" again (or `download_cloud_only` for root 1).
- Memory budget (Deferred decision #31, with phase 2.2): review the `ssimg` cache header
  (`Cache-Control: max-age=31536000, immutable`) for its share of the 1,208 MB measured after a full scroll in 1.4;
  a header change is a contract change, so it goes through the Owner (contract-tauri-ipc § Image URLs).
- Permanent delete: `request_permanent_delete` stores `{token -> (sorted ids, expiresAt)}` in memory only (60 s);
  `delete_permanently` requires the same token, unexpired, and `typedConfirmation == "DELETE"`; deletes with
  `std::fs::remove_file`; journal op `purge`, `undoable = 0`; tokens are single-use.

## Tests (all on generated fixtures in temp dirs, D10)

`query_test.rs`: each filter alone and combined; Vietnamese diacritic-insensitive filename search (`anh chup` finds
`Ảnh chụp.png`); 5,600-row query timing printed and asserted < 200 ms in release-like settings (debug: < 1 s).
`fileops_test.rs`: move/copy/rename/trash/recycle each followed by `undo` restores path, bytes (blake3) and DB row.
`delete_safety_test.rs`: no token -> `confirmationRequired`; wrong word -> `confirmationRequired`; expired token ->
`confirmationRequired`; token for set A used with set B impossible (token binds the set); reused token -> error; only the
happy path removes files; `undo` of a `purge` batch -> `invalidArgument`. Each case is seen red once by breaking the guard.
`thumbs_format_test.rs`: index of generated fixtures writes only `<id>_320.webp` files, each a lossy (VP8, not VP8L)
WebP; a first `ssimg` `/thumb/<id>/1280` answers 200 and leaves `<id>_1280.webp` on disk, a cloud-only image answers
202 `offline` without opening the file; a data dir with lossless thumbnails and no `thumbs\FORMAT` marker gets every `ok`
thumbnail regenerated once by the next index (old files gone, and not again by the index after); a cloud-only image
whose read fails while it is still cloud-only ends `offline`, not `error`, and a cloud-only `error` row is picked up
again by `download_cloud_only` (fault injected through the `FileAccess` seam).

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Permanent delete without the explicit confirmation | low | high | named before this phase: the 2-step guard + `delete_safety_test.rs` cases, D8 |
| Recycle Bin restore cannot find the item | medium | medium | journal stores original path + deletion time; restore matches both; test |
| Partial failure mid-batch | medium | medium | per-item `OpFailure`, journal only succeeded items |
