// ============================================================
// Sift - Folder Scan
// Smart scan: detects folder structure and pairs JPG with RAW
// Supports: loose files, RAWs that archive moved into RAW/ (paired with the
// JPGs left in the folder), and the older JPG/ + RAW/ archive layout, even
// with new photos copied into the root next to it
// Picks archived into 精选/ come back starred, paired only among themselves;
// subfolders of 精选/ (like every other subfolder) are not read
// RAW-only files get their embedded JPEG preview extracted; any whose
// preview can't be extracted are reported in `skipped`, never dropped, and
// so is anything that can't be read (a folder or one of its entries).
// Empty (0-byte) photo files are reported in `skipped` too, rather than
// paired: they are leftovers of an interrupted copy or move, and pairing one
// would push the real photo with the same name out of its pair.
// Each pair gets its capture time, from the JPG first and the RAW when the
// JPG doesn't record one; the session sorts by it.
// ============================================================

use crate::catalog::file_types::{is_jpg, is_raw};
use crate::catalog::pairing::{self, FileIndex, RawOnly};
use crate::media::capture_time::read_capture_time;
use crate::media::raw_preview::extract_raw_preview;
use crate::models::photo::{
    CaptureTime, PhotoPair, PhotoSource, PhotoStatus, ScanResult, SkipReason, SkippedFile,
};
use crate::organize::archive::{PICKS_DIR, RAW_DIR};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

/// RAW files are 25-60MB each; unbounded parallel preview extraction can exhaust memory
const PREVIEW_EXTRACTION_THREADS: usize = 2;

/// JPG folder of the archive layout before 精选/ (JPG/ + RAW/); its RAWs are in RAW/
pub(crate) const LEGACY_JPG_DIR: &str = "JPG";

pub fn scan_folder(folder: &Path, preview_cache_dir: &Path) -> Result<ScanResult, String> {
    if !folder.is_dir() {
        return Err(format!("Folder does not exist: {}", folder.display()));
    }

    let mut index = FileIndex::default();
    let mut skipped = index_dir(folder, &mut index);

    // RAW/ holds the RAWs of JPGs in the root (archive) or in JPG/ (older
    // archives). Reading only one side would turn every RAW of the other into
    // a RAW-only entry, so both are always read when present.
    for dir in [folder.join(RAW_DIR), folder.join(LEGACY_JPG_DIR)] {
        if dir.is_dir() {
            skipped.extend(index_dir(&dir, &mut index));
        }
    }

    // 精选/ is paired on its own: archive keeps a pick's files together
    // there, and a same-named photo outside it is a different photo.
    let mut picks_index = FileIndex::default();
    let picks_dir = folder.join(PICKS_DIR);
    if picks_dir.is_dir() {
        skipped.extend(index_dir(&picks_dir, &mut picks_index));
    }

    let empty_count = skipped
        .iter()
        .filter(|f| f.reason == SkipReason::Empty)
        .count();
    let entries = entries_of(&index, preview_cache_dir, &mut skipped)?;
    let mut picks = entries_of(&picks_index, preview_cache_dir, &mut skipped)?;
    for pair in &mut picks.pairs {
        pair.is_archived_pick = true;
        pair.status = PhotoStatus::Starred;
    }

    let mut pairs = entries.pairs;
    pairs.append(&mut picks.pairs);
    pairs.par_iter_mut().for_each(|pair| {
        pair.capture_time = capture_time_of(pair);
    });
    pairing::sort_naturally(&mut pairs);

    Ok(ScanResult {
        pairs,
        total_files: index.photo_count() + picks_index.photo_count() + empty_count,
        paired_count: entries.paired_count + picks.paired_count,
        jpg_only_count: entries.jpg_only_count + picks.jpg_only_count,
        raw_only_count: entries.raw_only_count + picks.raw_only_count,
        skipped,
    })
}

