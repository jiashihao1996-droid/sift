// ============================================================
// Sift - Cache Cleanup Command
// Removes temporary thumbnail and RAW preview cache directories
// ============================================================

use crate::media::cache_dirs;
use std::env;
use std::fs;
use tauri::Manager;

/// Let the webview load images from Sift's cache dirs via the asset protocol.
///
/// The static asset scope is empty: user photo folders are granted only when
/// the user picks them in the folder dialog (tauri-plugin-dialog adds a picked
/// directory to the asset scope recursively). Dirs are created first so the
/// canonical path (e.g. /private/var/... on macOS) is registered too.
pub fn allow_cache_dirs(app: &tauri::App) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    for name in cache_dirs::ALL {
        let dir = cache_dirs::ensure(name)?;
        scope
            .allow_directory(&dir, true)
            .map_err(|e| format!("Failed to allow cache dir {}: {}", name, e))?;
    }
    Ok(())
}

/// Synchronous cache cleanup — safe to call from startup and window close events
pub fn cleanup_cache_sync() {
    let temp = env::temp_dir();
    for dir_name in &cache_dirs::ALL {
        let dir = temp.join(dir_name);
        if dir.exists() {
            if let Err(e) = fs::remove_dir_all(&dir) {
                eprintln!("[Sift] Failed to clean cache {}: {}", dir_name, e);
            }
        }
    }
}

#[tauri::command]
pub async fn cleanup_cache() -> Result<(), String> {
    tokio::task::spawn_blocking(cleanup_cache_sync)
        .await
        .map_err(|e| format!("Task join error: {}", e))
}

/// The RAW preview cache that scan extracts into. Files in here are
/// Sift's own temp files; everything outside it belongs to the user.
pub fn raw_preview_cache_dir() -> std::path::PathBuf {
    env::temp_dir().join(cache_dirs::RAW_PREVIEWS)
}

/// True only if `path` exists and, with symlinks and `..` resolved, lies
/// strictly inside `dir`. Gate for every `remove_file` on a cache path.
pub fn is_inside_dir(path: &std::path::Path, dir: &std::path::Path) -> bool {
    match (path.canonicalize(), dir.canonicalize()) {
        (Ok(path), Ok(dir)) => path != dir && path.starts_with(&dir),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn accepts_files_inside_the_dir_at_any_depth() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        touch(&cache.join("a.jpg"));
        touch(&cache.join("sub/b.jpg"));

        assert!(is_inside_dir(&cache.join("a.jpg"), &cache));
        assert!(is_inside_dir(&cache.join("sub/b.jpg"), &cache));
    }

    #[test]
    fn rejects_the_dir_itself_siblings_and_missing_files() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        touch(&cache.join("a.jpg"));
        touch(&tmp.path().join("cache2/a.jpg"));
        touch(&tmp.path().join("photo.jpg"));

        assert!(!is_inside_dir(&cache, &cache));
        assert!(!is_inside_dir(&tmp.path().join("cache2/a.jpg"), &cache));
        assert!(!is_inside_dir(&tmp.path().join("photo.jpg"), &cache));
        assert!(!is_inside_dir(&cache.join("missing.jpg"), &cache));
        assert!(!is_inside_dir(
            &cache.join("a.jpg"),
            &tmp.path().join("no-cache")
        ));
    }

    #[test]
    fn rejects_dot_dot_escapes() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        touch(&tmp.path().join("photo.jpg"));

        assert!(!is_inside_dir(&cache.join("../photo.jpg"), &cache));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_point_outside() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let photo = tmp.path().join("photo.jpg");
        touch(&photo);
        std::os::unix::fs::symlink(&photo, cache.join("link.jpg")).unwrap();

        assert!(!is_inside_dir(&cache.join("link.jpg"), &cache));
    }
}
