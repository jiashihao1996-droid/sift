// ============================================================
// Sift - Archive Command
// IPC wrapper for organize::archive; forwards progress as events
// ============================================================

use crate::media::cache_dirs;
use crate::models::photo::{ArchivePairInput, ArchiveResult};
use crate::organize::archive::archive_pairs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn archive_photos(
    app: AppHandle,
    folder_path: String,
    pairs: Vec<ArchivePairInput>,
) -> Result<ArchiveResult, String> {
    tokio::task::spawn_blocking(move || {
        let preview_cache = cache_dirs::path(cache_dirs::RAW_PREVIEWS);
        archive_pairs(
            Path::new(&folder_path),
            &pairs,
            &preview_cache,
            &mut |progress| {
                let _ = app.emit("archive-progress", progress);
            },
        )
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
