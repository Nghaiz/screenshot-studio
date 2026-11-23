//! The AI job (`start_ai`): fills `ocr_result` + `text_fts.ocr_text`, `embedding`, `category` and
//! `image.dominant_rgb` through the sidecar, in contract-sized batches.
//!
//! Resumable by construction: each task selects only the images that lack its result for the current
//! model (`ocr_result.engine`, `embedding.model`, `category.model` = the `initialize` embed model;
//! colour: `dominant_rgb IS NULL`), and every batch is committed as it lands. Present, not trashed,
//! newest first; cloud-only files are skipped (checked from metadata again right before each batch,
//! contract-tauri-ipc rule 9). A per-item sidecar error leaves that image without a result (retried
//! next run). A lost / timed-out batch waits for the sidecar to recover and is retried once, then
//! skipped. One batch is in flight per job, so the second sidecar slot stays free for interactive
//! calls (`embed.text`, `pii.detect`).

pub mod semantic;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rusqlite::types::Value as SqlValue;
use rusqlite::{Transaction, params, params_from_iter};
use serde::Serialize;
use serde_json::Value;

use crate::catalog::Catalog;
use crate::contract::ipc::{AiTask, CloudState, JobKind, LibraryChangeReason, LibraryChanged, SidecarState, SidecarStatus};
use crate::contract::sidecar::{self as rpc, OcrEngine, SidecarCapability, error_codes, methods};
use crate::contract::{AppError, ErrorKind};
use crate::errors::{enum_str, invalid, now_ms};
use crate::fsaccess::FileAccess;
use crate::jobs::{EventSink, ProgressReporter};
use crate::scan::cancelled;
use crate::sidecar::config::{AI_BATCH_CLASSIFY, AI_BATCH_COLOR, AI_BATCH_EMBED, AI_BATCH_OCR, CATEGORY_KEEP_MIN_SCORE};
use crate::sidecar::vector::{decode_vector, floats_to_le};
use crate::sidecar::{CallError, Sidecar, SidecarInfo, parse_result, status_line};
use crate::smart::categories;
use crate::{filter, paths};
use semantic::SemanticIndex;

/// The OCR engine asked for in `initialize`; stored as `ocr_result.engine`.
pub const OCR_ENGINE: OcrEngine = OcrEngine::Rapidocr;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskReport {
    pub task: AiTask,
    /// Images that lacked this result.
    pub candidates: i64,
    /// Results written.
    pub done: i64,
    /// Images the sidecar could not process (per-item error or a skipped batch).
    pub failed: i64,
    /// Cloud-only images not sent.
    pub skipped_cloud: i64,
}

/// What `ssctl ai` prints.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReport {
    pub tasks: Vec<AiTaskReport>,
    pub sidecar: SidecarStatus,
    pub elapsed_ms: i64,
}

/// Request tasks, deduplicated in request order; at least one.
pub fn tasks_of(tasks: &[AiTask]) -> Result<Vec<AiTask>, AppError> {
    let mut out: Vec<AiTask> = Vec::with_capacity(tasks.len());
    for t in tasks {
        if !out.contains(t) {
            out.push(*t);
        }
    }
    if out.is_empty() {
        return Err(invalid("start_ai needs at least one task"));
    }
    Ok(out)
}

pub fn capability_of(task: AiTask) -> SidecarCapability {
    match task {
        AiTask::Ocr => SidecarCapability::Ocr,
        AiTask::Embed => SidecarCapability::Embed,
        AiTask::Classify => SidecarCapability::Classify,
        AiTask::Color => SidecarCapability::Color,
    }
}

fn unavailable(what: &str, s: &SidecarStatus) -> AppError {
    AppError::new(ErrorKind::SidecarUnavailable, format!("{what} needs the AI sidecar")).with_detail(status_line(s))
}

/// `start_ai` refuses at once unless the sidecar is `ready` with every task's capability, or
/// `starting` (the job then waits for it).
pub fn check_startable(sidecar: &Sidecar, tasks: &[AiTask]) -> Result<(), AppError> {
    let s = sidecar.status();
    match s.state {
        SidecarState::Starting => Ok(()),
        SidecarState::Ready => check_capabilities(&s, tasks),
        _ => Err(unavailable("the AI job", &s)),
    }
}

