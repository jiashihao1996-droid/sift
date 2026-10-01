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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(path: &Path) -> String {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, path.to_string_lossy().as_bytes()).unwrap();
        path.to_string_lossy().to_string()
    }

    /// Stand-in for the system trash: moves the file into `bin`.
    fn fake_trash(bin: &Path) -> impl FnMut(&Path) -> Result<(), String> + '_ {
        fs::create_dir_all(bin).unwrap();
        move |path| fs::rename(path, bin.join(path.file_name().unwrap())).map_err(|e| e.to_string())
    }

    fn labels(plan: &DeletePlan) -> Vec<&str> {
        plan.trash.iter().map(|item| item.label).collect()
    }

    #[test]
    fn plans_jpg_raw_and_xmp_of_a_normal_pair_for_the_trash() {
        let tmp = TempDir::new().unwrap();
        let jpg = touch(&tmp.path().join("IMG_1.jpg"));
        let raw = touch(&tmp.path().join("IMG_1.ARW"));
        let xmp = touch(&tmp.path().join("IMG_1.xmp"));

        let plan = plan_delete(&jpg, Some(&raw), None, &[xmp]).unwrap();

        assert_eq!(labels(&plan), ["JPG", "RAW", "XMP"]);
        assert_eq!(plan.preview, None);
    }

    #[test]
    fn rejects_a_normal_pair_whose_jpg_is_missing() {
        let tmp = TempDir::new().unwrap();
        let jpg = tmp.path().join("gone.jpg");

        let err = plan_delete(&jpg.to_string_lossy(), None, None, &[]).unwrap_err();

        assert!(err.contains("JPG file not found"), "{}", err);
    }

    #[test]
    fn skips_raw_and_xmp_that_no_longer_exist() {
        let tmp = TempDir::new().unwrap();
        let jpg = touch(&tmp.path().join("IMG_1.jpg"));
        let raw = tmp.path().join("IMG_1.ARW").to_string_lossy().to_string();
        let xmp = tmp.path().join("IMG_1.xmp").to_string_lossy().to_string();

        let plan = plan_delete(&jpg, Some(&raw), None, &[xmp]).unwrap();

        assert_eq!(labels(&plan), ["JPG"]);
    }

    #[test]
    fn moves_every_file_of_a_normal_pair_to_the_trash() {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        let jpg = touch(&tmp.path().join("IMG_1.jpg"));
        let raw = touch(&tmp.path().join("IMG_1.ARW"));
        let xmp = touch(&tmp.path().join("IMG_1.xmp"));
        let plan = plan_delete(&jpg, Some(&raw), None, std::slice::from_ref(&xmp)).unwrap();

        execute_delete(&plan, fake_trash(&bin)).unwrap();

        for (original, name) in [(jpg, "IMG_1.jpg"), (raw, "IMG_1.ARW"), (xmp, "IMG_1.xmp")] {
            assert!(
                !Path::new(&original).exists(),
                "{} still in place",
                original
            );
            assert!(bin.join(name).exists(), "{} not in trash", name);
        }
    }

    #[test]
    fn trashes_raw_and_removes_cached_preview_of_a_raw_only_pair() {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        let preview = touch(&tmp.path().join("cache/abc.jpg"));
        let raw = touch(&tmp.path().join("photos/IMG_2.CR3"));
        let xmp = touch(&tmp.path().join("photos/IMG_2.xmp"));
        let plan = plan_delete(&preview, Some(&raw), Some("rawPreview"), &[xmp]).unwrap();

        assert_eq!(labels(&plan), ["RAW", "XMP"]);
        execute_delete(&plan, fake_trash(&bin)).unwrap();

        assert!(bin.join("IMG_2.CR3").exists());
        assert!(bin.join("IMG_2.xmp").exists());
        assert!(!Path::new(&preview).exists());
        assert!(
            !bin.join("abc.jpg").exists(),
            "preview must not go to the trash"
        );
    }

    #[test]
    fn keeps_the_preview_when_trashing_the_raw_fails() {
        let tmp = TempDir::new().unwrap();
        let preview = touch(&tmp.path().join("cache/abc.jpg"));
        let raw = touch(&tmp.path().join("photos/IMG_2.CR3"));
        let plan = plan_delete(&preview, Some(&raw), Some("rawPreview"), &[]).unwrap();

        let err = execute_delete(&plan, |_| Err("trash unavailable".into())).unwrap_err();

        assert!(err.contains("Failed to trash RAW"), "{}", err);
        assert!(Path::new(&raw).exists());
        assert!(Path::new(&preview).exists());
    }
}
