// ============================================================
// Sift - Delete Command
// Moves JPG + RAW to system trash (recycle bin)
// Supports RAW-only pairs: deletes RAW to trash + cleans up
// temporary preview file
// Async: uses spawn_blocking to avoid blocking IPC on Windows
// ============================================================

use crate::media::cache_dirs;
use crate::paths;
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
            &cache_dirs::path(cache_dirs::RAW_PREVIEWS),
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
/// exist are skipped, except a normal pair's JPG, which must exist. A RAW-only
/// pair's preview must sit inside `preview_cache_dir`, or the plan is refused.
fn plan_delete(
    jpg_path: &str,
    raw_path: Option<&str>,
    source: Option<&str>,
    xmp_paths: &[String],
    preview_cache_dir: &Path,
) -> Result<DeletePlan, String> {
    let is_raw_preview = source == Some("rawPreview");
    let jpg = Path::new(jpg_path);
    let mut trash = Vec::new();
    let mut preview = None;

    if is_raw_preview {
        if jpg.exists() {
            // jpg_path comes from the frontend. Only a file we extracted
            // ourselves may be removed outright; anything else could be a
            // user photo, so refuse the whole request before touching disk.
            if !paths::is_inside_dir(jpg, preview_cache_dir) {
                return Err(format!(
                    "Refusing to delete {}: not a cached RAW preview",
                    jpg_path
                ));
            }
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

    /// Previews live in `<tmp>/cache`, user photos anywhere else.
    fn plan_in(
        tmp: &TempDir,
        jpg: &str,
        raw: Option<&str>,
        source: Option<&str>,
        xmps: &[String],
    ) -> Result<DeletePlan, String> {
        plan_delete(jpg, raw, source, xmps, &tmp.path().join("cache"))
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

        let plan = plan_in(&tmp, &jpg, Some(&raw), None, &[xmp]).unwrap();

        assert_eq!(labels(&plan), ["JPG", "RAW", "XMP"]);
        assert_eq!(plan.preview, None);
    }

    #[test]
    fn rejects_a_normal_pair_whose_jpg_is_missing() {
        let tmp = TempDir::new().unwrap();
        let jpg = tmp.path().join("gone.jpg");

        let err = plan_in(&tmp, &jpg.to_string_lossy(), None, None, &[]).unwrap_err();

        assert!(err.contains("JPG file not found"), "{}", err);
    }

    #[test]
    fn skips_raw_and_xmp_that_no_longer_exist() {
        let tmp = TempDir::new().unwrap();
        let jpg = touch(&tmp.path().join("IMG_1.jpg"));
        let raw = tmp.path().join("IMG_1.ARW").to_string_lossy().to_string();
        let xmp = tmp.path().join("IMG_1.xmp").to_string_lossy().to_string();

        let plan = plan_in(&tmp, &jpg, Some(&raw), None, &[xmp]).unwrap();

        assert_eq!(labels(&plan), ["JPG"]);
    }

    #[test]
    fn moves_every_file_of_a_normal_pair_to_the_trash() {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        let jpg = touch(&tmp.path().join("IMG_1.jpg"));
        let raw = touch(&tmp.path().join("IMG_1.ARW"));
        let xmp = touch(&tmp.path().join("IMG_1.xmp"));
        let plan = plan_in(&tmp, &jpg, Some(&raw), None, std::slice::from_ref(&xmp)).unwrap();

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
        let plan = plan_in(&tmp, &preview, Some(&raw), Some("rawPreview"), &[xmp]).unwrap();

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
        let plan = plan_in(&tmp, &preview, Some(&raw), Some("rawPreview"), &[]).unwrap();

        let err = execute_delete(&plan, |_| Err("trash unavailable".into())).unwrap_err();

        assert!(err.contains("Failed to trash RAW"), "{}", err);
        assert!(Path::new(&raw).exists());
        assert!(Path::new(&preview).exists());
    }

    #[test]
    fn refuses_a_raw_preview_outside_the_cache_and_touches_nothing() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("cache")).unwrap();
        let user_jpg = touch(&tmp.path().join("photos/IMG_3.jpg"));
        let raw = touch(&tmp.path().join("photos/IMG_3.CR3"));

        let err = plan_in(&tmp, &user_jpg, Some(&raw), Some("rawPreview"), &[]).unwrap_err();

        assert!(err.contains("not a cached RAW preview"), "{}", err);
        assert!(Path::new(&user_jpg).exists());
        assert!(Path::new(&raw).exists());
    }

    #[test]
    fn refuses_a_raw_preview_that_escapes_the_cache_with_dot_dot() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("cache")).unwrap();
        touch(&tmp.path().join("photos/IMG_3.jpg"));
        let sneaky = tmp.path().join("cache/../photos/IMG_3.jpg");

        let result = plan_in(
            &tmp,
            &sneaky.to_string_lossy(),
            None,
            Some("rawPreview"),
            &[],
        );

        assert!(result.is_err());
        assert!(sneaky.exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_cache_symlink_that_points_at_a_user_file() {
        let tmp = TempDir::new().unwrap();
        let user_jpg = touch(&tmp.path().join("photos/IMG_3.jpg"));
        let link = tmp.path().join("cache/abc.jpg");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&user_jpg, &link).unwrap();

        let result = plan_in(&tmp, &link.to_string_lossy(), None, Some("rawPreview"), &[]);

        assert!(result.is_err());
        assert!(Path::new(&user_jpg).exists());
    }

    #[test]
    fn still_trashes_the_raw_when_its_preview_was_already_cleaned_up() {
        let tmp = TempDir::new().unwrap();
        let preview = tmp
            .path()
            .join("cache/abc.jpg")
            .to_string_lossy()
            .to_string();
        let raw = touch(&tmp.path().join("photos/IMG_2.CR3"));

        let plan = plan_in(&tmp, &preview, Some(&raw), Some("rawPreview"), &[]).unwrap();

        assert_eq!(labels(&plan), ["RAW"]);
        assert_eq!(plan.preview, None);
    }
}
