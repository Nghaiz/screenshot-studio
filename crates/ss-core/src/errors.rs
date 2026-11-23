//! Conversions from library errors into the contract envelope (`AppError`), plus small helpers.

use crate::contract::{AppError, ErrorKind};

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::new(ErrorKind::Db, "database error").with_detail(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        let kind = match e.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Io,
        };
        AppError::new(kind, "file system error").with_detail(e.to_string())
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::InvalidArgument, message)
}

pub(crate) fn not_found(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::NotFound, message)
}

pub(crate) fn internal(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::Internal, message)
}

/// Parses a camelCase wire string (as stored in the DB) into a contract enum.
pub(crate) fn parse_enum<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, AppError> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|e| internal(format!("unexpected stored value `{s}`")).with_detail(e.to_string()))
}

/// The camelCase wire string of a contract enum (as stored in the DB).
pub(crate) fn enum_str<T: serde::Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        other => panic!("enum did not serialize to a string: {other:?}"),
    }
}

pub(crate) fn now_ms() -> i64 {
    system_time_ms(std::time::SystemTime::now()).unwrap_or(0)
}

pub(crate) fn system_time_ms(t: std::time::SystemTime) -> Option<i64> {
    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as i64)
}
