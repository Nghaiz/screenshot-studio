//! How the sidecar process is started (D6: `<venv>\Scripts\python.exe -m ss_sidecar`, cwd = the sidecar
//! source dir) and where its source and contract schema live in dev and in the bundled app.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::config::{DEFAULT_HF_HOME, DEFAULT_SIDECAR_VENV};
use crate::contract::sidecar::InitializeParams;

/// Env var naming the sidecar source dir (the dir that holds the `ss_sidecar` package).
pub const SIDECAR_DIR_ENV: &str = "SS_SIDECAR_DIR";
/// Env var the sidecar reads for its params schema (`sidecar/ss_sidecar/config.py`).
pub const CONTRACT_SCHEMA_ENV: &str = "SS_CONTRACT_SCHEMA";
/// Schema file, relative to the dir that holds both `sidecar/` and `contracts/` (repo or bundle).
pub const SCHEMA_REL: &str = "contracts/sidecar-jsonrpc.schema.json";
/// Python package dir inside the sidecar source dir.
pub const PACKAGE_DIR: &str = "ss_sidecar";
/// `python -m <module>`.
pub const MODULE: &str = "ss_sidecar";
/// Python's bytecode-cache root: every `.pyc` goes under it instead of a `__pycache__` next to its source.
pub const PYCACHE_PREFIX_ENV: &str = "PYTHONPYCACHEPREFIX";

/// Windows `CREATE_NO_WINDOW`: the app is a GUI process; a console child must not open a console window.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One concrete process to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarLaunch {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Added to the inherited environment.
    pub env: Vec<(OsString, OsString)>,
}

/// What the supervisor does on start / restart.
#[derive(Debug, Clone)]
pub enum LaunchPlan {
    /// No usable venv or source: state `absent` with this detail, no spawn attempted.
    Absent(String),
    Spawn(Box<SpawnSpec>),
}

#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub launch: SidecarLaunch,
    /// Sent as the first request.
    pub init: InitializeParams,
}

/// Where the sidecar's Python source and the contract schema are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarLayout {
    /// Dir holding the `ss_sidecar` package (the process cwd).
    pub sidecar_dir: PathBuf,
    /// `contracts/sidecar-jsonrpc.schema.json`, passed to the sidecar as `SS_CONTRACT_SCHEMA`.
    pub schema: PathBuf,
}

impl SidecarLayout {
    /// `<root>/sidecar` + `<root>/contracts/...`: the repo checkout, and the bundled app when phase 3.4
    /// maps the Tauri resources to `sidecar/` and `contracts/sidecar-jsonrpc.schema.json` under the
    /// resource dir.
    pub fn under(root: &Path) -> Self {
        Self { sidecar_dir: root.join("sidecar"), schema: root.join(SCHEMA_REL) }
    }

    /// True when the package dir exists (the schema is checked by the sidecar itself, which answers
    /// every request with a named -32000 when it is missing).
    pub fn is_present(&self) -> bool {
        self.sidecar_dir.join(PACKAGE_DIR).is_dir()
    }

    /// Dev / CLI layout: `SS_SIDECAR_DIR` (its parent holds `contracts/`), else the repo this crate was
    /// built from (`crates/ss-core/../../sidecar`). None when neither exists.
    pub fn detect() -> Option<Self> {
        if let Some(dir) = std::env::var_os(SIDECAR_DIR_ENV).filter(|v| !v.is_empty()) {
            let dir = PathBuf::from(dir);
            let root = dir.parent().map(Path::to_path_buf).unwrap_or_default();
            return Some(Self { sidecar_dir: dir, schema: root.join(SCHEMA_REL) });
        }
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let layout = Self::under(&repo);
        layout.is_present().then_some(layout)
    }
}

/// The venv's interpreter (`Scripts\python.exe` on Windows, `bin/python` elsewhere).
pub fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) { venv.join("Scripts").join("python.exe") } else { venv.join("bin").join("python") }
}

/// The venv the `sidecarVenv` setting names (`null` = [`DEFAULT_SIDECAR_VENV`]).
pub fn venv_from_setting(value: &serde_json::Value) -> PathBuf {
    match value.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => PathBuf::from(s),
        None => PathBuf::from(DEFAULT_SIDECAR_VENV),
    }
}

