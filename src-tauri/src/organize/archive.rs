// ============================================================
// Sift - Archive
// Sorts the folder once culling is done:
//   starred  → JPG, RAW, XMP (and a RAW-only pair's preview) into 精选/
//   unmarked → JPG stays where it is; RAW and its XMP into RAW/
// Deleted pairs are trashed beforehand by `delete`, never touched here.
// Never overwrites: a pair that collides anywhere gets one shared `_N`
// suffix on every file, including a JPG left in place, so a rescan still
// pairs it. Each destination is claimed with `create_new` before moving.
// Each pair moves as a unit (`transfer`): if one of its files fails, the
// rest go back and the archive stops, so a pair is never split.
// ============================================================

use super::file_name;
use super::transfer::{self, MoveFn, Step, Transfer};
use crate::models::organize::OrganizeError;
use crate::models::photo::{
    ArchivePairInput, ArchiveProgress, ArchiveResult, PhotoSource, PhotoStatus,
};
use crate::paths;
use std::fs;
use std::path::{Path, PathBuf};

/// Folder for starred pairs, inside the archived folder
pub const PICKS_DIR: &str = "精选";
/// Folder for the RAWs of unmarked pairs; scan pairs it with the JPGs left in place
pub const RAW_DIR: &str = "RAW";

pub type OnProgress<'a> = dyn FnMut(ArchiveProgress) + 'a;

struct ArchiveDirs {
    picks: PathBuf,
    raw: PathBuf,
    preview_cache: PathBuf,
}

fn archive_dirs(base: &Path, preview_cache: &Path) -> ArchiveDirs {
    ArchiveDirs {
        picks: base.join(PICKS_DIR),
        raw: base.join(RAW_DIR),
        preview_cache: preview_cache.to_path_buf(),
    }
}

/// A RAW-only pair's `jpg_path` comes from the frontend and is removed after
/// being copied, so it must be one of our cached previews. Anything else may
/// be a user photo: refuse the whole archive rather than guess.
fn ensure_previews_are_cached(
    pairs: &[ArchivePairInput],
    cache: &Path,
) -> Result<(), OrganizeError> {
    let stray = pairs
        .iter()
        .filter(|p| p.status != PhotoStatus::Deleted && p.source == PhotoSource::RawPreview)
        .map(|p| Path::new(&p.jpg_path))
        .find(|jpg| jpg.exists() && !paths::is_inside_dir(jpg, cache));
    match stray {
        Some(jpg) => Err(OrganizeError::other(format!(
            "Refusing to archive {}: not a cached RAW preview",
            jpg.display()
        ))),
        None => Ok(()),
    }
}

/// Sort every non-deleted pair under `base` into `精选/` and `RAW/`; folders
/// are only created when something moves into them. `on_progress` is called
/// once per pair whose primary file exists. Stops at the first pair that
/// fails, after putting that pair back; the error counts the photos moved
/// before it.
pub fn archive_pairs(
    base: &Path,
    pairs: &[ArchivePairInput],
    preview_cache: &Path,
    on_progress: &mut OnProgress,
) -> Result<ArchiveResult, OrganizeError> {
    archive_pairs_moving(
        base,
        pairs,
        preview_cache,
        on_progress,
        &mut transfer::move_file,
    )
}

