//! The supervisor actor: owns the child process, the request queue and the crash policy. Every input
//! (handle commands, stdout lines, timers, exits) arrives as one [`Cmd`] on one channel, so the loop
//! never waits on two things at once (no `select!`) and never blocks on the child's pipes.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, mpsc as std_mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::config::{self, SidecarTimings};
use super::launch::{self, LaunchPlan, SpawnSpec};
use super::{CallError, Shared, SidecarInfo, status};
use crate::contract::ipc::SidecarState;
use crate::contract::sidecar::{
    InitializeResult, JSONRPC_VERSION, PROTOCOL_VERSION, RpcCall, RpcError, RpcErrorData, RpcRequest, error_codes, methods,
};

pub(super) struct Outgoing {
    pub id: i64,
    pub method: &'static str,
    pub params: Value,
    pub deadline: Instant,
    pub reply: std_mpsc::Sender<Result<Value, CallError>>,
    pub progress: Arc<AtomicI64>,
}

pub(super) enum Cmd {
    /// The sender is answered once the plan has been acted on.
    Start(LaunchPlan, std_mpsc::Sender<()>),
    Call(Outgoing),
    Cancel(i64),
    Shutdown(Option<std_mpsc::Sender<()>>),
    // Internal; `u64` = process generation (messages of an older process are ignored).
    Line(u64, Vec<u8>),
    Violation(u64, String),
    StdoutClosed(u64),
    Exited(u64, Option<i32>),
    RequestTimeout(i64),
    CancelGrace(u64, i64),
    InitTimeout(u64),
    RestartDue(u64),
    ShutdownDeadline(u64),
}

/// Starts the actor thread; returns its command sender.
pub(super) fn spawn(shared: Arc<Shared>, timings: SidecarTimings) -> Result<UnboundedSender<Cmd>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the sidecar runtime: {e}"))?;
    let (tx, rx) = mpsc::unbounded_channel();
    let actor = Actor::new(shared, timings, tx.clone(), rx);
    std::thread::Builder::new()
        .name("ss-sidecar".into())
        .spawn(move || rt.block_on(actor.run()))
        .map_err(|e| format!("cannot start the sidecar thread: {e}"))?;
    Ok(tx)
}

struct Proc {
    generation: u64,
    child: Child,
    pid: Option<u32>,
    stdin: UnboundedSender<Vec<u8>>,
    init_id: i64,
    ready_at: Option<Instant>,
    shutting_down: bool,
}

struct Inflight {
    out: Outgoing,
    /// Taken when the caller has been answered (e.g. at its timeout) while the request still runs.
    answered: bool,
    cancel_sent: bool,
}

/// A process whose stdout closed on its own: the crash is handled once its exit code is in.
struct Dying {
    generation: u64,
    ready_at: Option<Instant>,
}

struct Actor {
    shared: Arc<Shared>,
    t: SidecarTimings,
    tx: UnboundedSender<Cmd>,
    rx: UnboundedReceiver<Cmd>,
    spec: Option<SpawnSpec>,
    proc: Option<Proc>,
    dying: Option<Dying>,
    generation: u64,
    queue: VecDeque<Outgoing>,
    inflight: HashMap<i64, Inflight>,
    crashes: VecDeque<Instant>,
    consecutive: usize,
    /// Bumped by every start / fatal error / shutdown, so a scheduled restart of an older epoch is void.
    epoch: u64,
    stopping: Option<Vec<std_mpsc::Sender<()>>>,
    done: bool,
}

impl Actor {
    fn new(shared: Arc<Shared>, t: SidecarTimings, tx: UnboundedSender<Cmd>, rx: UnboundedReceiver<Cmd>) -> Self {
        Self {
            shared,
            t,
            tx,
            rx,
            spec: None,
            proc: None,
            dying: None,
            generation: 0,
            queue: VecDeque::new(),
            inflight: HashMap::new(),
            crashes: VecDeque::new(),
            consecutive: 0,
            epoch: 0,
            stopping: None,
            done: false,
        }
    }

