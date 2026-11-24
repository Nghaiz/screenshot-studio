//! `AppCore`, phase 3.2 surface: sidecar lifecycle, the AI job, the semantic search arm and the
//! sidecar-free smart features (duplicates, sessions, blurry / tiny, stats).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::ai::{self, AiReport, AiRun};
use crate::app::AppCore;
use crate::catalog;
use crate::contract::ipc::*;
use crate::contract::sidecar::{InitializeParams, OcrLine, PROTOCOL_VERSION, PiiDetectParams, PiiDetectResult, PiiLine, SidecarCapability, methods};
use crate::contract::{AppError, ErrorKind};
use crate::errors::{internal, invalid};
use crate::filter::SemanticScores;
use crate::jobs::EventSink;
use crate::paths;
use crate::query;
use crate::sidecar::config::{
    DETECT_PII_TIMEOUT_S, EMBED_MODEL, LANGUAGES, SEMANTIC_ANY_MIN_SCORE, SEMANTIC_ANY_TOP_K, SEMANTIC_ONLY_TIMEOUT_MS,
    SEMANTIC_QUERY_TIMEOUT_MS, SEMANTIC_TOP_K,
};
use crate::sidecar::{LaunchPlan, Sidecar, SidecarLaunch, SidecarLayout, SpawnSpec, launch, status_line};
use crate::smart::{duplicates, issues, sessions, stats};

impl AppCore {
    // -----------------------------------------------------------------------------------------
    // Sidecar lifecycle (contract-sidecar-jsonrpc.md § Lifecycle)
    // -----------------------------------------------------------------------------------------

    pub fn sidecar(&self) -> &Sidecar {
        &self.sidecar
    }

    /// Where the sidecar source and schema are (the bundled app passes its resource layout; without
    /// it [`SidecarLayout::detect`] finds the dev checkout).
    pub fn set_sidecar_layout(&self, layout: SidecarLayout) {
        *self.sidecar_layout.lock().expect("sidecar layout") = Some(layout);
    }

    /// Starts the sidecar from the settings (`sidecarVenv`, `aiDevice`); returns at once (`starting`,
    /// or `absent` when the venv or the source is missing).
    pub fn start_sidecar(&self) -> Result<SidecarStatus, AppError> {
        Ok(self.sidecar.start(self.launch_plan()?))
    }

    /// Starts a fixed process instead of the settings-based one; `restart_sidecar` reuses it.
    pub fn start_sidecar_with(&self, launch: SidecarLaunch) -> Result<SidecarStatus, AppError> {
        *self.sidecar_launch.lock().expect("sidecar launch") = Some(launch);
        self.start_sidecar()
    }

    /// `restart_sidecar`: kills the running process (if any), clears the crash history and starts
    /// again from the current settings.
    pub fn restart_sidecar(&self) -> Result<SidecarStatus, AppError> {
        self.start_sidecar()
    }

    pub fn get_sidecar_status(&self) -> SidecarStatus {
        self.sidecar.status()
    }

    /// App exit: `shutdown`, then a kill after the shutdown wait (blocks at most that long).
    pub fn shutdown_sidecar(&self) {
        self.sidecar.shutdown();
    }

