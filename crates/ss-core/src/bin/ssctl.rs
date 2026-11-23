//! `ssctl`: agent-verifiable CLI over `ss-core` (design-intent.md § `ssctl` surface, AX-01).
//!
//! Every subcommand prints ONE JSON document on stdout (compact with `--json`, pretty without),
//! logs on stderr, and exits 0 ok / 1 error (the `AppError` envelope is the stdout document) /
//! 2 usage. Global `--data-dir <path>` (else `SS_DATA_DIR`, else the default data dir).
//! `ssctl` runs as `Caller::Cli`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller, ErrorKind};
use ss_core::fsaccess::OsFileAccess;
use ss_core::jobs::EventSink;
use ss_core::{AppCore, IndexOptions, paths, query};

const USAGE: &str = "\
usage: ssctl [--data-dir <path>] [--json] <subcommand> [args]

Subcommands (phase 1.2):
  add-root <path>                        add a library root; prints LibraryRoot
  remove-root <rootId>                   remove a root from the catalog (files untouched); prints OpResult
  index [<rootId>] [--wait]              scan + thumbnails, always runs to completion; prints CatalogCounts
  list [--since <ms>] [--limit N] [--cursor C] [--include-missing]
                                         images by taken time, newest first; prints ImagePage
  stats                                  prints CatalogCounts
  extensions                             indexed file extensions; prints string[]
  get <key>                              one setting value (JSON)
  set <key> <json>                       store a setting; prints null

Global flags:
  --data-dir <path>   data dir (else SS_DATA_DIR, else D:\\IOT_Tools\\data\\ScreenshotStudio)
  --json              compact one-line JSON on stdout
  -h, --help          this help

Exit codes: 0 ok, 1 error (stdout holds the AppError JSON), 2 usage.";

struct Usage(String);

enum Failure {
    Usage(Usage),
    App(AppError),
}

impl From<AppError> for Failure {
    fn from(e: AppError) -> Self {
        Failure::App(e)
    }
}

fn usage(msg: impl Into<String>) -> Failure {
    Failure::Usage(Usage(msg.into()))
}

/// Progress on stderr, one line per (throttled) event.
struct StderrSink;

impl EventSink for StderrSink {
    fn job_progress(&self, p: &JobProgress) {
        match &p.error {
            Some(e) => eprintln!("ssctl: {} failed: {}: {}", p.stage, e.message, e.detail.as_deref().unwrap_or("")),
            None => eprintln!("ssctl: {} {}/{} {}", p.stage, p.done, p.total, p.current_path.as_deref().unwrap_or("")),
        }
    }

    fn library_changed(&self, _: &LibraryChanged) {}
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let compact = take_flag(&mut args, "--json");
    let help = take_flag(&mut args, "--help") | take_flag(&mut args, "-h");
    let print = |v: &Value| {
        let s = if compact { v.to_string() } else { serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()) };
        println!("{s}");
    };
    if help || args.first().is_some_and(|a| a == "help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&mut args) {
        Ok(v) => {
            print(&v);
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(Usage(msg))) => {
            eprintln!("ssctl: {msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::App(e)) => {
            eprintln!("ssctl: error: {:?}: {}{}", e.kind, e.message, e.detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default());
            print(&serde_json::to_value(&e).expect("AppError serializes"));
            ExitCode::from(1)
        }
    }
}

