//! Phase 2.1 / D8: the two-step permanent delete. Only a fresh, unexpired, single-use token plus the
//! typed word `DELETE` removes files; the token binds the exact id set; `purge` is never undoable;
//! `ssctl` (`Caller::Cli`) can never reach it. Fixture libraries only (D10).

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::env::Env;
use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller, ErrorKind};

fn setup() -> (Env, Vec<(i64, PathBuf)>) {
    let env = Env::new(6);
    env.index(false);
    let rows = env.all().into_iter().map(|s| (s.id, PathBuf::from(env.detail(s.id).abs_path))).collect();
    (env, rows)
}

fn ticket(env: &Env, ids: &[i64]) -> PermanentDeleteTicket {
    env.core.request_permanent_delete(Caller::Ui, ImageIdsReq { image_ids: ids.to_vec() }).expect("ticket")
}

fn delete(env: &Env, token: &str, word: &str) -> Result<OpResult, AppError> {
    env.core.delete_permanently(Caller::Ui, DeletePermanentlyReq { token: token.into(), typed_confirmation: word.into() })
}

fn all_exist(rows: &[(i64, PathBuf)]) -> bool {
    rows.iter().all(|(_, p)| p.is_file())
}

#[test]
fn no_token_is_confirmation_required() {
    let (env, rows) = setup();
    // A live ticket exists: a bogus token must not redeem it.
    let live = ticket(&env, &[rows[0].0]);
    for token in ["", "deadbeef", "00000000000000000000000000000000", &live.token.to_uppercase()] {
        assert_eq!(delete(&env, token, "DELETE").unwrap_err().kind, ErrorKind::ConfirmationRequired, "{token:?}");
    }
    assert!(all_exist(&rows));
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), rows.len() as i64);
    assert_eq!(delete(&env, &live.token, "DELETE").unwrap().affected, 1, "the real ticket is still valid");
}

#[test]
fn wrong_word_is_confirmation_required_and_burns_the_token() {
    let (env, rows) = setup();
    for word in ["", "delete", "DELETE ", "Delete", "yes"] {
        let t = ticket(&env, &[rows[0].0]);
        assert_eq!(delete(&env, &t.token, word).unwrap_err().kind, ErrorKind::ConfirmationRequired, "{word:?}");
        // Single use: the right word on the same token is refused too.
        assert_eq!(delete(&env, &t.token, "DELETE").unwrap_err().kind, ErrorKind::ConfirmationRequired);
    }
    assert!(all_exist(&rows));
}

#[test]
fn expired_token_is_confirmation_required() {
    let (env, rows) = setup();
    env.core.delete_tokens().set_ttl_ms(50);
    let t = ticket(&env, &[rows[0].0]);
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(delete(&env, &t.token, "DELETE").unwrap_err().kind, ErrorKind::ConfirmationRequired);
    assert!(all_exist(&rows));
}

#[test]
fn token_binds_its_id_set() {
    let (env, rows) = setup();
    let set_a = [rows[0].0, rows[1].0];
    let set_b = [rows[2].0, rows[3].0];
    let ta = ticket(&env, &set_a);
    let tb = ticket(&env, &set_b);
    assert_ne!(ta.token, tb.token);
    assert_eq!(ta.count, 2);
    let bytes: i64 = rows[..2].iter().map(|(_, p)| std::fs::metadata(p).unwrap().len() as i64).sum();
    assert_eq!(ta.total_bytes, bytes);
    assert!(ta.expires_at_ms > 0);

    // Token A deletes exactly set A, whatever else was requested since.
    let r = delete(&env, &ta.token, "DELETE").unwrap();
    assert_eq!((r.affected, r.failed.len()), (2, 0));
    assert!(!rows[0].1.exists() && !rows[1].1.exists());
    assert!(all_exist(&rows[2..]), "set B and the rest untouched");
    // Token B still deletes set B only.
    let r = delete(&env, &tb.token, "DELETE").unwrap();
    assert_eq!(r.affected, 2);
    assert!(all_exist(&rows[4..]));
    assert_eq!(env.sql_i64("SELECT count(*) FROM image"), 2);
}

#[test]
fn reused_token_is_an_error() {
    let (env, rows) = setup();
    let t = ticket(&env, &[rows[0].0]);
    delete(&env, &t.token, "DELETE").unwrap();
    // Put a file back at the same path: a replayed token must not delete it.
    std::fs::write(&rows[0].1, b"new file").unwrap();
    assert_eq!(delete(&env, &t.token, "DELETE").unwrap_err().kind, ErrorKind::ConfirmationRequired);
    assert!(rows[0].1.is_file());
}

#[test]
fn happy_path_removes_files_rows_and_is_never_undoable() {
    let (env, rows) = setup();
    let victims = [rows[0].0, rows[1].0, rows[2].0];
    let t = ticket(&env, &victims);
    let r = delete(&env, &t.token, "DELETE").unwrap();
    assert_eq!((r.affected, r.failed.len()), (3, 0));
    let batch = r.batch_id.expect("journaled");
    for (id, p) in &rows[..3] {
        assert!(!p.exists(), "{} removed", p.display());
        assert_eq!(env.core.get_image(GetImageReq { id: *id }).unwrap_err().kind, ErrorKind::NotFound);
    }
    assert!(all_exist(&rows[3..]));
    assert_eq!(env.sql_i64("SELECT count(*) FROM text_fts"), 3, "FTS rows go with the images");
    assert!(!Path::new(&ss_core::paths::thumb_path(env.core.data_dir(), victims[0], 320)).exists());

    let j = env.core.list_journal(LimitReq { limit: 5 }).unwrap();
    assert_eq!((j.items[0].batch_id, j.items[0].op, j.items[0].undoable, j.items[0].count), (batch, JournalOp::Purge, false, 3));
    let u = env.core.undo(Caller::Ui, UndoReq { batch_id: Some(batch) }).unwrap_err();
    assert_eq!(u.kind, ErrorKind::InvalidArgument);
    // "Undo the latest" never picks a purge batch.
    assert_eq!(env.core.undo(Caller::Ui, UndoReq { batch_id: None }).unwrap_err().kind, ErrorKind::InvalidArgument);
    assert!(!rows[0].1.exists());
}

#[test]
fn request_validation_and_cli_is_refused() {
    let (env, rows) = setup();
    let ids = vec![rows[0].0];
    assert_eq!(ticket_err(&env, Caller::Ui, vec![]), ErrorKind::InvalidArgument);
    assert_eq!(ticket_err(&env, Caller::Ui, vec![rows[0].0, 999_999]), ErrorKind::NotFound);
    assert_eq!(ticket_err(&env, Caller::Cli, ids.clone()), ErrorKind::PermissionDenied);
    let t = ticket(&env, &ids);
    let cli = env.core.delete_permanently(Caller::Cli, DeletePermanentlyReq { token: t.token.clone(), typed_confirmation: "DELETE".into() });
    assert_eq!(cli.unwrap_err().kind, ErrorKind::PermissionDenied);
    assert!(all_exist(&rows));
    // The Cli attempt did not consume the token: the app can still use it.
    assert_eq!(delete(&env, &t.token, "DELETE").unwrap().affected, 1);
}

fn ticket_err(env: &Env, caller: Caller, ids: Vec<i64>) -> ErrorKind {
    env.core.request_permanent_delete(caller, ImageIdsReq { image_ids: ids }).unwrap_err().kind
}
