//! Screenshot Studio Tauri shell. Phase 1.1: every plugin registered with no behaviour yet, every
//! contract command wired to its stub (contract_test.rs (e) checks this handler list).

mod commands;

use commands::{ai, edit, fileops, library, organize, query};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance MUST be the first plugin (plugin docs). Second-launch focus + `open-image` land in 4.1.
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_drag::init())
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
        .run(tauri::generate_context!())
        .expect("error while running Screenshot Studio");
}
