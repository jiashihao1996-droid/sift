// ============================================================
// Sift - Export Command
// IPC wrapper for organize::export; forwards progress as events
// ============================================================

use crate::models::photo::{ExportPairInput, ExportResult};
use crate::organize::export::export_pairs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn export_picks(
    app: AppHandle,
    pairs: Vec<ExportPairInput>,
    target_folder: String,
) -> Result<ExportResult, String> {
    tokio::task::spawn_blocking(move || {
        let exported_count = export_pairs(&pairs, Path::new(&target_folder), |progress| {
            let _ = app.emit("export-progress", progress);
        })?;
        Ok(ExportResult {
            exported_count,
            export_folder: target_folder,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
