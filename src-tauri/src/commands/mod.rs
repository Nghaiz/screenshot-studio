//! Tauri commands, one per command in `ss_core::contract::ipc::COMMANDS` (every one implemented since phase 4.1).
//! Rule 1: each takes exactly one argument named `req` (or none); `save_edited_image` takes the raw
//! request body. Write commands forward to `ss_core::AppCore` with `Caller::Ui` (rule 8).
//! `contract_test.rs` (e) checks this set of `#[tauri::command]` fns equals COMMANDS.

pub mod ai;
pub mod edit;
pub mod fileops;
pub mod library;
pub mod organize;
pub mod query;