/// Session entries from one pairing scope, with their counts
struct Entries {
    pairs: Vec<PhotoPair>,
    paired_count: usize,
    jpg_only_count: usize,
    raw_only_count: usize,
}

/// Pair the files of `index` and extract previews for the RAW-only ones;
/// RAWs without a usable preview are added to `skipped`.
fn entries_of(
    index: &FileIndex,
    preview_cache_dir: &Path,
    skipped: &mut Vec<SkippedFile>,
) -> Result<Entries, String> {
    let pairing = pairing::pair(index);
    let (raw_only_pairs, no_preview) = extract_previews(pairing.raw_only, preview_cache_dir)?;
    skipped.extend(no_preview);

    let raw_only_count = raw_only_pairs.len();
    let mut pairs = pairing.pairs;
    pairs.extend(raw_only_pairs);
    Ok(Entries {
        pairs,
        paired_count: pairing.paired_count,
        jpg_only_count: pairing.jpg_only_count,
        raw_only_count,
    })
}

/// A RAW-only pair's `jpg_path` is a preview in the cache, which doesn't
/// record when the photo was taken; its RAW does.
fn capture_time_of(pair: &PhotoPair) -> Option<CaptureTime> {
    let jpg = (pair.source == PhotoSource::Jpg).then(|| Path::new(&pair.jpg_path));
    jpg.and_then(read_capture_time).or_else(|| {
        pair.raw_path
            .as_deref()
            .map(Path::new)
            .and_then(read_capture_time)
    })
}

/// Add the files directly inside `dir` (non-recursive) to the index.
/// Whatever can't be read (the folder itself or one of its entries) is
/// returned as skipped: it may hold photos the user would otherwise lose
/// track of. So are empty photo files, which are never indexed.
fn index_dir(dir: &Path, index: &mut FileIndex) -> Vec<SkippedFile> {
    let mut skipped = Vec::new();
    for entry in WalkDir::new(dir).max_depth(1) {
        match entry {
            Ok(e) if is_file(&e) && is_empty_photo(e.path()) => skipped.push(SkippedFile {
                path: e.path().to_string_lossy().into_owned(),
                reason: SkipReason::Empty,
            }),
            Ok(e) if is_file(&e) => index.add(e.path()),
            Ok(_) => {}
            Err(e) => skipped.push(unreadable_entry(dir, &e)),
        }
    }
    skipped
}

/// The entry's type comes from the directory listing, so plain files cost no
/// extra `stat`; only symlinks are resolved, keeping links to files indexed.
fn is_file(entry: &DirEntry) -> bool {
    let file_type = entry.file_type();
    file_type.is_file() || (file_type.is_symlink() && entry.path().is_file())
}

/// A JPG or RAW of zero bytes. Only photo files pay for the size check.
fn is_empty_photo(path: &Path) -> bool {
    let is_photo = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| is_jpg(ext) || is_raw(ext));
    is_photo && std::fs::metadata(path).is_ok_and(|m| m.len() == 0)
}

fn unreadable_entry(dir: &Path, error: &walkdir::Error) -> SkippedFile {
    let path = error.path().unwrap_or(dir);
    eprintln!("[Sift] Failed to read {}: {}", path.display(), error);
    SkippedFile {
        path: path.to_string_lossy().to_string(),
        reason: SkipReason::Unreadable,
    }
}

/// Extract embedded previews for RAW-only files. RAWs whose preview can't be
/// extracted are returned as skipped so the user is told about them.
fn extract_previews(
    raw_only: Vec<RawOnly>,
    cache_dir: &Path,
) -> Result<(Vec<PhotoPair>, Vec<SkippedFile>), String> {
    if raw_only.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let pool = ThreadPoolBuilder::new()
        .num_threads(PREVIEW_EXTRACTION_THREADS)
        .build()
        .map_err(|e| format!("Failed to build thread pool: {}", e))?;

    let results: Vec<Result<PhotoPair, SkippedFile>> = pool.install(|| {
        raw_only
            .into_par_iter()
            .map(|raw| raw_preview_pair(raw, cache_dir))
            .collect()
    });
    let (pairs, skipped): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);
    Ok((
        pairs.into_iter().filter_map(Result::ok).collect(),
        skipped.into_iter().filter_map(Result::err).collect(),
    ))
}