    fn launch_plan(&self) -> Result<LaunchPlan, AppError> {
        let device: AiDevice = serde_json::from_value(self.setting(SettingKey::AiDevice)?)
            .map_err(|e| internal("setting aiDevice holds an invalid value").with_detail(e.to_string()))?;
        let init = InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            device,
            languages: LANGUAGES.iter().map(|s| (*s).to_owned()).collect(),
            embed_model: EMBED_MODEL.to_owned(),
            ocr_engine: ai::OCR_ENGINE,
        };
        if let Some(l) = self.sidecar_launch.lock().expect("sidecar launch").clone() {
            return Ok(LaunchPlan::Spawn(Box::new(SpawnSpec { launch: l, init })));
        }
        let venv = launch::venv_from_setting(&self.setting(SettingKey::SidecarVenv)?);
        let layout = self.sidecar_layout.lock().expect("sidecar layout").clone().or_else(SidecarLayout::detect);
        Ok(launch::plan(&venv, layout.as_ref(), init, &paths::sidecar_pycache_dir(self.data_dir())))
    }

    fn setting(&self, key: SettingKey) -> Result<serde_json::Value, AppError> {
        self.catalog.read(|conn| catalog::get_setting(conn, key))
    }

    fn setting_i64(&self, key: SettingKey) -> Result<i64, AppError> {
        let v = self.setting(key)?;
        v.as_i64().or_else(|| key.default_value().as_i64()).ok_or_else(|| internal(format!("setting {key:?} is not an integer")))
    }

    // -----------------------------------------------------------------------------------------
    // AI job
    // -----------------------------------------------------------------------------------------

    /// `start_ai`: refused with `sidecarUnavailable` unless the sidecar is `ready` (with every task's
    /// capability) or `starting`; runs on a background thread (contract rule 6).
    pub fn start_ai(self: &Arc<Self>, req: StartAiReq, sink: Arc<dyn EventSink>) -> Result<JobStarted, AppError> {
        let tasks = ai::tasks_of(&req.tasks)?;
        ai::check_startable(&self.sidecar, &tasks)?;
        let (job_id, cancel) = self.jobs.begin(JobKind::Ai)?;
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name(format!("ss-ai-{job_id}"))
            .spawn(move || {
                let _ = me.run_ai(job_id, &tasks, req.image_ids.as_deref(), sink.as_ref(), &cancel);
                me.jobs.end(job_id);
            })
            .map_err(|e| internal("cannot start the AI job").with_detail(e.to_string()))?;
        Ok(JobStarted { job_id })
    }

    /// The AI job to completion on the calling thread (`ssctl ai`).
    pub fn ai_blocking(&self, req: StartAiReq, sink: &dyn EventSink) -> Result<AiReport, AppError> {
        let tasks = ai::tasks_of(&req.tasks)?;
        ai::check_startable(&self.sidecar, &tasks)?;
        let (job_id, cancel) = self.jobs.begin(JobKind::Ai)?;
        let r = self.run_ai(job_id, &tasks, req.image_ids.as_deref(), sink, &cancel);
        self.jobs.end(job_id);
        r
    }

    fn run_ai(&self, job_id: i64, tasks: &[AiTask], ids: Option<&[i64]>, sink: &dyn EventSink, cancel: &AtomicBool) -> Result<AiReport, AppError> {
        let run = AiRun { cat: &self.catalog, fa: self.fa.as_ref(), sidecar: &self.sidecar, semantic: &self.semantic, sink, cancel };
        ai::run(&run, job_id, tasks, ids)
    }

    // -----------------------------------------------------------------------------------------
    // Semantic arm of text queries (contract-sidecar-jsonrpc.md § Degradation)
    // -----------------------------------------------------------------------------------------

    /// Scores for `textMode` `semantic` (required: `sidecarUnavailable` without a ready sidecar with
    /// `embed`) or `any` (best effort: only when ready, bounded by `SEMANTIC_QUERY_TIMEOUT_MS`, any
    /// failure = no arm). None for other modes and blank text.
    pub(crate) fn semantic_arm(&self, q: &ImageQuery) -> Result<Option<SemanticScores>, AppError> {
        let Some(text) = q.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
        let strict = match q.text_mode {
            TextMode::Semantic => true,
            TextMode::Any => false,
            TextMode::Filename | TextMode::Ocr => return Ok(None),
        };
        let info = match self.sidecar.info().filter(|i| i.capabilities.contains(&SidecarCapability::Embed)) {
            Some(i) => i,
            None if strict => {
                return Err(AppError::new(ErrorKind::SidecarUnavailable, "semantic search needs the AI sidecar")
                    .with_detail(status_line(&self.sidecar.status())));
            }
            None => return Ok(None),
        };
        let timeout = Duration::from_millis(if strict { SEMANTIC_ONLY_TIMEOUT_MS } else { SEMANTIC_QUERY_TIMEOUT_MS });
        let qv = match self.semantic.embed_text(&self.sidecar, &info.embed_model, text, timeout) {
            Ok(v) => v,
            Err(e) if strict => return Err(e.to_app_error("semantic search")),
            Err(e) => {
                eprintln!("ss-core: semantic arm of an `any` search skipped: {e:?}");
                return Ok(None);
            }
        };
        let (k, min) = if strict { (SEMANTIC_TOP_K, None) } else { (SEMANTIC_ANY_TOP_K, Some(SEMANTIC_ANY_MIN_SCORE)) };
        let scores = self.semantic.top(&self.catalog, &info.embed_model, &qv, k, min)?;
        Ok((strict || !scores.0.is_empty()).then_some(scores))
    }

    // -----------------------------------------------------------------------------------------
    // Smart features (never call the sidecar, except detect_pii)
    // -----------------------------------------------------------------------------------------

    pub fn find_duplicates(&self, req: FindDuplicatesReq) -> Result<DuplicateResult, AppError> {
        let hamming = match req.hamming {
            Some(h) => h,
            None => self.setting_i64(SettingKey::DupHamming)?,
        };
        if !(0..=duplicates::MAX_HAMMING).contains(&hamming) {
            return Err(invalid(format!("hamming must be 0..{}, got {hamming}", duplicates::MAX_HAMMING)));
        }
        if let Some(root) = req.root_id {
            self.catalog.read(|conn| catalog::get_root(conn, root))?;
        }
        self.catalog.read(|conn| {
            let cands = duplicates::load(conn, req.root_id)?;
            duplicates::find(conn, &cands, req.mode, hamming as u32)
        })
    }

    pub fn get_sessions(&self, req: GetSessionsReq) -> Result<SessionsResult, AppError> {
        let gap = match req.gap_minutes {
            Some(g) => g,
            None => self.setting_i64(SettingKey::SessionGapMinutes)?,
        };
        if !(1..=sessions::GAP_MINUTES_MAX).contains(&gap) {
            return Err(invalid(format!("gapMinutes must be 1..{}, got {gap}", sessions::GAP_MINUTES_MAX)));
        }
        let sem = self.semantic_arm(&req.query)?;
        let rows = self.catalog.read(|conn| query::taken_of(conn, &req.query, sem.as_ref()))?;
        Ok(SessionsResult { sessions: sessions::split(rows, gap * 60_000, req.query.sort.dir) })
    }

    /// Scores missing blur values first (from the 320 thumbnails), then lists blurry and tiny images.
    pub fn detect_issues(&self, req: DetectIssuesReq) -> Result<IssuesResult, AppError> {
        if let Some(root) = req.root_id {
            self.catalog.read(|conn| catalog::get_root(conn, root))?;
        }
        issues::backfill_blur(&self.catalog, req.root_id, &AtomicBool::new(false))?;
        let threshold = self.setting(SettingKey::BlurThreshold)?.as_f64().unwrap_or(f64::NAN);
        if !threshold.is_finite() {
            return Err(internal("setting blurThreshold is not a number"));
        }
        let tiny = self.setting_i64(SettingKey::TinyMaxPixels)?;
        self.catalog.read(|conn| issues::detect(conn, req.root_id, threshold, tiny))
    }

    pub fn get_stats(&self) -> Result<Stats, AppError> {
        let hamming = self.setting_i64(SettingKey::DupHamming)?.clamp(0, duplicates::MAX_HAMMING) as u32;
        self.catalog.read(|conn| stats::get_stats(conn, hamming))
    }

    /// `detect_pii`: the image's OCR lines (when an `ocr_result` row exists, else `null` = the sidecar
    /// runs OCR) forwarded to `pii.detect`. An explicit action on one image (rule 9).
    pub fn detect_pii(&self, req: ImageIdReq) -> Result<PiiResult, AppError> {
        let id = req.image_id;
        let (detail, lines_json) = self.catalog.read(|conn| {
            let detail = query::get_image(conn, id)?;
            let lines: Option<String> = rusqlite::OptionalExtension::optional(conn.query_row(
                "SELECT lines_json FROM ocr_result WHERE image_id = ?1",
                [id],
                |r| r.get(0),
            ))?;
            Ok((detail, lines))
        })?;
        let lines = match lines_json {
            Some(j) => Some(
                serde_json::from_str::<Vec<OcrLine>>(&j)
                    .map_err(|e| internal("stored OCR lines are not valid").with_detail(e.to_string()))?
                    .into_iter()
                    .map(|l| PiiLine { text: l.text, bbox: l.bbox })
                    .collect(),
            ),
            None => None,
        };
        let s = self.sidecar.status();
        if !s.capabilities.iter().any(|c| c == "pii") || s.state != SidecarState::Ready {
            return Err(AppError::new(ErrorKind::SidecarUnavailable, "PII detection needs the AI sidecar").with_detail(status_line(&s)));
        }
        let params = PiiDetectParams { image_id: id, path: detail.abs_path, lines };
        let r: PiiDetectResult = self
            .sidecar
            .call(methods::PII_DETECT, &params, Duration::from_secs(DETECT_PII_TIMEOUT_S))
            .typed()
            .map_err(|e| e.to_app_error("PII detection"))?;
        Ok(PiiResult { findings: r.findings })
    }
}
