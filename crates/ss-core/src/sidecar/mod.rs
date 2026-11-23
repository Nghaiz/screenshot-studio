//! Supervisor of the optional Python AI sidecar (contract-sidecar-jsonrpc.md, D6).
//!
//! [`Sidecar`] is a synchronous handle over an actor that runs on its own thread with a
//! current-thread tokio runtime (`tokio::process`). The actor owns the child process; one reader task
//! per stream (stdout = protocol, stderr = log) and one writer task feed it messages, so a chatty or
//! stuck child can never block it. Callers block on a std channel, never on the runtime, so every
//! method is safe from any thread (Tauri `spawn_blocking`, job threads, `ssctl`).
//!
//! States: `absent` (not started, or no venv / source: nothing spawned), `starting` (spawned, waiting
//! for `initialize`; also while a restart is pending), `ready`, `error` (spawn / initialize failure,
//! protocol violation, or [`config::SIDECAR_CRASH_LIMIT`] crashes in the window; stays until
//! [`Sidecar::restart`]). Crash policy and timeouts: [`config`].

mod actor;
pub mod config;
pub mod launch;
pub mod vector;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc as std_mpsc};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::contract::ipc::{SidecarState, SidecarStatus};
use crate::contract::sidecar::{RpcError, SidecarCapability, error_codes};
use crate::contract::{AppError, ErrorKind};
pub use config::SidecarTimings;
pub use launch::{LaunchPlan, SidecarLaunch, SidecarLayout, SpawnSpec};

use actor::Cmd;

/// What `initialize` told us about the running sidecar (state `ready` only).
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarInfo {
    pub embed_model: String,
    pub embed_dim: i64,
    pub capabilities: Vec<SidecarCapability>,
}

/// Why a call produced no result.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// The sidecar is not `ready` (status at call time).
    Unavailable(SidecarStatus),
    /// The sidecar answered with a whole-request error.
    Rpc(RpcError),
    /// No answer within the call's timeout (`$/cancel` was sent).
    Timeout,
    /// The process ended (crash, kill, restart, shutdown) before answering.
    Lost(String),
    /// The result does not have the contract's shape.
    BadResult(String),
}

impl CallError {
    /// The `AppError` a command returns for this failure (`what` names the operation).
    pub fn to_app_error(&self, what: &str) -> AppError {
        let unavailable = |detail: String| AppError::new(ErrorKind::SidecarUnavailable, format!("{what} needs the AI sidecar")).with_detail(detail);
        match self {
            CallError::Unavailable(s) => unavailable(status_line(s)),
            CallError::Timeout => unavailable("the sidecar did not answer in time".into()),
            CallError::Lost(why) => unavailable(why.clone()),
            CallError::Rpc(e) if e.code == error_codes::CANCELLED => {
                AppError::new(ErrorKind::Cancelled, format!("{what} was cancelled"))
            }
            CallError::Rpc(e)
                if [error_codes::MODEL_NOT_LOADED, error_codes::DEVICE_UNAVAILABLE, error_codes::CAPABILITY_DISABLED]
                    .contains(&e.code) =>
            {
                unavailable(rpc_line(e))
            }
            CallError::Rpc(e) => AppError::new(ErrorKind::Internal, format!("{what} failed in the sidecar")).with_detail(rpc_line(e)),
            CallError::BadResult(why) => AppError::new(ErrorKind::Internal, format!("{what}: unexpected sidecar result")).with_detail(why.clone()),
        }
    }
}

fn rpc_line(e: &RpcError) -> String {
    match &e.data.detail {
        Some(d) => format!("{} ({}): {d}", e.message, e.code),
        None => format!("{} ({})", e.message, e.code),
    }
}

/// `state: detail` for error messages.
pub fn status_line(s: &SidecarStatus) -> String {
    let state = crate::errors::enum_str(&s.state);
    match &s.detail {
        Some(d) => format!("sidecar {state}: {d}"),
        None => format!("sidecar {state}"),
    }
}

type Listener = Arc<dyn Fn(&SidecarStatus) + Send + Sync>;

/// State shared by the handle and the actor.
pub(crate) struct Shared {
    snap: Mutex<(SidecarStatus, Option<SidecarInfo>)>,
    changed: Condvar,
    listener: Mutex<Option<Listener>>,
    stderr_tail: Mutex<VecDeque<String>>,
    /// Request ids: positive and increasing for the supervisor's lifetime (the sidecar refuses
    /// negative ids), shared by the handle's calls and the actor's own `initialize` / `shutdown`.
    next_id: AtomicI64,
}

impl Shared {
    fn new() -> Self {
        Self {
            snap: Mutex::new((not_started(), None)),
            changed: Condvar::new(),
            listener: Mutex::new(None),
            stderr_tail: Mutex::new(VecDeque::new()),
            next_id: AtomicI64::new(0),
        }
    }

