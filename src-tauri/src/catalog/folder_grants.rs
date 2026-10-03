// ============================================================
// Sift - Folder Grants
// Folders the user explicitly opened (folder dialog or drag-and-drop),
// persisted so "recent folders" keep working after a restart without
// letting the frontend grant itself arbitrary paths.
// ============================================================

use std::fs;
use std::path::{Path, PathBuf};

/// Comfortably above the frontend's recent-folder list (5)
pub const MAX_GRANTED_FOLDERS: usize = 20;

/// Outcome of checking whether `folder` may be scanned.
#[derive(Debug, PartialEq)]
pub enum Access {
    Granted,
    /// Not a directory: deleted, moved, or on a drive that is unplugged.
    /// Reported separately so the user isn't told to re-grant a folder that
    /// isn't there.
    Missing,
    NotGranted,
}

pub fn check(folder: &Path, is_allowed: impl Fn(&Path) -> bool) -> Access {
    if !folder.is_dir() {
        Access::Missing
    } else if is_granted(folder, is_allowed) {
        Access::Granted
    } else {
        Access::NotGranted
    }
}

/// Whether the user has granted access to `folder`: the folder itself is
/// allowed (picked in the dialog or dropped), or a regular file directly
/// inside it is (a dropped photo opens its folder).
///
/// Only regular files count. Allowing a folder also allows the folder path
/// itself, so if an allowed subfolder (or a symlink resolving into one) made
/// its parent count, scanning parent after parent would widen the scope up to
/// the root.
pub fn is_granted(folder: &Path, is_allowed: impl Fn(&Path) -> bool) -> bool {
    if is_allowed(folder) {
        return true;
    }
    fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                // DirEntry::file_type does not follow symlinks
                .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
                .any(|entry| is_allowed(&entry.path()))
        })
        .unwrap_or(false)
}

/// Most-recent-first list with `folder` moved to the front, capped at `max`.
pub fn remember(folders: &[PathBuf], folder: &Path, max: usize) -> Vec<PathBuf> {
    std::iter::once(folder.to_path_buf())
        .chain(folders.iter().filter(|f| f.as_path() != folder).cloned())
        .take(max)
        .collect()
}

/// Load the persisted list; a missing or unreadable file means no grants.
pub fn load(file: &Path) -> Vec<PathBuf> {
    let Ok(raw) = fs::read_to_string(file) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!(
            "[Sift] Ignoring corrupt folder grants {}: {}",
            file.display(),
            e
        );
        Vec::new()
    })
}

pub fn save(file: &Path, folders: &[PathBuf]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    }
    let json = serde_json::to_string_pretty(folders).map_err(|e| e.to_string())?;
    fs::write(file, json).map_err(|e| format!("Failed to save folder grants: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::write;
    use tempfile::tempdir;

    #[test]
    fn granted_when_folder_itself_is_allowed() {
        let dir = tempdir().unwrap();
        assert!(is_granted(dir.path(), |p| p == dir.path()));
    }

    #[test]
    fn granted_when_a_file_inside_is_allowed() {
        let dir = tempdir().unwrap();
        let photo = write(dir.path(), "DSC_0001.JPG", b"x");
        write(dir.path(), "DSC_0002.JPG", b"x");

        assert!(is_granted(dir.path(), |p| p == photo));
    }

    #[test]
    fn not_granted_otherwise() {
        let dir = tempdir().unwrap();
        write(dir.path(), "DSC_0001.JPG", b"x");

        assert!(!is_granted(dir.path(), |_| false));
        assert!(!is_granted(&dir.path().join("missing"), |_| false));
    }

    #[test]
    fn not_granted_by_files_in_subfolders() {
        let dir = tempdir().unwrap();
        let nested = write(dir.path(), "sub/DSC_0001.JPG", b"x");

        assert!(!is_granted(dir.path(), |p| p == nested));
    }

    #[test]
    fn not_granted_by_an_allowed_subfolder() {
        // Allowing a folder also allows the folder path itself. If that made
        // its parent count as granted, scanning parent after parent would
        // widen the scope all the way to the root.
        let dir = tempdir().unwrap();
        let granted = dir.path().join("2024");
        write(&granted, "DSC_0001.JPG", b"x");

        assert!(!is_granted(dir.path(), |p| p.starts_with(&granted)));
    }

    #[cfg(unix)]
    #[test]
    fn not_granted_by_a_symlink_to_an_allowed_file() {
        let dir = tempdir().unwrap();
        let photo = write(&dir.path().join("granted"), "DSC_0001.JPG", b"x")
            .canonicalize()
            .unwrap();
        let parent = dir.path().join("parent");
        std::fs::create_dir_all(&parent).unwrap();
        std::os::unix::fs::symlink(&photo, parent.join("link.JPG")).unwrap();
        // Like Tauri's scope, resolve symlinks before matching
        let is_allowed = |p: &Path| p.canonicalize().is_ok_and(|p| p == photo);
        assert!(is_allowed(&parent.join("link.JPG")));

        assert!(!is_granted(&parent, is_allowed));
    }

    #[test]
    fn check_reports_a_missing_folder_before_permissions() {
        let dir = tempdir().unwrap();
        let gone = dir.path().join("unplugged-drive");

        assert_eq!(check(&gone, |_| true), Access::Missing);
        assert_eq!(check(dir.path(), |_| false), Access::NotGranted);
        assert_eq!(check(dir.path(), |p| p == dir.path()), Access::Granted);
    }

    #[test]
    fn check_treats_a_file_path_as_missing_folder() {
        let dir = tempdir().unwrap();
        let photo = write(dir.path(), "DSC_0001.JPG", b"x");

        assert_eq!(check(&photo, |_| true), Access::Missing);
    }

    #[test]
    fn remember_moves_folder_to_front_without_duplicates() {
        let list = vec![
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/c"),
        ];
        let got = remember(&list, Path::new("/b"), 10);
        assert_eq!(
            got,
            [
                PathBuf::from("/b"),
                PathBuf::from("/a"),
                PathBuf::from("/c")
            ]
        );
    }

    #[test]
    fn remember_caps_the_list() {
        let list = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(
            remember(&list, Path::new("/c"), 2),
            [PathBuf::from("/c"), PathBuf::from("/a")]
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("nested/grants.json");
        let list = vec![PathBuf::from("/photos/2024"), PathBuf::from("/photos/2025")];

        save(&file, &list).unwrap();

        assert_eq!(load(&file), list);
    }

    #[test]
    fn load_tolerates_missing_and_corrupt_files() {
        let dir = tempdir().unwrap();
        assert!(load(&dir.path().join("missing.json")).is_empty());

        let corrupt = write(dir.path(), "grants.json", b"{not json");
        assert!(load(&corrupt).is_empty());
    }
}
