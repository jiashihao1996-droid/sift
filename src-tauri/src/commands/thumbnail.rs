// ============================================================
// Sift - Thumbnail Command
// ============================================================

use crate::commands::cache;
use crate::media::{cache_dirs, thumbnail};
use crate::models::photo::{ThumbnailInput, ThumbnailResult};

#[tauri::command]
pub async fn generate_thumbnails(
    pairs: Vec<ThumbnailInput>,
) -> Result<Vec<ThumbnailResult>, String> {
    tokio::task::spawn_blocking(move || {
        cache::wait_for_trim();
        let cache_dir = cache_dirs::ensure(cache_dirs::THUMBNAILS)?;
        Ok(thumbnail::generate_thumbnails(&pairs, &cache_dir))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
