//! Executable contract check (plans/261123-screenshot-studio/contracts.md).
//!
//! (a) TS bindings are fresh (byte-equal to the committed `bindings/`) and contain no `bigint`;
//!     `SS_WRITE_BINDINGS=1` rewrites the committed dir instead of comparing.
//! (b) every sidecar fixture validates against the JSON Schema AND round-trips through the Rust types.
//! (c) `0001_init.sql` is verbatim from the contract, applies to an empty DB, and has the contract columns.
//! (d) `AppError` serializes to the error envelope; Option fields are `null`, never omitted.
//! (e) the command table matches the contract doc, the binding files and the Tauri command stubs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use ss_core::contract::ipc::{self, COMMANDS};
use ss_core::contract::sidecar::{self, *};
use ss_core::contract::{self as contract, AppError, ErrorKind};

const REGEN_HINT: &str = "run SS_WRITE_BINDINGS=1 cargo test -p ss-core --test contract_test and commit";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repo root")
}

fn plan_dir() -> PathBuf {
    repo_root().join("plans/261123-screenshot-studio")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Relative path (with `/`) -> bytes, for every file under `dir`.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry.expect("walk");
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
            out.insert(rel, fs::read(entry.path()).unwrap());
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------
// (a) bindings
// ------------------------------------------------------------------------------------------------

#[test]
fn a_bindings_are_fresh_and_have_no_bigint() {
    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings");
    if std::env::var("SS_WRITE_BINDINGS").as_deref() == Ok("1") {
        if committed.exists() {
            fs::remove_dir_all(&committed).expect("clear bindings");
        }
        fs::create_dir_all(&committed).unwrap();
        contract::export_bindings(&committed).expect("export bindings");
    }

    let tmp = tempfile::tempdir().unwrap();
    contract::export_bindings(tmp.path()).expect("export bindings");
    let fresh = tree(tmp.path());
    assert!(fresh.len() > 50, "expected the full binding set, got {} files", fresh.len());

    for (name, bytes) in &fresh {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains("bigint"), "{name} contains `bigint`: an i64 field lacks its #[ts(type)] override (rule 3)");
    }

    let old = tree(&committed);
    let fresh_names: BTreeSet<_> = fresh.keys().collect();
    let old_names: BTreeSet<_> = old.keys().collect();
    let missing: Vec<_> = fresh_names.difference(&old_names).collect();
    let extra: Vec<_> = old_names.difference(&fresh_names).collect();
    let changed: Vec<_> = fresh.iter().filter(|(k, v)| old.get(*k).is_some_and(|o| o != *v)).map(|(k, _)| k).collect();
    assert!(
        missing.is_empty() && extra.is_empty() && changed.is_empty(),
        "bindings stale ({REGEN_HINT}): missing {missing:?}, extra {extra:?}, changed {changed:?}"
    );
}

// ------------------------------------------------------------------------------------------------
// (b) sidecar fixtures
// ------------------------------------------------------------------------------------------------

fn schema() -> Value {
    serde_json::from_str(&read(&repo_root().join("contracts/sidecar-jsonrpc.schema.json"))).expect("schema json")
}

/// Validator for one `$defs` entry (or the root Message when `def` is None).
fn validator(schema: &Value, def: Option<&str>) -> jsonschema::Validator {
    let mut s = schema.clone();
    if let Some(def) = def {
        assert!(s["$defs"].get(def).is_some(), "schema has no $defs/{def}");
        s["$ref"] = json!(format!("#/$defs/{def}"));
    }
    jsonschema::validator_for(&s).expect("compile schema")
}

fn assert_valid(v: &jsonschema::Validator, instance: &Value, what: &str) {
    let errors: Vec<String> = v.iter_errors(instance).map(|e| format!("{} at {}", e, e.instance_path())).collect();
    assert!(errors.is_empty(), "{what} fails the schema: {errors:#?}");
}

