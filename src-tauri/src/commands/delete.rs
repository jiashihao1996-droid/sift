// ============================================================
// Sift - Delete Command
// IPC wrapper for organize::delete_batch; injects the system trash and
// forwards progress as events
// ============================================================

use crate::media::cache_dirs;
use crate::models::organize::{DeletePairInput, DeleteResult, OrganizeError};
use crate::organize::delete_batch::{delete_pairs as delete_batch, SystemTrash};
use tauri::{AppHandle, Emitter};

/// Trash every pair, carrying on past failures; the result lists each pair
/// as trashed or failed.
#[tauri::command]
pub async fn delete_pairs(
    app: AppHandle,
    pairs: Vec<DeletePairInput>,
) -> Result<DeleteResult, OrganizeError> {
    tokio::task::spawn_blocking(move || {
        let preview_cache = cache_dirs::path(cache_dirs::RAW_PREVIEWS);
        delete_batch(&pairs, &preview_cache, &mut SystemTrash, &mut |progress| {
            let _ = app.emit("delete-progress", progress);
        })
    })
    .await
    .map_err(|e| OrganizeError::other(format!("Task join error: {}", e)))
}
