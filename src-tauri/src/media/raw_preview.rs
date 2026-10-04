// ============================================================
// Sift - RAW Preview Extraction
// Extracts embedded JPEG previews from RAW files into a cache dir
// Strategy: EXIF offsets -> JPEG SOI/EOI scan (fallback)
// ============================================================

use crate::media::exif::read_orientation;
use crate::media::jpeg::exif_orientation;
use crate::media::jpeg::scan::find_largest_jpeg;
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Minimum JPEG size to consider as a preview (50KB)
/// This filters out small EXIF thumbnails
const MIN_PREVIEW_SIZE: usize = 50 * 1024;

/// Largest embedded preview we will read. Real full-size previews are a few
/// MB; a bigger declared length means a corrupt or hostile file, and reading
/// it could exhaust memory.
const MAX_PREVIEW_BYTES: usize = 64 * 1024 * 1024;

/// Generate a cache filename for a RAW file based on its path
/// Uses the file stem + a hash of the full path to avoid collisions
fn cache_filename(raw_path: &Path) -> String {
    let stem = raw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let path_str = raw_path.to_string_lossy();
    let hash = simple_hash(path_str.as_bytes());
    format!("{}_{:016x}_preview.jpg", stem, hash)
}

/// Simple FNV-1a hash for path deduplication
fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Extract an embedded JPEG preview from a RAW file.
/// Returns the path to the cached preview JPEG (inside `cache_dir`) on success.
///
/// Strategy:
/// 1. Try EXIF-based extraction (fast, ~5ms)
/// 2. Fall back to JPEG SOI/EOI binary scan (~50-200ms)
///
/// Orientation handling: some RAW formats (notably Sony ARW) store the
/// embedded preview JPEG with its pixels in sensor-native orientation
/// (always landscape) and keep the "this should be rotated" hint only in
/// the outer TIFF IFD0. To make the preview display correctly in a plain
/// `<img>`, we sync that Orientation into the preview JPEG's own APP1/EXIF
/// before caching.
pub fn extract_raw_preview(raw_path: &str, cache_dir: &Path) -> Result<String, String> {
    let path = Path::new(raw_path);
    if !path.exists() {
        return Err(format!("RAW file not found: {}", raw_path));
    }

    let cache_path = cache_dir.join(cache_filename(path));

    // Orientation stored in the outer RAW's TIFF IFD0 (1 = normal, >1 = needs rotate/flip).
    // Failing to read it is non-fatal; we just treat it as "unknown / normal".
    let outer_orientation = read_orientation(raw_path).unwrap_or(1);

    if is_cache_fresh(&cache_path, outer_orientation) {
        return Ok(cache_path.to_string_lossy().to_string());
    }

    let preview = extract_via_exif(raw_path)
        .ok()
        .filter(|jpeg| jpeg.len() >= MIN_PREVIEW_SIZE)
        .or_else(|| extract_via_jpeg_scan(raw_path).ok())
        .ok_or_else(|| format!("Could not extract preview from RAW file: {}", raw_path))?;

    let final_data = exif_orientation::ensure_orientation(preview, outer_orientation);
    fs::write(&cache_path, &final_data)
        .map_err(|e| format!("Failed to write preview cache: {}", e))?;
    Ok(cache_path.to_string_lossy().to_string())
}

/// A cached preview is reusable unless it lacks the RAW's outer orientation
/// (common on Sony ARW previews cached before 1.0.2).
fn is_cache_fresh(cache_path: &Path, outer_orientation: u16) -> bool {
    if !cache_path.exists() {
        return false;
    }
    if outer_orientation <= 1 {
        return true;
    }
    fs::read(cache_path)
        .map(|existing| {
            exif_orientation::read_orientation(&existing).unwrap_or(1) == outer_orientation
        })
        .unwrap_or(false)
}

