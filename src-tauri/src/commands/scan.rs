// ============================================================
// Sift - Scan Command
// Smart scan: detects folder structure and pairs JPG with RAW
// Supports: loose files, archived JPG/RAW subdirectories, or both
// Now supports RAW-only files with embedded JPEG preview extraction
// ============================================================

use crate::catalog::file_types::{is_jpg, is_raw, raw_format_name};
use crate::media::raw_preview::{extract_raw_preview, preview_cache_dir};
use crate::models::photo::{PhotoPair, PhotoSource, PhotoStatus, ScanResult};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

#[tauri::command]
pub async fn scan_folder(folder_path: String) -> Result<ScanResult, String> {
    tokio::task::spawn_blocking(move || scan_folder_sync(&folder_path, &preview_cache_dir()?))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

/// Collect JPG, RAW, and XMP files from a single directory (non-recursive)
fn collect_files(
    dir: &Path,
    jpg_map: &mut HashMap<String, String>,
    raw_map: &mut HashMap<String, (String, String)>,
    xmp_map: &mut HashMap<String, Vec<String>>,
) -> usize {
    let mut count: usize = 0;
    for entry in WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }

        if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            let full_path = entry_path.to_string_lossy().to_string();

            if ext_lower == "xmp" {
                // XMP sidecar: try to match by stem
                // e.g. "IMG_001.CR3.xmp" -> stem is "IMG_001.CR3", base stem is "IMG_001"
                // e.g. "IMG_001.xmp" -> stem is "IMG_001"
                let file_stem = entry_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                // Get the base stem (without secondary extension)
                let base_stem = Path::new(&file_stem)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&file_stem)
                    .to_string();

                // Store under both the full stem and base stem for matching
                xmp_map
                    .entry(base_stem.clone())
                    .or_default()
                    .push(full_path.clone());
                if file_stem != base_stem {
                    xmp_map.entry(file_stem).or_default().push(full_path);
                }
                continue;
            }

            let stem = entry_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if is_jpg(&ext_lower) {
                count += 1;
                jpg_map.insert(stem, full_path);
            } else if is_raw(&ext_lower) {
                count += 1;
                raw_map.insert(stem, (full_path, ext_lower));
            }
        }
    }
    count
}

