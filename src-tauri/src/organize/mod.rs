// ============================================================
// Sift - Organize
// Moving, copying and trashing the user's photos: archive picks into 精选/
// and unmarked RAWs into RAW/, export picks, delete to the trash. No Tauri dependency; the IPC
// wrappers live in commands/.
// ============================================================

use std::path::Path;

pub mod archive;
pub mod delete;
pub mod delete_batch;
mod error;
pub mod export;
mod transfer;

#[cfg(test)]
mod rescan_tests;

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