/// The real sidecar launch, or the reason it is absent. `pycache` is where Python keeps the bytecode it
/// compiles ([`PYCACHE_PREFIX_ENV`]; the app passes [`crate::paths::sidecar_pycache_dir`]): without it
/// the bundled app's first start wrote `__pycache__` into `<install dir>\sidecar`, which the uninstaller
/// does not remove. Measured (phase 3.4b, `--selftest` on CUDA): 13.2 s with the venv's own caches, 40.1 s
/// on the first start into an empty prefix (3,162 `.pyc`, 78 MB: the venv's and the standard library's
/// caches are not shared with it), 13.4 s on every start after.
pub fn plan(venv: &Path, layout: Option<&SidecarLayout>, init: InitializeParams, pycache: &Path) -> LaunchPlan {
    let python = venv_python(venv);
    if !python.is_file() {
        return LaunchPlan::Absent(format!(
            "no Python at {} (setting sidecarVenv); set it up with `uv sync --project sidecar` (sidecar/README.md)",
            python.display()
        ));
    }
    let Some(layout) = layout.filter(|l| l.is_present()) else {
        return LaunchPlan::Absent(format!(
            "sidecar source not found{} (set {SIDECAR_DIR_ENV})",
            layout.map(|l| format!(" at {}", l.sidecar_dir.display())).unwrap_or_default()
        ));
    };
    let hf_home = std::env::var_os("HF_HOME").filter(|v| !v.is_empty()).unwrap_or_else(|| DEFAULT_HF_HOME.into());
    let launch = SidecarLaunch {
        program: python,
        args: vec!["-m".into(), MODULE.into()],
        cwd: layout.sidecar_dir.clone(),
        env: vec![
            ("HF_HOME".into(), hf_home),
            ("PYTHONUTF8".into(), "1".into()),
            ("PYTHONUNBUFFERED".into(), "1".into()),
            (CONTRACT_SCHEMA_ENV.into(), layout.schema.clone().into_os_string()),
            (PYCACHE_PREFIX_ENV.into(), pycache.as_os_str().to_owned()),
        ],
    };
    LaunchPlan::Spawn(Box::new(SpawnSpec { launch, init }))
}

/// Kills a process and its children (the uv venv's `python.exe` is a launcher: the interpreter is its
/// child, docs/lessons.md phase 3.1). Blocking; best effort.
pub(crate) fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let r = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        if let Err(e) = r {
            eprintln!("ss-sidecar: taskkill {pid}: {e}");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid; // the child itself is killed through tokio's `start_kill`
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ipc::AiDevice;
    use crate::contract::sidecar::{OcrEngine, PROTOCOL_VERSION};

    fn init() -> InitializeParams {
        InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            device: AiDevice::Auto,
            languages: vec!["vi".into()],
            embed_model: "m".into(),
            ocr_engine: OcrEngine::Rapidocr,
        }
    }

    #[test]
    fn spawn_sends_python_bytecode_to_the_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let venv = tmp.path().join("venv");
        std::fs::create_dir_all(venv_python(&venv).parent().unwrap()).unwrap();
        std::fs::write(venv_python(&venv), b"").unwrap();
        let root = tmp.path().join("install");
        std::fs::create_dir_all(root.join("sidecar").join(PACKAGE_DIR)).unwrap();
        let data = tmp.path().join("data");
        let pycache = crate::paths::sidecar_pycache_dir(&data);
        let LaunchPlan::Spawn(spec) = plan(&venv, Some(&SidecarLayout::under(&root)), init(), &pycache) else {
            panic!("expected a spawn plan");
        };
        let env: Vec<(String, String)> =
            spec.launch.env.iter().map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned())).collect();
        let prefix: Vec<&String> = env.iter().filter(|(k, _)| k == PYCACHE_PREFIX_ENV).map(|(_, v)| v).collect();
        assert_eq!(prefix, [&data.join("sidecar-pycache").to_string_lossy().into_owned()], "{env:?}");
        assert!(!prefix[0].starts_with(&*root.to_string_lossy()), "never under the install dir");
        assert_eq!(spec.launch.cwd, root.join("sidecar"));
    }
}
