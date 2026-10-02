// ============================================================
// Sift - Scan Command
// Only scans folders the user granted (folder dialog or drag-and-drop);
// see catalog/folder_grants.rs.
// ============================================================

use crate::catalog::{self, folder_grants};
use crate::media::cache_dirs;
use crate::models::photo::ScanResult;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const GRANTS_FILE: &str = "granted-folders.json";

const NOT_GRANTED: &str = "没有访问该文件夹的权限，请点击「选择文件夹」或把文件夹拖进窗口重新打开";

#[tauri::command]
pub async fn scan_folder(app: AppHandle, folder_path: String) -> Result<ScanResult, String> {
    tokio::task::spawn_blocking(move || {
        let folder = Path::new(&folder_path);
        authorize(&app, folder)?;
        let cache_dir = cache_dirs::ensure(cache_dirs::RAW_PREVIEWS)?;
        catalog::scan::scan_folder(folder, &cache_dir)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Refuse folders the user never granted; for granted ones, extend the asset
/// scope to subfolders (archived JPG/ and RAW/) and remember the grant.
fn authorize(app: &AppHandle, folder: &Path) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    if !folder_grants::is_granted(folder, |p| scope.is_allowed(p)) {
        return Err(NOT_GRANTED.to_string());
    }
    scope
        .allow_directory(folder, true)
        .map_err(|e| format!("Failed to allow folder: {}", e))?;

    let file = grants_file(app)?;
    let folders = folder_grants::remember(
        &folder_grants::load(&file),
        folder,
        folder_grants::MAX_GRANTED_FOLDERS,
    );
    // Not fatal: the folder is granted for this run either way
    if let Err(e) = folder_grants::save(&file, &folders) {
        eprintln!("[Sift] {}", e);
    }
    Ok(())
}

/// Re-grant folders the user opened in earlier runs, so "recent folders" work.
pub fn restore_folder_grants(app: &tauri::App) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    let file = grants_file(app.handle())?;
    for folder in folder_grants::load(&file).iter().filter(|f| f.is_dir()) {
        scope
            .allow_directory(folder, true)
            .map_err(|e| format!("Failed to restore folder grant: {}", e))?;
    }
    Ok(())
}

fn grants_file(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(GRANTS_FILE))
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))
}
