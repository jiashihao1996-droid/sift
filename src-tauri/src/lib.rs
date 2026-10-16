// ============================================================
// Sift - Tauri Library Entry Point
// Registers all commands and plugins
// ============================================================

mod catalog;
mod commands;
mod media;
mod models;
mod organize;
mod os;
mod paths;
#[cfg(test)]
mod test_support;

use commands::{
    af_point, archive, cache, delete, exif, export, file_actions, organizing, progress, scan,
    thumbnail,
};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must come first: a second instance exits inside this plugin's setup,
        // before anything below runs. The cache dirs have fixed names, so a
        // second instance would otherwise wipe the first one's RAW previews.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(organizing::Organizing::default())
        .setup(|app| {
            // Clean up any leftover cache from previous sessions; only the
            // single running instance gets here
            cache::cleanup_cache_sync();
            cache::allow_cache_dirs(app)?;
            scan::restore_folder_grants(app);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Archiving, exporting or deleting must not be cut off mid-way
            WindowEvent::CloseRequested { api, .. }
                if organizing::refuse_while_organizing(window.app_handle()) =>
            {
                api.prevent_close();
            }
            WindowEvent::Destroyed => cache::cleanup_cache_sync(),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            scan::scan_folder,
            delete::delete_pairs,
            delete::folder_no_trash_reason,
            archive::archive_photos,
            export::export_picks,
            thumbnail::generate_thumbnails,
            file_actions::show_in_folder,
            file_actions::copy_image_to_clipboard,
            cache::cleanup_cache,
            exif::read_exif,
            af_point::read_af_point,
            progress::load_progress,
            progress::save_progress,
            progress::clear_progress,
            organizing::set_organizing,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Quitting (Cmd+Q on macOS) is refused for the same reason as closing
            if let RunEvent::ExitRequested { api, .. } = event {
                if organizing::refuse_while_organizing(app) {
                    api.prevent_exit();
                }
            }
        });
}

/// Bring the running instance's window forward when the user launches Sift again
fn focus_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let shown = window
        .unminimize()
        .and_then(|_| window.show())
        .and_then(|_| window.set_focus());
    if let Err(e) = shown {
        eprintln!("[Sift] Failed to focus the existing window: {}", e);
    }
}