fn run(args: &mut Vec<String>) -> Result<Value, Failure> {
    let data_dir = take_value(args, "--data-dir")?.map(PathBuf::from).unwrap_or_else(paths::data_dir);
    if args.is_empty() {
        return Err(usage("missing subcommand"));
    }
    let sub = args.remove(0);
    let open = || AppCore::open(&data_dir, Arc::new(OsFileAccess));
    match sub.as_str() {
        "add-root" => {
            let path = one_positional(args, "<path>")?;
            json(open()?.add_library_root(Caller::Cli, AddLibraryRootReq { path })?)
        }
        "remove-root" => {
            let root_id = parse_int(&one_positional(args, "<rootId>")?, "<rootId>")?;
            json(open()?.remove_library_root(Caller::Cli, RemoveLibraryRootReq { root_id })?)
        }
        "index" => {
            let _wait = take_flag(args, "--wait"); // the CLI always waits: a background job would die with the process
            let root_id = match args.len() {
                0 => None,
                1 => Some(parse_int(&args.remove(0), "<rootId>")?),
                _ => return Err(usage(format!("unexpected arguments: {}", args.join(" ")))),
            };
            let core = open()?;
            let report = core.index_blocking(root_id, IndexOptions::default(), &StderrSink)?;
            eprintln!("ssctl: index report {}", serde_json::to_string(&report).unwrap_or_default());
            json(core.get_counts()?)
        }
        "list" => {
            let since = take_value(args, "--since")?.map(|v| parse_int(&v, "--since")).transpose()?;
            let limit = take_value(args, "--limit")?.map(|v| parse_int(&v, "--limit")).transpose()?.unwrap_or(100);
            let cursor = take_value(args, "--cursor")?;
            let include_missing = take_flag(args, "--include-missing");
            no_more(args)?;
            let mut q = query::default_query(limit);
            q.date_from = since;
            q.cursor = cursor;
            q.include_missing = include_missing;
            json(open()?.query_images(&q)?)
        }
        "stats" => {
            no_more(args)?;
            json(open()?.get_counts()?)
        }
        "extensions" => {
            no_more(args)?;
            json(ss_core::app::extensions())
        }
        "get" => {
            let key = setting_key(&one_positional(args, "<key>")?)?;
            let settings = open()?.get_settings()?;
            let name = serde_json::to_value(key).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
            Ok(settings.0.get(&name).cloned().unwrap_or(Value::Null))
        }
        "set" => {
            if args.len() != 2 {
                return Err(usage("set needs <key> <json>"));
            }
            let key = setting_key(&args[0])?;
            let value: Value = serde_json::from_str(&args[1])
                .map_err(|e| AppError::new(ErrorKind::InvalidArgument, "value is not valid JSON").with_detail(e.to_string()))?;
            open()?.set_setting(Caller::Cli, SetSettingReq { key, value })?;
            Ok(Value::Null)
        }
        other => Err(usage(format!("unknown subcommand `{other}`"))),
    }
}

fn json<T: Serialize>(v: T) -> Result<Value, Failure> {
    serde_json::to_value(v).map_err(|e| Failure::App(AppError::new(ErrorKind::Internal, "serialize").with_detail(e.to_string())))
}

fn setting_key(s: &str) -> Result<SettingKey, Failure> {
    serde_json::from_value(Value::String(s.to_owned())).map_err(|_| {
        Failure::App(AppError::new(ErrorKind::InvalidArgument, format!("unknown setting key `{s}`")))
    })
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != flag);
    args.len() != before
}

fn take_value(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, Failure> {
    match args.iter().position(|a| a == flag) {
        None => Ok(None),
        Some(i) if i + 1 < args.len() => {
            let v = args.remove(i + 1);
            args.remove(i);
            Ok(Some(v))
        }
        Some(_) => Err(usage(format!("{flag} needs a value"))),
    }
}

fn one_positional(args: &mut Vec<String>, name: &str) -> Result<String, Failure> {
    if args.len() != 1 {
        return Err(usage(format!("expected exactly one argument {name}")));
    }
    Ok(args.remove(0))
}

fn no_more(args: &[String]) -> Result<(), Failure> {
    if args.is_empty() { Ok(()) } else { Err(usage(format!("unexpected arguments: {}", args.join(" ")))) }
}

fn parse_int(s: &str, name: &str) -> Result<i64, Failure> {
    s.parse().map_err(|_| usage(format!("{name} must be an integer, got `{s}`")))
}
