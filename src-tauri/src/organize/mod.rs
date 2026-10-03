// ============================================================
// Sift - Organize
// Moving, copying and trashing the user's photos: archive into JPG/ and
// RAW/, export picks, delete to the trash. No Tauri dependency; the IPC
// wrappers live in commands/.
// ============================================================

use std::path::Path;

pub mod archive;
pub mod delete;
pub mod export;
#[cfg(test)]
mod rescan_tests;

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
