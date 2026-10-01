// ============================================================
// Sift - Scan Command
// ============================================================

use crate::catalog;
use crate::media::cache_dirs;
use crate::models::photo::ScanResult;
use std::path::Path;

#[tauri::command]
pub async fn scan_folder(folder_path: String) -> Result<ScanResult, String> {
    tokio::task::spawn_blocking(move || {
        let cache_dir = cache_dirs::ensure(cache_dirs::RAW_PREVIEWS)?;
        catalog::scan::scan_folder(Path::new(&folder_path), &cache_dir)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