fn scan_folder_sync(folder_path: &str, preview_cache_dir: &Path) -> Result<ScanResult, String> {
    let path = Path::new(folder_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Folder does not exist: {}", folder_path));
    }

    let mut jpg_map: HashMap<String, String> = HashMap::new();
    let mut raw_map: HashMap<String, (String, String)> = HashMap::new();
    let mut xmp_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut total_files: usize = 0;

    // Phase 1: Scan root directory for loose JPG/RAW files
    total_files += collect_files(path, &mut jpg_map, &mut raw_map, &mut xmp_map);

    // Phase 2: If root has no JPG files, check for archived JPG/ + RAW/ subdirectories
    if jpg_map.is_empty() {
        let jpg_dir = path.join("JPG");
        let raw_dir = path.join("RAW");

        if jpg_dir.is_dir() {
            total_files += collect_files(&jpg_dir, &mut jpg_map, &mut raw_map, &mut xmp_map);
        }
        if raw_dir.is_dir() {
            total_files += collect_files(&raw_dir, &mut jpg_map, &mut raw_map, &mut xmp_map);
        }
    }

    // === Three-way pairing ===
    let mut pairs: Vec<PhotoPair> = Vec::new();
    let mut paired_count: usize = 0;
    let mut jpg_only_count: usize = 0;
    let mut raw_only_count: usize = 0;

    // Track which RAW stems have been paired
    let mut paired_raw_stems: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Route 1 & 2: JPG+RAW paired / JPG-only
    for (stem, jpg_path) in &jpg_map {
        let (raw_path, raw_format) = if let Some((rp, ext)) = raw_map.get(stem) {
            paired_count += 1;
            paired_raw_stems.insert(stem.clone());
            (Some(rp.clone()), Some(raw_format_name(ext)))
        } else {
            jpg_only_count += 1;
            (None, None)
        };

        // Collect XMP files associated with this stem (dedup)
        let xmp_paths = collect_xmp_for_stem(stem, &xmp_map);

        pairs.push(PhotoPair {
            id: uuid::Uuid::new_v4().to_string(),
            jpg_path: jpg_path.clone(),
            raw_path,
            raw_format,
            status: PhotoStatus::Unprocessed,
            thumbnail_path: None,
            dominant_color: None,
            source: PhotoSource::Jpg,
            xmp_paths,
        });
    }

    // Route 3: RAW-only (no matching JPG)
    let unpaired_raws: Vec<(String, String, String)> = raw_map
        .iter()
        .filter(|(stem, _)| !paired_raw_stems.contains(*stem))
        .map(|(stem, (rp, ext))| (stem.clone(), rp.clone(), ext.clone()))
        .collect();

    if !unpaired_raws.is_empty() {
        // Use a limited thread pool (2 threads) to avoid memory spike
        // Each RAW file is 25-60MB; unlimited parallelism can exhaust memory
        let pool = ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .map_err(|e| format!("Failed to build thread pool: {}", e))?;

        let raw_only_pairs: Vec<PhotoPair> = pool.install(|| {
            unpaired_raws
                .par_iter()
                .filter_map(|(stem, raw_path, ext)| {
                    match extract_raw_preview(raw_path, preview_cache_dir) {
                        Ok(preview_path) => {
                            let xmp_paths = collect_xmp_for_stem(stem, &xmp_map);
                            Some(PhotoPair {
                                id: uuid::Uuid::new_v4().to_string(),
                                jpg_path: preview_path,
                                raw_path: Some(raw_path.clone()),
                                raw_format: Some(raw_format_name(ext)),
                                status: PhotoStatus::Unprocessed,
                                thumbnail_path: None,
                                dominant_color: None,
                                source: PhotoSource::RawPreview,
                                xmp_paths,
                            })
                        }
                        Err(e) => {
                            eprintln!("[Sift] Failed to extract preview from {}: {}", raw_path, e);
                            None
                        }
                    }
                })
                .collect()
        });

        raw_only_count = raw_only_pairs.len();
        pairs.extend(raw_only_pairs);
    }

    // Natural sort by filename
    pairs.sort_by(|a, b| {
        let name_a = Path::new(&a.jpg_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let name_b = Path::new(&b.jpg_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        natord::compare(name_a, name_b)
    });

    Ok(ScanResult {
        pairs,
        total_files,
        paired_count,
        jpg_only_count,
        raw_only_count,
    })
}

/// Collect and deduplicate XMP sidecar paths for a given file stem
fn collect_xmp_for_stem(stem: &str, xmp_map: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut xmp_paths: Vec<String> = Vec::new();
    if let Some(paths) = xmp_map.get(stem) {
        for p in paths {
            if !xmp_paths.contains(p) {
                xmp_paths.push(p.clone());
            }
        }
    }
    xmp_paths
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
        scan_folder_sync(dir.to_str().unwrap(), &cache).unwrap()
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
    fn pairs_jpg_with_raw_of_same_stem() {
        let dir = folder(&["IMG_1.JPG", "IMG_1.CR3"]);
        let result = scan(dir.path());

        assert_eq!(result.paired_count, 1);
        let pair = &result.pairs[0];
        assert_eq!(file_name(pair.raw_path.as_deref().unwrap()), "IMG_1.CR3");
        assert_eq!(pair.raw_format.as_deref(), Some("CR3"));
        assert_eq!(pair.source, PhotoSource::Jpg);
    }

    #[test]
    fn counts_jpg_only_and_ignores_unrelated_files() {
        let dir = folder(&["IMG_1.jpg", "notes.txt", "IMG_1.png"]);
        let result = scan(dir.path());

        assert_eq!(result.total_files, 1);
        assert_eq!(result.jpg_only_count, 1);
        assert!(result.pairs[0].raw_path.is_none());
    }

    #[test]
    fn attaches_both_xmp_naming_styles_without_duplicates() {
        let dir = folder(&["IMG_1.JPG", "IMG_1.CR3", "IMG_1.xmp", "IMG_1.CR3.xmp"]);
        let result = scan(dir.path());

        let mut xmps: Vec<&str> = result.pairs[0]
            .xmp_paths
            .iter()
            .map(|p| file_name(p))
            .collect();
        xmps.sort();
        assert_eq!(xmps, ["IMG_1.CR3.xmp", "IMG_1.xmp"]);
        assert_eq!(
            result.total_files, 2,
            "XMP sidecars are not counted as photos"
        );
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
    fn stem_matching_is_case_sensitive() {
        // Current behaviour: differently-cased stems do not pair.
        let dir = folder(&["img_1.jpg", "IMG_1.CR3"]);
        let result = scan(dir.path());

        assert_eq!(result.paired_count, 0);
        assert_eq!(result.jpg_only_count, 1);
    }

    #[test]
    fn keeps_only_one_file_per_stem_and_kind() {
        // Current behaviour: a second JPG or RAW with the same stem is dropped
        // from pairing (which one survives depends on directory order).
        let dir = folder(&["IMG_1.jpg", "IMG_1.jpeg", "IMG_1.CR3", "IMG_1.DNG"]);
        let result = scan(dir.path());

        assert_eq!(result.total_files, 4);
        assert_eq!(result.pairs.len(), 1);
    }

    #[test]
    fn errors_for_missing_folder() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert!(scan_folder_sync(missing.to_str().unwrap(), dir.path()).is_err());
    }
}
