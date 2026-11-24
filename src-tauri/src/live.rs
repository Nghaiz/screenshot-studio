//! Phase 4.1 shell features: the global hotkey (setting `hotkey`), autostart (setting `autostart`), the
//! "new screenshot" toast and `open-image` (toast click, tray Recent, second launch with a file argument).

use std::path::Path;
use std::sync::Mutex;

use ss_core::contract::ipc::{AppWarningCode, NewScreenshot, OpenImage, SettingKey, events};
use ss_core::contract::{AppError, ErrorKind};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;
use crate::window;

/// The shortcut currently registered by [`apply_hotkey`] (None = none registered).
#[derive(Default)]
pub struct HotkeyState(Mutex<Option<Shortcut>>);

/// Checks a `hotkey` value parses as a shortcut (`Ctrl+Alt+S`); `invalidArgument` otherwise.
pub fn parse_hotkey(s: &str) -> Result<Shortcut, AppError> {
    s.parse::<Shortcut>()
        .map_err(|e| AppError::new(ErrorKind::InvalidArgument, format!("not a valid hotkey: `{s}`")).with_detail(e.to_string()))
}

/// (Re)registers the global hotkey that brings the main window to the front. A failure (taken by another
/// app, unparsable) becomes the `hotkeyUnavailable` warning, never an error: the app works without it.
pub fn apply_hotkey(app: &AppHandle, value: &str) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let core = state.core.clone();
    let held = app.state::<HotkeyState>();
    let mut current = held.0.lock().expect("hotkey state");
    let gs = app.global_shortcut();
    if let Some(old) = current.take()
        && let Err(e) = gs.unregister(old)
    {
        eprintln!("screenshot-studio: unregister hotkey: {e}");
    }
    let result = parse_hotkey(value).and_then(|shortcut| {
        gs.on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                window::focus_main(app);
            }
        })
        .map(|_| shortcut)
        .map_err(|e| AppError::new(ErrorKind::Conflict, format!("hotkey {value} is not available")).with_detail(e.to_string()))
    });
    match result {
        Ok(shortcut) => {
            *current = Some(shortcut);
            core.set_warning(AppWarningCode::HotkeyUnavailable, None);
        }
        Err(e) => {
            let msg = format!("{}{}", e.message, e.detail.map(|d| format!(": {d}")).unwrap_or_default());
            eprintln!("screenshot-studio: {msg}");
            core.set_warning(AppWarningCode::HotkeyUnavailable, Some(msg));
        }
    }
}

/// Turns autostart on or off through the plugin (HKCU Run). Called before the setting is stored, so a
/// failure leaves the stored value as it was.
pub fn apply_autostart(app: &AppHandle, on: bool) -> Result<(), AppError> {
    let al = app.autolaunch();
    let r = if on { al.enable() } else { al.disable() };
    r.map_err(|e| AppError::new(ErrorKind::Io, format!("could not turn autostart {}", if on { "on" } else { "off" })).with_detail(e.to_string()))
}

/// At startup the OS state wins: when the Run entry was added or removed outside the app (Task Manager),
/// the `autostart` setting is updated to match. Reads only (`is_enabled`), never writes the registry.
pub fn reconcile_autostart(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let enabled = match app.autolaunch().is_enabled() {
        Ok(v) => v,
        Err(e) => return eprintln!("screenshot-studio: autostart state unreadable: {e}"),
    };
    let stored = state.core.get_settings().ok().and_then(|s| s.0.get("autostart").and_then(|v| v.as_bool()));
    if stored != Some(enabled) {
        let req = ss_core::contract::ipc::SetSettingReq { key: SettingKey::Autostart, value: serde_json::Value::Bool(enabled) };
        if let Err(e) = state.core.set_setting(ss_core::contract::Caller::Ui, req) {
            eprintln!("screenshot-studio: store autostart: {}", e.message);
        }
    }
}

/// Shows the main window and asks the UI to open `image_id`.
pub fn open_image(app: &AppHandle, image_id: i64) {
    window::focus_main(app);
    if let Err(e) = app.emit(events::OPEN_IMAGE, OpenImage { image_id }) {
        eprintln!("screenshot-studio: emit {}: {e}", events::OPEN_IMAGE);
    }
}

/// Registers the toast click handler (before any toast is shown: earlier ones report nothing).
pub fn init_toast_clicks(app: &AppHandle) {
    let handle = app.clone();
    let r = app.notification().on_action(move |a| {
        if a.action_id() != "tap" {
            return;
        }
        match a.notification().map(|n| n.id()) {
            Some(id) if id > 0 => open_image(&handle, i64::from(id)),
            _ => window::focus_main(&handle),
        }
    });
    if let Err(e) = r {
        eprintln!("screenshot-studio: toast click handler: {e}");
    }
}

/// One toast per watcher batch: the newest file's name (`+N` more); a click opens that image.
pub fn toast_new(app: &AppHandle, items: &[NewScreenshot]) {
    let Some(last) = items.last() else { return };
    let body = if items.len() == 1 { last.file_name.clone() } else { format!("{} (+{})", last.file_name, items.len() - 1) };
    let mut b = app.notification().builder().body(body);
    if let Ok(id) = i32::try_from(last.image_id) {
        b = b.id(id);
    }
    if let Err(e) = b.show() {
        eprintln!("screenshot-studio: toast: {e}");
    }
}

/// Second launch: focus the window; a file argument inside a library root opens that image.
pub fn second_instance(app: &AppHandle, argv: Vec<String>) {
    window::focus_main(app);
    let Some(state) = app.try_state::<AppState>() else { return };
    let core = state.core.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        for arg in argv.iter().skip(1) {
            let path = Path::new(arg);
            if !path.is_file() {
                continue;
            }
            match core.image_for_path(path) {
                Ok(Some(id)) => {
                    open_image(&app, id);
                    return;
                }
                Ok(None) => eprintln!("screenshot-studio: {arg} is outside every library root"),
                Err(e) => eprintln!("screenshot-studio: open {arg}: {}: {}", e.message, e.detail.unwrap_or_default()),
            }
        }
    });
}
