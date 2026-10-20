// ============================================================
// Sift - Cache Directories
// Single source of truth for where Sift's caches live. They sit in the
// app cache dir and outlive restarts; media/cache_lru.rs keeps their
// total size in bounds.
// ============================================================

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

pub const THUMBNAILS: &str = "thumbnails";
pub const RAW_PREVIEWS: &str = "raw-previews";

/// Every cache dir; the size limit applies to all of them together.
pub const ALL: [&str; 2] = [THUMBNAILS, RAW_PREVIEWS];

/// Where versions up to 1.0.2 kept the caches, under the system temp dir
pub const LEGACY_TEMP_DIRS: [&str; 2] = ["sift-thumbnails", "sift-raw-previews"];

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set the folder holding the cache dirs, once at startup (the app cache dir)
pub fn init(root: PathBuf) {
    if ROOT.set(root).is_err() {
        eprintln!("[Sift] Cache root was already set");
    }
}

/// `<root>/<name>`, without creating it. Before `init` (or if it was never
/// called) the root is a folder in the system temp dir.
pub fn path(name: &str) -> PathBuf {
    ROOT.get()
        .cloned()
        .unwrap_or_else(|| env::temp_dir().join("sift-cache"))
        .join(name)
}

/// Return `<root>/<name>`, creating it if needed.
pub fn ensure(name: &str) -> Result<PathBuf, String> {
    let dir = path(name);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir {}: {}", name, e))?;
    Ok(dir)
}
