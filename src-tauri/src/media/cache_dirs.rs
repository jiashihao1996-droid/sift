// ============================================================
// Sift - Cache Directories
// Single source of truth for the temp dirs Sift creates and wipes.
// ============================================================

use std::env;
use std::fs;
use std::path::PathBuf;

pub const THUMBNAILS: &str = "sift-thumbnails";
pub const RAW_PREVIEWS: &str = "sift-raw-previews";

/// Every cache dir under the system temp dir; cleanup removes all of them.
pub const ALL: [&str; 2] = [THUMBNAILS, RAW_PREVIEWS];

/// Return `<temp>/<name>`, creating it if needed.
pub fn ensure(name: &str) -> Result<PathBuf, String> {
    let dir = env::temp_dir().join(name);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir {}: {}", name, e))?;
    Ok(dir)
}
