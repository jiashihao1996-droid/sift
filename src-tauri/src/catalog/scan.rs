// ============================================================
// Sift - Folder Scan
// Smart scan: detects folder structure and pairs JPG with RAW
// Supports: loose files, archived JPG/RAW subdirectories, or both
// RAW-only files get their embedded JPEG preview extracted
// ============================================================

use crate::catalog::pairing::{self, FileIndex, RawOnly};
use crate::media::raw_preview::extract_raw_preview;
use crate::models::photo::{PhotoPair, PhotoSource, ScanResult};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::path::Path;
use walkdir::WalkDir;

/// RAW files are 25-60MB each; unbounded parallel preview extraction can exhaust memory
const PREVIEW_EXTRACTION_THREADS: usize = 2;

/// Archive layout produced by Sift itself (see commands/archive.rs)
const ARCHIVED_SUBDIRS: [&str; 2] = ["JPG", "RAW"];

pub fn scan_folder(folder: &Path, preview_cache_dir: &Path) -> Result<ScanResult, String> {
    if !folder.is_dir() {
        return Err(format!("Folder does not exist: {}", folder.display()));
    }

    let mut index = FileIndex::default();
    index_dir(folder, &mut index);

    // Root has no JPGs: maybe it was already archived into JPG/ + RAW/
    if !index.has_jpgs() {
        for sub in ARCHIVED_SUBDIRS {
            let dir = folder.join(sub);
            if dir.is_dir() {
                index_dir(&dir, &mut index);
            }
        }
    }

    let pairing = pairing::pair(&index);
    let raw_only_pairs = extract_previews(&pairing.raw_only, &index, preview_cache_dir)?;

    let raw_only_count = raw_only_pairs.len();
    let mut pairs = pairing.pairs;
    pairs.extend(raw_only_pairs);
    pairing::sort_naturally(&mut pairs);

    Ok(ScanResult {
        pairs,
        total_files: index.photo_count(),
        paired_count: pairing.paired_count,
        jpg_only_count: pairing.jpg_only_count,
        raw_only_count,
    })
}

/// Add the files directly inside `dir` (non-recursive) to the index
fn index_dir(dir: &Path, index: &mut FileIndex) {
    WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .for_each(|e| index.add(e.path()));
}

/// Extract embedded previews for RAW-only files. RAWs whose preview can't be
/// extracted are logged and left out of the result.
fn extract_previews(
    raw_only: &[RawOnly],
    index: &FileIndex,
    cache_dir: &Path,
) -> Result<Vec<PhotoPair>, String> {
    if raw_only.is_empty() {
        return Ok(Vec::new());
    }

    let pool = ThreadPoolBuilder::new()
        .num_threads(PREVIEW_EXTRACTION_THREADS)
        .build()
        .map_err(|e| format!("Failed to build thread pool: {}", e))?;

    Ok(pool.install(|| {
        raw_only
            .par_iter()
            .filter_map(|raw| match extract_raw_preview(&raw.raw_path, cache_dir) {
                Ok(preview_path) => Some(pairing::new_pair(
                    preview_path,
                    Some((raw.raw_path.clone(), raw.ext.as_str())),
                    PhotoSource::RawPreview,
                    index.xmps_for(&raw.stem),
                )),
                Err(e) => {
                    eprintln!(
                        "[Sift] Failed to extract preview from {}: {}",
                        raw.raw_path, e
                    );
                    None
                }
            })
            .collect()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{noise_jpeg, tiff_raw_with_preview, write};
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
    fn drops_raw_only_files_without_extractable_preview() {
        // Current behaviour: such RAWs are silently left out of the session.
        let dir = folder(&["DSC_1.ARW"]);
        let result = scan(dir.path());

        assert_eq!(result.total_files, 1);
        assert!(result.pairs.is_empty());
    }

    #[test]
    fn reads_archived_jpg_and_raw_subfolders_when_root_has_no_jpg() {
        let dir = folder(&["JPG/IMG_1.JPG", "RAW/IMG_1.CR3", "RAW/IMG_1.xmp"]);
        let result = scan(dir.path());

        assert_eq!(result.paired_count, 1);
        assert_eq!(result.pairs[0].xmp_paths.len(), 1);
    }

    #[test]
    fn ignores_archived_subfolders_when_root_has_jpg() {
        let dir = folder(&["IMG_1.JPG", "JPG/IMG_2.JPG"]);
        assert_eq!(names(&scan(dir.path())), ["IMG_1.JPG"]);
    }

    #[test]
    fn errors_for_missing_folder() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert!(scan_folder(&missing, dir.path()).is_err());
    }
}
