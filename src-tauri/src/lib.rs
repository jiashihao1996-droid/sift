// ============================================================
// Sift - Tauri Library Entry Point
// Registers all commands and plugins
// ============================================================

mod catalog;
mod commands;
mod media;
mod models;
mod paths;
#[cfg(test)]
mod test_support;

use commands::{archive, cache, delete, exif, export, file_actions, scan, thumbnail};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Clean up any leftover cache from previous sessions
    cache::cleanup_cache_sync();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                cache::cleanup_cache_sync();
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan::scan_folder,
            delete::delete_pair,
            archive::archive_photos,
            export::export_picks,
            thumbnail::generate_thumbnails,
            file_actions::show_in_folder,
            file_actions::copy_image_to_clipboard,
            cache::cleanup_cache,
            exif::read_exif,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
