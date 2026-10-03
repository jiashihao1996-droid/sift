// ============================================================
// Sift - Culling Progress Commands
// Save / restore marks per folder (see catalog/progress.rs)
// ============================================================

use crate::catalog::progress;
use crate::models::photo::FolderProgress;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn store_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(progress::PROGRESS_DIR))
        .map_err(|e| format!("No app data dir: {}", e))
}

#[tauri::command]
pub async fn load_progress(
    app: AppHandle,
    folder_path: String,
) -> Result<Option<FolderProgress>, String> {
    let dir = store_dir(&app)?;
    tokio::task::spawn_blocking(move || progress::load(&dir, &folder_path))
        .await
        .map_err(|e| format!("Task join error: {}", e))
}

#[tauri::command]
pub async fn save_progress(app: AppHandle, progress: FolderProgress) -> Result<(), String> {
    let dir = store_dir(&app)?;
    tokio::task::spawn_blocking(move || progress::save(&dir, &progress))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn clear_progress(app: AppHandle, folder_path: String) -> Result<(), String> {
    let dir = store_dir(&app)?;
    tokio::task::spawn_blocking(move || progress::clear(&dir, &folder_path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}
