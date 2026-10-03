// ============================================================
// Sift - Cache Cleanup Command
// Removes temporary thumbnail and RAW preview cache directories
// ============================================================

use crate::media::cache_dirs;
use std::env;
use std::fs;
use tauri::Manager;

/// Let the webview load images from Sift's cache dirs via the asset protocol.
///
/// The static asset scope is empty: user photo folders are granted only when
/// the user picks them in the folder dialog (tauri-plugin-dialog adds a picked
/// directory to the asset scope recursively). Dirs are created first so the
/// canonical path (e.g. /private/var/... on macOS) is registered too.
pub fn allow_cache_dirs(app: &tauri::App) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    for name in cache_dirs::ALL {
        let dir = cache_dirs::ensure(name)?;
        scope
            .allow_directory(&dir, true)
            .map_err(|e| format!("Failed to allow cache dir {}: {}", name, e))?;
    }
    Ok(())
}

/// Synchronous cache cleanup — safe to call from startup and window close events
pub fn cleanup_cache_sync() {
    let temp = env::temp_dir();
    for dir_name in &cache_dirs::ALL {
        let dir = temp.join(dir_name);
        if dir.exists() {
            if let Err(e) = fs::remove_dir_all(&dir) {
                eprintln!("[Sift] Failed to clean cache {}: {}", dir_name, e);
            }
        }
    }
}

#[tauri::command]
pub async fn cleanup_cache() -> Result<(), String> {
    tokio::task::spawn_blocking(cleanup_cache_sync)
        .await
        .map_err(|e| format!("Task join error: {}", e))
}