    pub(crate) fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) fn status(&self) -> SidecarStatus {
        self.snap.lock().expect("sidecar status").0.clone()
    }

    /// Publishes a new status (and the ready info) and calls the listener outside the lock.
    pub(crate) fn set(&self, status: SidecarStatus, info: Option<SidecarInfo>) {
        {
            let mut snap = self.snap.lock().expect("sidecar status");
            if snap.0 == status && snap.1 == info {
                return;
            }
            *snap = (status.clone(), info);
        }
        self.changed.notify_all();
        let listener = self.listener.lock().expect("sidecar listener").clone();
        if let Some(f) = listener {
            f(&status);
        }
    }

    pub(crate) fn push_stderr(&self, line: String) {
        let mut tail = self.stderr_tail.lock().expect("stderr tail");
        if tail.len() == config::STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }

    /// The last stderr lines, newest last, for error details.
    pub(crate) fn stderr_tail(&self, n: usize) -> String {
        let tail = self.stderr_tail.lock().expect("stderr tail");
        let skip = tail.len().saturating_sub(n);
        tail.iter().skip(skip).cloned().collect::<Vec<_>>().join(" | ")
    }
}

pub(crate) fn status(state: SidecarState, detail: Option<String>) -> SidecarStatus {
    SidecarStatus { state, detail, capabilities: Vec::new(), device: None, version: None }
}

fn not_started() -> SidecarStatus {
    status(SidecarState::Absent, Some("sidecar not started".into()))
}

/// Handle to the supervisor. The actor thread starts on the first [`Sidecar::start`].
pub struct Sidecar {
    shared: Arc<Shared>,
    timings: Mutex<SidecarTimings>,
    actor: Mutex<Option<mpsc::UnboundedSender<Cmd>>>,
}

impl Default for Sidecar {
    fn default() -> Self {
        Self::new(SidecarTimings::default())
    }
}

impl Sidecar {
    pub fn new(timings: SidecarTimings) -> Self {
        Self { shared: Arc::new(Shared::new()), timings: Mutex::new(timings), actor: Mutex::new(None) }
    }

    /// Replaces the timings used by the next actor start (tests: call before the first start).
    pub fn set_timings(&self, t: SidecarTimings) {
        *self.timings.lock().expect("sidecar timings") = t;
    }

    pub fn timings(&self) -> SidecarTimings {
        self.timings.lock().expect("sidecar timings").clone()
    }

    /// Called on every status change (the Tauri app emits `sidecar-status`).
    pub fn set_listener(&self, f: impl Fn(&SidecarStatus) + Send + Sync + 'static) {
        *self.shared.listener.lock().expect("sidecar listener") = Some(Arc::new(f));
    }

    pub fn status(&self) -> SidecarStatus {
        self.shared.status()
    }

    /// `initialize` result of the running sidecar (None unless `ready`).
    pub fn info(&self) -> Option<SidecarInfo> {
        let snap = self.shared.snap.lock().expect("sidecar status");
        (snap.0.state == SidecarState::Ready).then(|| snap.1.clone()).flatten()
    }

    /// Starts (or replaces) the sidecar per `plan` and resets the crash history. Returns once the
    /// supervisor has acted on it (`starting`, `absent`, or `error` when the spawn failed), without
    /// waiting for `initialize`.
    pub fn start(&self, plan: LaunchPlan) -> SidecarStatus {
        match self.ensure_actor() {
            Ok(tx) => {
                let (ack_tx, ack_rx) = std_mpsc::channel();
                if tx.send(Cmd::Start(plan, ack_tx)).is_ok() {
                    let _ = ack_rx.recv_timeout(START_ACK_WAIT);
                }
            }
            Err(e) => self.shared.set(status(SidecarState::Error, Some(e)), None),
        }
        self.status()
    }

    /// Sends one request. The supervisor always answers the returned call: with the result, an
    /// error, or [`CallError::Timeout`] once `timeout` (counted from now) expires.
    pub fn call<P: Serialize>(&self, method: &'static str, params: &P, timeout: Duration) -> PendingCall {
        let id = self.shared.next_id();
        let (reply_tx, reply_rx) = std_mpsc::channel();
        let progress = Arc::new(AtomicI64::new(0));
        let actor = self.actor.lock().expect("sidecar actor").clone();
        let sent = match (serde_json::to_value(params), &actor) {
            (Err(e), _) => {
                let _ = reply_tx.send(Err(CallError::BadResult(format!("params do not serialize: {e}"))));
                true
            }
            (Ok(params), Some(tx)) => tx
                .send(Cmd::Call(actor::Outgoing {
                    id,
                    method,
                    params,
                    deadline: Instant::now() + timeout,
                    reply: reply_tx.clone(),
                    progress: progress.clone(),
                }))
                .is_ok(),
            (Ok(_), None) => false,
        };
        if !sent {
            let _ = reply_tx.send(Err(CallError::Unavailable(self.status())));
        }
        PendingCall { id, rx: reply_rx, actor, progress }
    }

