// ============================================================
// Sift - Archive Command
// IPC wrapper for organize::archive; forwards progress as events and
// rejects with an `OrganizeError`
// ============================================================

use crate::media::cache_dirs;
use crate::models::organize::OrganizeError;
use crate::models::photo::{ArchivePairInput, ArchiveResult};
use crate::organize::archive::archive_pairs;
use crate::organize::delete_batch::SystemTrash;
use crate::organize::no_trash::TrashOrRemove;
use crate::os::trash_support::no_trash_reason;
use std::path::Path;
use tauri::{AppHandle, Emitter};

/// `permanent` is the user's confirmation, for this archive, that files on a
/// volume without a trash are deleted for good: here only the previews of
/// unstarred RAW-only picks are ever deleted.
#[tauri::command]
pub async fn archive_photos(
    app: AppHandle,
    folder_path: String,
    pairs: Vec<ArchivePairInput>,
    permanent: bool,
) -> Result<ArchiveResult, OrganizeError> {
    tokio::task::spawn_blocking(move || {
        let preview_cache = cache_dirs::path(cache_dirs::RAW_PREVIEWS);
        let mut trash = TrashOrRemove::new(SystemTrash, permanent, no_trash_reason);
        archive_pairs(
            Path::new(&folder_path),
            &pairs,
            &preview_cache,
            &mut trash,
            &mut |progress| {
                let _ = app.emit("archive-progress", progress);
            },
        )
    })
    .await
    .map_err(|e| OrganizeError::other(format!("Task join error: {}", e)))?
}
