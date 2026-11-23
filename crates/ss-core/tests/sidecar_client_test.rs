//! Phase 3.2: the sidecar supervisor against `fake_sidecar.py` (stdlib-only Python speaking the contract,
//! answers built from `contracts/fixtures/sidecar/`): ready path (AI job, semantic search, PII), absent venv
//! (every non-AI command still works), crash -> backoff -> `error`, timeout -> `$/cancel`, garbage on stdout,
//! at most two requests in flight with a chatty child. Needs Python 3 on PATH; without it every test FAILS
//! with that message (never a silent skip).

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use common::env::Env;
use serde_json::{Value, json};
use ss_core::contract::ipc::*;
use ss_core::contract::sidecar::{EmbedTextParams, EmbedTextResult, error_codes, methods};
use ss_core::contract::{Caller, ErrorKind};
use ss_core::jobs::EventSink;
use ss_core::sidecar::{CallError, SidecarLaunch, SidecarTimings};
use ss_core::smart::categories::LABELS;
use ss_core::{AppCore, query};
use tempfile::TempDir;

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

/// The Python on PATH (`python`, else `python3`). Panics with a clear message when there is none.
fn python() -> &'static str {
    static FOUND: OnceLock<Result<&'static str, String>> = OnceLock::new();
    let found = FOUND.get_or_init(|| {
        let mut tried = Vec::new();
        for cand in ["python", "python3"] {
            match std::process::Command::new(cand).arg("--version").output() {
                Ok(o) if o.status.success() => return Ok(cand),
                Ok(o) => tried.push(format!("`{cand} --version` exited {:?}", o.status.code())),
                Err(e) => tried.push(format!("`{cand}`: {e}")),
            }
        }
        Err(tried.join("; "))
    });
    match found {
        Ok(p) => p,
        Err(why) => panic!("the sidecar tests need Python 3 on PATH to run fake_sidecar.py, and none ran: {why}"),
    }
}

struct Fake {
    dir: TempDir,
}

impl Fake {
    fn new(mode: &str) -> Self {
        let f = Fake { dir: tempfile::tempdir().expect("fake dir") };
        f.set_mode(mode);
        f
    }

    fn set_mode(&self, mode: &str) {
        std::fs::write(self.dir.path().join("mode"), mode).expect("mode file");
    }

    fn launch(&self) -> SidecarLaunch {
        let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
        SidecarLaunch {
            program: python().into(),
            args: vec![tests.join("fake_sidecar.py").into_os_string()],
            cwd: tests,
            env: vec![
                ("FAKE_SIDECAR_DIR".into(), self.dir.path().as_os_str().to_owned()),
                ("PYTHONUTF8".into(), "1".into()),
                ("PYTHONUNBUFFERED".into(), "1".into()),
            ],
        }
    }

    fn log(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.path().join("log.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).expect("log line"))
            .collect()
    }

    fn events(&self, kind: &str) -> Vec<Value> {
        self.log().into_iter().filter(|e| e["event"] == kind).collect()
    }

    fn requests(&self, method: &str) -> Vec<Value> {
        self.events("request").into_iter().filter(|e| e["method"] == method).collect()
    }
}

fn timings() -> SidecarTimings {
    SidecarTimings {
        init_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(20),
        cancel_grace: Duration::from_millis(800),
        shutdown_wait: Duration::from_secs(2),
        restart_backoff: vec![Duration::from_millis(150), Duration::from_millis(300), Duration::from_millis(450)],
        crash_limit: 3,
        crash_window: Duration::from_secs(600),
        exit_grace: Duration::from_secs(2),
    }
}

/// Every status the supervisor published, in order.
fn record_statuses(core: &AppCore) -> Arc<Mutex<Vec<SidecarStatus>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    core.sidecar().set_listener(move |s| s2.lock().unwrap().push(s.clone()));
    seen
}

fn start_fake(env: &Env, fake: &Fake) -> SidecarStatus {
    env.core.sidecar().set_timings(timings());
    env.core.start_sidecar_with(fake.launch()).expect("start");
    env.core.sidecar().wait_settled()
}

