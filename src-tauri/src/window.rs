//! The main window: created in `setup` (not from config) so its WebView2 profile lives in the app's data dir,
//! and brought to the front when a second instance starts (phase 2.3).

use std::path::{Path, PathBuf};

use tauri::{App, AppHandle, Manager, WebviewWindowBuilder};

/// Label of the main window in `tauri.conf.json` (`create: false`: [`create_main`] builds it).
pub const MAIN_LABEL: &str = "main";
/// WebView2 user data folder inside the data dir; WebView2 writes its `EBWebView` profile under it.
pub const WEBVIEW_DIR: &str = "webview";

/// `<data dir>\webview`. Without an explicit folder Tauri uses `%LOCALAPPDATA%\<identifier>` on C:, while every
/// byte of app data belongs under the data dir (design-intent.md § Names and paths). Every later window
/// (tray popup, phase 4.1) must use this same folder: WebView2 shares one browser process per folder.
pub fn webview_data_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(WEBVIEW_DIR)
}

/// Builds the main window from its `tauri.conf.json` entry with the WebView2 data folder under `data_dir`.
pub fn create_main(app: &App, data_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN_LABEL)
        .ok_or_else(|| format!("tauri.conf.json has no window labelled `{MAIN_LABEL}`"))?
        .clone();
    WebviewWindowBuilder::from_config(app.handle(), &config)?
        .data_directory(webview_data_dir(data_dir))
        .build()?;
    Ok(())
}

/// Second launch (single-instance plugin callback): restore, show and focus the existing main window.
pub fn focus_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        eprintln!("screenshot-studio: second instance started before the main window exists");
        return;
    };
    for (step, result) in [("unminimize", window.unminimize()), ("show", window.show()), ("set_focus", window.set_focus())] {
        if let Err(e) = result {
            eprintln!("screenshot-studio: focus main window ({step}): {e}");
        }
    }
}