fn check_capabilities(s: &SidecarStatus, tasks: &[AiTask]) -> Result<(), AppError> {
    let missing: Vec<String> =
        tasks.iter().map(|t| enum_str(&capability_of(*t))).filter(|c| !s.capabilities.contains(c)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(AppError::new(ErrorKind::SidecarUnavailable, format!("the sidecar has no capability {}", missing.join(", ")))
        .with_detail(status_line(s)))
}

/// Everything one run needs.
pub struct AiRun<'a> {
    pub cat: &'a Catalog,
    pub fa: &'a dyn FileAccess,
    pub sidecar: &'a Sidecar,
    pub semantic: &'a SemanticIndex,
    pub sink: &'a dyn EventSink,
    pub cancel: &'a AtomicBool,
}

struct Item {
    id: i64,
    path: String,
    cloud: bool,
}

/// Runs the job to the end on the calling thread and reports `job-progress` (kind `ai`).
pub fn run(r: &AiRun<'_>, job_id: i64, tasks: &[AiTask], image_ids: Option<&[i64]>) -> Result<AiReport, AppError> {
    let reporter = ProgressReporter::new(r.sink, job_id, JobKind::Ai);
    let started = Instant::now();
    let mut done_total = 0;
    let mut total = 0;
    let result = (|| {
        let status = r.sidecar.wait_settled();
        if status.state != SidecarState::Ready {
            return Err(unavailable("the AI job", &status));
        }
        check_capabilities(&status, tasks)?;
        let info = r.sidecar.info().ok_or_else(|| unavailable("the AI job", &r.sidecar.status()))?;
        let mut plans = Vec::new();
        for &task in tasks {
            let items = r.cat.read(|conn| candidates(conn, task, &info, image_ids))?;
            total += items.iter().filter(|i| !i.cloud).count() as i64;
            plans.push((task, items));
        }
        let mut reports = Vec::new();
        for (task, items) in plans {
            reports.push(run_task(r, &reporter, &info, task, items, &mut done_total, total)?);
        }
        Ok(reports)
    })();
    let report = |tasks| AiReport { tasks, sidecar: r.sidecar.status(), elapsed_ms: started.elapsed().as_millis() as i64 };
    match result {
        Ok(tasks) => {
            reporter.finish("done", done_total, total, None);
            Ok(report(tasks))
        }
        Err(e) => {
            reporter.finish("done", done_total, total, Some(e.clone()));
            Err(e)
        }
    }
}

