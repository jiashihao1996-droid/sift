// ============================================================
// Sift - Focus Point Command
// Where the camera focused, for the viewer's focus overlay. Paths come
// from the frontend and are only read, a few KB from the start.
// ============================================================

use crate::media::af_point::read_for_pair;
use crate::models::af_point::AfInfo;
use std::path::Path;

/// Async so the file read runs off the main thread instead of stalling the UI
#[tauri::command]
pub async fn read_af_point(
    jpg_path: String,
    raw_path: Option<String>,
    source: Option<String>,
) -> Result<AfInfo, String> {
    tokio::task::spawn_blocking(move || {
        let is_raw_preview = source.as_deref() == Some("rawPreview");
        read_for_pair(
            Path::new(&jpg_path),
            raw_path.as_deref().map(Path::new),
            is_raw_preview,
        )
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))
}
