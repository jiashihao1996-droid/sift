// ============================================================
// Sift - Cache Lifecycle
// The caches live in the app cache dir and outlive restarts. They are
// trimmed to their size limit at startup (in the background; commands
// that read or write them wait for it) and when the window is destroyed,
// the two moments no session can be showing or archiving a cached file.
// ============================================================

use crate::media::{cache_dirs, cache_lru};
use std::env;
use std::fs;
use std::sync::{Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use tauri::Manager;

static STARTUP_TRIM: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Put the caches in the app cache dir; falls back to the temp dir
pub fn init_cache_root(app: &tauri::App) {
    match app.path().app_cache_dir() {
        Ok(dir) => cache_dirs::init(dir),
        Err(e) => eprintln!("[Sift] No app cache dir, caching in the temp dir: {}", e),
    }
}

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

/// Remove the caches that versions up to 1.0.2 kept in the temp dir
pub fn remove_legacy_caches() {
    let temp = env::temp_dir();
    for name in cache_dirs::LEGACY_TEMP_DIRS {
        let dir = temp.join(name);
        if dir.is_dir() {
            if let Err(e) = fs::remove_dir_all(&dir) {
                eprintln!("[Sift] Failed to remove old cache {}: {}", dir.display(), e);
            }
        }
    }
}

/// Trim the caches in the background; call before any command can run
pub fn start_trim() {
    let handle = thread::spawn(trim_now);
    *STARTUP_TRIM.lock().unwrap_or_else(PoisonError::into_inner) = Some(handle);
}

/// Block until the startup trim is done, so it never removes a file a scan
/// has just found in the cache. The lock is held while joining, so every
/// caller waits, not just the first.
pub fn wait_for_trim() {
    let mut startup = STARTUP_TRIM.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(handle) = startup.take() {
        if handle.join().is_err() {
            eprintln!("[Sift] Cache trim panicked");
        }
    }
}

/// Trim the caches now, after any startup trim; for when the window closes
pub fn trim_sync() {
    wait_for_trim();
    trim_now();
}

fn trim_now() {
    let dirs = cache_dirs::ALL.map(cache_dirs::path);
    let trimmed = cache_lru::trim(&dirs, cache_lru::MAX_CACHE_BYTES);
    if trimmed.removed_files > 0 {
        eprintln!(
            "[Sift] Trimmed the cache: removed {} files ({} MB), kept {} MB",
            trimmed.removed_files,
            trimmed.removed_bytes / 1_000_000,
            trimmed.kept_bytes / 1_000_000
        );
    }
}
