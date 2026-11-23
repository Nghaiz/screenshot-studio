//! Permanent delete (D8, Deferred decision #1): two steps, never undoable, UI only.
//!
//! 1. `request_permanent_delete` binds a fresh random token to the exact (sorted) id set, kept in
//!    memory only, valid [`TOKEN_TTL_MS`].
//! 2. `delete_permanently` needs that token, unexpired, and `typedConfirmation == "DELETE"`; anything
//!    else is `confirmationRequired`. A token is single-use: any attempt with a known token consumes
//!    it, so a retry needs a new request. Files go with `std::fs::remove_file`; the rows are removed
//!    and the batch is journaled as `purge` with `undoable = 0`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use crate::catalog::{self, Catalog};
use crate::contract::ipc::*;
use crate::contract::{AppError, Caller, ErrorKind};
use crate::errors::{invalid, not_found, now_ms};

use super::fsio::FileAction;
use super::journal::{Entry, PurgePayload, Step, commit_steps};
use super::{failure, guard_file_write, load_rows, not_found_failure, snapshot};

/// Token lifetime.
pub const TOKEN_TTL_MS: i64 = 60_000;
/// The word the user must type.
pub const CONFIRM_WORD: &str = "DELETE";

struct Ticket {
    ids: Vec<i64>,
    expires_at_ms: i64,
}

/// In-memory tokens of pending permanent deletes (one store per `AppCore`).
pub struct DeleteTokens {
    tickets: Mutex<HashMap<String, Ticket>>,
    ttl_ms: AtomicI64,
    counter: AtomicU64,
}

impl Default for DeleteTokens {
    fn default() -> Self {
        Self { tickets: Mutex::new(HashMap::new()), ttl_ms: AtomicI64::new(TOKEN_TTL_MS), counter: AtomicU64::new(0) }
    }
}

impl DeleteTokens {
    /// Tests only: shortens the token lifetime so expiry can be exercised without waiting a minute.
    #[doc(hidden)]
    pub fn set_ttl_ms(&self, ms: i64) {
        self.ttl_ms.store(ms, Ordering::Relaxed);
    }

    fn issue(&self, ids: Vec<i64>) -> (String, i64) {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut h = blake3::Hasher::new();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        h.update(&nanos.to_le_bytes());
        h.update(&n.to_le_bytes());
        h.update(&std::process::id().to_le_bytes());
        h.update(&(&n as *const u64 as usize).to_le_bytes());
        for id in &ids {
            h.update(&id.to_le_bytes());
        }
        let token = h.finalize().to_hex()[..32].to_owned();
        let expires_at_ms = now_ms() + self.ttl_ms.load(Ordering::Relaxed);
        let mut tickets = self.tickets.lock().expect("delete tokens");
        let now = now_ms();
        tickets.retain(|_, t| t.expires_at_ms > now);
        tickets.insert(token.clone(), Ticket { ids, expires_at_ms });
        (token, expires_at_ms)
    }

    /// Removes the token (single use) and returns its id set when it was still valid.
    fn take(&self, token: &str) -> Option<Ticket> {
        let t = self.tickets.lock().expect("delete tokens").remove(token)?;
        (t.expires_at_ms > now_ms()).then_some(t)
    }
}

fn ui_only(caller: Caller) -> Result<(), AppError> {
    match caller {
        Caller::Ui => Ok(()),
        Caller::Cli => Err(AppError::new(ErrorKind::PermissionDenied, "permanent delete is only available in the app")),
    }
}

fn confirmation_required(msg: &str) -> AppError {
    AppError::new(ErrorKind::ConfirmationRequired, msg)
}

pub fn request_permanent_delete(
    caller: Caller,
    cat: &Catalog,
    tokens: &DeleteTokens,
    req: ImageIdsReq,
) -> Result<PermanentDeleteTicket, AppError> {
    ui_only(caller)?;
    let mut ids = req.image_ids;
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(invalid("no images selected"));
    }
    let rows = cat.read(|c| load_rows(c, &ids))?;
    if let Some(id) = ids.iter().find(|id| !rows.contains_key(id)) {
        return Err(not_found(format!("image {id} not found")));
    }
    let total_bytes = rows.values().map(|r| r.size_bytes).sum();
    let count = ids.len() as i64;
    let (token, expires_at_ms) = tokens.issue(ids);
    Ok(PermanentDeleteTicket { token, count, total_bytes, expires_at_ms })
}

pub fn delete_permanently(
    caller: Caller,
    cat: &Catalog,
    tokens: &DeleteTokens,
    req: DeletePermanentlyReq,
) -> Result<OpResult, AppError> {
    ui_only(caller)?;
    let ticket = tokens
        .take(&req.token)
        .ok_or_else(|| confirmation_required("the delete confirmation expired or was already used; ask again"))?;
    if req.typed_confirmation != CONFIRM_WORD {
        return Err(confirmation_required("type DELETE to confirm the permanent delete"));
    }
    let roots = cat.read(catalog::list_roots)?;
    let rows = cat.read(|c| load_rows(c, &ticket.ids))?;
    let (mut steps, mut failed) = (Vec::new(), Vec::new());
    for id in ticket.ids {
        let Some(row) = rows.get(&id) else {
            failed.push(not_found_failure(id));
            continue;
        };
        let abs: PathBuf = row.abs();
        let removed = guard_file_write(caller, &roots, &[&abs]).and_then(|_| match std::fs::remove_file(&abs) {
            Ok(()) => Ok(()),
            // Already gone (a `missing` row): only the row goes.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::from(e)),
        });
        if let Err(e) = removed {
            failed.push(failure(id, &e));
            continue;
        }
        let payload = PurgePayload { abs: abs.to_string_lossy().into_owned(), size_bytes: row.size_bytes };
        let data_dir = cat.data_dir().to_path_buf();
        let db: super::journal::DbStep = Box::new(move |tx| {
            snapshot::remove(tx, id)?;
            let after: super::journal::AfterCommit = Box::new(move |_| crate::thumbs::remove_thumbs(&data_dir, id));
            Ok((Entry::new(JournalOp::Purge, id, payload), Some(after)))
        });
        steps.push(Step { image_id: id, action: FileAction::None, db });
    }
    commit_steps(cat, steps, failed).map(|(r, _)| r)
}
