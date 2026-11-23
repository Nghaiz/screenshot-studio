# Phase 2.1: Core search, organize, file ops, undo, delete paths (lane core)

Lane: core
Depends-on: 1.4
Steps: lane-decides
Effort: L
Owns: `crates/**`, `src-tauri/**` (except the frozen contract paths in README D2)
Creates: `crates/ss-core/tests/query_test.rs`, `crates/ss-core/tests/fileops_test.rs`, `crates/ss-core/tests/delete_safety_test.rs`
Accept: `node scripts/accept/lane-tests.mjs crates/ss-core/tests/query_test.rs crates/ss-core/tests/fileops_test.rs crates/ss-core/tests/delete_safety_test.rs`

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

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Permanent delete without the explicit confirmation | low | high | named before this phase: the 2-step guard + `delete_safety_test.rs` cases, D8 |
| Recycle Bin restore cannot find the item | medium | medium | journal stores original path + deletion time; restore matches both; test |
| Partial failure mid-batch | medium | medium | per-item `OpFailure`, journal only succeeded items |
