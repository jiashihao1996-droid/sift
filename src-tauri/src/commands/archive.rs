// ============================================================
// Sift - Archive Command
// IPC wrapper for organize::archive; forwards progress as events and
// rejects with an `OrganizeError`
// ============================================================

use crate::media::cache_dirs;
use crate::models::organize::OrganizeError;
use crate::models::photo::{ArchivePairInput, ArchiveResult};
use crate::organize::archive::archive_pairs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn archive_photos(
    app: AppHandle,
    folder_path: String,
    pairs: Vec<ArchivePairInput>,
) -> Result<ArchiveResult, OrganizeError> {
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
    .map_err(|e| OrganizeError::other(format!("Task join error: {}", e)))?
}
