// ============================================================
// Sift - Delete Command
// IPC wrapper for organize::delete_batch; injects the system trash and
// forwards progress as events
// ============================================================

use crate::media::cache_dirs;
use crate::models::organize::{DeletePairInput, DeleteResult, NoTrashReason, OrganizeError};
use crate::organize::delete_batch::{delete_pairs as delete_batch, SystemTrash};
use crate::organize::no_trash::TrashOrRemove;
use crate::os::trash_support::no_trash_reason;
use std::path::Path;
use tauri::{AppHandle, Emitter};

/// Trash every pair, carrying on past failures; the result lists each pair
/// as trashed or failed. `permanent` is the user's confirmation, for this
/// archive, that files on a volume without a trash are deleted for good; it
/// never affects a volume that has one.
#[tauri::command]
pub async fn delete_pairs(
    app: AppHandle,
    pairs: Vec<DeletePairInput>,
    permanent: bool,
) -> Result<DeleteResult, OrganizeError> {
    tokio::task::spawn_blocking(move || {
        let preview_cache = cache_dirs::path(cache_dirs::RAW_PREVIEWS);
        let mut trash = TrashOrRemove::new(SystemTrash, permanent, no_trash_reason);
        delete_batch(&pairs, &preview_cache, &mut trash, &mut |progress| {
            let _ = app.emit("delete-progress", progress);
        })
    })
    .await
    .map_err(|e| OrganizeError::other(format!("Task join error: {}", e)))
}

/// Why the volume holding `folder_path` has no trash, or `None` when it has
/// one. Only reads volume information.
#[tauri::command]
pub async fn folder_no_trash_reason(folder_path: String) -> Option<NoTrashReason> {
    tokio::task::spawn_blocking(move || no_trash_reason(Path::new(&folder_path)))
        .await
        .unwrap_or_else(|e| {
            eprintln!("[Sift] Trash check failed: {}", e);
            None
        })
}
