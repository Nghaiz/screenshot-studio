# Lessons (project-local)

Non-obvious gotchas found while building Screenshot Studio. Owner rule: lessons stay in this repo, nothing goes
to a kit. Newest first. One entry: what happened, why, what to do.

## 2026-11-23 — phase 1.1

### Vitest finds no suite when the cwd drive letter is lowercase
- **Seen:** every test file, even `it("x", ...)` alone, failed with "Vitest failed to find the current suite",
  `Tests: no tests`. The same file passed from a temp dir.
- **Why:** VS Code starts terminals in `d:\Screenshot_Picture` (lowercase drive). Vitest then loads its runner
  under two path spellings (`d:/` and `D:/`), and `it()` registers on the wrong instance.
- **Do:** run vitest from a cwd with an upper-case drive (`cd /d D:\Screenshot_Picture`).
  `scripts/accept/lane-tests.mjs` already normalizes the drive letter for every runner it spawns, so always go
  through it for Accept runs.

### Tool shells mangle backslashes in inline scripts and heredocs
- **Seen:** `\\` written in a bash heredoc landed as `\` in JSON fixtures (invalid escape); an inline
  `node -e` with a `\\` regex failed to parse.
- **Do:** write anything containing backslashes (Windows paths in JSON, regexes) with the editor's Write tool or
  a script file, not inline shell strings. `contract_test.rs` (b) catches a broken fixture.

### Contract-test gotchas (ts-rs 12, serde, Git for Windows)
- `#[ts(type = "...")]` replaces the whole field type, so `Option<i64>` needs `"number | null"` and `Vec<i64>`
  `"number[]"`; the ts-rs default for i64 is `bigint`. `contract_test` (a) fails on any `bigint`.
- ts-rs does not parse `#[serde(transparent)]` (warning). A serde newtype struct already serializes as its
  inner value, so leave the attribute off.
- Git for Windows ships `core.autocrlf=true`; `.gitattributes` pins `eol=lf` so the byte-compare of committed
  bindings does not flip on checkout.
- `contract_test` sub-tests run in parallel: under `SS_WRITE_BINDINGS=1`, (a) rewrites `bindings/`, so no
  other sub-test may read the committed dir. They export to their own temp dir.
- Fixture floats: serde_json treats `12` and `12.0` as different `Value`s, so a round-trip of an `f64` field
  needs `12.0` in the fixture (as Python's `json` writes it).

### First full build is slow
- The first `cargo test -p ss-core` compiled ~330 crates in 10.5 min (image, printpdf, zip, rusqlite bundled).
  sccache (`D:\DevCache\cache\sccache`) makes later clean builds much faster; incremental test runs take seconds.
