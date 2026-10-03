// ============================================================
// Sift - Saved culling progress
// One JSON file per folder under the app data dir, so marks survive
// quitting halfway through. Never touches the user's photo folders.
// ============================================================

use crate::models::photo::FolderProgress;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Subdirectory of the app data dir that holds the progress files
pub const PROGRESS_DIR: &str = "progress";

/// Older folders beyond this many are forgotten
pub const MAX_SAVED_FOLDERS: usize = 20;

const EXTENSION: &str = "json";

/// Saved progress for `folder`, or None when there is none or it is unreadable
pub fn load(store_dir: &Path, folder: &str) -> Option<FolderProgress> {
    let path = file_for(store_dir, folder);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(e) => {
            eprintln!("[Sift] Failed to read progress {}: {}", path.display(), e);
            return None;
        }
    };
    match serde_json::from_str::<FolderProgress>(&text) {
        // A hash collision must not hand one folder's marks to another
        Ok(progress) if progress.folder == folder => Some(progress),
        Ok(_) => None,
        Err(e) => {
            eprintln!("[Sift] Ignoring corrupt progress {}: {}", path.display(), e);
            None
        }
    }
}

/// Write progress atomically (temp file + rename), then drop the oldest folders
pub fn save(store_dir: &Path, progress: &FolderProgress) -> Result<(), String> {
    fs::create_dir_all(store_dir).map_err(|e| format!("Failed to create progress dir: {}", e))?;
    let path = file_for(store_dir, &progress.folder);
    let tmp = path.with_extension("tmp");
    let json = serde_json::to_vec(progress).map_err(|e| format!("Serialize failed: {}", e))?;
    fs::write(&tmp, json).map_err(|e| format!("Failed to write progress: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Failed to write progress: {}", e))?;
    prune(store_dir, MAX_SAVED_FOLDERS);
    Ok(())
}

/// Forget the saved progress of `folder`; missing is fine
pub fn clear(store_dir: &Path, folder: &str) -> Result<(), String> {
    match fs::remove_file(file_for(store_dir, folder)) {
        Err(e) if e.kind() != ErrorKind::NotFound => {
            Err(format!("Failed to clear progress: {}", e))
        }
        _ => Ok(()),
    }
}

/// Keep only the `keep` most recently written progress files
fn prune(store_dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(store_dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == EXTENSION))
        .filter_map(|path| Some((fs::metadata(&path).ok()?.modified().ok()?, path)))
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in &files[keep..] {
        if let Err(e) = fs::remove_file(path) {
            eprintln!("[Sift] Failed to prune progress {}: {}", path.display(), e);
        }
    }
}

/// File name is a hash of the folder path, never the path itself, so an
/// untrusted path from the frontend cannot point anywhere else
fn file_for(store_dir: &Path, folder: &str) -> PathBuf {
    store_dir.join(format!("{:016x}.{}", fnv1a64(folder.as_bytes()), EXTENSION))
}

/// FNV-1a: stable across Rust releases, unlike `DefaultHasher`
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |hash, &b| (hash ^ u64::from(b)).wrapping_mul(PRIME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::photo::PhotoStatus;
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::time::Duration;
    use tempfile::tempdir;

    fn progress(folder: &str, marks: &[(&str, PhotoStatus)]) -> FolderProgress {
        FolderProgress {
            folder: folder.to_string(),
            current_key: Some(format!("{}/IMG_2.jpg", folder)),
            marks: marks
                .iter()
                .map(|(key, status)| (format!("{}/{}", folder, key), status.clone()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn json_files(dir: &Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == EXTENSION))
            .count()
    }

    #[test]
    fn saved_progress_loads_back() {
        let dir = tempdir().unwrap();
        let saved = progress(
            "/photos/trip",
            &[
                ("IMG_1.jpg", PhotoStatus::Starred),
                ("IMG_2.CR3", PhotoStatus::Deleted),
            ],
        );

        save(dir.path(), &saved).unwrap();

        assert_eq!(load(dir.path(), "/photos/trip"), Some(saved));
    }

    #[test]
    fn reads_the_json_shape_the_frontend_sends() {
        // Mirrors snapshotProgress() in src/core/progress.ts
        let json = r#"{
            "folder": "/photos",
            "currentKey": "/photos/IMG_2.jpg",
            "marks": { "/photos/IMG_1.jpg": "starred", "/photos/IMG_2.CR3": "deleted" }
        }"#;

        let parsed: FolderProgress = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.current_key.as_deref(), Some("/photos/IMG_2.jpg"));
        assert_eq!(parsed.marks["/photos/IMG_1.jpg"], PhotoStatus::Starred);
        assert_eq!(parsed.marks["/photos/IMG_2.CR3"], PhotoStatus::Deleted);
    }

    #[test]
    fn missing_progress_loads_as_none() {
        let dir = tempdir().unwrap();

        assert_eq!(load(dir.path(), "/photos/never-opened"), None);
    }

    #[test]
    fn corrupt_progress_loads_as_none() {
        let dir = tempdir().unwrap();
        save(dir.path(), &progress("/photos/a", &[])).unwrap();
        fs::write(file_for(dir.path(), "/photos/a"), "{ not json").unwrap();

        assert_eq!(load(dir.path(), "/photos/a"), None);
    }

    #[test]
    fn progress_for_another_folder_is_not_returned() {
        let dir = tempdir().unwrap();
        let other = progress("/photos/b", &[("IMG_1.jpg", PhotoStatus::Starred)]);
        // Simulate a hash collision: /photos/a's file holds /photos/b's data
        fs::write(
            file_for(dir.path(), "/photos/a"),
            serde_json::to_vec(&other).unwrap(),
        )
        .unwrap();

        assert_eq!(load(dir.path(), "/photos/a"), None);
    }

    #[test]
    fn clear_removes_progress_and_tolerates_missing() {
        let dir = tempdir().unwrap();
        save(dir.path(), &progress("/photos/a", &[])).unwrap();

        clear(dir.path(), "/photos/a").unwrap();
        clear(dir.path(), "/photos/a").unwrap();

        assert_eq!(load(dir.path(), "/photos/a"), None);
    }

    #[test]
    fn hostile_folder_path_stays_inside_store_dir() {
        let dir = tempdir().unwrap();
        let store = dir.path().join("progress");

        save(&store, &progress("../../escape", &[])).unwrap();

        assert_eq!(json_files(&store), 1);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn save_overwrites_without_leaving_temp_files() {
        let dir = tempdir().unwrap();
        save(dir.path(), &progress("/photos/a", &[])).unwrap();
        let updated = progress("/photos/a", &[("IMG_1.jpg", PhotoStatus::Skipped)]);

        save(dir.path(), &updated).unwrap();

        assert_eq!(load(dir.path(), "/photos/a"), Some(updated));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn prune_keeps_the_most_recently_written_folders() {
        let dir = tempdir().unwrap();
        let base = SystemTime::now() - Duration::from_secs(3600);
        for i in 0..4 {
            let folder = format!("/photos/{}", i);
            save(dir.path(), &progress(&folder, &[])).unwrap();
            let file = File::options()
                .write(true)
                .open(file_for(dir.path(), &folder))
                .unwrap();
            file.set_modified(base + Duration::from_secs(i * 60))
                .unwrap();
        }

        prune(dir.path(), 2);

        assert_eq!(json_files(dir.path()), 2);
        assert!(load(dir.path(), "/photos/0").is_none());
        assert!(load(dir.path(), "/photos/1").is_none());
        assert!(load(dir.path(), "/photos/2").is_some());
        assert!(load(dir.path(), "/photos/3").is_some());
    }
}