fn wait_state(core: &AppCore, state: SidecarState, secs: u64) -> SidecarStatus {
    let s = core.sidecar().wait_until(|s| s.state == state, Duration::from_secs(secs));
    assert_eq!(s.state, state, "waited {secs} s for {state:?}: {s:?}");
    s
}

#[derive(Default)]
struct Rec {
    progress: Mutex<Vec<JobProgress>>,
    changed: Mutex<Vec<LibraryChanged>>,
}

impl EventSink for Rec {
    fn job_progress(&self, p: &JobProgress) {
        self.progress.lock().unwrap().push(p.clone());
    }
    fn library_changed(&self, c: &LibraryChanged) {
        self.changed.lock().unwrap().push(c.clone());
    }
}

impl Rec {
    fn finished(&self, job_id: i64, secs: u64) -> JobProgress {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            if let Some(p) = self.progress.lock().unwrap().iter().find(|p| p.job_id == job_id && p.finished) {
                return p.clone();
            }
            assert!(Instant::now() < end, "job {job_id} did not finish within {secs} s");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn all_tasks() -> Vec<AiTask> {
    vec![AiTask::Ocr, AiTask::Embed, AiTask::Classify, AiTask::Color]
}

fn text_query(text: &str, mode: TextMode) -> ImageQuery {
    let mut q = query::default_query(500);
    q.text = Some(text.into());
    q.text_mode = mode;
    q.sort = SortSpec { field: SortField::Relevance, dir: SortDir::Desc };
    q
}

fn ids(page: &ImagePage) -> Vec<i64> {
    page.items.iter().map(|s| s.id).collect()
}

/// Copies fixture `src` under a new name in the root (a valid PNG the scanner indexes).
fn plant_copy(env: &Env, src: usize, name: &str) {
    std::fs::copy(&env.fixtures[src].path, env.root_dir().join(name)).expect("plant copy");
}

// ---------------------------------------------------------------------------------------------
// Ready path
// ---------------------------------------------------------------------------------------------

#[test]
fn ready_path_ai_job_semantic_search_and_pii() {
    let env = Env::new(12);
    plant_copy(&env, 3, "receipt.png");
    std::fs::write(env.root_dir().join("broken.png"), b"not an image").unwrap();
    env.index(true);
    let fake = Fake::new("ok");
    let seen = record_statuses(&env.core);

    let st = start_fake(&env, &fake);
    assert_eq!(st.state, SidecarState::Ready, "{st:?}");
    assert_eq!(st.capabilities, ["ocr", "embed", "classify", "color", "pii"]);
    assert_eq!((st.device, st.version.as_deref()), (Some(ComputeDevice::Cuda), Some("0.1.0")));
    assert_eq!(env.core.get_app_info().unwrap().sidecar, st);
    assert_eq!(env.core.get_sidecar_status(), st);
    let states: Vec<SidecarState> = seen.lock().unwrap().iter().map(|s| s.state).collect();
    assert_eq!(states, [SidecarState::Starting, SidecarState::Ready], "sidecar-status events");
    let init = &fake.requests("initialize")[0]["params"];
    assert_eq!(init["ocrEngine"], "rapidocr");
    assert_eq!(init["languages"], json!(["vi", "en"]));
    assert_eq!(init["protocolVersion"], 1);

    // The AI job fills every table, skipping the unreadable file.
    let all = env.all();
    let n = all.len() as i64;
    let broken = all.iter().find(|s| s.file_name == "broken.png").unwrap().id;
    let rec = Arc::new(Rec::default());
    let job = env.core.start_ai(StartAiReq { tasks: all_tasks(), image_ids: None }, rec.clone()).unwrap();
    let fin = rec.finished(job.job_id, 60);
    assert!(fin.error.is_none(), "{fin:?}");
    assert_eq!((fin.kind, fin.done, fin.total), (JobKind::Ai, 4 * n, 4 * n));
    assert!(rec.changed.lock().unwrap().iter().all(|c| c.reason == LibraryChangeReason::Ai));
    assert_eq!(env.sql_i64("SELECT count(*) FROM ocr_result WHERE engine = 'rapidocr'"), n - 1);
    assert_eq!(env.sql_i64("SELECT count(*) FROM embedding WHERE dim = 8"), n - 1);
    assert_eq!(env.sql_i64("SELECT count(*) FROM image WHERE dominant_rgb IS NOT NULL"), n - 1);
    assert_eq!(env.sql_i64(&format!("SELECT count(*) FROM ocr_result WHERE image_id = {broken}")), 0);
    for s in all.iter().filter(|s| s.id != broken) {
        let labels: Vec<String> = {
            let id = s.id;
            env.core
                .catalog()
                .read(move |c| {
                    let mut st = c.prepare("SELECT label FROM category WHERE image_id = ?1")?;
                    Ok(st.query_map([id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?)
                })
                .unwrap()
        };
        assert_eq!(labels, [LABELS[s.id as usize % LABELS.len()]], "image {} keeps only its best label", s.id);
    }

    // OCR text is searchable; `đ` folds for FTS while the stored text stays verbatim. The target has the
    // largest two-digit id, so the prefix query `code<id>*` matches no other image.
    let target = all.iter().filter(|s| s.id != broken && s.id >= 10).map(|s| s.id).max().unwrap();
    let page = env.core.query_images(&text_query(&format!("code{target}"), TextMode::Ocr)).unwrap();
    assert_eq!(ids(&page), [target]);
    let receipt = all.iter().find(|s| s.file_name == "receipt.png").unwrap().id;
    let page = env.core.query_images(&text_query("hoa don", TextMode::Ocr)).unwrap();
    assert_eq!(ids(&page), [receipt], "đ folded in the FTS copy");
    assert!(env.detail(receipt).ocr_text.unwrap().contains("Hoá đơn tiền điện"));
    let mut has_text = query::default_query(500);
    has_text.has_text = Some(true);
    assert_eq!(env.all_of(has_text).len() as i64, n - 1);

    // Semantic: ranked by cosine. `near:k` matches the images with id % 8 == k.
    let k = target % 8;
    let started = Instant::now();
    let page = env.core.query_images(&text_query(&format!("near:{k}"), TextMode::Semantic)).unwrap();
    println!("semantic query over {} embeddings: {:?}", n - 1, started.elapsed());
    let want: BTreeSet<i64> = all.iter().filter(|s| s.id != broken && s.id % 8 == k).map(|s| s.id).collect();
    assert_eq!(page.total, n - 1, "semantic = every embedded image, ranked");
    let head: BTreeSet<i64> = ids(&page)[..want.len()].iter().copied().collect();
    assert_eq!(head, want, "the closest images come first");

    // Any: FTS hit first, then semantic-only hits above the minimum score, no duplicates.
    let page = env.core.query_images(&text_query(&format!("code{target}"), TextMode::Any)).unwrap();
    let got = ids(&page);
    assert_eq!(got[0], target, "FTS hit first");
    assert_eq!(got.iter().copied().collect::<BTreeSet<_>>(), want, "FTS hit + its semantic neighbours, once each");
    assert_eq!(got.len(), want.len());
    let mut paged = Vec::new();
    let mut q = text_query(&format!("code{target}"), TextMode::Any);
    q.limit = 1;
    loop {
        let p = env.core.query_images(&q).unwrap();
        paged.extend(ids(&p));
        match p.next_cursor {
            Some(c) => q.cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(paged, got, "paging the merged result");
    let tl = env.core.get_timeline(&GetTimelineReq { query: text_query(&format!("near:{k}"), TextMode::Semantic), bucket: TimelineBucketSize::Month }).unwrap();
    assert_eq!(tl.buckets.iter().map(|b| b.count).sum::<i64>(), n - 1);
    let ss = env.core.get_sessions(GetSessionsReq { query: text_query(&format!("near:{k}"), TextMode::Any), gap_minutes: None }).unwrap();
    assert_eq!(ss.sessions.iter().flat_map(|s| s.image_ids.clone()).collect::<BTreeSet<_>>(), want);

    // PII: stored OCR lines are forwarded; without an OCR row the sidecar gets `lines: null`.
    let pii = env.core.detect_pii(ImageIdReq { image_id: receipt }).unwrap();
    assert_eq!(pii.findings.len(), 1);
    assert_eq!((pii.findings[0].kind, pii.findings[0].text.as_str()), (PiiKind::Email, "someone@example.com"));
    let sent = fake.requests("pii.detect");
    assert!(sent[0]["params"]["lines"].is_array());
    env.core.detect_pii(ImageIdReq { image_id: broken }).unwrap();
    assert!(fake.requests("pii.detect")[1]["params"]["lines"].is_null());

    // Resumable: a second run asks only for what is still missing (the unreadable file).
    let ocr_before = fake.requests("ocr.recognize").len();
    let report = env.core.ai_blocking(StartAiReq { tasks: all_tasks(), image_ids: None }, &Rec::default()).unwrap();
    for t in &report.tasks {
        assert_eq!((t.candidates, t.done, t.failed), (1, 0, 1), "{t:?}");
    }
    let again = fake.requests("ocr.recognize");
    assert_eq!(again.len(), ocr_before + 1);
    assert_eq!(again.last().unwrap()["params"]["items"][0]["imageId"], broken);
    let only = env.core.ai_blocking(StartAiReq { tasks: vec![AiTask::Color], image_ids: Some(vec![target]) }, &Rec::default()).unwrap();
    assert_eq!(only.tasks[0].candidates, 0, "imageIds narrows the selection");

    // restart_sidecar: a fresh process, ready again.
    let starts = fake.events("start").len();
    env.core.restart_sidecar().unwrap();
    wait_state(&env.core, SidecarState::Ready, 30);
    assert_eq!(fake.events("start").len(), starts + 1);
    env.core.shutdown_sidecar();
    assert_eq!(env.core.get_sidecar_status().state, SidecarState::Absent);
    assert_eq!(fake.events("shutdown").len(), 1, "shutdown request sent, the process exited by itself");
}

// ---------------------------------------------------------------------------------------------
// Absent venv
// ---------------------------------------------------------------------------------------------

#[test]
fn absent_venv_every_non_ai_command_works() {
    let env = Env::new(30);
    plant_copy(&env, 5, "copy of five.png");
    env.index(true);
    let c = &env.core;
    let no_venv = env.data.path().join("no-venv");
    c.set_setting(Caller::Ui, SetSettingReq { key: SettingKey::SidecarVenv, value: json!(no_venv.to_string_lossy()) }).unwrap();
    let st = c.start_sidecar().unwrap();
    assert_eq!(st.state, SidecarState::Absent);
    assert!(st.detail.as_deref().unwrap().contains("no Python"), "{st:?}");
    assert_eq!(c.get_app_info().unwrap().sidecar.state, SidecarState::Absent);
    assert_eq!(c.restart_sidecar().unwrap().state, SidecarState::Absent);

    // AI commands: sidecarUnavailable.
    let ai = StartAiReq { tasks: vec![AiTask::Ocr], image_ids: None };
    assert_eq!(c.start_ai(ai.clone(), Arc::new(Rec::default())).unwrap_err().kind, ErrorKind::SidecarUnavailable);
    assert_eq!(c.ai_blocking(ai, &Rec::default()).unwrap_err().kind, ErrorKind::SidecarUnavailable);
    assert_eq!(c.query_images(&text_query("cat", TextMode::Semantic)).unwrap_err().kind, ErrorKind::SidecarUnavailable);
    let id0 = env.all()[0].id;
    assert_eq!(c.detect_pii(ImageIdReq { image_id: id0 }).unwrap_err().kind, ErrorKind::SidecarUnavailable);

    // Every other command works.
    let by_name = c.query_images(&text_query("copy", TextMode::Filename)).unwrap();
    let any = c.query_images(&text_query("copy", TextMode::Any)).unwrap();
    assert_eq!(ids(&any), ids(&by_name), "`any` = FTS alone without the sidecar");
    assert_eq!(any.total, 1);
    assert_eq!(c.query_images(&query::default_query(500)).unwrap().total, 31);
    c.get_image(GetImageReq { id: id0 }).unwrap();
    c.get_timeline(&GetTimelineReq { query: text_query("screenshot", TextMode::Any), bucket: TimelineBucketSize::Day }).unwrap();
    assert_eq!(c.get_sessions(GetSessionsReq { query: query::default_query(500), gap_minutes: Some(30) }).unwrap().sessions.iter().map(|s| s.image_ids.len()).sum::<usize>(), 31);
    let exact = c.find_duplicates(FindDuplicatesReq { mode: DuplicateMode::Exact, hamming: None, root_id: None }).unwrap();
    assert_eq!(exact.groups.len(), 1);
    c.find_duplicates(FindDuplicatesReq { mode: DuplicateMode::Similar, hamming: Some(4), root_id: Some(env.root.id) }).unwrap();
    c.detect_issues(DetectIssuesReq { root_id: None }).unwrap();
    let stats = c.get_stats().unwrap();
    assert_eq!(stats.counts, c.get_counts().unwrap());
    assert_eq!(stats.duplicates_exact, 1);
    c.get_settings().unwrap();
    c.list_journal(LimitReq { limit: 10 }).unwrap();
    let tag = c.create_tag(Caller::Ui, CreateTagReq { name: "t".into(), color: "#102030".into() }).unwrap();
    c.set_tags(Caller::Ui, SetTagsReq { image_ids: vec![id0], add: vec![tag.id], remove: vec![] }).unwrap();
    assert_eq!(c.list_tags().unwrap()[0].count, 1);
    let r = c.set_favorite(Caller::Ui, SetFavoriteReq { image_ids: vec![id0], value: true }).unwrap();
    c.trash_images(Caller::Ui, ImageIdsReq { image_ids: vec![id0] }).unwrap();
    c.restore_images(Caller::Ui, ImageIdsReq { image_ids: vec![id0] }).unwrap();
    assert!(r.batch_id.is_some());
    assert_eq!(c.undo(Caller::Ui, UndoReq { batch_id: None }).unwrap().affected, 1, "undo the restore");
    c.list_albums().unwrap();
    c.list_smart_filters().unwrap();
    let saved = c.save_smart_filter(Caller::Ui, SaveSmartFilterReq { id: None, name: "cats".into(), query: text_query("cat", TextMode::Semantic) });
    assert!(saved.is_ok(), "a semantic smart filter can be saved without the sidecar: {saved:?}");
    assert!(c.ssimg(&format!("/thumb/{id0}/320")).status == 200);
}

// ---------------------------------------------------------------------------------------------
// Crash policy
// ---------------------------------------------------------------------------------------------

#[test]
fn crash_restarts_with_backoff_then_error_after_three() {
    let env = Env::new(2);
    let fake = Fake::new("crash-after-init");
    let seen = record_statuses(&env.core);
    env.core.sidecar().set_timings(timings());
    let started = Instant::now();
    env.core.start_sidecar_with(fake.launch()).unwrap();
    let st = wait_state(&env.core, SidecarState::Error, 60);
    let elapsed = started.elapsed();
    assert!(st.detail.as_deref().unwrap().contains("3 crashes"), "{st:?}");
    assert!(st.detail.as_deref().unwrap().contains("exit code 4"), "{st:?}");
    assert_eq!(fake.events("start").len(), 3, "first start + 2 restarts, then error");
    assert!(elapsed >= Duration::from_millis(450), "backoff 150 ms + 300 ms before the third start: {elapsed:?}");
    let restarting: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.state == SidecarState::Starting)
        .filter_map(|s| s.detail.clone())
        .filter(|d| d.contains("restarting in"))
        .collect();
    assert_eq!(restarting.len(), 2, "{restarting:?}");
    assert!(restarting[0].contains("150ms") && restarting[1].contains("300ms"), "{restarting:?}");
    assert_eq!(seen.lock().unwrap().iter().filter(|s| s.state == SidecarState::Ready).count(), 3);

    // Stays `error` (no more restarts) until restart_sidecar.
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(fake.events("start").len(), 3);
    let ai = StartAiReq { tasks: vec![AiTask::Embed], image_ids: None };
    assert_eq!(env.core.start_ai(ai, Arc::new(Rec::default())).unwrap_err().kind, ErrorKind::SidecarUnavailable);
    fake.set_mode("ok");
    env.core.restart_sidecar().unwrap();
    wait_state(&env.core, SidecarState::Ready, 30);
    assert_eq!(fake.events("start").len(), 4);

    // A process that dies before `initialize` is a crash too; spawn failure is `error` at once.
    fake.set_mode("crash-on-start");
    env.core.restart_sidecar().unwrap();
    let st = wait_state(&env.core, SidecarState::Error, 60);
    assert!(st.detail.as_deref().unwrap().contains("exit code 3"), "{st:?}");
    let mut bad = fake.launch();
    bad.program = env.data.path().join("no-such-python.exe");
    env.core.start_sidecar_with(bad).unwrap();
    let st = wait_state(&env.core, SidecarState::Error, 10);
    assert!(st.detail.as_deref().unwrap().contains("cannot start"), "{st:?}");
    // `initialize` answered with an error: `error` with the sidecar's message, no restart.
    fake.set_mode("init-error");
    let starts = fake.events("start").len();
    env.core.start_sidecar_with(fake.launch()).unwrap();
    let st = wait_state(&env.core, SidecarState::Error, 30);
    assert!(st.detail.as_deref().unwrap().contains("device unavailable"), "{st:?}");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(fake.events("start").len(), starts + 1, "no restart after an initialize error");
}

// ---------------------------------------------------------------------------------------------
// Timeouts and cancel
// ---------------------------------------------------------------------------------------------

fn embed_text_call(core: &AppCore, timeout: Duration) -> ss_core::sidecar::PendingCall {
    core.sidecar().call(methods::EMBED_TEXT, &EmbedTextParams { texts: vec!["x".into()] }, timeout)
}

#[test]
fn timeout_sends_cancel_and_an_unanswered_cancel_restarts() {
    let env = Env::new(3);
    env.index(true);
    let fake = Fake::new("hang");
    let seen = record_statuses(&env.core);
    assert_eq!(start_fake(&env, &fake).state, SidecarState::Ready);

    // The caller gets Timeout at its deadline; the sidecar gets `$/cancel` and answers -32004.
    let t0 = Instant::now();
    let call = embed_text_call(&env.core, Duration::from_millis(400));
    let id = call.id;
    assert_eq!(call.wait().unwrap_err(), CallError::Timeout);
    assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
    std::thread::sleep(Duration::from_millis(1200)); // past the cancel grace
    assert_eq!(fake.events("cancel").iter().map(|e| e["id"].as_i64().unwrap()).collect::<Vec<_>>(), [id]);
    assert_eq!(env.core.get_sidecar_status().state, SidecarState::Ready, "answered the cancel: no restart");
    assert_eq!(fake.events("start").len(), 1);

    // An explicit cancel answers the call with -32004.
    let call = embed_text_call(&env.core, Duration::from_secs(20));
    env.core.sidecar().cancel(call.id);
    match call.wait() {
        Err(CallError::Rpc(e)) => assert_eq!(e.code, error_codes::CANCELLED),
        other => panic!("expected -32004, got {other:?}"),
    }

    // cancel_job during the AI job: `$/cancel` for the batch, the job ends `cancelled`.
    let rec = Arc::new(Rec::default());
    let job = env.core.start_ai(StartAiReq { tasks: vec![AiTask::Ocr], image_ids: None }, rec.clone()).unwrap();
    let end = Instant::now() + Duration::from_secs(20);
    while fake.requests("ocr.recognize").is_empty() {
        assert!(Instant::now() < end, "the job never sent its batch");
        std::thread::sleep(Duration::from_millis(20));
    }
    env.core.cancel_job(CancelJobReq { job_id: job.job_id }).unwrap();
    let fin = rec.finished(job.job_id, 20);
    assert_eq!(fin.error.map(|e| e.kind), Some(ErrorKind::Cancelled));

    // A sidecar that ignores `$/cancel` is killed after the grace and restarted.
    fake.set_mode("hang-deaf");
    env.core.restart_sidecar().unwrap();
    wait_state(&env.core, SidecarState::Ready, 30);
    let starts = fake.events("start").len();
    assert_eq!(embed_text_call(&env.core, Duration::from_millis(300)).wait().unwrap_err(), CallError::Timeout);
    let end = Instant::now() + Duration::from_secs(30);
    while fake.events("start").len() < starts + 1 {
        assert!(Instant::now() < end, "no restart after an unanswered $/cancel");
        std::thread::sleep(Duration::from_millis(50));
    }
    wait_state(&env.core, SidecarState::Ready, 30);
    assert!(
        seen.lock().unwrap().iter().any(|s| s.detail.as_deref().is_some_and(|d| d.contains("did not answer $/cancel"))),
        "the restart names its reason"
    );
}

// ---------------------------------------------------------------------------------------------
// Garbage on stdout
// ---------------------------------------------------------------------------------------------

#[test]
fn garbage_on_stdout_is_an_error_and_queries_keep_working() {
    let env = Env::new(10);
    env.index(true);
    let fake = Fake::new("garbage");
    env.core.sidecar().set_timings(timings());
    env.core.start_sidecar_with(fake.launch()).unwrap();
    let st = wait_state(&env.core, SidecarState::Error, 30);
    assert!(st.detail.as_deref().unwrap().contains("not JSON"), "{st:?}");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(fake.events("start").len(), 1, "a protocol violation is not restarted");

    let c = &env.core;
    assert_eq!(c.query_images(&query::default_query(500)).unwrap().total, 10);
    let fts = c.query_images(&text_query("screenshot", TextMode::Filename)).unwrap();
    assert_eq!(ids(&c.query_images(&text_query("screenshot", TextMode::Any)).unwrap()), ids(&fts));
    assert_eq!(c.query_images(&text_query("cat", TextMode::Semantic)).unwrap_err().kind, ErrorKind::SidecarUnavailable);
    let ai = StartAiReq { tasks: vec![AiTask::Color], image_ids: None };
    assert_eq!(c.start_ai(ai, Arc::new(Rec::default())).unwrap_err().kind, ErrorKind::SidecarUnavailable);
    c.get_stats().unwrap();
    c.find_duplicates(FindDuplicatesReq { mode: DuplicateMode::Similar, hamming: None, root_id: None }).unwrap();
}

// ---------------------------------------------------------------------------------------------
// In-flight limit, chatty child
// ---------------------------------------------------------------------------------------------

#[test]
fn at_most_two_requests_in_flight_and_a_chatty_sidecar_never_blocks() {
    let env = Env::new(1);
    let fake = Fake::new("ok delay=300 chatty=1");
    assert_eq!(start_fake(&env, &fake).state, SidecarState::Ready);
    let t0 = Instant::now();
    let calls: Vec<_> = (0..5).map(|_| embed_text_call(&env.core, Duration::from_secs(30))).collect();
    for call in calls {
        let r: EmbedTextResult = call.typed().expect("answered");
        assert_eq!((r.dim, r.vectors.len()), (8, 1));
    }
    let elapsed = t0.elapsed();
    let inflight: Vec<i64> = fake.events("inflight").iter().map(|e| e["n"].as_i64().unwrap()).collect();
    assert_eq!(inflight.iter().max(), Some(&2), "{inflight:?}");
    assert!(elapsed >= Duration::from_millis(850), "5 requests, 2 at a time, 300 ms each: {elapsed:?}");
    println!("5 chatty requests (each 300 x 4 KB on stderr + 200 progress lines): {elapsed:?}");
}