    /// Asks the sidecar to stop request `id` (`$/cancel`); its call then answers -32004.
    pub fn cancel(&self, id: i64) {
        if let Some(tx) = self.actor.lock().expect("sidecar actor").as_ref() {
            let _ = tx.send(Cmd::Cancel(id));
        }
    }

    /// Waits until `pred(status)` holds or `timeout` passes; returns the status at that point.
    pub fn wait_until(&self, pred: impl Fn(&SidecarStatus) -> bool, timeout: Duration) -> SidecarStatus {
        let end = Instant::now() + timeout;
        let mut snap = self.shared.snap.lock().expect("sidecar status");
        while !pred(&snap.0) {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            snap = self.shared.changed.wait_timeout(snap, left).expect("sidecar status").0;
        }
        snap.0.clone()
    }

    /// Waits until the sidecar is no longer `starting` (bounded by the recovery budget).
    pub fn wait_settled(&self) -> SidecarStatus {
        let budget = self.timings().recovery_budget();
        self.wait_until(|s| s.state != SidecarState::Starting, budget)
    }

    /// Sends `shutdown`, gives the process the shutdown wait to exit, then kills it. Blocks until done
    /// (at most the shutdown wait plus a second). The supervisor is stopped afterwards.
    pub fn shutdown(&self) {
        let Some(tx) = self.actor.lock().expect("sidecar actor").take() else { return };
        let (done_tx, done_rx) = std_mpsc::channel();
        if tx.send(Cmd::Shutdown(Some(done_tx))).is_ok() {
            let wait = self.timings().shutdown_wait + Duration::from_secs(1);
            let _ = done_rx.recv_timeout(wait);
        }
    }

    fn ensure_actor(&self) -> Result<mpsc::UnboundedSender<Cmd>, String> {
        let mut guard = self.actor.lock().expect("sidecar actor");
        if let Some(tx) = guard.as_ref() {
            return Ok(tx.clone());
        }
        let tx = actor::spawn(self.shared.clone(), self.timings())?;
        *guard = Some(tx.clone());
        Ok(tx)
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        // Not waited for: the actor sends `shutdown`, then kills the process after the shutdown wait.
        if let Some(tx) = self.actor.get_mut().ok().and_then(Option::take) {
            let _ = tx.send(Cmd::Shutdown(None));
        }
    }
}

/// A request in flight.
pub struct PendingCall {
    pub id: i64,
    rx: std_mpsc::Receiver<Result<Value, CallError>>,
    actor: Option<mpsc::UnboundedSender<Cmd>>,
    progress: Arc<AtomicI64>,
}

/// `start` waits at most this long for the supervisor to act on it (it only spawns a process).
const START_ACK_WAIT: Duration = Duration::from_secs(10);

/// How often a cancellable wait checks its flag.
const CANCEL_POLL: Duration = Duration::from_millis(100);

impl PendingCall {
    /// Blocks until the supervisor answers (it always does, by the call's timeout at the latest).
    pub fn wait(self) -> Result<Value, CallError> {
        self.rx.recv().unwrap_or_else(|_| Err(CallError::Lost("sidecar supervisor stopped".into())))
    }

    /// Like [`PendingCall::wait`], but sends `$/cancel` once `cancel` is set and keeps waiting for the
    /// answer (-32004, or the result if it raced). `on_progress` gets the sidecar's `done` count.
    pub fn wait_cancellable(self, cancel: &AtomicBool, on_progress: &dyn Fn(i64)) -> Result<Value, CallError> {
        let mut sent = false;
        let mut seen = 0;
        loop {
            match self.rx.recv_timeout(CANCEL_POLL) {
                Ok(r) => return r,
                Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(CallError::Lost("sidecar supervisor stopped".into()));
                }
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    let done = self.progress.load(Ordering::Relaxed);
                    if done != seen {
                        seen = done;
                        on_progress(done);
                    }
                    if !sent && cancel.load(Ordering::Relaxed) {
                        sent = true;
                        if let Some(tx) = &self.actor {
                            let _ = tx.send(Cmd::Cancel(self.id));
                        }
                    }
                }
            }
        }
    }

    /// Waits and deserializes the result into the contract type `R`.
    pub fn typed<R: DeserializeOwned>(self) -> Result<R, CallError> {
        let v = self.wait()?;
        serde_json::from_value(v).map_err(|e| CallError::BadResult(e.to_string()))
    }
}

/// Deserializes a call result into the contract type `R`.
pub fn parse_result<R: DeserializeOwned>(v: Value) -> Result<R, CallError> {
    serde_json::from_value(v).map_err(|e| CallError::BadResult(e.to_string()))
}
