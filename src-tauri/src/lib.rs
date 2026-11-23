//! Screenshot Studio Tauri shell: plugins, the `ss-core` app state, the main window (WebView2 profile in the
//! data dir), the async `ssimg` scheme (served off the main thread), the AI sidecar (started after the window,
//! status pushed as `sidecar-status`, stopped on exit) and every contract command (contract_test.rs (e) checks
//! this handler list).

mod commands;
mod state;
mod window;

use commands::{ai, edit, fileops, library, organize, query};
use ss_core::AppCore;
use ss_core::contract::ipc::{events, ssimg};
use ss_core::sidecar::SidecarLayout;
use tauri::{Emitter, Manager};

use state::{AppState, http_response};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance MUST be the first plugin (plugin docs). A second launch exits and focuses this
        // instance's window; forwarding a file argument as `open-image` lands in 4.1.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| window::focus_main(app)))
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            let core = AppCore::open_default()?;
            let data_dir = core.data_dir().to_path_buf();
            // Sidecar source: the bundled resources when they carry it (`<resource_dir>/sidecar` and
            // `<resource_dir>/contracts/sidecar-jsonrpc.schema.json`, mapped by phase 3.4), else the dev checkout.
            if let Ok(res) = app.path().resource_dir() {
                let layout = SidecarLayout::under(&res);
                if layout.is_present() {
                    core.set_sidecar_layout(layout);
                }
            }
            let handle = app.handle().clone();
            core.sidecar().set_listener(move |s| {
                if let Err(e) = handle.emit(events::SIDECAR_STATUS, s) {
                    eprintln!("screenshot-studio: emit {}: {e}", events::SIDECAR_STATUS);
                }
            });
            app.manage(AppState { core: core.clone() });
            // After `manage`: the page's first command or `ssimg` request must find the state.
            window::create_main(app, &data_dir)?;
            // Absent venv = state `absent`, nothing spawned; the app works the same minus AI (D6).
            if let Err(e) = core.start_sidecar() {
                eprintln!("screenshot-studio: sidecar: {}: {}", e.message, e.detail.unwrap_or_default());
            }
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol(ssimg::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                let response = match app.try_state::<AppState>() {
                    Some(state) => state.core.ssimg(&path),
                    None => ss_core::ssimg::SsimgResponse { status: 503, headers: Vec::new(), body: Vec::new() },
                };
                responder.respond(http_response(response));
            });
        })
        .invoke_handler(tauri::generate_handler![
            library::get_app_info,
            library::add_library_root,
            library::remove_library_root,
            library::start_index,
            library::download_cloud_only,
            library::cancel_job,
            library::get_settings,
            library::set_setting,
            library::get_counts,
            library::get_recent,
            query::query_images,
            query::get_image,
            query::get_timeline,
            query::list_journal,
            query::get_stats,
            organize::list_tags,
            organize::create_tag,
            organize::update_tag,
            organize::delete_tag,
            organize::set_tags,
            organize::list_albums,
            organize::create_album,
            organize::rename_album,
            organize::delete_album,
            organize::album_add,
            organize::album_remove,
            organize::album_reorder,
            organize::set_favorite,
            organize::set_rating,
            organize::set_note,
            organize::list_smart_filters,
            organize::save_smart_filter,
            organize::delete_smart_filter,
            fileops::move_images,
            fileops::preview_rename,
            fileops::rename_images,
            fileops::trash_images,
            fileops::restore_images,
            fileops::recycle_images,
            fileops::request_permanent_delete,
            fileops::delete_permanently,
            fileops::undo,
            fileops::export_zip,
            fileops::export_pdf,
            fileops::copy_image_to_clipboard,
            ai::get_sidecar_status,
            ai::restart_sidecar,
            ai::start_ai,
            ai::find_duplicates,
            ai::get_sessions,
            ai::detect_issues,
            ai::detect_pii,
            edit::save_edited_image,
            edit::convert_images,
            edit::stitch_images,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Screenshot Studio")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // `shutdown`, then a kill after the shutdown wait (contract § Lifecycle 4).
                if let Some(state) = app.try_state::<AppState>() {
                    state.core.shutdown_sidecar();
                }
            }
        });
}
