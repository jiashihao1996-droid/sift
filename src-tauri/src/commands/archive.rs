// ============================================================
// Sift - Archive Command
// Moves surviving photos into RAW/ and JPG/ subfolders
// Supports RAW-only pairs: RAW → RAW/, preview JPG → JPG/
// Never overwrites: colliding pairs get a shared `_N` suffix in both folders
// ============================================================

use crate::media::cache_dirs;
use crate::models::photo::{ArchivePairInput, ArchiveProgress, ArchiveResult, PhotoSource};
use crate::paths;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// Move a file to the destination, falling back to copy+remove if rename fails
/// (e.g. cross-volume moves on Windows).
///
/// The fallback removes `src` only after `fs::copy` has returned Ok, so a
/// failed copy never loses the original. If the remove then fails, both copies
/// exist and an error is returned: duplicated, never lost. `dest` must already
/// be a fresh path from `paths::unique_paths_sharing_stem`.
fn move_file(src: &Path, dest: &Path) -> Result<(), String> {
    let src_str = src.to_string_lossy();
    let dest_str = dest.to_string_lossy();

    match fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Fallback: copy then remove (handles cross-volume moves on Windows)
            fs::copy(src, dest)
                .map_err(|e| format!("Failed to copy {} -> {}: {}", src_str, dest_str, e))?;
            fs::remove_file(src)
                .map_err(|e| format!("Failed to remove source after copy {}: {}", src_str, e))?;
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn archive_photos(
    app: AppHandle,
    folder_path: String,
    pairs: Vec<ArchivePairInput>,
) -> Result<ArchiveResult, String> {
    tokio::task::spawn_blocking(move || {
        let preview_cache = cache_dirs::path(cache_dirs::RAW_PREVIEWS);
        archive_pairs(
            Path::new(&folder_path),
            &pairs,
            &preview_cache,
            &mut |progress| {
                let _ = app.emit("archive-progress", progress);
            },
        )
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

type OnProgress<'a> = dyn FnMut(ArchiveProgress) + 'a;

struct ArchiveDirs {
    jpg: PathBuf,
    raw: PathBuf,
    preview_cache: PathBuf,
}

/// A RAW-only pair's `jpg_path` comes from the frontend and is removed after
/// being copied, so it must be one of our cached previews. Anything else may
/// be a user photo: refuse the whole archive rather than guess.
fn ensure_previews_are_cached(pairs: &[ArchivePairInput], cache: &Path) -> Result<(), String> {
    let stray = pairs
        .iter()
        .filter(|p| p.status != "deleted" && p.source == PhotoSource::RawPreview)
        .map(|p| Path::new(&p.jpg_path))
        .find(|jpg| jpg.exists() && !paths::is_inside_dir(jpg, cache));
    match stray {
        Some(jpg) => Err(format!(
            "Refusing to archive {}: not a cached RAW preview",
            jpg.display()
        )),
        None => Ok(()),
    }
}

/// Move every non-deleted pair under `base` into `JPG/` and `RAW/`.
/// `on_progress` is called once per pair whose primary file exists.
fn archive_pairs(
    base: &Path,
    pairs: &[ArchivePairInput],
    preview_cache: &Path,
    on_progress: &mut OnProgress,
) -> Result<ArchiveResult, String> {
    // Validate before creating folders or moving anything, so a bad request
    // leaves the user's folder exactly as it was.
    ensure_previews_are_cached(pairs, preview_cache)?;

    let dirs = ArchiveDirs {
        jpg: base.join("JPG"),
        raw: base.join("RAW"),
        preview_cache: preview_cache.to_path_buf(),
    };
    fs::create_dir_all(&dirs.jpg).map_err(|e| format!("Failed to create JPG dir: {}", e))?;
    fs::create_dir_all(&dirs.raw).map_err(|e| format!("Failed to create RAW dir: {}", e))?;

    let total = pairs.len();
    let mut moved_count: usize = 0;

    for (i, pair) in pairs.iter().enumerate() {
        if pair.status == "deleted" {
            continue;
        }
        if let Some(primary) = primary_file(pair) {
            on_progress(progress(i + 1, total, primary));
        }
        moved_count += archive_pair(pair, &dirs)?;
    }

    Ok(ArchiveResult {
        moved_count,
        jpg_folder: dirs.jpg.to_string_lossy().to_string(),
        raw_folder: dirs.raw.to_string_lossy().to_string(),
    })
}

fn progress(current: usize, total: usize, file: &Path) -> ArchiveProgress {
    ArchiveProgress {
        current,
        total,
        current_file: file_name(file),
    }
}

/// Best-effort cleanup once the preview has been copied into JPG/. A failure
/// is logged, not returned: the archive itself succeeded, and the cache dir is
/// wiped on exit anyway. Re-checks the cache dir right at the remove site.
fn remove_cached_preview(preview: &Path, cache: &Path) {
    if !paths::is_inside_dir(preview, cache) {
        eprintln!(
            "[Sift] Not removing {}: outside the RAW preview cache",
            preview.display()
        );
        return;
    }
    if let Err(e) = fs::remove_file(preview) {
        eprintln!(
            "[Sift] Failed to remove cached preview {}: {}",
            preview.display(),
            e
        );
    }
}

/// Destination file name for a RAW-only pair's preview: `<raw stem>.jpg`.
fn preview_dest_name(pair: &ArchivePairInput) -> String {
    match &pair.raw_path {
        Some(raw) => {
            let stem = Path::new(raw).file_stem().and_then(|s| s.to_str());
            format!("{}.jpg", stem.unwrap_or("preview"))
        }
        None => Path::new(&pair.jpg_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    }
}

/// How a file reaches its destination.
#[derive(Clone, Copy, PartialEq)]
enum Transfer {
    /// A user file: moved.
    Move,
    /// Our cached RAW preview: copied, then removed from the cache.
    CopyPreview,
}

/// One file of a pair that still has to go into `JPG/` or `RAW/`.
struct PlannedFile<'a> {
    src: &'a Path,
    dest: PathBuf,
    transfer: Transfer,
    is_photo: bool,
}

/// The file shown in progress: the JPG of a normal pair, the RAW of a RAW-only one.
fn primary_file(pair: &ArchivePairInput) -> Option<&Path> {
    let primary = match pair.source {
        PhotoSource::RawPreview => pair.raw_path.as_deref()?,
        PhotoSource::Jpg => &pair.jpg_path,
    };
    Some(Path::new(primary)).filter(|p| p.exists())
}

/// Where each existing file of a pair should go, before collision handling.
/// Normal pair: JPG → JPG/, RAW → RAW/, XMP follows the RAW if there is one,
/// otherwise the JPG. RAW-only pair: RAW → RAW/, cached preview → JPG/ named
/// after the RAW (so re-opening the folder yields a proper JPG+RAW pair), XMP → RAW/.
/// Files already in their target folder are left out.
fn plan_pair<'a>(pair: &'a ArchivePairInput, dirs: &ArchiveDirs) -> Vec<PlannedFile<'a>> {
    let raw = pair.raw_path.as_deref().map(Path::new);
    let mut files = Vec::new();
    let mut add = |src: &'a Path, dir: &Path, name: &str, transfer, is_photo| {
        files.push(PlannedFile {
            src,
            dest: dir.join(name),
            transfer,
            is_photo,
        });
    };
    let jpg = Path::new(&pair.jpg_path);

    if pair.source == PhotoSource::RawPreview {
        if let Some(raw) = raw {
            add(raw, &dirs.raw, &file_name(raw), Transfer::Move, true);
        }
        let name = preview_dest_name(pair);
        add(jpg, &dirs.jpg, &name, Transfer::CopyPreview, true);
    } else {
        add(jpg, &dirs.jpg, &file_name(jpg), Transfer::Move, true);
        if let Some(raw) = raw {
            add(raw, &dirs.raw, &file_name(raw), Transfer::Move, true);
        }
    }

    let xmp_dir = if raw.is_some() { &dirs.raw } else { &dirs.jpg };
    for xmp in pair.xmp_paths.iter().map(Path::new) {
        add(xmp, xmp_dir, &file_name(xmp), Transfer::Move, false);
    }

    files.retain(|f| {
        let dir = f.dest.parent().unwrap_or(&f.dest);
        f.src.exists() && !is_already_in_dir(f.src, dir)
    });
    files
}

/// The stem that ties a pair together: the RAW's when present, since the
/// preview and XMP sidecars are named after it.
fn pair_stem(pair: &ArchivePairInput) -> String {
    let anchor = pair.raw_path.as_deref().unwrap_or(&pair.jpg_path);
    Path::new(anchor)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Move one pair into `JPG/` and `RAW/`. If any of its names is taken, the whole
/// pair gets the same `_N` suffix in both folders, so a rescan still pairs it.
/// Returns how many photos (JPG/RAW, not XMP) were moved.
fn archive_pair(pair: &ArchivePairInput, dirs: &ArchiveDirs) -> Result<usize, String> {
    let files = plan_pair(pair, dirs);
    let candidates: Vec<PathBuf> = files.iter().map(|f| f.dest.clone()).collect();
    let dests = paths::unique_paths_sharing_stem(&candidates, &pair_stem(pair));

    for (file, dest) in files.iter().zip(&dests) {
        match file.transfer {
            Transfer::Move => move_file(file.src, dest)?,
            Transfer::CopyPreview => {
                fs::copy(file.src, dest)
                    .map_err(|e| format!("Failed to copy preview JPG: {}", e))?;
                remove_cached_preview(file.src, &dirs.preview_cache);
            }
        }
    }
    Ok(files.iter().filter(|f| f.is_photo).count())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Check if a file is already located inside the target directory
fn is_already_in_dir(file: &Path, dir: &Path) -> bool {
    let file_parent = match file.parent().and_then(|p| p.canonicalize().ok()) {
        Some(p) => p,
        None => return false,
    };
    let target = match dir.canonicalize().ok() {
        Some(p) => p,
        None => return false,
    };
    file_parent == target
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch(path: &Path) -> String {
        touch_with(path, &path.to_string_lossy())
    }

    fn touch_with(path: &Path, contents: &str) -> String {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        path.to_string_lossy().to_string()
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn pair(jpg: &str, raw: Option<&str>, source: PhotoSource, xmps: &[&str]) -> ArchivePairInput {
        ArchivePairInput {
            jpg_path: jpg.to_string(),
            raw_path: raw.map(str::to_string),
            status: "starred".to_string(),
            source,
            xmp_paths: xmps.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Photos go in `<tmp>/photos`, previews in `<tmp>/cache`, so the cache
    /// never resolves outside the test's own temp dir.
    fn layout() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("photos");
        let cache = tmp.path().join("cache");
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&cache).unwrap();
        (tmp, base, cache)
    }

    fn try_archive(
        base: &Path,
        cache: &Path,
        pairs: &[ArchivePairInput],
    ) -> Result<ArchiveResult, String> {
        archive_pairs(base, pairs, cache, &mut |_| {})
    }

    fn archive(
        base: &Path,
        cache: &Path,
        pairs: &[ArchivePairInput],
    ) -> (ArchiveResult, Vec<ArchiveProgress>) {
        let mut events = Vec::new();
        let result = archive_pairs(base, pairs, cache, &mut |p| events.push(p)).unwrap();
        (result, events)
    }

    #[test]
    fn moves_a_normal_pair_into_jpg_and_raw_folders() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));
        let xmp = touch(&base.join("IMG_1.xmp"));

        let (result, events) = archive(
            &base,
            &cache,
            &[pair(&jpg, Some(&raw), PhotoSource::Jpg, &[&xmp])],
        );

        assert_eq!(result.moved_count, 2);
        assert_eq!(read(&base.join("JPG/IMG_1.jpg")), jpg);
        assert_eq!(read(&base.join("RAW/IMG_1.ARW")), raw);
        assert_eq!(read(&base.join("RAW/IMG_1.xmp")), xmp);
        assert!(!Path::new(&jpg).exists() && !Path::new(&raw).exists());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].current_file, "IMG_1.jpg");
    }

    #[test]
    fn xmp_of_a_jpg_only_pair_follows_the_jpg() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let xmp = touch(&base.join("IMG_1.xmp"));

        archive(
            &base,
            &cache,
            &[pair(&jpg, None, PhotoSource::Jpg, &[&xmp])],
        );

        assert!(base.join("JPG/IMG_1.xmp").exists());
    }

    #[test]
    fn leaves_deleted_pairs_alone() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let mut deleted = pair(&jpg, None, PhotoSource::Jpg, &[]);
        deleted.status = "deleted".to_string();

        let (result, _) = archive(&base, &cache, &[deleted]);

        assert_eq!(result.moved_count, 0);
        assert!(Path::new(&jpg).exists());
    }

    #[test]
    fn renames_on_collision_and_never_overwrites() {
        let (_tmp, base, cache) = layout();
        touch_with(&base.join("JPG/IMG_1.jpg"), "already archived");
        touch_with(&base.join("RAW/IMG_1.ARW"), "already archived");
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));

        archive(
            &base,
            &cache,
            &[pair(&jpg, Some(&raw), PhotoSource::Jpg, &[])],
        );

        assert_eq!(read(&base.join("JPG/IMG_1.jpg")), "already archived");
        assert_eq!(read(&base.join("RAW/IMG_1.ARW")), "already archived");
        assert_eq!(read(&base.join("JPG/IMG_1_1.jpg")), jpg);
        assert_eq!(read(&base.join("RAW/IMG_1_1.ARW")), raw);
    }

    #[test]
    fn skips_files_already_in_their_target_folder() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("JPG/IMG_1.jpg"));
        let raw = touch(&base.join("RAW/IMG_1.ARW"));

        let (result, _) = archive(
            &base,
            &cache,
            &[pair(&jpg, Some(&raw), PhotoSource::Jpg, &[])],
        );

        assert_eq!(result.moved_count, 0);
        assert!(Path::new(&jpg).exists() && Path::new(&raw).exists());
        assert!(!base.join("JPG/IMG_1_1.jpg").exists());
    }

    #[test]
    fn archives_a_raw_only_pair_and_cleans_up_its_cached_preview() {
        let (_tmp, base, cache) = layout();
        let preview = touch_with(&cache.join("abc.jpg"), "preview");
        let raw = touch(&base.join("IMG_2.CR3"));
        let xmp = touch(&base.join("IMG_2.xmp"));

        let (result, events) = archive(
            &base,
            &cache,
            &[pair(&preview, Some(&raw), PhotoSource::RawPreview, &[&xmp])],
        );

        assert_eq!(result.moved_count, 2);
        assert_eq!(read(&base.join("RAW/IMG_2.CR3")), raw);
        assert_eq!(read(&base.join("JPG/IMG_2.jpg")), "preview");
        assert_eq!(read(&base.join("RAW/IMG_2.xmp")), xmp);
        assert!(!Path::new(&preview).exists());
        assert_eq!(events[0].current_file, "IMG_2.CR3");
    }

    #[test]
    fn raw_only_preview_is_renamed_on_collision() {
        let (_tmp, base, cache) = layout();
        touch_with(&base.join("JPG/IMG_2.jpg"), "user jpg");
        let preview = touch_with(&cache.join("abc.jpg"), "preview");
        let raw = touch(&base.join("IMG_2.CR3"));

        archive(
            &base,
            &cache,
            &[pair(&preview, Some(&raw), PhotoSource::RawPreview, &[])],
        );

        assert_eq!(read(&base.join("JPG/IMG_2.jpg")), "user jpg");
        assert_eq!(read(&base.join("JPG/IMG_2_1.jpg")), "preview");
        assert_eq!(read(&base.join("RAW/IMG_2_1.CR3")), raw);
    }

    #[test]
    fn a_jpg_collision_renames_the_whole_pair_so_it_still_matches() {
        let (_tmp, base, cache) = layout();
        touch_with(&base.join("JPG/IMG_1.jpg"), "other photo");
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));
        let xmp = touch(&base.join("IMG_1.ARW.xmp"));

        archive(
            &base,
            &cache,
            &[pair(&jpg, Some(&raw), PhotoSource::Jpg, &[&xmp])],
        );

        assert_eq!(read(&base.join("JPG/IMG_1.jpg")), "other photo");
        assert_eq!(read(&base.join("JPG/IMG_1_1.jpg")), jpg);
        assert_eq!(read(&base.join("RAW/IMG_1_1.ARW")), raw);
        assert_eq!(read(&base.join("RAW/IMG_1_1.ARW.xmp")), xmp);
        assert!(!base.join("RAW/IMG_1.ARW").exists());
    }

    #[test]
    fn an_xmp_collision_renames_the_raw_only_pair_and_its_preview() {
        let (_tmp, base, cache) = layout();
        touch_with(&base.join("RAW/IMG_2.xmp"), "other edits");
        let preview = touch_with(&cache.join("abc.jpg"), "preview");
        let raw = touch(&base.join("IMG_2.CR3"));
        let xmp = touch(&base.join("IMG_2.xmp"));

        archive(
            &base,
            &cache,
            &[pair(&preview, Some(&raw), PhotoSource::RawPreview, &[&xmp])],
        );

        assert_eq!(read(&base.join("RAW/IMG_2.xmp")), "other edits");
        assert_eq!(read(&base.join("RAW/IMG_2_1.CR3")), raw);
        assert_eq!(read(&base.join("RAW/IMG_2_1.xmp")), xmp);
        assert_eq!(read(&base.join("JPG/IMG_2_1.jpg")), "preview");
    }

    #[test]
    fn move_file_moves_contents_and_removes_the_source() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("a.jpg");
        touch_with(&src, "data");
        let dest = tmp.path().join("sub/a.jpg");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();

        move_file(&src, &dest).unwrap();

        assert!(!src.exists());
        assert_eq!(read(&dest), "data");
    }

    #[test]
    fn refuses_a_raw_only_pair_whose_preview_is_outside_the_cache() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let user_jpg = touch_with(&base.join("IMG_3.jpg"), "user jpg");
        let raw = touch(&base.join("IMG_3.CR3"));
        let pairs = [
            pair(&jpg, None, PhotoSource::Jpg, &[]),
            pair(&user_jpg, Some(&raw), PhotoSource::RawPreview, &[]),
        ];

        let err = try_archive(&base, &cache, &pairs).unwrap_err();

        assert!(err.contains("not a cached RAW preview"), "{}", err);
        assert_eq!(read(Path::new(&user_jpg)), "user jpg");
        assert!(Path::new(&raw).exists());
        assert!(
            Path::new(&jpg).exists(),
            "nothing may move before validation"
        );
        assert!(!base.join("JPG").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_preview_cleanup_does_not_fail_the_archive() {
        use std::os::unix::fs::PermissionsExt;
        let (_tmp, base, cache) = layout();
        let preview = touch_with(&cache.join("abc.jpg"), "preview");
        let raw = touch(&base.join("IMG_2.CR3"));
        fs::set_permissions(&cache, fs::Permissions::from_mode(0o555)).unwrap();

        let result = try_archive(
            &base,
            &cache,
            &[pair(&preview, Some(&raw), PhotoSource::RawPreview, &[])],
        );

        fs::set_permissions(&cache, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(result.unwrap().moved_count, 2);
        assert_eq!(read(&base.join("JPG/IMG_2.jpg")), "preview");
        assert!(
            Path::new(&preview).exists(),
            "read-only cache keeps the preview"
        );
    }
}
