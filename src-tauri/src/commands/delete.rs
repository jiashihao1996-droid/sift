// ============================================================
// Sift - Delete Command
// IPC wrapper for organize::delete; injects the system trash
// ============================================================

use crate::media::cache_dirs;
use crate::models::organize::OrganizeError;
use crate::organize::delete::{execute_delete, plan_delete};

#[tauri::command]
pub async fn delete_pair(
    jpg_path: String,
    raw_path: Option<String>,
    source: Option<String>,
    xmp_paths: Option<Vec<String>>,
) -> Result<(), OrganizeError> {
    tokio::task::spawn_blocking(move || {
        let plan = plan_delete(
            &jpg_path,
            raw_path.as_deref(),
            source.as_deref(),
            xmp_paths.as_deref().unwrap_or_default(),
            &cache_dirs::path(cache_dirs::RAW_PREVIEWS),
        )?;
        execute_delete(&plan, |path| {
            trash::delete(path).map_err(|e| OrganizeError::trash(&e, path))
        })
    })
    .await
    .map_err(|e| OrganizeError::other(format!("Task join error: {}", e)))?
}
