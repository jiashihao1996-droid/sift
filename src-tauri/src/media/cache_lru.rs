// ============================================================
// Sift - Cache Size Limit
// The thumbnail and RAW preview caches outlive restarts, so they are
// trimmed to a total size, least recently used first. A cache hit marks
// its file used by setting the file's access time (not its modification
// time: a RAW preview's thumbnail is keyed by the preview's mtime).
// Trimming only ever runs while no session can be using the files: at
// startup before the first scan, and when the window is destroyed.
// ============================================================

use crate::paths::is_inside_dir;
use std::fs::{self, File, FileTimes, Metadata};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const GIB: u64 = 1024 * 1024 * 1024;

/// Total size the caches are trimmed to. Thumbnails are ~10-20KB, so a
/// 20,000-photo folder takes ~0.3GB; a RAW-only photo's preview is 1-3MB.
/// 5GB keeps the thumbnails of a dozen large folders plus the previews of
/// a couple of thousand RAW-only photos.
pub const MAX_CACHE_BYTES: u64 = 5 * GIB;

/// A cache hit only updates the access time when it is older than this,
/// so rescanning a folder costs no writes; LRU order at day granularity
/// is plenty for a cache this size.
const MARK_USED_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// What `trim` did
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Trimmed {
    pub removed_files: usize,
    pub removed_bytes: u64,
    pub kept_bytes: u64,
}

/// Record that the cached file at `path` (with metadata `meta`) was used.
/// Best effort: failing only makes the file look older to `trim`.
pub fn mark_used(path: &Path, meta: &Metadata) {
    let now = SystemTime::now();
    let is_recent = last_used(meta).is_some_and(|t| t + MARK_USED_AFTER > now);
    if is_recent {
        return;
    }
    let marked = File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_times(FileTimes::new().set_accessed(now)));
    if let Err(e) = marked {
        eprintln!("[Sift] Failed to mark {} used: {}", path.display(), e);
    }
}

/// Remove files from `dirs` (each read non-recursively), least recently
/// used first, until the rest take at most `max_bytes`. Leftover temporary
/// files of interrupted writes always go. Subfolders and symlinks are
/// never touched, and nothing outside `dirs` is removed.
pub fn trim(dirs: &[PathBuf], max_bytes: u64) -> Trimmed {
    let mut files = Vec::new();
    let mut trimmed = Trimmed::default();
    for dir in dirs {
        for file in cache_files(dir) {
            if file.is_temporary {
                remove(&file, dir, &mut trimmed);
            } else {
                files.push((file, dir));
            }
        }
    }
    files.sort_by_key(|(file, _)| file.last_used);
    let mut total: u64 = files.iter().map(|(file, _)| file.len).sum();
    for (file, dir) in &files {
        if total <= max_bytes {
            break;
        }
        if remove(file, dir, &mut trimmed) {
            total -= file.len;
        }
    }
    trimmed.kept_bytes = total;
    trimmed
}

struct CacheFile {
    path: PathBuf,
    len: u64,
    last_used: Option<SystemTime>,
    is_temporary: bool,
}

/// The regular files directly inside `dir`; an unreadable dir has none
fn cache_files(dir: &Path) -> Vec<CacheFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            // The entry's own metadata: a symlink is not followed
            let meta = entry.metadata().ok().filter(Metadata::is_file)?;
            let path = entry.path();
            Some(CacheFile {
                is_temporary: path.extension().is_some_and(|e| e == "tmp"),
                len: meta.len(),
                last_used: last_used(&meta),
                path,
            })
        })
        .collect()
}

/// Whether the file was removed; counts it in `trimmed` if so
fn remove(file: &CacheFile, dir: &Path, trimmed: &mut Trimmed) -> bool {
    if !is_inside_dir(&file.path, dir) {
        eprintln!(
            "[Sift] Not removing {}: outside the cache",
            file.path.display()
        );
        return false;
    }
    match fs::remove_file(&file.path) {
        Ok(()) => {
            trimmed.removed_files += 1;
            trimmed.removed_bytes += file.len;
            true
        }
        Err(e) => {
            eprintln!("[Sift] Failed to trim {}: {}", file.path.display(), e);
            false
        }
    }
}

