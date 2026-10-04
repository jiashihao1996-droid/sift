// ============================================================
// Sift - File Action Commands
// Provides "Show in Finder" and "Copy Image to Clipboard"
// ============================================================

use crate::os::{clipboard, reveal};
use std::path::PathBuf;

/// Open the system file manager and highlight the given file.
#[tauri::command]
pub async fn show_in_folder(path: String) -> Result<(), String> {
    let path = existing_file(path)?;
    tokio::task::spawn_blocking(move || reveal::reveal(&path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

/// Copy an image file to the system clipboard.
#[tauri::command]
pub async fn copy_image_to_clipboard(path: String) -> Result<(), String> {
    let path = existing_file(path)?;
    tokio::task::spawn_blocking(move || clipboard::copy_image(&path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn existing_file(path: String) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }
    Ok(path)
}
