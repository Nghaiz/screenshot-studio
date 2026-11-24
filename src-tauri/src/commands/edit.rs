//! Edit save, convert and stitch (phase 4.1): thin wrappers over `ss_core::AppCore` as `Caller::Ui`, which
//! may overwrite an image under a protected root only through these journaled paths (rule 8).

use ss_core::contract::ipc::*;
use ss_core::contract::{AppError, Caller, ErrorKind};
use ss_core::ops::SaveEditedImage;
use tauri::{AppHandle, State};

use crate::state::{AppState, blocking, emit_changed};

fn bad(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::InvalidArgument, message)
}

/// Reads one `x-*` header as text.
fn header<'a>(request: &'a tauri::ipc::Request<'_>, name: &str) -> Result<Option<&'a str>, AppError> {
    match request.headers().get(name) {
        None => Ok(None),
        Some(v) => v.to_str().map(Some).map_err(|_| bad(format!("header {name} is not text"))),
    }
}

fn enum_header<T: serde::de::DeserializeOwned>(request: &tauri::ipc::Request<'_>, name: &str) -> Result<T, AppError> {
    let v = header(request, name)?.ok_or_else(|| bad(format!("header {name} is missing")))?;
    serde_json::from_value(serde_json::Value::String(v.trim().to_owned()))
        .map_err(|_| bad(format!("header {name} has an unknown value `{v}`")))
}

/// Decodes the raw-body request (rule 1): body = PNG bytes, headers `x-image-id`, `x-mode`, `x-format`, `x-quality`.
fn decode(request: &tauri::ipc::Request<'_>) -> Result<SaveEditedImage, AppError> {
    let png = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.clone(),
        tauri::ipc::InvokeBody::Json(_) => return Err(bad("save_edited_image takes the PNG bytes as a raw body")),
    };
    let image_id = header(request, edit_headers::IMAGE_ID)?
        .ok_or_else(|| bad(format!("header {} is missing", edit_headers::IMAGE_ID)))?
        .trim()
        .parse::<i64>()
        .map_err(|_| bad(format!("header {} is not a number", edit_headers::IMAGE_ID)))?;
    let quality = match header(request, edit_headers::QUALITY)?.map(str::trim).filter(|q| !q.is_empty()) {
        None => None,
        Some(q) => Some(q.parse::<i64>().map_err(|_| bad(format!("header {} is not a number", edit_headers::QUALITY)))?),
    };
    Ok(SaveEditedImage {
        image_id,
        mode: enum_header(request, edit_headers::MODE)?,
        format: enum_header(request, edit_headers::FORMAT)?,
        quality,
        png,
    })
}

/// The one raw-body command (rule 1).
#[tauri::command]
pub async fn save_edited_image(
    request: tauri::ipc::Request<'_>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ImageSummary, AppError> {
    let req = decode(&request)?;
    let core = state.core.clone();
    let out = blocking(move || core.save_edited_image(Caller::Ui, req)).await?;
    let changed = OpResult { batch_id: out.batch_id, affected: 1, failed: Vec::new() };
    emit_changed(&app, LibraryChangeReason::FileOp, &changed, None);
    Ok(out.image)
}

#[tauri::command]
pub async fn convert_images(req: ConvertImagesReq, state: State<'_, AppState>, app: AppHandle) -> Result<OpResult, AppError> {
    let core = state.core.clone();
    let r = blocking(move || core.convert_images(Caller::Ui, req)).await?;
    emit_changed(&app, LibraryChangeReason::FileOp, &r, None);
    Ok(r)
}

#[tauri::command]
pub async fn stitch_images(req: StitchImagesReq, state: State<'_, AppState>, app: AppHandle) -> Result<ImageSummary, AppError> {
    let core = state.core.clone();
    let out = blocking(move || core.stitch_images(Caller::Ui, req)).await?;
    let changed = OpResult { batch_id: out.batch_id, affected: 1, failed: Vec::new() };
    emit_changed(&app, LibraryChangeReason::FileOp, &changed, Some(vec![out.image.id]));
    Ok(out.image)
}