/// Strategy 1: Extract JPEG preview via EXIF data
/// Works for TIFF-based RAW formats: NEF, CR2, ARW, DNG, ORF, PEF, RW2, etc.
/// Checks both PRIMARY and THUMBNAIL IFDs, returns the largest valid JPEG.
fn extract_via_exif(raw_path: &str) -> Result<Vec<u8>, String> {
    let file = File::open(raw_path).map_err(|e| format!("Failed to open RAW for EXIF: {}", e))?;
    let mut reader = BufReader::new(file);

    let exif_reader = exif::Reader::new();
    let exif = exif_reader
        .read_from_container(&mut reader)
        .map_err(|e| format!("Failed to read EXIF from RAW: {}", e))?;

    // Scan IFD0~IFD9: different camera brands store full-size JPEG previews in different IFDs
    // (e.g. Nikon NEF uses IFD#2/3, Canon CR2 uses IFD#3). Extra non-existent IFDs simply return None.
    let ifds: Vec<exif::In> = (0..10).map(exif::In).collect();
    let mut best_jpeg: Option<Vec<u8>> = None;
    let mut best_size: usize = 0;

    for &ifd in &ifds {
        let offset = exif
            .get_field(exif::Tag::JPEGInterchangeFormat, ifd)
            .and_then(|f| match &f.value {
                exif::Value::Long(v) => v.first().map(|&x| x as u64),
                _ => None,
            });

        let length = exif
            .get_field(exif::Tag::JPEGInterchangeFormatLength, ifd)
            .and_then(|f| match &f.value {
                exif::Value::Long(v) => v.first().map(|&x| x as usize),
                _ => None,
            });

        if let (Some(offset), Some(length)) = (offset, length) {
            if length < 100 || length <= best_size {
                continue;
            }

            // Read the JPEG data from the RAW file at the specified offset
            if let Ok(jpeg_data) = read_jpeg_at_offset(raw_path, offset, length) {
                if jpeg_data.len() > best_size {
                    best_size = jpeg_data.len();
                    best_jpeg = Some(jpeg_data);
                }
            }
        }
    }

    best_jpeg.ok_or_else(|| "No JPEG preview found in EXIF".to_string())
}

/// Read and validate JPEG data at a specific offset in a file
fn read_jpeg_at_offset(path: &str, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("Failed to read file size: {}", e))?
        .len();
    let length = preview_read_len(offset, length, file_len)?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Failed to seek to JPEG offset: {}", e))?;

    let mut jpeg_data = vec![0u8; length];
    file.read_exact(&mut jpeg_data)
        .map_err(|e| format!("Failed to read JPEG data: {}", e))?;

    // Verify JPEG signature (FFD8)
    if jpeg_data.len() >= 2 && jpeg_data[0] == 0xFF && jpeg_data[1] == 0xD8 {
        Ok(jpeg_data)
    } else {
        Err("Data at offset is not a valid JPEG".to_string())
    }
}

/// How many bytes to read for a preview that EXIF says is `length` bytes at
/// `offset`. Both come from the file itself, so they are untrusted: the read
/// stops at the end of the file and never exceeds `MAX_PREVIEW_BYTES`.
fn preview_read_len(offset: u64, length: usize, file_len: u64) -> Result<usize, String> {
    let available = file_len
        .checked_sub(offset)
        .filter(|&n| n > 0)
        .ok_or_else(|| format!("JPEG offset {} is past the end of the file", offset))?;
    let length = usize::try_from(available).map_or(length, |n| length.min(n));
    if length > MAX_PREVIEW_BYTES {
        return Err(format!("Embedded JPEG too large: {} bytes", length));
    }
    Ok(length)
}