    async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            self.handle(cmd);
            if self.done {
                break;
            }
        }
        if let Some(p) = self.proc.take() {
            kill(p);
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Start(plan, ack) => {
                self.start(plan);
                let _ = ack.send(());
            }
            Cmd::Call(out) => self.on_call(out),
            Cmd::Cancel(id) => self.on_cancel(id),
            Cmd::Shutdown(done) => self.on_shutdown(done),
            Cmd::Line(generation, line) if self.is_current(generation) => self.on_line(&line),
            Cmd::Violation(generation, why) if self.is_current(generation) => self.fatal(format!("protocol violation: {why}")),
            Cmd::StdoutClosed(generation) if self.is_current(generation) => self.on_stdout_closed(),
            Cmd::Exited(generation, code) => self.on_exited(generation, code),
            Cmd::RequestTimeout(id) => self.on_request_timeout(id),
            Cmd::CancelGrace(generation, id) if self.is_current(generation) => self.on_cancel_grace(id),
            Cmd::InitTimeout(generation) if self.is_current(generation) => self.on_init_timeout(),
            Cmd::RestartDue(epoch) if epoch == self.epoch && self.proc.is_none() && self.stopping.is_none() => {
                if let Some(spec) = self.spec.clone() {
                    self.spawn_proc(&spec);
                }
            }
            Cmd::ShutdownDeadline(generation) if self.is_current(generation) => {
                if let Some(p) = self.proc.take() {
                    kill(p);
                }
                self.finish_shutdown();
            }
            _ => {} // a message of an older process
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.proc.as_ref().is_some_and(|p| p.generation == generation)
    }

    fn after(&self, delay: Duration, cmd: Cmd) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = tx.send(cmd);
        });
    }

    fn set(&self, state: SidecarState, detail: impl Into<Option<String>>) {
        self.shared.set(status(state, detail.into()), None);
    }

    // -----------------------------------------------------------------------------------------
    // Start / spawn
    // -----------------------------------------------------------------------------------------

    fn start(&mut self, plan: LaunchPlan) {
        if self.stopping.is_some() {
            return;
        }
        self.epoch += 1;
        self.crashes.clear();
        self.consecutive = 0;
        self.dying = None;
        if let Some(p) = self.proc.take() {
            kill(p);
        }
        self.fail_all(|| CallError::Lost("the sidecar was restarted".into()));
        match plan {
            LaunchPlan::Absent(detail) => {
                self.spec = None;
                self.set(SidecarState::Absent, detail);
            }
            LaunchPlan::Spawn(spec) => {
                self.spec = Some((*spec).clone());
                self.spawn_proc(&spec);
            }
        }
    }

    fn spawn_proc(&mut self, spec: &SpawnSpec) {
        self.generation += 1;
        let generation = self.generation;
        let l = &spec.launch;
        let mut cmd = Command::new(&l.program);
        cmd.args(&l.args)
            .current_dir(&l.cwd)
            .envs(l.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(launch::CREATE_NO_WINDOW);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.set(SidecarState::Error, format!("cannot start {}: {e}", l.program.display()));
                return;
            }
        };
        let pid = child.id();
        let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            self.set(SidecarState::Error, "cannot open the sidecar's stdio pipes".to_owned());
            return;
        };
        let (line_tx, line_rx) = mpsc::unbounded_channel();
        tokio::spawn(read_stdout(generation, stdout, self.tx.clone()));
        tokio::spawn(read_stderr(stderr, pid, self.shared.clone()));
        tokio::spawn(write_stdin(stdin, line_rx));

        let init_id = self.shared.next_id();
        let req = RpcRequest { jsonrpc: JSONRPC_VERSION.into(), id: init_id, call: RpcCall::Initialize(spec.init.clone()) };
        let _ = line_tx.send(line_of(&serde_json::to_value(&req).unwrap_or(Value::Null)));
        self.after(self.t.init_timeout, Cmd::InitTimeout(generation));
        self.proc = Some(Proc { generation, child, pid, stdin: line_tx, init_id, ready_at: None, shutting_down: false });
        self.set(SidecarState::Starting, "loading models".to_owned());
    }

    // -----------------------------------------------------------------------------------------
    // Requests
    // -----------------------------------------------------------------------------------------

    fn ready(&self) -> bool {
        self.proc.as_ref().is_some_and(|p| p.ready_at.is_some() && !p.shutting_down)
    }

    fn on_call(&mut self, out: Outgoing) {
        if !self.ready() {
            let _ = out.reply.send(Err(CallError::Unavailable(self.shared.status())));
            return;
        }
        let delay = out.deadline.saturating_duration_since(Instant::now());
        self.after(delay, Cmd::RequestTimeout(out.id));
        self.queue.push_back(out);
        self.pump();
    }

    fn pump(&mut self) {
        while self.inflight.len() < config::SIDECAR_MAX_IN_FLIGHT {
            let Some(out) = self.queue.pop_front() else { break };
            let Some(p) = &self.proc else {
                let _ = out.reply.send(Err(CallError::Unavailable(self.shared.status())));
                continue;
            };
            let msg = json!({ "jsonrpc": JSONRPC_VERSION, "id": out.id, "method": out.method, "params": out.params });
            let _ = p.stdin.send(line_of(&msg));
            self.inflight.insert(out.id, Inflight { out, answered: false, cancel_sent: false });
        }
    }

    fn send_cancel(&mut self, id: i64) {
        let Some(p) = &self.proc else { return };
        let generation = p.generation;
        let msg = json!({ "jsonrpc": JSONRPC_VERSION, "method": methods::CANCEL, "params": { "id": id } });
        let _ = p.stdin.send(line_of(&msg));
        if let Some(f) = self.inflight.get_mut(&id) {
            f.cancel_sent = true;
        }
        self.after(self.t.cancel_grace, Cmd::CancelGrace(generation, id));
    }

    fn on_cancel(&mut self, id: i64) {
        if let Some(pos) = self.queue.iter().position(|o| o.id == id) {
            let out = self.queue.remove(pos).expect("queued call");
            let _ = out.reply.send(Err(CallError::Rpc(cancelled_error())));
            return;
        }
        if self.inflight.get(&id).is_some_and(|f| !f.cancel_sent) {
            self.send_cancel(id);
        }
    }

    /// The caller's deadline: answer it now; a request still running gets `$/cancel`.
    fn on_request_timeout(&mut self, id: i64) {
        if let Some(pos) = self.queue.iter().position(|o| o.id == id) {
            let out = self.queue.remove(pos).expect("queued call");
            let _ = out.reply.send(Err(CallError::Timeout));
            return;
        }
        let Some(f) = self.inflight.get_mut(&id) else { return };
        if !f.answered {
            f.answered = true;
            let _ = f.out.reply.send(Err(CallError::Timeout));
        }
        if !f.cancel_sent {
            self.send_cancel(id);
        }
    }

    /// `$/cancel` went unanswered: the process is stuck. Kill it; the crash policy restarts it.
    fn on_cancel_grace(&mut self, id: i64) {
        if !self.inflight.contains_key(&id) {
            return;
        }
        let ready_at = self.proc.as_ref().and_then(|p| p.ready_at);
        if let Some(p) = self.proc.take() {
            kill(p);
        }
        let why = format!("request {id} did not answer $/cancel within {:?}", self.t.cancel_grace);
        self.fail_all(|| CallError::Lost(why.clone()));
        self.crashed(why, ready_at);
    }

    fn fail_all(&mut self, err: impl Fn() -> CallError) {
        for (_, f) in self.inflight.drain() {
            if !f.answered {
                let _ = f.out.reply.send(Err(err()));
            }
        }
        for out in self.queue.drain(..) {
            let _ = out.reply.send(Err(err()));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Protocol input
    // -----------------------------------------------------------------------------------------

    fn on_line(&mut self, raw: &[u8]) {
        let raw = raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(raw);
        if raw.iter().all(u8::is_ascii_whitespace) {
            return;
        }
        let msg: Value = match serde_json::from_slice(raw) {
            Ok(v) => v,
            Err(e) => return self.fatal(format!("protocol violation: a stdout line is not JSON ({e}): {}", preview(raw))),
        };
        if msg.get("jsonrpc").and_then(Value::as_str) != Some(JSONRPC_VERSION) {
            return self.fatal(format!("protocol violation: not a JSON-RPC 2.0 message: {}", preview(raw)));
        }
        match (msg.get("id"), msg.get("method")) {
            (None, Some(m)) => self.on_notification(m.as_str().unwrap_or_default(), msg.get("params")),
            (Some(Value::Null), None) => {
                eprintln!("ss-sidecar: the sidecar could not parse one of our lines: {}", preview(raw));
            }
            (Some(id), None) => match id.as_i64() {
                Some(id) => self.on_response(id, &msg),
                None => self.fatal(format!("protocol violation: response id is not an integer: {}", preview(raw))),
            },
            _ => self.fatal(format!("protocol violation: unexpected message: {}", preview(raw))),
        }
    }

    fn on_notification(&mut self, method: &str, params: Option<&Value>) {
        if method == methods::PROGRESS {
            let p = params.unwrap_or(&Value::Null);
            if let (Some(rid), Some(done)) = (p.get("requestId").and_then(Value::as_i64), p.get("done").and_then(Value::as_i64))
                && let Some(f) = self.inflight.get(&rid)
            {
                f.out.progress.store(done, Ordering::Relaxed);
            }
        } else {
            eprintln!("ss-sidecar: ignoring notification `{method}`");
        }
    }

    fn on_response(&mut self, id: i64, msg: &Value) {
        let result = match (msg.get("result"), msg.get("error")) {
            (Some(r), None) => Ok(r.clone()),
            (None, Some(e)) => Err(serde_json::from_value::<RpcError>(e.clone()).unwrap_or_else(|_| RpcError {
                code: error_codes::INTERNAL,
                message: e.to_string(),
                data: RpcErrorData { detail: None },
            })),
            _ => return self.fatal(format!("protocol violation: response {id} has neither result nor error")),
        };
        if self.proc.as_ref().is_some_and(|p| p.init_id == id) {
            return self.on_init(result);
        }
        // None: the shutdown answer, or a late answer to a request already given up.
        if let Some(f) = self.inflight.remove(&id) {
            if !f.answered {
                let _ = f.out.reply.send(result.map_err(CallError::Rpc));
            }
            self.pump();
        }
    }

    fn on_init(&mut self, result: Result<Value, RpcError>) {
        let parsed = result
            .map_err(|e| format!("initialize failed: {}", super::rpc_line(&e)))
            .and_then(|v| serde_json::from_value::<InitializeResult>(v).map_err(|e| format!("initialize result: {e}")))
            .and_then(|r| {
                if r.protocol_version == PROTOCOL_VERSION {
                    Ok(r)
                } else {
                    Err(format!("sidecar speaks protocol {}, expected {PROTOCOL_VERSION}", r.protocol_version))
                }
            });
        match parsed {
            Ok(r) => {
                if let Some(p) = self.proc.as_mut() {
                    p.ready_at = Some(Instant::now());
                }
                let capabilities: Vec<String> = r.capabilities.iter().map(crate::errors::enum_str).collect();
                let st = crate::contract::ipc::SidecarStatus {
                    state: SidecarState::Ready,
                    detail: None,
                    capabilities,
                    device: Some(r.device),
                    version: Some(r.sidecar_version.clone()),
                };
                let info = SidecarInfo { embed_model: r.embed_model, embed_dim: r.embed_dim, capabilities: r.capabilities };
                self.shared.set(st, Some(info));
                self.pump();
            }
            Err(why) => self.fatal(why),
        }
    }

    fn on_init_timeout(&mut self) {
        if self.proc.as_ref().is_some_and(|p| p.ready_at.is_none()) {
            self.fatal(format!("initialize did not answer within {:?}", self.t.init_timeout));
        }
    }

    /// Kill, state `error` with `why` (plus the stderr tail); no restart until `restart_sidecar`.
    fn fatal(&mut self, why: String) {
        self.epoch += 1;
        if let Some(p) = self.proc.take() {
            kill(p);
        }
        self.fail_all(|| CallError::Lost(why.clone()));
        let tail = self.shared.stderr_tail(3);
        let detail = if tail.is_empty() { why } else { format!("{why}; stderr: {tail}") };
        self.set(SidecarState::Error, detail);
    }

    // -----------------------------------------------------------------------------------------
    // Exit, crash policy, shutdown
    // -----------------------------------------------------------------------------------------

    fn on_stdout_closed(&mut self) {
        let Some(p) = self.proc.take() else { return };
        self.fail_all(|| CallError::Lost("the sidecar process ended".into()));
        if self.stopping.is_none() {
            // At once, so nobody sees a stale `ready` while the exit code is collected.
            self.set(SidecarState::Starting, "the sidecar process ended".to_owned());
        }
        if self.stopping.is_some() {
            reap(p, None, None);
            return self.finish_shutdown();
        }
        self.dying = Some(Dying { generation: p.generation, ready_at: p.ready_at });
        reap(p, Some(self.tx.clone()), Some(self.t.exit_grace));
    }

    fn on_exited(&mut self, generation: u64, code: Option<i32>) {
        let Some(d) = self.dying.take_if(|d| d.generation == generation) else { return };
        let code = code.map(|c| format!("exit code {c}")).unwrap_or_else(|| "killed".into());
        self.crashed(format!("sidecar exited unexpectedly ({code})"), d.ready_at);
    }

    /// Crash policy: restart after the backoff, or state `error` at the crash limit.
    fn crashed(&mut self, why: String, ready_at: Option<Instant>) {
        let now = Instant::now();
        while self.crashes.front().is_some_and(|t| now.duration_since(*t) > self.t.crash_window) {
            self.crashes.pop_front();
        }
        self.crashes.push_back(now);
        if ready_at.is_some_and(|t| now.duration_since(t) >= self.t.crash_window) {
            self.consecutive = 0; // it had run well for a whole window: start the backoff over
        }
        self.consecutive += 1;
        let tail = self.shared.stderr_tail(3);
        let why = if tail.is_empty() { why } else { format!("{why}; stderr: {tail}") };
        if self.crashes.len() >= self.t.crash_limit {
            self.epoch += 1;
            let detail = format!(
                "{why}; {} crashes within {:?}: stopped until restart_sidecar",
                self.crashes.len(),
                self.t.crash_window
            );
            self.set(SidecarState::Error, detail);
            return;
        }
        let delay = self.t.backoff(self.consecutive);
        self.set(SidecarState::Starting, format!("{why}; restarting in {delay:?}"));
        self.after(delay, Cmd::RestartDue(self.epoch));
    }

    fn on_shutdown(&mut self, done: Option<std_mpsc::Sender<()>>) {
        let already = self.stopping.is_some();
        self.stopping.get_or_insert_with(Vec::new).extend(done);
        if already {
            return;
        }
        self.epoch += 1;
        self.dying = None;
        self.fail_all(|| CallError::Lost("the sidecar is shutting down".into()));
        let id = self.shared.next_id();
        let Some(p) = self.proc.as_mut() else { return self.finish_shutdown() };
        p.shutting_down = true;
        let msg = json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "method": methods::SHUTDOWN, "params": {} });
        let _ = p.stdin.send(line_of(&msg));
        let generation = p.generation;
        self.after(self.t.shutdown_wait, Cmd::ShutdownDeadline(generation));
    }

    fn finish_shutdown(&mut self) {
        self.set(SidecarState::Absent, "sidecar stopped".to_owned());
        for w in self.stopping.take().unwrap_or_default() {
            let _ = w.send(());
        }
        self.done = true;
    }
}