/// `archive_pairs` with the file move injectable, so tests can fail it.
pub(super) fn archive_pairs_moving(
    base: &Path,
    pairs: &[ArchivePairInput],
    preview_cache: &Path,
    on_progress: &mut OnProgress,
    move_file: &mut MoveFn,
) -> Result<ArchiveResult, OrganizeError> {
    // Validate before creating folders or moving anything, so a bad request
    // leaves the user's folder exactly as it was.
    ensure_previews_are_cached(pairs, preview_cache)?;

    let dirs = archive_dirs(base, preview_cache);
    let total = pairs.len();
    let mut moved_count: usize = 0;

    for (i, pair) in pairs.iter().enumerate() {
        if pair.status == PhotoStatus::Deleted {
            continue;
        }
        if let Some(primary) = primary_file(pair) {
            on_progress(progress(i + 1, total, primary));
        }
        moved_count += archive_pair_choosing(
            pair,
            &dirs,
            paths::unique_paths_sharing_stem_from,
            move_file,
        )
        .map_err(|e| e.with_completed(moved_count))?;
    }

    Ok(ArchiveResult {
        moved_count,
        picks_folder: dirs.picks.to_string_lossy().to_string(),
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

/// Best-effort cleanup once the preview has been copied into 精选/. A failure
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

/// One file of a pair and where it should end up. `dest == src` means it
/// stays put unless the group needs a suffix.
struct PlannedFile<'a> {
    src: &'a Path,
    dest: PathBuf,
    transfer: Transfer,
    is_photo: bool,
}

impl PlannedFile<'_> {
    fn stays(&self, dest: &Path) -> bool {
        self.src == dest
    }
}

/// The file shown in progress: the JPG of a normal pair, the RAW of a RAW-only one.
fn primary_file(pair: &ArchivePairInput) -> Option<&Path> {
    let primary = match pair.source {
        PhotoSource::RawPreview => pair.raw_path.as_deref()?,
        PhotoSource::Jpg => &pair.jpg_path,
    };
    Some(Path::new(primary)).filter(|p| p.exists())
}

/// Where each existing file of a pair should go. Starred: everything into
/// 精选/, plus a RAW-only pair's preview saved as `<raw stem>.jpg` so the
/// folder can be browsed and re-opened as a proper pair. Unmarked: the RAW
/// into RAW/, the JPG stays; XMP follows the RAW if there is one, otherwise
/// the JPG. A file already in its target folder stays. Empty when nothing
/// would move.
fn plan_pair<'a>(pair: &'a ArchivePairInput, dirs: &ArchiveDirs) -> Vec<PlannedFile<'a>> {
    let is_pick = pair.status == PhotoStatus::Starred;
    let raw = pair.raw_path.as_deref().map(Path::new);
    let jpg = Path::new(&pair.jpg_path);
    // None: the file stays in its own folder
    let raw_dir = Some(if is_pick { &dirs.picks } else { &dirs.raw });
    let jpg_dir = is_pick.then_some(&dirs.picks);

    let mut files = Vec::new();
    let mut add = |src: &'a Path, dir: Option<&PathBuf>, name: String, transfer, is_photo| {
        let dest = match dir {
            Some(dir) if !is_already_in_dir(src, dir) => dir.join(name),
            _ => src.to_path_buf(),
        };
        files.push(PlannedFile {
            src,
            dest,
            transfer,
            is_photo,
        });
    };

    if pair.source == PhotoSource::RawPreview {
        if is_pick {
            add(
                jpg,
                jpg_dir,
                preview_dest_name(pair),
                Transfer::CopyPreview,
                true,
            );
        }
    } else {
        add(jpg, jpg_dir, file_name(jpg), Transfer::Move, true);
    }
    if let Some(raw) = raw {
        add(raw, raw_dir, file_name(raw), Transfer::Move, true);
    }
    let xmp_dir = if raw.is_some() { raw_dir } else { jpg_dir };
    for xmp in pair.xmp_paths.iter().map(Path::new) {
        add(xmp, xmp_dir, file_name(xmp), Transfer::Move, false);
    }

    files.retain(|f| f.src.exists());
    if files.iter().all(|f| f.stays(&f.dest)) {
        files.clear();
    }
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