fn candidates(
    conn: &rusqlite::Connection,
    task: AiTask,
    info: &SidecarInfo,
    image_ids: Option<&[i64]>,
) -> Result<Vec<Item>, AppError> {
    let mut params: Vec<SqlValue> = Vec::new();
    let lacking = match task {
        AiTask::Ocr => {
            params.push(enum_str(&OCR_ENGINE).into());
            "NOT EXISTS (SELECT 1 FROM ocr_result o WHERE o.image_id = i.id AND o.engine = ?)"
        }
        AiTask::Embed => {
            params.push(info.embed_model.clone().into());
            "NOT EXISTS (SELECT 1 FROM embedding e WHERE e.image_id = i.id AND e.model = ?)"
        }
        AiTask::Classify => {
            params.push(info.embed_model.clone().into());
            "NOT EXISTS (SELECT 1 FROM category c WHERE c.image_id = i.id AND c.model = ?)"
        }
        AiTask::Color => "i.dominant_rgb IS NULL",
    };
    let mut sql = format!(
        "SELECT i.id, r.path, i.folder, i.file_name, i.cloud_state = 'cloudOnly' FROM image i
         JOIN library_root r ON r.id = i.root_id WHERE i.missing = 0 AND i.trashed_at IS NULL AND {lacking}"
    );
    if let Some(ids) = image_ids {
        sql.push_str(" AND i.id IN (SELECT value FROM json_each(?))");
        params.push(serde_json::to_string(ids).unwrap_or_else(|_| "[]".into()).into());
    }
    sql.push_str(" ORDER BY i.taken_ms DESC, i.id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&params), |row| {
        let (root, folder, name): (String, String, String) = (row.get(1)?, row.get(2)?, row.get(3)?);
        Ok(Item {
            id: row.get(0)?,
            path: paths::image_abs_path(&root, &folder, &name).to_string_lossy().into_owned(),
            cloud: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn batch_size(task: AiTask) -> usize {
    match task {
        AiTask::Ocr => AI_BATCH_OCR,
        AiTask::Embed => AI_BATCH_EMBED,
        AiTask::Classify => AI_BATCH_CLASSIFY,
        AiTask::Color => AI_BATCH_COLOR,
    }
}

fn method_of(task: AiTask) -> &'static str {
    match task {
        AiTask::Ocr => methods::OCR_RECOGNIZE,
        AiTask::Embed => methods::EMBED_IMAGES,
        AiTask::Classify => methods::CLASSIFY_ZERO_SHOT,
        AiTask::Color => methods::COLOR_DOMINANT,
    }
}

fn run_task(
    r: &AiRun<'_>,
    reporter: &ProgressReporter<'_>,
    info: &SidecarInfo,
    task: AiTask,
    items: Vec<Item>,
    done_total: &mut i64,
    total: i64,
) -> Result<AiTaskReport, AppError> {
    let stage = enum_str(&task);
    let mut rep = AiTaskReport { task, candidates: items.len() as i64, done: 0, failed: 0, skipped_cloud: 0 };
    let (cloud, local): (Vec<Item>, Vec<Item>) = items.into_iter().partition(|i| i.cloud);
    rep.skipped_cloud = cloud.len() as i64;
    for chunk in local.chunks(batch_size(task)) {
        if r.cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        // Metadata only, right before sending: a file that went cloud-only since the index is not sent.
        let send: Vec<&Item> = chunk.iter().filter(|i| !now_cloud_only(r.fa, &i.path)).collect();
        let late_cloud = (chunk.len() - send.len()) as i64;
        rep.skipped_cloud += late_cloud;
        *done_total += late_cloud;
        if send.is_empty() {
            continue;
        }
        let image_items: Vec<rpc::ImageItem> = send.iter().map(|i| rpc::ImageItem { image_id: i.id, path: i.path.clone() }).collect();
        let params = match task {
            AiTask::Ocr => serde_json::to_value(rpc::OcrRecognizeParams { items: image_items }),
            AiTask::Embed => serde_json::to_value(rpc::EmbedImagesParams { items: image_items }),
            AiTask::Classify => serde_json::to_value(rpc::ClassifyZeroShotParams { items: image_items, labels: categories::labels() }),
            AiTask::Color => serde_json::to_value(rpc::ColorDominantParams { items: image_items }),
        }
        .map_err(|e| AppError::new(ErrorKind::Internal, "serialize AI params").with_detail(e.to_string()))?;
        let base = *done_total;
        let first_path = send[0].path.clone();
        let on_progress = |d: i64| reporter.report(&stage, base + d, total, Some(&first_path));
        let sent_ids: Vec<i64> = send.iter().map(|i| i.id).collect();
        let (ok, failed) = match call_batch(r, method_of(task), &params, &on_progress)? {
            Some(v) => store(r, info, task, v, &sent_ids)?,
            None => (Vec::new(), sent_ids.len() as i64),
        };
        rep.done += ok.len() as i64;
        rep.failed += failed;
        *done_total += sent_ids.len() as i64;
        if task == AiTask::Embed && !ok.is_empty() {
            r.semantic.invalidate();
        }
        if !ok.is_empty() {
            r.sink.library_changed(&LibraryChanged { reason: LibraryChangeReason::Ai, image_ids: Some(ok) });
        }
        reporter.report(&stage, *done_total, total, Some(&first_path));
    }
    Ok(rep)
}

fn now_cloud_only(fa: &dyn FileAccess, path: &str) -> bool {
    let p = std::path::Path::new(path);
    std::fs::symlink_metadata(p).is_ok_and(|m| fa.cloud_state(p, &m) == CloudState::CloudOnly)
}

/// One batch request. `Ok(None)`: the batch was given up after a retry (its images count as failed).
fn call_batch(r: &AiRun<'_>, method: &'static str, params: &Value, on_progress: &dyn Fn(i64)) -> Result<Option<Value>, AppError> {
    let timeout = r.sidecar.timings().request_timeout;
    for attempt in 0..2 {
        let outcome = r.sidecar.call(method, params, timeout).wait_cancellable(r.cancel, on_progress);
        let err = match outcome {
            Ok(v) => return Ok(Some(v)),
            Err(e) => e,
        };
        if r.cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        match &err {
            CallError::Lost(_) | CallError::Timeout | CallError::Unavailable(_) => {
                let s = r.sidecar.wait_settled();
                if s.state != SidecarState::Ready {
                    return Err(err.to_app_error("the AI job").with_detail(status_line(&s)));
                }
                eprintln!("ss-core ai: {method} batch failed ({err:?}), attempt {}", attempt + 1);
            }
            CallError::Rpc(e) if e.code == error_codes::CANCELLED => return Err(cancelled()),
            _ => return Err(err.to_app_error("the AI job")),
        }
    }
    Ok(None)
}

/// Writes one batch result; returns the ids written and the number of failed items.
fn store(r: &AiRun<'_>, info: &SidecarInfo, task: AiTask, v: Value, sent: &[i64]) -> Result<(Vec<i64>, i64), AppError> {
    let bad = |e: CallError| e.to_app_error("the AI job");
    let sent_set: HashSet<i64> = sent.iter().copied().collect();
    let model = info.embed_model.clone();
    let now = now_ms();
    let written = match task {
        AiTask::Ocr => {
            let res: rpc::OcrRecognizeResult = parse_result(v).map_err(bad)?;
            let rows: Vec<rpc::OcrItemResult> =
                res.results.into_iter().filter(|x| sent_set.contains(&x.image_id) && log_item(task, x.image_id, x.error.as_ref())).collect();
            r.cat.write(move |tx| write_rows(tx, rows, |tx, x| write_ocr(tx, x, now)))?
        }
        AiTask::Embed => {
            let res: rpc::EmbedImagesResult = parse_result(v).map_err(bad)?;
            let dim = usize::try_from(res.dim).unwrap_or(0);
            let mut rows = Vec::new();
            for x in res.results.into_iter().filter(|x| sent_set.contains(&x.image_id) && log_item(task, x.image_id, x.error.as_ref())) {
                match x.vector.as_deref().map(|b| decode_vector(b, dim)) {
                    Some(Ok(vec)) => rows.push((x.image_id, floats_to_le(&vec))),
                    Some(Err(e)) => eprintln!("ss-core ai: embed {}: {e}", x.image_id),
                    None => eprintln!("ss-core ai: embed {}: no vector", x.image_id),
                }
            }
            let (m, d) = (res.model, res.dim);
            r.cat.write(move |tx| {
                write_rows(tx, rows, |tx, (id, blob)| {
                    tx.prepare_cached(
                        "INSERT INTO embedding (image_id, model, dim, vec, done_at) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(image_id) DO UPDATE SET model = excluded.model, dim = excluded.dim, vec = excluded.vec,
                           done_at = excluded.done_at",
                    )?
                    .execute(params![id, m, d, blob, now])?;
                    Ok(id)
                })
            })?
        }
        AiTask::Classify => {
            let res: rpc::ClassifyZeroShotResult = parse_result(v).map_err(bad)?;
            let rows: Vec<rpc::ClassifyItemResult> = res
                .results
                .into_iter()
                .filter(|x| sent_set.contains(&x.image_id) && log_item(task, x.image_id, x.error.as_ref()) && !x.scores.is_empty())
                .collect();
            r.cat.write(move |tx| write_rows(tx, rows, |tx, x| write_categories(tx, x, &model)))?
        }
        AiTask::Color => {
            let res: rpc::ColorDominantResult = parse_result(v).map_err(bad)?;
            let mut rows = Vec::new();
            for x in res.results.into_iter().filter(|x| sent_set.contains(&x.image_id) && log_item(task, x.image_id, x.error.as_ref())) {
                match x.rgb.as_deref().filter(|c| filter::parse_rgb(c).is_some()) {
                    Some(c) => rows.push((x.image_id, c.to_ascii_uppercase())),
                    None => eprintln!("ss-core ai: color {}: no #RRGGBB in {:?}", x.image_id, x.rgb),
                }
            }
            r.cat.write(move |tx| {
                write_rows(tx, rows, |tx, (id, rgb)| {
                    tx.prepare_cached("UPDATE image SET dominant_rgb = ?2 WHERE id = ?1")?.execute(params![id, rgb])?;
                    Ok(id)
                })
            })?
        }
    };
    let failed = sent.len() as i64 - written.len() as i64;
    Ok((written, failed))
}

fn log_item(task: AiTask, id: i64, err: Option<&rpc::RpcItemError>) -> bool {
    match err {
        Some(e) => {
            eprintln!("ss-core ai: {} {id}: {:?}: {}", enum_str(&task), e.code, e.message);
            false
        }
        None => true,
    }
}

/// Writes each row whose image still exists (it may have been purged meanwhile); returns their ids.
fn write_rows<T>(
    tx: &Transaction<'_>,
    rows: Vec<T>,
    mut one: impl FnMut(&Transaction<'_>, T) -> Result<i64, AppError>,
) -> Result<Vec<i64>, AppError>
where
    T: HasId,
{
    let mut ids = Vec::new();
    for row in rows {
        let exists: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM image WHERE id = ?1)", [row.id()], |r| r.get(0))?;
        if exists {
            ids.push(one(tx, row)?);
        }
    }
    Ok(ids)
}

trait HasId {
    fn id(&self) -> i64;
}
impl HasId for rpc::OcrItemResult {
    fn id(&self) -> i64 {
        self.image_id
    }
}
impl HasId for rpc::ClassifyItemResult {
    fn id(&self) -> i64 {
        self.image_id
    }
}
impl<T> HasId for (i64, T) {
    fn id(&self) -> i64 {
        self.0
    }
}

fn write_ocr(tx: &Transaction<'_>, x: rpc::OcrItemResult, now: i64) -> Result<i64, AppError> {
    let lines = serde_json::to_string(&x.lines).map_err(|e| AppError::new(ErrorKind::Internal, "serialize OCR lines").with_detail(e.to_string()))?;
    tx.prepare_cached(
        "INSERT INTO ocr_result (image_id, text, lines_json, engine, done_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(image_id) DO UPDATE SET text = excluded.text, lines_json = excluded.lines_json,
           engine = excluded.engine, done_at = excluded.done_at",
    )?
    .execute(params![x.image_id, x.text, lines, enum_str(&OCR_ENGINE), now])?;
    crate::ops::organize::fts_set(tx, x.image_id, "ocr_text", &filter::fts_ocr_text(&x.text))?;
    Ok(x.image_id)
}

/// The best label always, the others at or above [`CATEGORY_KEEP_MIN_SCORE`].
fn write_categories(tx: &Transaction<'_>, x: rpc::ClassifyItemResult, model: &str) -> Result<i64, AppError> {
    let mut scores = x.scores;
    scores.sort_by(|a, b| b.score.total_cmp(&a.score));
    tx.prepare_cached("DELETE FROM category WHERE image_id = ?1")?.execute([x.image_id])?;
    let mut ins = tx.prepare_cached("INSERT INTO category (image_id, label, score, model) VALUES (?1, ?2, ?3, ?4)")?;
    for (n, s) in scores.iter().enumerate() {
        if n == 0 || s.score >= CATEGORY_KEEP_MIN_SCORE {
            ins.execute(params![x.image_id, s.label, s.score, model])?;
        }
    }
    Ok(x.image_id)
}
