//! Tray icon (phase 4.1): menu Open, Recent (the last [`RECENT_ITEMS`] via `get_recent`, a click opens the
//! image), Pause watcher, Quit; a left click toggles the small `tray-recent` popup window (its UI is phase
//! 4.2). Menu labels are Vietnamese, the app's default language (design-intent.md): the shell cannot read
//! the UI's language choice, which lives in the webview's `localStorage` (D9).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ss_core::contract::Caller;
use ss_core::contract::ipc::{LimitReq, SetSettingReq, SettingKey};
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::live;
use crate::state::AppState;
use crate::window;

pub const TRAY_ID: &str = "main";
/// Label of the popup window; listed in the capability's `windows` (contract § Capabilities).
pub const RECENT_LABEL: &str = "tray-recent";
/// Items in the Recent submenu.
pub const RECENT_ITEMS: i64 = 8;
const POPUP_SIZE: (f64, f64) = (360.0, 480.0);
/// Smallest gap between two menu rebuilds (index batches can fire `library-changed` many times a second).
const REFRESH_MIN_GAP: Duration = Duration::from_millis(1500);

const ID_OPEN: &str = "open";
const ID_PAUSE: &str = "pause";
const ID_QUIT: &str = "quit";
const RECENT_PREFIX: &str = "recent:";

/// When the menu was last rebuilt.
#[derive(Default)]
pub struct TrayState(Mutex<Option<Instant>>);

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let core = app.state::<AppState>().core.clone();
    let recent = core.get_recent(LimitReq { limit: RECENT_ITEMS }).unwrap_or_else(|e| {
        eprintln!("screenshot-studio: tray recent: {}", e.message);
        Vec::new()
    });
    let paused = core
        .get_settings()
        .ok()
        .and_then(|s| s.0.get("watcherEnabled").and_then(|v| v.as_bool()))
        .is_some_and(|on| !on);
    let items: Vec<MenuItem<tauri::Wry>> = recent
        .iter()
        .map(|s| MenuItem::with_id(app, format!("{RECENT_PREFIX}{}", s.id), s.file_name.replace('&', "&&"), true, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let empty = MenuItem::with_id(app, "recent-empty", "(trống)", false, None::<&str>)?;
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        if items.is_empty() { vec![&empty] } else { items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect() };
    let recent_menu = Submenu::with_id_and_items(app, "recent", "Gần đây", true, &refs)?;
    let open = MenuItem::with_id(app, ID_OPEN, "Mở Screenshot Studio", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, ID_PAUSE, "Tạm dừng theo dõi thư mục", true, paused, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Thoát", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&open, &recent_menu, &pause, &sep, &quit])
}

/// Creates the tray icon (after the app state is managed).
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    app.manage(TrayState::default());
    let mut b = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Screenshot Studio")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(on_icon);
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

/// Rebuilds the menu (Recent and the pause check), at most once per [`REFRESH_MIN_GAP`] unless `force`.
pub fn refresh(app: &AppHandle, force: bool) {
    let Some(state) = app.try_state::<TrayState>() else { return };
    {
        let mut last = state.0.lock().expect("tray state");
        if !force && last.is_some_and(|t| t.elapsed() < REFRESH_MIN_GAP) {
            return;
        }
        *last = Some(Instant::now());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
        match build_menu(&app) {
            Ok(menu) => {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    eprintln!("screenshot-studio: tray menu: {e}");
                }
            }
            Err(e) => eprintln!("screenshot-studio: tray menu: {e}"),
        }
    });
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    match id {
        ID_OPEN => window::focus_main(app),
        ID_QUIT => app.exit(0),
        ID_PAUSE => {
            let core = app.state::<AppState>().core.clone();
            let on = core.get_settings().ok().and_then(|s| s.0.get("watcherEnabled").and_then(|v| v.as_bool())).unwrap_or(true);
            let req = SetSettingReq { key: SettingKey::WatcherEnabled, value: serde_json::Value::Bool(!on) };
            if let Err(e) = core.set_setting(Caller::Ui, req) {
                eprintln!("screenshot-studio: pause watcher: {}", e.message);
            }
            refresh(app, true);
        }
        _ => {
            if let Some(image_id) = id.strip_prefix(RECENT_PREFIX).and_then(|n| n.parse::<i64>().ok()) {
                live::open_image(app, image_id);
            }
        }
    }
}

fn on_icon(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, position, .. } = event
        && let Err(e) = toggle_popup(tray.app_handle(), position)
    {
        eprintln!("screenshot-studio: tray popup: {e}");
    }
}

/// Shows the popup above the click point (the taskbar is usually at the bottom), or hides it.
fn toggle_popup(app: &AppHandle, at: PhysicalPosition<f64>) -> tauri::Result<()> {
    let popup = match app.get_webview_window(RECENT_LABEL) {
        Some(w) if w.is_visible()? => return w.hide(),
        Some(w) => w,
        None => {
            let data_dir = app.state::<AppState>().core.data_dir().to_path_buf();
            let w = WebviewWindowBuilder::new(app, RECENT_LABEL, WebviewUrl::default())
                .title("Screenshot Studio")
                .inner_size(POPUP_SIZE.0, POPUP_SIZE.1)
                .resizable(false)
                .decorations(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .visible(false)
                // Same WebView2 folder as the main window: one browser process (window.rs).
                .data_directory(window::webview_data_dir(&data_dir))
                .build()?;
            let handle = w.clone();
            w.on_window_event(move |e| {
                if let WindowEvent::Focused(false) = e {
                    let _ = handle.hide();
                }
            });
            w
        }
    };
    let size = popup.outer_size()?;
    let x = (at.x - size.width as f64).max(0.0);
    let y = (at.y - size.height as f64).max(0.0);
    popup.set_position(PhysicalPosition::new(x, y))?;
    popup.show()?;
    popup.set_focus()?;
    refresh(app, false);
    Ok(())
}