fn cancelled_error() -> RpcError {
    RpcError { code: error_codes::CANCELLED, message: "cancelled".into(), data: RpcErrorData { detail: None } }
}

fn line_of(v: &Value) -> Vec<u8> {
    let mut s = serde_json::to_vec(v).unwrap_or_default();
    s.push(b'\n');
    s
}

fn preview(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    let s: String = s.chars().take(120).collect();
    format!("`{s}`")
}

/// Kills a process we gave up on, now: its tree first (the venv's `python.exe` is a launcher whose
/// child is the interpreter, docs/lessons.md), synchronously on this dedicated thread, so it happens
/// even when the runtime is about to stop. The exit is collected in the background.
fn kill(p: Proc) {
    let Proc { mut child, pid, stdin, .. } = p;
    drop(stdin);
    if let Some(pid) = pid {
        launch::kill_tree(pid);
    }
    let _ = child.start_kill();
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
}

/// Collects the exit of a process that is ending on its own. `grace`: how long it may take before
/// its tree is killed (None = the shutdown wait). `report`: where to send `Exited`.
fn reap(p: Proc, report: Option<UnboundedSender<Cmd>>, grace: Option<Duration>) {
    let Proc { generation, mut child, pid, stdin, .. } = p;
    drop(stdin); // closes the writer task, so the child sees EOF on stdin
    tokio::spawn(async move {
        let wait = grace.unwrap_or(Duration::from_secs(config::SIDECAR_SHUTDOWN_WAIT_S));
        let code = match tokio::time::timeout(wait, child.wait()).await {
            Ok(Ok(st)) => st.code(),
            _ => {
                if let Some(pid) = pid {
                    let _ = tokio::task::spawn_blocking(move || launch::kill_tree(pid)).await;
                }
                let _ = child.start_kill();
                child.wait().await.ok().and_then(|s| s.code())
            }
        };
        if let Some(tx) = report {
            let _ = tx.send(Cmd::Exited(generation, code));
        }
    });
}