/// When the file was last written or marked used
fn last_used(meta: &Metadata) -> Option<SystemTime> {
    meta.accessed().ok().max(meta.modified().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    /// A `len`-byte cache file last used `days_ago`
    fn cached(dir: &Path, name: &str, len: usize, days_ago: u32) -> PathBuf {
        cached_at(dir, name, len, SystemTime::now() - DAY * days_ago)
    }

    fn cached_at(dir: &Path, name: &str, len: usize, at: SystemTime) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, vec![0u8; len]).unwrap();
        let times = FileTimes::new().set_accessed(at).set_modified(at);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(times)
            .unwrap();
        path
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn removes_the_least_recently_used_files_until_under_the_limit() {
        let dir = tempdir().unwrap();
        cached(dir.path(), "a.jpg", 100, 5);
        cached(dir.path(), "b.jpg", 100, 1);
        cached(dir.path(), "c.jpg", 100, 3);
        cached(dir.path(), "d.jpg", 100, 2);

        let trimmed = trim(&[dir.path().to_path_buf()], 250);

        assert_eq!(names(dir.path()), ["b.jpg", "d.jpg"]);
        assert_eq!(
            trimmed,
            Trimmed {
                removed_files: 2,
                removed_bytes: 200,
                kept_bytes: 200,
            }
        );
    }

    #[test]
    fn applies_one_limit_across_all_dirs() {
        let root = tempdir().unwrap();
        let thumbs = root.path().join("thumbnails");
        let previews = root.path().join("raw-previews");
        fs::create_dir_all(&thumbs).unwrap();
        fs::create_dir_all(&previews).unwrap();
        cached(&thumbs, "old.jpg", 100, 9);
        cached(&previews, "older.jpg", 300, 10);
        cached(&previews, "new.jpg", 300, 1);

        trim(&[thumbs.clone(), previews.clone()], 400);

        assert_eq!(names(&thumbs), ["old.jpg"]);
        assert_eq!(names(&previews), ["new.jpg"]);
    }

    #[test]
    fn keeps_everything_under_the_limit() {
        let dir = tempdir().unwrap();
        cached(dir.path(), "a.jpg", 100, 30);

        let trimmed = trim(&[dir.path().to_path_buf()], 100);

        assert_eq!(names(dir.path()), ["a.jpg"]);
        assert_eq!(trimmed.removed_files, 0);
    }

    #[test]
    fn a_file_marked_used_outlives_a_newer_one() {
        let dir = tempdir().unwrap();
        let old = cached(dir.path(), "old.jpg", 100, 10);
        cached(dir.path(), "newer.jpg", 100, 5);
        let modified_before = fs::metadata(&old).unwrap().modified().unwrap();

        mark_used(&old, &fs::metadata(&old).unwrap());
        trim(&[dir.path().to_path_buf()], 100);

        assert_eq!(names(dir.path()), ["old.jpg"]);
        // The modification time keys the thumbnails of RAW previews
        assert_eq!(
            fs::metadata(&old).unwrap().modified().unwrap(),
            modified_before
        );
    }

    #[test]
    fn marking_a_recently_used_file_writes_nothing() {
        let dir = tempdir().unwrap();
        let an_hour_ago = SystemTime::now() - Duration::from_secs(3_600);
        let path = cached_at(dir.path(), "a.jpg", 10, an_hour_ago);

        mark_used(&path, &fs::metadata(&path).unwrap());

        assert_eq!(
            fs::metadata(&path).unwrap().accessed().unwrap(),
            an_hour_ago
        );
    }

    #[test]
    fn always_removes_leftover_temporary_files() {
        let dir = tempdir().unwrap();
        cached(dir.path(), "a.jpg", 10, 1);
        cached(dir.path(), "a_preview.123-0.tmp", 10, 0);

        let trimmed = trim(&[dir.path().to_path_buf()], 1_000);

        assert_eq!(names(dir.path()), ["a.jpg"]);
        assert_eq!(trimmed.removed_files, 1);
    }

    #[test]
    fn never_touches_subfolders_or_what_symlinks_point_to() {
        let root = tempdir().unwrap();
        let cache = root.path().join("cache");
        let outside = root.path().join("photos");
        fs::create_dir_all(cache.join("sub")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        cached(&cache.join("sub"), "nested.jpg", 100, 10);
        let photo = cached(&outside, "photo.jpg", 100, 10);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&photo, cache.join("link.jpg")).unwrap();

        trim(std::slice::from_ref(&cache), 0);

        assert!(cache.join("sub/nested.jpg").exists());
        assert!(photo.exists());
    }

    #[test]
    fn a_missing_dir_has_nothing_to_trim() {
        let dir = tempdir().unwrap();
        let trimmed = trim(&[dir.path().join("missing")], 0);
        assert_eq!(trimmed, Trimmed::default());
    }
}
