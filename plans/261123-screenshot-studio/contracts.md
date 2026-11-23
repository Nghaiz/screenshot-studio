# Contracts index — Screenshot Studio

Owner: plan author (integration phase lead). Lanes never edit these files or their code transcriptions
(`crates/ss-core/src/contract/**`, `contracts/**`, `crates/ss-core/bindings/**`). A lane that needs a change
stops and reports to the Owner, who edits the contract file AND its transcription in one commit and re-briefs
every lane.

Pinned before any fan-out (phase 1.1 transcribes them; no lane spawns until 1.1 is committed):

| Boundary | Contract file | Code SSOT after phase 1.1 | Checked by |
|---|---|---|---|
| Rust <-> UI (Tauri commands, events, `ssimg` URLs) | `contract-tauri-ipc.md` | `crates/ss-core/src/contract/ipc.rs` -> ts-rs -> `crates/ss-core/bindings/*.ts` | `crates/ss-core/tests/contract_test.rs` (bindings fresh: ts-rs `export_all_to` into a temp dir, byte-compared with the committed `bindings/`; `SS_WRITE_BINDINGS=1` makes the same test rewrite the committed dir; no `#[ts(export)]` attribute anywhere, so a plain `cargo test` never rewrites committed bindings; FAILS on any `bigint`), `src/__tests__/contract.test.ts` (UI compiles against bindings) |
| Rust <-> Python sidecar (JSON-RPC 2.0 over stdio) | `contract-sidecar-jsonrpc.md` | `contracts/sidecar-jsonrpc.schema.json` + `contracts/fixtures/sidecar/*.json`; Rust serde types `crates/ss-core/src/contract/sidecar.rs` | `contract_test.rs` (every fixture deserializes into the Rust types and validates against the schema), `sidecar/tests/test_contract.py` (sidecar replies validate against the same schema) |
| SQLite catalog | `contract-sqlite-schema.md` | `contracts/sqlite/0001_init.sql` (embedded with `include_str!`) | `contract_test.rs` (migration applies on an empty DB; `PRAGMA table_xinfo(image)` (includes generated columns) matches the column list) |
| Shared design intent (UI look, naming, paths, defaults) | `design-intent.md` | n/a (prose) | integration phase checks |

Contract version: `CONTRACT_VERSION = 1` (Rust const in `contract/mod.rs`, returned by `get_app_info`, sent in the sidecar `initialize`).