/// Protocol lines -> the actor. Ends with `StdoutClosed` at EOF or a read error.
async fn read_stdout(generation: u64, stdout: impl AsyncRead + Unpin, tx: UnboundedSender<Cmd>) {
    let mut r = BufReader::with_capacity(64 * 1024, stdout);
    let mut line = Vec::new();
    loop {
        let buf = match r.fill_buf().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("ss-sidecar: stdout read failed: {e}");
                break;
            }
        };
        if buf.is_empty() {
            break;
        }
        let (take, complete) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        line.extend_from_slice(&buf[..take]);
        r.consume(take);
        if complete {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let _ = tx.send(Cmd::Line(generation, std::mem::take(&mut line)));
        } else if line.len() > config::MAX_LINE_BYTES {
            let _ = tx.send(Cmd::Violation(generation, format!("a stdout line exceeds {} bytes", config::MAX_LINE_BYTES)));
            line.clear();
        }
    }
    if !line.is_empty() {
        let _ = tx.send(Cmd::Line(generation, line));
    }
    let _ = tx.send(Cmd::StdoutClosed(generation));
}

/// Sidecar log lines -> the app log (stderr) and the tail kept for error details.
async fn read_stderr(stderr: impl AsyncRead + Unpin, pid: Option<u32>, shared: Arc<Shared>) {
    let mut r = BufReader::with_capacity(16 * 1024, stderr);
    let mut raw = Vec::new();
    loop {
        raw.clear();
        match r.read_until(b'\n', &mut raw).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        raw.truncate(config::STDERR_LINE_MAX);
        let line = String::from_utf8_lossy(&raw).trim_end().to_owned();
        if line.is_empty() {
            continue;
        }
        eprintln!("ss-sidecar[{}]: {line}", pid.unwrap_or_default());
        shared.push_stderr(line);
    }
}

/// Request lines -> the child's stdin. Ends (closing stdin) when the actor drops the sender.
async fn write_stdin(mut stdin: ChildStdin, mut rx: UnboundedReceiver<Vec<u8>>) {
    while let Some(bytes) = rx.recv().await {
        if let Err(e) = stdin.write_all(&bytes).await {
            eprintln!("ss-sidecar: stdin write failed: {e}");
            return;
        }
        let _ = stdin.flush().await;
    }
}