fn raw_preview_pair(raw: RawOnly, cache_dir: &Path) -> Result<PhotoPair, SkippedFile> {
    match extract_raw_preview(&raw.raw_path, cache_dir) {
        Ok(preview_path) => Ok(pairing::new_pair(
            preview_path,
            Some((raw.raw_path, raw.ext.as_str())),
            PhotoSource::RawPreview,
            raw.xmp_paths,
        )),
        Err(e) => {
            eprintln!(
                "[Sift] Failed to extract preview from {}: {}",
                raw.raw_path, e
            );
            Err(SkippedFile {
                path: raw.raw_path,
                reason: SkipReason::NoPreview,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        exif_tiff, jpeg_with_exif, noise_jpeg, raf_with_jpeg, tiff_raw_declaring_preview_len,
        tiff_raw_with_preview, write, ExifByteOrder,
    };
    use tempfile::{tempdir, TempDir};

    fn scan(dir: &Path) -> ScanResult {
        // Previews land in a subfolder of the scanned dir so they outlive this call
        let cache = dir.join(".cache");
        std::fs::create_dir_all(&cache).unwrap();
        scan_folder(dir, &cache).unwrap()
    }

    fn file_name(path: &str) -> &str {
        Path::new(path).file_name().unwrap().to_str().unwrap()
    }

    fn names(result: &ScanResult) -> Vec<&str> {
        result
            .pairs
            .iter()
            .map(|p| file_name(&p.jpg_path))
            .collect()
    }

    fn folder(files: &[&str]) -> TempDir {
        let dir = tempdir().unwrap();
        for f in files {
            write(dir.path(), f, b"x");
        }
        dir
    }

    #[test]
    fn sorts_pairs_naturally() {
        let dir = folder(&["IMG_10.jpg", "IMG_2.jpg", "IMG_1.jpg"]);
        assert_eq!(
            names(&scan(dir.path())),
            ["IMG_1.jpg", "IMG_2.jpg", "IMG_10.jpg"]
        );
    }

    #[test]
    fn pairs_and_counts_loose_files() {
        let dir = folder(&[
            "IMG_1.JPG",
            "IMG_1.CR3",
            "IMG_2.jpg",
            "IMG_1.xmp",
            "notes.txt",
        ]);
        let result = scan(dir.path());

        assert_eq!(result.total_files, 3);
        assert_eq!(result.paired_count, 1);
        assert_eq!(result.jpg_only_count, 1);
        assert_eq!(result.raw_only_count, 0);
        assert_eq!(result.pairs[0].xmp_paths.len(), 1);
    }

    #[test]
    fn extracts_preview_for_raw_only_files() {
        let dir = tempdir().unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), Some(6));
        write(dir.path(), "DSC_1.ARW", &raw);
        let result = scan(dir.path());

        assert_eq!(result.raw_only_count, 1);
        let pair = &result.pairs[0];
        assert_eq!(pair.source, PhotoSource::RawPreview);
        assert_eq!(file_name(pair.raw_path.as_deref().unwrap()), "DSC_1.ARW");
        assert!(Path::new(&pair.jpg_path).exists());
    }

    #[test]
    fn reports_raw_only_files_without_extractable_preview() {
        let dir = folder(&["DSC_1.ARW"]);
        let result = scan(dir.path());

        assert_eq!(result.total_files, 1);
        assert!(result.pairs.is_empty());
        assert_eq!(result.raw_only_count, 0);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(file_name(&result.skipped[0].path), "DSC_1.ARW");
        assert_eq!(result.skipped[0].reason, SkipReason::NoPreview);
    }

    #[test]
    fn reports_a_raw_whose_exif_declares_a_huge_preview() {
        let dir = tempdir().unwrap();
        let raw = tiff_raw_declaring_preview_len(&noise_jpeg(16, 16), u32::MAX);
        write(dir.path(), "DSC_1.NEF", &raw);

        let result = scan(dir.path());

        assert!(result.pairs.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].reason, SkipReason::NoPreview);
    }

    #[test]
    fn keeps_same_stem_collisions_in_the_session() {
        let dir = folder(&["IMG_1.jpg", "IMG_1.jpeg"]);
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        write(dir.path(), "IMG_1.CR3", &raw);
        write(dir.path(), "IMG_1.DNG", &raw);
        let result = scan(dir.path());

        assert_eq!(result.pairs.len(), 3);
        assert_eq!(result.paired_count, 1);
        assert_eq!(result.jpg_only_count, 1);
        assert_eq!(result.raw_only_count, 1);
        let mut raws: Vec<&str> = result
            .pairs
            .iter()
            .filter_map(|p| p.raw_path.as_deref().map(file_name))
            .collect();
        raws.sort();
        assert_eq!(raws, ["IMG_1.CR3", "IMG_1.DNG"]);
    }

    #[test]
    fn pairs_files_whose_stems_differ_only_in_case() {
        let dir = folder(&["img_1.jpg", "IMG_1.CR3", "Img_1.xmp", "img_1.cr3.xmp"]);
        let result = scan(dir.path());

        assert_eq!(result.total_files, 2);
        assert_eq!(result.paired_count, 1);
        assert_eq!(result.raw_only_count, 0);
        let pair = &result.pairs[0];
        assert_eq!(file_name(&pair.jpg_path), "img_1.jpg");
        assert_eq!(file_name(pair.raw_path.as_deref().unwrap()), "IMG_1.CR3");
        let mut xmps: Vec<&str> = pair.xmp_paths.iter().map(|p| file_name(p)).collect();
        xmps.sort();
        assert_eq!(xmps, ["Img_1.xmp", "img_1.cr3.xmp"]);
    }

    #[test]
    fn accounts_for_every_photo_file() {
        let dir = folder(&[
            "IMG_1.jpg",
            "IMG_1.jpeg",
            "IMG_1.CR3",
            "DSC_1.ARW",
            "DSC_1.NEF",
        ]);
        let result = scan(dir.path());

        let in_pairs = 2 * result.paired_count + result.jpg_only_count + result.raw_only_count;
        assert_eq!(result.total_files, 5);
        assert_eq!(in_pairs + result.skipped.len(), result.total_files);
    }

    #[test]
    fn pairs_jpgs_left_in_place_with_raws_in_the_raw_subfolder() {
        let dir = folder(&["IMG_1.JPG", "RAW/IMG_1.CR3", "RAW/IMG_1.xmp"]);

        let result = scan(dir.path());

        assert_eq!(names(&result), ["IMG_1.JPG"]);
        let pair = &result.pairs[0];
        assert_eq!(pair.raw_path.as_deref().map(file_name), Some("IMG_1.CR3"));
        assert_eq!(pair.xmp_paths.len(), 1);
    }

    #[test]
    fn a_raw_in_the_raw_subfolder_without_a_jpg_is_still_loaded() {
        let dir = folder(&["IMG_1.JPG"]);
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        write(dir.path(), "RAW/IMG_2.ARW", &raw);

        let result = scan(dir.path());

        assert_eq!(result.pairs.len(), 2);
        assert_eq!(result.raw_only_count, 1);
    }

    #[test]
    fn reads_archived_jpg_and_raw_subfolders_when_root_has_no_jpg() {
        let dir = folder(&["JPG/IMG_1.JPG", "RAW/IMG_1.CR3", "RAW/IMG_1.xmp"]);
        let result = scan(dir.path());

        assert_eq!(result.paired_count, 1);
        assert_eq!(result.pairs[0].xmp_paths.len(), 1);
    }

    #[test]
    fn pairs_the_jpg_and_raw_subfolders_even_when_root_has_photos() {
        // An older archive's JPG/ + RAW/, plus a new card copied into the root
        let dir = folder(&["DSC_1.JPG", "DSC_1.ARW", "JPG/DSC_2.JPG", "RAW/DSC_2.ARW"]);

        let result = scan(dir.path());

        assert_eq!(names(&result), ["DSC_1.JPG", "DSC_2.JPG"]);
        assert!(result.pairs.iter().all(|p| p.raw_path.is_some()));
        assert_eq!(result.raw_only_count, 0);
    }

    #[test]
    fn ignores_other_subfolders() {
        let dir = folder(&[
            "IMG_1.JPG",
            "other/IMG_4.JPG",
            "精选/RAW/IMG_5.ARW",
            "精选/精选/IMG_6.JPG",
            "精选/JPG/IMG_7.JPG",
        ]);
        assert_eq!(names(&scan(dir.path())), ["IMG_1.JPG"]);
    }

    fn pair_named<'a>(result: &'a ScanResult, path: &str) -> &'a PhotoPair {
        result
            .pairs
            .iter()
            .find(|p| p.jpg_path.ends_with(path))
            .unwrap_or_else(|| panic!("no pair for {}", path))
    }

    #[test]
    fn loads_archived_picks_starred_with_their_raw_and_xmp() {
        let dir = folder(&[
            "IMG_1.JPG",
            "精选/IMG_3.JPG",
            "精选/IMG_3.ARW",
            "精选/IMG_3.xmp",
        ]);
        let result = scan(dir.path());

        assert_eq!(names(&result), ["IMG_1.JPG", "IMG_3.JPG"]);
        assert_eq!(result.total_files, 3);
        assert_eq!(result.paired_count, 1);
        assert_eq!(result.jpg_only_count, 1);

        let loose = pair_named(&result, "IMG_1.JPG");
        assert_eq!(loose.status, PhotoStatus::Unprocessed);
        assert!(!loose.is_archived_pick);

        let pick = pair_named(&result, &format!("{}/IMG_3.JPG", PICKS_DIR));
        assert_eq!(pick.status, PhotoStatus::Starred);
        assert!(pick.is_archived_pick);
        assert_eq!(file_name(pick.raw_path.as_deref().unwrap()), "IMG_3.ARW");
        assert_eq!(pick.xmp_paths.len(), 1);
    }

    #[test]
    fn never_pairs_a_pick_with_a_same_named_file_outside_the_picks_folder() {
        let dir = folder(&[
            // Two different photos named DSC_1: one unmarked, one archived pick
            "DSC_1.JPG",
            "RAW/DSC_1.ARW",
            "精选/DSC_1.JPG",
            "精选/DSC_1.ARW",
            // A loose JPG whose only same-named RAW is a pick's
            "DSC_2.JPG",
            "精选/DSC_2.ARW",
            // A pick's sidecar never goes with a loose photo
            "DSC_3.JPG",
            "精选/DSC_3.xmp",
        ]);
        let picks = dir.path().join(PICKS_DIR);
        let result = scan(dir.path());

        assert_eq!(result.pairs.len(), 4);
        assert_eq!(result.paired_count, 2);
        assert_eq!(result.jpg_only_count, 2);
        // The pick's RAW stays RAW-only (its fixture has no preview to show)
        assert_eq!(result.skipped.len(), 1);
        assert!(Path::new(&result.skipped[0].path).starts_with(&picks));
        for pair in &result.pairs {
            let in_picks = Path::new(&pair.jpg_path).starts_with(&picks);
            assert_eq!(pair.is_archived_pick, in_picks, "{}", pair.jpg_path);
            if let Some(raw) = &pair.raw_path {
                let raw_in_picks = Path::new(raw).starts_with(&picks);
                assert_eq!(raw_in_picks, in_picks, "{} paired across folders", raw);
            }
            assert!(pair.xmp_paths.is_empty(), "{}", pair.jpg_path);
        }
    }

    #[test]
    fn a_raw_only_pick_with_its_saved_preview_is_one_entry() {
        // Archive saves a RAW-only pick's preview next to it as <stem>.jpg
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        write(
            dir.path(),
            "精选/DSC_1.ARW",
            &tiff_raw_with_preview(&preview, None),
        );
        write(dir.path(), "精选/DSC_1.jpg", &preview);
        let result = scan(dir.path());

        assert_eq!(result.pairs.len(), 1);
        assert_eq!(result.paired_count, 1);
        assert_eq!(result.raw_only_count, 0);
        let pick = &result.pairs[0];
        assert_eq!(pick.source, PhotoSource::Jpg);
        assert!(pick.is_archived_pick);
    }

    #[test]
    fn extracts_a_preview_for_a_raw_only_pick() {
        let dir = tempdir().unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        write(dir.path(), "精选/DSC_1.ARW", &raw);
        write(dir.path(), "精选/DSC_2.ARW", b"no preview");
        let result = scan(dir.path());

        assert_eq!(result.raw_only_count, 1);
        let pick = &result.pairs[0];
        assert_eq!(pick.source, PhotoSource::RawPreview);
        assert_eq!(pick.status, PhotoStatus::Starred);
        assert!(pick.is_archived_pick);
        assert_eq!(
            result.skipped.len(),
            1,
            "a pick without a preview is reported"
        );
        assert_eq!(result.skipped[0].reason, SkipReason::NoPreview);
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_to_photo_files_but_not_to_folders() {
        use std::os::unix::fs::symlink;

        let dir = folder(&["IMG_1.JPG"]);
        let elsewhere = folder(&["IMG_2.JPG", "IMG_3.JPG/IMG_4.JPG"]);
        symlink(
            elsewhere.path().join("IMG_2.JPG"),
            dir.path().join("IMG_2.JPG"),
        )
        .unwrap();
        symlink(
            elsewhere.path().join("IMG_3.JPG"),
            dir.path().join("IMG_3.JPG"),
        )
        .unwrap();
        symlink(dir.path().join("missing.JPG"), dir.path().join("IMG_5.JPG")).unwrap();

        assert_eq!(names(&scan(dir.path())), ["IMG_1.JPG", "IMG_2.JPG"]);
    }

    #[cfg(unix)]
    #[test]
    fn reports_a_subfolder_that_cannot_be_read() {
        use std::fs::{set_permissions, Permissions};
        use std::os::unix::fs::PermissionsExt;

        let dir = folder(&["IMG_1.JPG", "RAW/IMG_1.CR3"]);
        let raw_dir = dir.path().join(RAW_DIR);
        set_permissions(&raw_dir, Permissions::from_mode(0o000)).unwrap();

        let result = scan(dir.path());
        set_permissions(&raw_dir, Permissions::from_mode(0o755)).unwrap();

        assert_eq!(names(&result), ["IMG_1.JPG"]);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].reason, SkipReason::Unreadable);
        assert_eq!(Path::new(&result.skipped[0].path), raw_dir);
    }

    #[test]
    fn reports_empty_photo_files_as_skipped_instead_of_pairing_them() {
        let dir = folder(&["IMG_1.JPG", "IMG_1.CR3"]);
        // An empty RAW/IMG_1.CR3 left by an interrupted move must not take the
        // real RAW's place next to the JPG
        write(dir.path(), "RAW/IMG_1.CR3", b"");
        write(dir.path(), "IMG_2.JPG", b"");
        write(dir.path(), "notes.txt", b"");

        let result = scan(dir.path());

        assert_eq!(names(&result), ["IMG_1.JPG"]);
        assert_eq!(result.paired_count, 1);
        assert_eq!(result.raw_only_count, 0);
        let mut skipped: Vec<_> = result
            .skipped
            .iter()
            .map(|f| (file_name(&f.path), f.reason))
            .collect();
        skipped.sort_by_key(|(name, _)| *name);
        assert_eq!(
            skipped,
            [
                ("IMG_1.CR3", SkipReason::Empty),
                ("IMG_2.JPG", SkipReason::Empty)
            ]
        );
        assert_eq!(result.total_files, 4, "empty photos are still counted");
    }

    fn exif_taken_at(datetime: &str) -> Vec<u8> {
        exif_tiff(&[(0x9003, datetime)], ExifByteOrder::Little)
    }

    fn local_ms(pair: &PhotoPair) -> Option<i64> {
        pair.capture_time.map(|t| t.local_ms)
    }

    #[test]
    fn records_capture_times_but_keeps_file_name_order() {
        let dir = tempdir().unwrap();
        let jpeg = noise_jpeg(16, 16);
        write(
            dir.path(),
            "DSC_0001.JPG",
            &jpeg_with_exif(&jpeg, &exif_taken_at("2026:09:15 10:00:05")),
        );
        write(
            dir.path(),
            "DSC_0002.JPG",
            &jpeg_with_exif(&jpeg, &exif_taken_at("2026:09:15 10:00:01")),
        );
        write(dir.path(), "DSC_0003.JPG", &jpeg);

        let result = scan(dir.path());

        assert_eq!(
            names(&result),
            ["DSC_0001.JPG", "DSC_0002.JPG", "DSC_0003.JPG"]
        );
        let times: Vec<_> = result.pairs.iter().map(local_ms).collect();
        assert!(times[0] > times[1]);
        assert_eq!(times[2], None);
    }

    #[test]
    fn reads_the_raw_when_its_jpg_has_no_capture_time() {
        let dir = tempdir().unwrap();
        write(dir.path(), "DSC_0001.JPG", &noise_jpeg(16, 16));
        write(
            dir.path(),
            "DSC_0001.ARW",
            &exif_taken_at("2026:09:15 10:00:00"),
        );

        let result = scan(dir.path());

        assert_eq!(result.paired_count, 1);
        assert!(local_ms(&result.pairs[0]).is_some());
    }

    #[test]
    fn reads_a_raw_only_photo_from_the_raw_not_its_cached_preview() {
        let dir = tempdir().unwrap();
        let preview = jpeg_with_exif(&noise_jpeg(256, 256), &exif_taken_at("2026:09:15 10:00:00"));
        write(dir.path(), "DSCF0001.RAF", &raf_with_jpeg(&preview));

        let result = scan(dir.path());

        assert_eq!(result.raw_only_count, 1);
        assert!(local_ms(&result.pairs[0]).is_some());
    }

    /// Times scanning a real or generated folder:
    /// `SIFT_SCAN_BENCH_DIR=~/photos cargo test --release scan_benchmark -- --ignored --nocapture`
    #[test]
    #[ignore = "needs SIFT_SCAN_BENCH_DIR"]
    fn scan_benchmark() {
        let dir = std::env::var("SIFT_SCAN_BENCH_DIR").expect("set SIFT_SCAN_BENCH_DIR");
        let cache = tempdir().unwrap();
        for run in 1..=5 {
            let started = std::time::Instant::now();
            let result = scan_folder(Path::new(&dir), cache.path()).unwrap();
            let timed = result
                .pairs
                .iter()
                .filter(|p| p.capture_time.is_some())
                .count();
            println!(
                "run {run}: {} pairs ({timed} with capture time) in {:?}",
                result.pairs.len(),
                started.elapsed()
            );
        }
    }

    #[test]
    fn errors_for_missing_folder() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert!(scan_folder(&missing, dir.path()).is_err());
    }
}
