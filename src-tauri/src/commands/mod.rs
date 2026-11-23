//! Tauri command stubs, one per command in `ss_core::contract::ipc::COMMANDS`.
//! Rule 1: each takes exactly one argument named `req` (or none); `save_edited_image` takes the raw
//! request body. Unimplemented commands return `AppError { kind: internal, message: "not implemented" }`.
//! Write commands forward to `ss_core::ops::*` with `Caller::Ui` (rule 8).
//! `contract_test.rs` (e) checks this set of `#[tauri::command]` fns equals COMMANDS.

pub mod ai;
pub mod edit;
pub mod fileops;
pub mod library;
pub mod organize;
pub mod query;

use ss_core::contract::AppError;

pub(crate) fn not_implemented<T>(name: &str, phase: &str) -> Result<T, AppError> {
    Err(AppError::not_implemented().with_detail(format!("{name} lands in phase {phase}")))
}
