// ============================================================
// Sift - Scan Command
// Only scans folders the user granted (folder dialog or drag-and-drop);
// see catalog/folder_grants.rs. Sends `scan-progress` while RAW-only
// previews are extracted; a newer scan or `cancel_scan` stops it (see
// catalog/scan_control.rs).
// ============================================================

use crate::catalog;
use crate::catalog::folder_grants::{self, Access};
use crate::catalog::scan_control::{ScanControl, ScanGate};
use crate::media::cache_dirs;
use crate::models::photo::{ScanProgress, ScanResult};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

const PROGRESS_EVENT: &str = "scan-progress";

const GRANTS_FILE: &str = "granted-folders.json";

const MISSING: &str = "文件夹不存在，可能已被删除、移动，或所在的移动硬盘未连接：";

const NOT_GRANTED: &str = "没有访问该文件夹的权限，请点击「选择文件夹」或把文件夹拖进窗口重新打开";

/// Scan a folder; `scan_id` (from the frontend, never 0) makes any earlier
/// scan stale and tags this one's progress events.
#[tauri::command]
pub async fn scan_folder(
    app: AppHandle,
    folder_path: String,
    scan_id: u32,
) -> Result<ScanResult, String> {
    app.state::<ScanGate>().begin(scan_id);
    tokio::task::spawn_blocking(move || {
        let folder = Path::new(&folder_path);
        authorize(&app, folder)?;
        let cache_dir = cache_dirs::ensure(cache_dirs::RAW_PREVIEWS)?;
        let gate = app.state::<ScanGate>();
        let is_cancelled = || !gate.is_current(scan_id);
        let on_progress = |done, total| {
            if gate.is_current(scan_id) {
                let progress = ScanProgress {
                    scan_id,
                    done,
                    total,
                };
                let _ = app.emit(PROGRESS_EVENT, progress);
            }
        };
        let control = ScanControl {
            is_cancelled: &is_cancelled,
            on_progress: &on_progress,
        };
        catalog::scan::scan_folder_with(folder, &cache_dir, &control)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Stop scan `scan_id` before its next photo, unless a newer scan began
#[tauri::command]
pub fn cancel_scan(gate: State<ScanGate>, scan_id: u32) {
    gate.cancel(scan_id);
}

/// Refuse folders the user never granted; for granted ones, extend the asset
/// scope to subfolders (archived JPG/ and RAW/) and remember the grant.
fn authorize(app: &AppHandle, folder: &Path) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    match folder_grants::check(folder, |p| scope.is_allowed(p)) {
        Access::Granted => {}
        Access::Missing => return Err(format!("{}{}", MISSING, folder.display())),
        Access::NotGranted => return Err(NOT_GRANTED.to_string()),
    }
    scope
        .allow_directory(folder, true)
        .map_err(|e| format!("Failed to allow folder: {}", e))?;

    let file = grants_file(app)?;
    let folders = folder_grants::remember(
        &folder_grants::load(&file),
        folder,
        folder_grants::MAX_GRANTED_FOLDERS,
    );
    // Not fatal: the folder is granted for this run either way
    if let Err(e) = folder_grants::save(&file, &folders) {
        eprintln!("[Sift] {}", e);
    }
    Ok(())
}

/// Re-grant folders the user opened in earlier runs, so "recent folders" work.
/// Best effort: a folder that can't be restored is logged and skipped, since
/// failing app setup over it would keep Sift from starting at all.
pub fn restore_folder_grants(app: &tauri::App) {
    let file = match grants_file(app.handle()) {
        Ok(file) => file,
        Err(e) => return eprintln!("[Sift] Not restoring folder grants: {}", e),
    };
    let scope = app.asset_protocol_scope();
    for folder in folder_grants::load(&file).iter().filter(|f| f.is_dir()) {
        if let Err(e) = scope.allow_directory(folder, true) {
            eprintln!(
                "[Sift] Failed to restore folder grant {}: {}",
                folder.display(),
                e
            );
        }
    }
}

fn grants_file(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(GRANTS_FILE))
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))
}