/// Archive one pair. If any of its names is taken, the whole pair gets the
/// same `_N` suffix in every folder, a JPG left in place included, so a rescan
/// still pairs it. Returns how many photos (JPG/RAW, not XMP) were moved.
/// `choose` picks the destinations; tests inject it to create a file between
/// the choice and the claim.
fn archive_pair_choosing(
    pair: &ArchivePairInput,
    dirs: &ArchiveDirs,
    mut choose: impl FnMut(&[&Path], &[PathBuf], &str) -> Vec<PathBuf>,
    move_file: &mut MoveFn,
) -> Result<usize, OrganizeError> {
    let files = plan_pair(pair, dirs);
    if files.is_empty() {
        return Ok(0);
    }
    for file in files.iter().filter(|f| !f.stays(&f.dest)) {
        let dir = file.dest.parent().unwrap_or(&file.dest);
        fs::create_dir_all(dir).map_err(|e| OrganizeError::io(&e, dir, "Failed to create"))?;
    }

    let sources: Vec<&Path> = files.iter().map(|f| f.src).collect();
    let candidates: Vec<PathBuf> = files.iter().map(|f| f.dest.clone()).collect();
    let stem = pair_stem(pair);
    let mut dests = Vec::new();
    let mut placeholders = Vec::new();
    // Only files that change path are claimed; one that stays is its own dest
    let claimed = paths::claim_first_free(
        || {
            dests = choose(&sources, &candidates, &stem);
            files
                .iter()
                .zip(&dests)
                .filter(|(file, dest)| !file.stays(dest))
                .map(|(_, dest)| dest.clone())
                .collect()
        },
        &mut placeholders,
    );
    if let Err(e) = claimed {
        transfer::remove_empty_placeholders(&placeholders, &[]);
        let photo = primary_file(pair).unwrap_or(files[0].src);
        return Err(OrganizeError::io(
            &e,
            photo,
            "Failed to reserve archive file names for",
        ));
    }

    // Every changed dest is now an empty placeholder this call created, so
    // rename and fs::copy only ever replace our own file.
    let moving: Vec<(&PlannedFile, &PathBuf)> = files
        .iter()
        .zip(&dests)
        .filter(|(file, dest)| !file.stays(dest))
        .collect();
    let steps: Vec<Step> = moving
        .iter()
        .map(|(file, dest)| Step {
            src: file.src,
            dest,
            transfer: file.transfer,
        })
        .collect();
    transfer::move_group(&steps, placeholders, move_file)
        .map_err(|e| e.context(format_args!("Failed to archive {}", stem)))?;

    for (file, _) in moving
        .iter()
        .filter(|(f, _)| f.transfer == Transfer::CopyPreview)
    {
        remove_cached_preview(file.src, &dirs.preview_cache);
    }
    Ok(moving.iter().filter(|(file, _)| file.is_photo).count())
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
    use crate::models::photo::PhotoStatus;
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
            status: PhotoStatus::Starred,
            source,
            xmp_paths: xmps.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn unmarked(mut input: ArchivePairInput) -> ArchivePairInput {
        input.status = PhotoStatus::Unprocessed;
        input
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

    fn picks(base: &Path) -> PathBuf {
        base.join(PICKS_DIR)
    }

    fn try_archive(
        base: &Path,
        cache: &Path,
        pairs: &[ArchivePairInput],
    ) -> Result<ArchiveResult, OrganizeError> {
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
    fn moves_a_starred_pair_with_its_xmp_into_the_picks_folder() {
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
        assert_eq!(read(&picks(&base).join("IMG_1.jpg")), jpg);
        assert_eq!(read(&picks(&base).join("IMG_1.ARW")), raw);
        assert_eq!(read(&picks(&base).join("IMG_1.xmp")), xmp);
        assert!(!Path::new(&jpg).exists() && !Path::new(&raw).exists());
        assert!(!base.join(RAW_DIR).exists());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].current_file, "IMG_1.jpg");
    }

    #[test]
    fn an_unmarked_pair_keeps_its_jpg_in_place_and_moves_its_raw_and_xmp() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));
        let xmp = touch(&base.join("IMG_1.xmp"));

        let (result, _) = archive(
            &base,
            &cache,
            &[unmarked(pair(&jpg, Some(&raw), PhotoSource::Jpg, &[&xmp]))],
        );

        assert_eq!(result.moved_count, 1);
        assert_eq!(read(Path::new(&jpg)), jpg);
        assert_eq!(read(&base.join("RAW/IMG_1.ARW")), raw);
        assert_eq!(read(&base.join("RAW/IMG_1.xmp")), xmp);
        assert!(!Path::new(&raw).exists());
        assert!(!picks(&base).exists());
    }

    #[test]
    fn an_unmarked_jpg_only_pair_and_its_xmp_stay_put() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let xmp = touch(&base.join("IMG_1.xmp"));

        let (result, _) = archive(
            &base,
            &cache,
            &[unmarked(pair(&jpg, None, PhotoSource::Jpg, &[&xmp]))],
        );

        assert_eq!(result.moved_count, 0);
        assert!(Path::new(&jpg).exists() && Path::new(&xmp).exists());
        assert!(
            !base.join(RAW_DIR).exists() && !picks(&base).exists(),
            "no empty folders"
        );
    }

    #[test]
    fn xmp_of_a_starred_jpg_only_pair_goes_with_it() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let xmp = touch(&base.join("IMG_1.xmp"));

        archive(
            &base,
            &cache,
            &[pair(&jpg, None, PhotoSource::Jpg, &[&xmp])],
        );

        assert_eq!(read(&picks(&base).join("IMG_1.xmp")), xmp);
    }

    #[test]
    fn leaves_deleted_pairs_alone() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let mut deleted = pair(&jpg, None, PhotoSource::Jpg, &[]);
        deleted.status = PhotoStatus::Deleted;

        let (result, _) = archive(&base, &cache, &[deleted]);

        assert_eq!(result.moved_count, 0);
        assert!(Path::new(&jpg).exists());
    }

    #[test]
    fn a_starred_pair_is_renamed_as_a_group_and_never_overwrites() {
        let (_tmp, base, cache) = layout();
        touch_with(&picks(&base).join("IMG_1.jpg"), "already archived");
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));
        let xmp = touch(&base.join("IMG_1.ARW.xmp"));

        archive(
            &base,
            &cache,
            &[pair(&jpg, Some(&raw), PhotoSource::Jpg, &[&xmp])],
        );

        assert_eq!(read(&picks(&base).join("IMG_1.jpg")), "already archived");
        assert_eq!(read(&picks(&base).join("IMG_1_1.jpg")), jpg);
        assert_eq!(read(&picks(&base).join("IMG_1_1.ARW")), raw);
        assert_eq!(read(&picks(&base).join("IMG_1_1.ARW.xmp")), xmp);
        assert!(!picks(&base).join("IMG_1.ARW").exists());
    }

    #[test]
    fn a_raw_collision_renames_the_jpg_left_in_place_so_the_pair_still_matches() {
        let (_tmp, base, cache) = layout();
        touch_with(&base.join("RAW/IMG_1.ARW"), "other card");
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));
        let xmp = touch(&base.join("IMG_1.xmp"));

        let (result, _) = archive(
            &base,
            &cache,
            &[unmarked(pair(&jpg, Some(&raw), PhotoSource::Jpg, &[&xmp]))],
        );

        assert_eq!(result.moved_count, 2);
        assert_eq!(read(&base.join("RAW/IMG_1.ARW")), "other card");
        assert_eq!(read(&base.join("RAW/IMG_1_1.ARW")), raw);
        assert_eq!(read(&base.join("RAW/IMG_1_1.xmp")), xmp);
        assert_eq!(read(&base.join("IMG_1_1.jpg")), jpg);
        assert!(!Path::new(&jpg).exists());
    }

    #[test]
    fn the_in_place_rename_never_overwrites_a_photo_next_to_it() {
        let (_tmp, base, cache) = layout();
        touch_with(&base.join("RAW/IMG_1.ARW"), "other card");
        touch_with(&base.join("IMG_1_1.jpg"), "another photo");
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));

        archive(
            &base,
            &cache,
            &[unmarked(pair(&jpg, Some(&raw), PhotoSource::Jpg, &[]))],
        );

        assert_eq!(read(&base.join("IMG_1_1.jpg")), "another photo");
        assert_eq!(read(&base.join("IMG_1_2.jpg")), jpg);
        assert_eq!(read(&base.join("RAW/IMG_1_2.ARW")), raw);
    }

    #[test]
    fn skips_files_already_in_their_target_folder() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("RAW/IMG_1.ARW"));

        let (result, _) = archive(
            &base,
            &cache,
            &[unmarked(pair(&jpg, Some(&raw), PhotoSource::Jpg, &[]))],
        );

        assert_eq!(result.moved_count, 0);
        assert!(Path::new(&jpg).exists() && Path::new(&raw).exists());
        assert!(!base.join("RAW/IMG_1_1.ARW").exists());
    }

    #[test]
    fn a_starred_pair_whose_raw_was_archived_earlier_joins_the_picks() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("RAW/IMG_1.ARW"));

        let (result, _) = archive(
            &base,
            &cache,
            &[pair(&jpg, Some(&raw), PhotoSource::Jpg, &[])],
        );

        assert_eq!(result.moved_count, 2);
        assert_eq!(read(&picks(&base).join("IMG_1.jpg")), jpg);
        assert_eq!(read(&picks(&base).join("IMG_1.ARW")), raw);
    }

    #[test]
    fn a_starred_raw_only_pair_gets_its_preview_saved_next_to_it() {
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
        assert_eq!(read(&picks(&base).join("IMG_2.CR3")), raw);
        assert_eq!(read(&picks(&base).join("IMG_2.jpg")), "preview");
        assert_eq!(read(&picks(&base).join("IMG_2.xmp")), xmp);
        assert!(!Path::new(&preview).exists());
        assert_eq!(events[0].current_file, "IMG_2.CR3");
    }

    #[test]
    fn an_unmarked_raw_only_pair_moves_its_raw_without_writing_a_jpg() {
        let (_tmp, base, cache) = layout();
        let preview = touch_with(&cache.join("abc.jpg"), "preview");
        let raw = touch(&base.join("IMG_2.CR3"));
        let xmp = touch(&base.join("IMG_2.xmp"));

        let (result, _) = archive(
            &base,
            &cache,
            &[unmarked(pair(
                &preview,
                Some(&raw),
                PhotoSource::RawPreview,
                &[&xmp],
            ))],
        );

        assert_eq!(result.moved_count, 1);
        assert_eq!(read(&base.join("RAW/IMG_2.CR3")), raw);
        assert_eq!(read(&base.join("RAW/IMG_2.xmp")), xmp);
        assert!(!base.join("IMG_2.jpg").exists());
        assert_eq!(read(Path::new(&preview)), "preview", "cache left alone");
    }

    #[test]
    fn raw_only_preview_is_renamed_on_collision() {
        let (_tmp, base, cache) = layout();
        touch_with(&picks(&base).join("IMG_2.jpg"), "user jpg");
        let preview = touch_with(&cache.join("abc.jpg"), "preview");
        let raw = touch(&base.join("IMG_2.CR3"));

        archive(
            &base,
            &cache,
            &[pair(&preview, Some(&raw), PhotoSource::RawPreview, &[])],
        );

        assert_eq!(read(&picks(&base).join("IMG_2.jpg")), "user jpg");
        assert_eq!(read(&picks(&base).join("IMG_2_1.jpg")), "preview");
        assert_eq!(read(&picks(&base).join("IMG_2_1.CR3")), raw);
    }

    #[test]
    fn a_file_created_after_the_names_were_chosen_is_never_overwritten() {
        let (_tmp, base, cache) = layout();
        let jpg = touch(&base.join("IMG_1.jpg"));
        let raw = touch(&base.join("IMG_1.ARW"));
        let xmp = touch(&base.join("IMG_1.xmp"));
        let dirs = archive_dirs(&base, &cache);
        let mut is_first_try = true;

        // A sync client drops 精选/IMG_1.ARW right after archive checked that
        // the name was free: the window `create_new` has to close.
        let moved = archive_pair_choosing(
            &pair(&jpg, Some(&raw), PhotoSource::Jpg, &[&xmp]),
            &dirs,
            |sources, candidates, stem| {
                let chosen = paths::unique_paths_sharing_stem_from(sources, candidates, stem);
                if std::mem::take(&mut is_first_try) {
                    fs::write(&chosen[1], "other process").unwrap();
                }
                chosen
            },
            &mut transfer::move_file,
        )
        .unwrap();

        assert_eq!(moved, 2);
        assert_eq!(read(&picks(&base).join("IMG_1.ARW")), "other process");
        assert_eq!(read(&picks(&base).join("IMG_1_1.jpg")), jpg);
        assert_eq!(read(&picks(&base).join("IMG_1_1.ARW")), raw);
        assert_eq!(read(&picks(&base).join("IMG_1_1.xmp")), xmp);
        assert!(!Path::new(&jpg).exists() && !Path::new(&raw).exists());
        assert!(
            !picks(&base).join("IMG_1.jpg").exists(),
            "the abandoned empty placeholder is removed"
        );
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

        assert!(err.detail.contains("not a cached RAW preview"), "{:?}", err);
        assert_eq!(read(Path::new(&user_jpg)), "user jpg");
        assert!(Path::new(&raw).exists());
        assert!(
            Path::new(&jpg).exists(),
            "nothing may move before validation"
        );
        assert!(!picks(&base).exists());
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
        assert_eq!(read(&picks(&base).join("IMG_2.jpg")), "preview");
        assert!(
            Path::new(&preview).exists(),
            "read-only cache keeps the preview"
        );
    }
}