/// Strategy 2: Binary scan for the largest embedded JPEG
/// Works for non-TIFF RAW formats: CR3, RAF, etc.
fn extract_via_jpeg_scan(raw_path: &str) -> Result<Vec<u8>, String> {
    let data = fs::read(raw_path).map_err(|e| format!("Failed to read RAW file: {}", e))?;
    find_largest_jpeg(&data, MIN_PREVIEW_SIZE)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| "No embedded JPEG found in RAW file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        exif_orientation, noise_jpeg, opaque_raw_with_preview, tiff_raw_declaring_preview_len,
        tiff_raw_with_preview, write,
    };
    use tempfile::tempdir;

    fn extract(raw: &Path, cache: &Path) -> Result<Vec<u8>, String> {
        let out = extract_raw_preview(raw.to_str().unwrap(), cache)?;
        Ok(fs::read(out).unwrap())
    }

    #[test]
    fn fixture_preview_exceeds_minimum_size() {
        assert!(noise_jpeg(256, 256).len() >= MIN_PREVIEW_SIZE);
    }

    #[test]
    fn extracts_preview_from_tiff_based_raw() {
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = write(dir.path(), "a.NEF", &tiff_raw_with_preview(&preview, None));

        assert_eq!(extract(&raw, dir.path()).unwrap(), preview);
    }

    #[test]
    fn syncs_outer_raw_orientation_into_preview() {
        // Regression (1.0.2): Sony ARW previews rendered sideways because the
        // rotation lives only in the outer IFD0, not in the preview JPEG.
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = write(
            dir.path(),
            "a.ARW",
            &tiff_raw_with_preview(&preview, Some(6)),
        );

        let out = extract(&raw, dir.path()).unwrap();

        assert_eq!(exif_orientation(&out), Some(6));
        assert!(image::load_from_memory(&out).is_ok());
    }

    #[test]
    fn falls_back_to_binary_scan_for_non_tiff_containers() {
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = write(dir.path(), "a.CR3", &opaque_raw_with_preview(&preview));

        assert_eq!(extract(&raw, dir.path()).unwrap(), preview);
    }

    #[test]
    fn rejects_previews_below_minimum_size() {
        let dir = tempdir().unwrap();
        let tiny = noise_jpeg(16, 16);
        let raw = write(dir.path(), "a.NEF", &tiff_raw_with_preview(&tiny, None));

        assert!(extract(&raw, dir.path()).is_err());
    }

    #[test]
    fn clamps_the_declared_preview_length_to_the_end_of_the_file() {
        assert_eq!(preview_read_len(100, 1_000, 600), Ok(500));
        assert_eq!(preview_read_len(100, 200, 600), Ok(200));
        assert!(preview_read_len(600, 200, 600).is_err());
        assert!(preview_read_len(700, 200, 600).is_err());
    }

    #[test]
    fn rejects_previews_larger_than_the_cap() {
        let huge_file = 10 * MAX_PREVIEW_BYTES as u64;
        assert!(preview_read_len(0, MAX_PREVIEW_BYTES + 1, huge_file).is_err());
        assert!(preview_read_len(0, u32::MAX as usize, huge_file).is_err());
        assert_eq!(
            preview_read_len(0, MAX_PREVIEW_BYTES, huge_file),
            Ok(MAX_PREVIEW_BYTES)
        );
    }

    #[test]
    fn errors_instead_of_allocating_for_a_bogus_preview_length() {
        // EXIF claims a 4GB preview; only the bytes actually in the file are read
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = tiff_raw_declaring_preview_len(&preview[..1024], u32::MAX);
        let raw = write(dir.path(), "a.NEF", &raw);

        assert!(extract(&raw, dir.path()).is_err());
    }

    #[test]
    fn errors_when_raw_is_missing() {
        let dir = tempdir().unwrap();
        assert!(extract(&dir.path().join("missing.ARW"), dir.path()).is_err());
    }

    #[test]
    fn refreshes_cached_preview_missing_orientation() {
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = write(dir.path(), "a.ARW", &tiff_raw_with_preview(&preview, None));
        let first = extract(&raw, dir.path()).unwrap();
        assert_eq!(exif_orientation(&first), None);

        // Same path, now carrying an outer orientation: the stale cache must be replaced
        write(
            dir.path(),
            "a.ARW",
            &tiff_raw_with_preview(&preview, Some(8)),
        );
        let second = extract(&raw, dir.path()).unwrap();

        assert_eq!(exif_orientation(&second), Some(8));
    }

    /// Opt-in check against real camera files, which are too large for the repo:
    ///   SIFT_RAW_SAMPLES=/path/to/raws cargo test real_raw -- --ignored --nocapture
    #[test]
    #[ignore = "needs SIFT_RAW_SAMPLES pointing at a folder of real RAW files"]
    fn real_raw_samples_yield_upright_previews_and_capture_times() {
        let samples = std::env::var("SIFT_RAW_SAMPLES").expect("set SIFT_RAW_SAMPLES");
        let cache = tempdir().unwrap();
        let mut checked = 0;
        let mut failures = Vec::new();

        for entry in fs::read_dir(&samples).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            let is_raw_file = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(crate::catalog::file_types::is_raw);
            if !is_raw_file {
                continue;
            }
            checked += 1;
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            match check_real_sample(&path, cache.path()) {
                Ok(summary) => println!("ok   {name}: {summary}"),
                Err(e) => failures.push(format!("{name}: {e}")),
            }
        }

        assert!(checked > 0, "no RAW files found in {samples}");
        assert!(
            failures.is_empty(),
            "{} of {checked} failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    fn check_real_sample(raw: &Path, cache: &Path) -> Result<String, String> {
        let raw_str = raw.to_str().unwrap();
        let bytes = fs::read(extract_raw_preview(raw_str, cache)?).unwrap();
        let img = image::load_from_memory(&bytes).map_err(|e| format!("undecodable: {e}"))?;
        let outer = read_orientation(raw_str).unwrap_or(1);
        let embedded = exif_orientation::read_orientation(&bytes).unwrap_or(1);
        if outer > 1 && embedded != outer {
            return Err(format!("orientation {embedded}, RAW says {outer}"));
        }
        let taken = crate::media::capture_time::read_capture_time(raw).ok_or("no capture time")?;
        Ok(format!(
            "{}x{}, {} KB, orientation {embedded}, taken at {} ms",
            img.width(),
            img.height(),
            bytes.len() / 1024,
            taken.local_ms
        ))
    }
}
