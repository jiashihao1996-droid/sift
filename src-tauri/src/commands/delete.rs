// ============================================================
// Sift - Delete Command
// Moves JPG + RAW to system trash (recycle bin)
// Supports RAW-only pairs: deletes RAW to trash + cleans up
// temporary preview file
// Async: uses spawn_blocking to avoid blocking IPC on Windows
// ============================================================

use std::path::{Path, PathBuf};

#[tauri::command]
pub async fn delete_pair(
    jpg_path: String,
    raw_path: Option<String>,
    source: Option<String>,
    xmp_paths: Option<Vec<String>>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let plan = plan_delete(
            &jpg_path,
            raw_path.as_deref(),
            source.as_deref(),
            xmp_paths.as_deref().unwrap_or_default(),
        )?;
        execute_delete(&plan, |path| trash::delete(path).map_err(|e| e.to_string()))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// A user file to send to the trash, with a label for error messages.
#[derive(Debug, PartialEq)]
struct TrashItem {
    label: &'static str,
    path: PathBuf,
}

/// Everything `delete_pair` will touch, decided before any IO happens.
#[derive(Debug, PartialEq)]
struct DeletePlan {
    /// User files, trashed in order. Recoverable.
    trash: Vec<TrashItem>,
    /// Temporary RAW preview, removed permanently. Never a user file.
    preview: Option<PathBuf>,
}

/// Build the delete plan from frontend-supplied paths. Files that no longer
/// exist are skipped, except a normal pair's JPG, which must exist.
fn plan_delete(
    jpg_path: &str,
    raw_path: Option<&str>,
    source: Option<&str>,
    xmp_paths: &[String],
) -> Result<DeletePlan, String> {
    let is_raw_preview = source == Some("rawPreview");
    let jpg = Path::new(jpg_path);
    let mut trash = Vec::new();
    let mut preview = None;

    if is_raw_preview {
        if jpg.exists() {
            preview = Some(jpg.to_path_buf());
        }
    } else {
        if !jpg.exists() {
            return Err(format!("JPG file not found: {}", jpg_path));
        }
        trash.push(TrashItem {
            label: "JPG",
            path: jpg.to_path_buf(),
        });
    }

    let raw = raw_path.map(Path::new).filter(|p| p.exists());
    trash.extend(raw.map(|p| TrashItem {
        label: "RAW",
        path: p.to_path_buf(),
    }));

    let xmps = xmp_paths.iter().map(Path::new).filter(|p| p.exists());
    trash.extend(xmps.map(|p| TrashItem {
        label: "XMP",
        path: p.to_path_buf(),
    }));

    Ok(DeletePlan { trash, preview })
}

/// Carry out a plan. `trash_file` is injected so tests can run without the
/// system trash; production passes `trash::delete`.
fn execute_delete(
    plan: &DeletePlan,
    mut trash_file: impl FnMut(&Path) -> Result<(), String>,
) -> Result<(), String> {
    for item in &plan.trash {
        trash_file(&item.path).map_err(|e| {
            format!(
                "Failed to trash {} {}: {}",
                item.label,
                item.path.display(),
                e
            )
        })?;
    }
    if let Some(preview) = &plan.preview {
        std::fs::remove_file(preview)
            .map_err(|e| format!("Failed to clean preview file: {}", e))?;
    }
    Ok(())
}