/// Deserialize into `T`, serialize back, require identical JSON.
fn roundtrip<T: Serialize + DeserializeOwned>(value: &Value, what: &str) -> T {
    let typed: T = serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{what} does not deserialize: {e}"));
    let back = serde_json::to_value(&typed).unwrap();
    assert_eq!(&back, value, "{what} does not round-trip through the Rust types");
    typed
}

/// `ocr.recognize` -> `OcrRecognize`
fn pascal(method: &str) -> String {
    method
        .split('.')
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

fn roundtrip_result(method: &str, value: &Value, what: &str) {
    match method {
        methods::INITIALIZE => drop(roundtrip::<RpcResponse<InitializeResult>>(value, what)),
        methods::OCR_RECOGNIZE => drop(roundtrip::<RpcResponse<OcrRecognizeResult>>(value, what)),
        methods::EMBED_IMAGES => drop(roundtrip::<RpcResponse<EmbedImagesResult>>(value, what)),
        methods::EMBED_TEXT => drop(roundtrip::<RpcResponse<EmbedTextResult>>(value, what)),
        methods::CLASSIFY_ZERO_SHOT => drop(roundtrip::<RpcResponse<ClassifyZeroShotResult>>(value, what)),
        methods::COLOR_DOMINANT => drop(roundtrip::<RpcResponse<ColorDominantResult>>(value, what)),
        methods::PII_DETECT => drop(roundtrip::<RpcResponse<PiiDetectResult>>(value, what)),
        methods::SHUTDOWN => drop(roundtrip::<RpcResponse<EmptyObject>>(value, what)),
        other => panic!("no result type for method {other}"),
    }
}

#[test]
fn b_sidecar_fixtures_match_schema_and_rust_types() {
    let schema = schema();
    let message = validator(&schema, None);
    let dir = repo_root().join("contracts/fixtures/sidecar");
    let load = |name: &str| -> Value {
        serde_json::from_str(&read(&dir.join(name))).unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    let mut used = BTreeSet::new();

    for method in methods::ALL_REQUESTS {
        let req_name = format!("{method}.request.json");
        let req = load(&req_name);
        assert_valid(&message, &req, &req_name);
        let params_def =
            if method == methods::SHUTDOWN { "EmptyObject".to_owned() } else { format!("{}Params", pascal(method)) };
        assert_valid(&validator(&schema, Some(&params_def)), &req["params"], &req_name);
        let typed: RpcRequest = roundtrip(&req, &req_name);
        let back = serde_json::to_value(&typed).unwrap();
        assert_eq!(back["method"], json!(method), "{req_name}: method mismatch");
        assert_eq!(typed.jsonrpc, sidecar::JSONRPC_VERSION);
        used.insert(req_name);

        let res_name = format!("{method}.response.json");
        let res = load(&res_name);
        assert_valid(&message, &res, &res_name);
        assert_valid(&validator(&schema, Some(&format!("{}Result", pascal(method)))), &res["result"], &res_name);
        roundtrip_result(method, &res, &res_name);
        assert_eq!(res["id"], req["id"], "{res_name}: id must answer its request");
        used.insert(res_name);
    }

    let err = load("error.json");
    assert_valid(&message, &err, "error.json");
    let typed: RpcErrorResponse = roundtrip(&err, "error.json");
    assert_eq!(typed.error.code, error_codes::MODEL_NOT_LOADED);
    used.insert("error.json".into());

    for (name, method) in [("progress.notification.json", methods::PROGRESS), ("cancel.notification.json", methods::CANCEL)] {
        let note = load(name);
        assert_valid(&message, &note, name);
        let _: RpcNotification = roundtrip(&note, name);
        assert_eq!(note["method"], json!(method));
        used.insert(name.into());
    }

    let on_disk: BTreeSet<String> =
        fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    let orphans: Vec<_> = on_disk.difference(&used).collect();
    assert!(orphans.is_empty(), "fixtures not covered by this test: {orphans:?}");
}

#[test]
fn b_schema_rejects_malformed_messages() {
    let schema = schema();
    let message = validator(&schema, None);
    let bad = [
        json!({"jsonrpc":"2.0","id":1,"method":"ocr.recognize","params":{"items":[{"imageId":1}]}}),
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":2,"device":"auto","languages":[],"embedModel":"m","ocrEngine":"rapidocr"}}),
        json!({"jsonrpc":"2.0","id":1,"method":"nope","params":{}}),
        json!({"jsonrpc":"1.0","id":1,"result":{}}),
        json!({"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"x","data":{"detail":null}}}),
    ];
    for b in &bad {
        assert!(!message.is_valid(b), "schema accepted a malformed message: {b}");
    }
}

// ------------------------------------------------------------------------------------------------
// (c) SQLite schema
// ------------------------------------------------------------------------------------------------

const IMAGE_COLUMNS: [&str; 25] = [
    "id", "root_id", "folder", "file_name", "rel_path", "size_bytes", "mtime_ms", "created_ms", "taken_ms", "width",
    "height", "aspect", "content_hash", "dhash", "phash", "blur_score", "dominant_rgb", "thumb_state", "cloud_state",
    "favorite", "rating", "note", "trashed_at", "missing", "indexed_at",
];

const TABLES: [&str; 14] = [
    "schema_version", "library_root", "image", "text_fts", "ocr_result", "embedding", "category", "tag", "image_tag",
    "album", "album_image", "smart_filter", "op_journal", "setting",
];

#[test]
fn c_sql_migration_is_verbatim_and_applies() {
    let md = read(&plan_dir().join("contract-sqlite-schema.md")).replace("\r\n", "\n");
    let start = md.find("```sql\n").expect("sql block") + "```sql\n".len();
    let end = start + md[start..].find("\n```").expect("sql block end") + 1;
    assert_eq!(contract::MIGRATION_0001_INIT.replace("\r\n", "\n"), &md[start..end], "0001_init.sql is not verbatim from contract-sqlite-schema.md");

    let tmp = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(tmp.path().join("catalog.sqlite")).unwrap();
    db.execute_batch(contract::MIGRATION_0001_INIT).expect("migration applies to an empty DB");

    let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    let version: i64 = db.query_row("SELECT version FROM schema_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 1);

    let mut stmt = db.prepare("SELECT name FROM pragma_table_xinfo('image') ORDER BY cid").unwrap();
    let cols: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(cols, IMAGE_COLUMNS, "image columns differ from the contract");

    let mut stmt = db
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'text_fts_%'")
        .unwrap();
    let tables: BTreeSet<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    let expected: BTreeSet<String> = TABLES.iter().map(|s| s.to_string()).collect();
    assert_eq!(tables, expected);
    assert_eq!(db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0)).unwrap(), 1);

    // Every JournalOp wire string is accepted by the op_journal CHECK; an unknown op is rejected.
    let ops: Vec<String> = [
        ipc::JournalOp::Move, ipc::JournalOp::Copy, ipc::JournalOp::Rename, ipc::JournalOp::Trash,
        ipc::JournalOp::Restore, ipc::JournalOp::Recycle, ipc::JournalOp::Purge, ipc::JournalOp::Tag,
        ipc::JournalOp::Untag, ipc::JournalOp::Favorite, ipc::JournalOp::Rating, ipc::JournalOp::Note,
        ipc::JournalOp::AlbumAdd, ipc::JournalOp::AlbumRemove, ipc::JournalOp::AlbumReorder,
        ipc::JournalOp::EditOverwrite, ipc::JournalOp::Convert,
    ]
    .iter()
    .map(|op| serde_json::to_value(op).unwrap().as_str().unwrap().to_owned())
    .collect();
    let insert = "INSERT INTO op_journal (batch_id, op, payload_json, undoable, created_at) VALUES (1, ?1, '{}', 1, 0)";
    for op in &ops {
        db.execute(insert, [op]).unwrap_or_else(|e| panic!("op_journal rejects JournalOp {op}: {e}"));
    }
    assert!(db.execute(insert, ["delete"]).is_err(), "op_journal accepted an unknown op");

    // The generated aspect column follows the contract's ratio bands.
    db.execute("INSERT INTO library_root (id, path, added_at) VALUES (1, 'D:/lib', 0)", []).unwrap();
    for (i, (w, h, want)) in [(1080, 2400, Some("phone")), (1920, 1080, Some("pc")), (1000, 1000, Some("square")), (3440, 1000, Some("wide")), (1200, 1000, Some("other"))]
        .into_iter()
        .enumerate()
    {
        db.execute(
            "INSERT INTO image (root_id, folder, file_name, size_bytes, mtime_ms, taken_ms, width, height, indexed_at) VALUES (1, 'a', ?1, 1, 0, 0, ?2, ?3, 0)",
            rusqlite::params![format!("{i}.png"), w, h],
        )
        .unwrap();
        let (aspect, rel): (Option<String>, String) = db
            .query_row("SELECT aspect, rel_path FROM image WHERE file_name = ?1", [format!("{i}.png")], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(aspect.as_deref(), want, "aspect of {w}x{h}");
        assert_eq!(rel, format!("a/{i}.png"));
    }
}

// ------------------------------------------------------------------------------------------------
// (d) error envelope and null discipline
// ------------------------------------------------------------------------------------------------

#[test]
fn d_app_error_envelope() {
    let e = AppError::new(ErrorKind::NotFound, "image 7 not found");
    assert_eq!(serde_json::to_value(&e).unwrap(), json!({"kind":"notFound","message":"image 7 not found","detail":null}));
    let e = AppError::not_implemented().with_detail("phase 1.2");
    assert_eq!(serde_json::to_value(&e).unwrap(), json!({"kind":"internal","message":"not implemented","detail":"phase 1.2"}));

    let kinds = [
        (ErrorKind::NotFound, "notFound"), (ErrorKind::Io, "io"), (ErrorKind::Db, "db"),
        (ErrorKind::InvalidArgument, "invalidArgument"), (ErrorKind::Conflict, "conflict"),
        (ErrorKind::SidecarUnavailable, "sidecarUnavailable"), (ErrorKind::Cancelled, "cancelled"),
        (ErrorKind::PermissionDenied, "permissionDenied"), (ErrorKind::ProtectedRoot, "protectedRoot"),
        (ErrorKind::ConfirmationRequired, "confirmationRequired"), (ErrorKind::Internal, "internal"),
    ];
    // Own temp export: test (a) may be rewriting the committed dir concurrently under SS_WRITE_BINDINGS=1.
    let tmp = tempfile::tempdir().unwrap();
    contract::export_bindings(tmp.path()).unwrap();
    let ts = read(&tmp.path().join("ErrorKind.ts"));
    for (k, s) in kinds {
        assert_eq!(serde_json::to_value(k).unwrap(), json!(s));
        assert!(ts.contains(&format!("\"{s}\"")), "ErrorKind.ts lacks {s}");
    }
    let parsed: AppError = serde_json::from_value(json!({"kind":"io","message":"m"})).unwrap();
    assert_eq!(parsed.detail, None, "a missing optional key reads as null");
}

#[test]
fn d_options_serialize_as_null_and_missing_inputs_read_as_null() {
    let page = ipc::ImagePage { items: vec![], total: 0, next_cursor: None };
    assert_eq!(serde_json::to_value(&page).unwrap(), json!({"items":[],"total":0,"nextCursor":null}));

    let q: ipc::ImageQuery = serde_json::from_value(json!({
        "textMode":"any","aspects":[],"folders":[],"tagIds":[],"tagMatch":"any","colorTolerance":0,
        "categories":[],"favoriteOnly":false,"minRating":0,"trash":"exclude","includeMissing":false,
        "sort":{"field":"taken","dir":"desc"},"limit":100
    }))
    .expect("ImageQuery with every optional key missing");
    let v = serde_json::to_value(&q).unwrap();
    for key in ["text", "dateFrom", "dateTo", "minWidth", "maxWidth", "minHeight", "maxHeight", "minSize", "maxSize", "albumId", "color", "hasText", "cursor"] {
        assert_eq!(v.get(key), Some(&Value::Null), "ImageQuery.{key} must serialize as null");
    }

    for key in ipc::SettingKey::ALL {
        let name = serde_json::to_value(key).unwrap();
        assert!(name.is_string(), "{key:?}");
        let _ = key.default_value();
    }
    assert_eq!(ipc::SettingKey::Hotkey.default_value(), json!("Ctrl+Alt+S"));
}

// ------------------------------------------------------------------------------------------------
// (e) command table
// ------------------------------------------------------------------------------------------------

/// Command names from the `## Commands` table of contract-tauri-ipc.md (first column).
fn contract_command_names() -> BTreeSet<String> {
    let md = read(&plan_dir().join("contract-tauri-ipc.md")).replace("\r\n", "\n");
    let start = md.find("## Commands").expect("Commands section");
    let end = start + md[start..].find("\n## Events").expect("Events section");
    let mut names = BTreeSet::new();
    for line in md[start..end].lines().filter(|l| l.starts_with("| `")) {
        let first = line.split('|').nth(1).unwrap();
        for part in first.split('`').skip(1).step_by(2) {
            names.insert(part.to_owned());
        }
    }
    names
}

/// `fn` names that carry `#[tauri::command]` in src-tauri/src/commands/*.rs.
fn stub_command_names() -> BTreeSet<String> {
    let dir = repo_root().join("src-tauri/src/commands");
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(&dir).expect("src-tauri/src/commands") {
        let path = entry.unwrap().path();
        let text = read(&path);
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            if line.trim() == "#[tauri::command]" {
                let sig = lines.next().expect("fn after #[tauri::command]");
                let after = sig.split("fn ").nth(1).unwrap_or_else(|| panic!("{}: not a fn: {sig}", path.display()));
                names.insert(after.split(['(', '<']).next().unwrap().trim().to_owned());
            }
        }
    }
    names
}

#[test]
fn e_command_table_matches_contract_bindings_and_stubs() {
    let table: BTreeSet<String> = COMMANDS.iter().map(|c| c.name.to_owned()).collect();
    assert_eq!(table.len(), COMMANDS.len(), "duplicate command name in COMMANDS");
    assert_eq!(table, contract_command_names(), "COMMANDS differs from contract-tauri-ipc.md § Commands");
    assert_eq!(stub_command_names(), table, "src-tauri command stubs differ from COMMANDS");

    let lib = read(&repo_root().join("src-tauri/src/lib.rs"));
    for name in &table {
        assert!(lib.contains(&format!("::{name},")) || lib.contains(&format!("::{name}\n")), "lib.rs generate_handler! lacks {name}");
    }

    let raw: Vec<_> = COMMANDS.iter().filter(|c| c.raw_body).map(|c| c.name).collect();
    assert_eq!(raw, ["save_edited_image"], "rule 1: exactly one raw-body command");
    assert!(COMMANDS.iter().filter(|c| c.raw_body).all(|c| c.req.is_none()));

    let tmp = tempfile::tempdir().unwrap();
    contract::export_bindings(tmp.path()).unwrap();
    for c in COMMANDS {
        for ty in c.req.into_iter().chain([c.res]) {
            for id in contract::type_idents(ty) {
                assert!(tmp.path().join(format!("{id}.ts")).exists(), "{}: type {id} is not an exported binding", c.name);
            }
        }
    }
}
