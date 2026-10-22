// ============================================================
// Sift - RAW Preview Extraction
// Extracts embedded JPEG previews from RAW files into a cache dir
// Strategy: the JPEGs the TIFF IFDs point to -> a JPEG SOI/EOI scan of the
// regions the format says hold its JPEGs (CR3, RAF, NEF; located/) -> the
// same scan of the whole file (fallback)
// A cached preview is found without opening the RAW: its name comes from
// the RAW's path, size and modification time. Extracting reads the RAW's
// head and the preview; only the fallback scan reads the whole file.
// ============================================================

mod located;
mod tiff_layout;

use crate::media::cache_lru;
use crate::media::jpeg::exif_orientation;
use crate::media::jpeg::scan::find_largest_jpeg;
use crate::media::metadata::head::HeadCached;
use crate::paths::is_inside_dir;
use std::cmp::Reverse;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

/// Minimum JPEG size to consider as a preview (50KB)
/// This filters out small EXIF thumbnails
const MIN_PREVIEW_SIZE: usize = 50 * 1024;

/// Largest embedded preview we will read. Real full-size previews are a few
/// MB; a bigger declared length means a corrupt or hostile file, and reading
/// it could exhaust memory.
const MAX_PREVIEW_BYTES: usize = 64 * 1024 * 1024;

/// Covers the IFD tables of TIFF-based RAWs in practice; anything further
/// is still read, just with extra reads
const HEAD_LEN: usize = 64 * 1024;

/// Cache filename for a RAW: its stem plus a hash of its path, size and
/// modification time, so a changed or replaced RAW gets a fresh preview
fn cache_filename(raw_path: &Path, meta: &fs::Metadata) -> String {
    let stem = raw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let modified_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let mut key = raw_path.to_string_lossy().into_owned().into_bytes();
    key.extend(meta.len().to_le_bytes());
    key.extend(modified_ns.to_le_bytes());
    format!("{}_{:016x}_preview.jpg", stem, simple_hash(&key))
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
/// 1. The JPEGs the TIFF IFD chain points to (TIFF-based RAWs)
/// 2. JPEG SOI/EOI binary scan of the regions the format points to (CR3,
///    RAF, NEF)
/// 3. Fall back to the same scan of the whole file
///
/// Orientation handling: some RAW formats (notably Sony ARW) store the
/// embedded preview JPEG with its pixels in sensor-native orientation
/// (always landscape) and keep the "this should be rotated" hint only in
/// the outer TIFF IFD0. To make the preview display correctly in a plain
/// `<img>`, we sync that Orientation into the preview JPEG's own APP1/EXIF
/// before caching.
pub fn extract_raw_preview(raw_path: &str, cache_dir: &Path) -> Result<String, String> {
    let path = Path::new(raw_path);
    let meta =
        fs::metadata(path).map_err(|e| format!("RAW file not found: {}: {}", raw_path, e))?;
    let cache_path = cache_dir.join(cache_filename(path, &meta));
    match fs::metadata(&cache_path) {
        Ok(cached) if cached.is_file() => cache_lru::mark_used(&cache_path, &cached),
        _ => write_cache(&cache_path, cache_dir, &raw_preview_bytes(raw_path)?)?,
    }
    Ok(cache_path.to_string_lossy().to_string())
}

/// The bytes `extract_raw_preview` caches for `raw_path`, read fresh without
/// touching the cache. Archive saves exactly these next to a RAW-only pick,
/// so they identify that saved preview later.
pub fn raw_preview_bytes(raw_path: &str) -> Result<Vec<u8>, String> {
    preview_bytes(raw_path, Search::Located)
}

/// The preview versions before the located search cached for `raw_path`,
/// when it may differ from `raw_preview_bytes` (the format points to its
/// JPEGs, so the whole file was scanned); None otherwise. Reads the whole
/// RAW.
pub fn earlier_preview_bytes(raw_path: &str) -> Option<Vec<u8>> {
    preview_bytes(raw_path, Search::Earlier).ok()
}

/// `Search::Earlier` finds what `Search::Located` does
const SAME_AS_LOCATED: &str = "Earlier versions extracted the same preview";

#[derive(Clone, Copy, PartialEq)]
enum Search {
    /// IFD chain, then the regions the format points to, then the whole file
    Located,
    /// IFD chain, then the whole file, only when `Located` would have
    /// looked at regions instead
    Earlier,
}

fn preview_bytes(raw_path: &str, search: Search) -> Result<Vec<u8>, String> {
    let file = File::open(raw_path).map_err(|e| format!("Failed to open RAW: {}", e))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("Failed to read RAW size: {}", e))?
        .len();
    let mut file =
        HeadCached::new(file, HEAD_LEN).map_err(|e| format!("Failed to read RAW: {}", e))?;
    let layout = tiff_layout::read_layout(&mut file, file_len);
    // Failing to read the orientation is non-fatal; it is treated as normal
    let outer_orientation = layout.as_ref().and_then(|l| l.orientation).unwrap_or(1);
    let from_chain = layout
        .and_then(|l| largest_jpeg(&mut file, &l.jpegs, file_len))
        .filter(|jpeg| jpeg.len() >= MIN_PREVIEW_SIZE);
    let preview = match (from_chain, search) {
        (Some(jpeg), Search::Located) => jpeg,
        (Some(_), Search::Earlier) => return Err(SAME_AS_LOCATED.to_string()),
        (None, _) => preview_outside_ifds(&mut file, raw_path, file_len, search)?,
    };
    Ok(exif_orientation::ensure_orientation(
        preview,
        outer_orientation,
    ))
}

/// A RAW whose IFD chain doesn't point to a preview
fn preview_outside_ifds<R: Read + Seek>(
    reader: &mut R,
    raw_path: &str,
    file_len: u64,
    search: Search,
) -> Result<Vec<u8>, String> {
    let regions = located::preview_regions(reader, Path::new(raw_path), file_len);
    let from_regions = match search {
        Search::Located => largest_in_regions(reader, &regions, file_len),
        // Located scans the whole file too
        Search::Earlier if regions.is_empty() => return Err(SAME_AS_LOCATED.to_string()),
        Search::Earlier => None,
    };
    from_regions
        .or_else(|| extract_via_jpeg_scan(raw_path).ok())
        .ok_or_else(|| format!("Could not extract preview from RAW file: {}", raw_path))
}

/// The largest JPEG of at least `MIN_PREVIEW_SIZE` found by scanning each
/// region; of equally long ones the one in the earlier region wins, as in
/// a scan of the whole file. Regions are read longest first, and one too
/// short to hold a larger JPEG than the best so far is not read at all.
/// Regions that can't be read are skipped.
fn largest_in_regions<R: Read + Seek>(
    reader: &mut R,
    regions: &[(u64, usize)],
    file_len: u64,
) -> Option<Vec<u8>> {
    let mut longest_first = regions.to_vec();
    longest_first.sort_unstable_by_key(|&(offset, length)| (Reverse(length), offset));
    // (offset of its region, bytes) of the best JPEG so far
    let mut best: Option<(u64, Vec<u8>)> = None;
    for (offset, length) in longest_first {
        let best_len = best.as_ref().map_or(0, |(_, jpeg)| jpeg.len());
        if length < best_len.max(MIN_PREVIEW_SIZE) {
            continue;
        }
        let Ok(bytes) = read_region(reader, offset, length, file_len) else {
            continue;
        };
        let Some(jpeg) = find_largest_jpeg(&bytes, best_len.max(MIN_PREVIEW_SIZE)) else {
            continue;
        };
        // It is at least as long as the best so far
        let is_better = best
            .as_ref()
            .is_none_or(|(best_at, best)| jpeg.len() > best.len() || offset < *best_at);
        if is_better {
            best = Some((offset, jpeg.to_vec()));
        }
    }
    best.map(|(_, jpeg)| jpeg)
}

/// Write the preview under a temporary name and rename it into place, so
/// an interrupted write never leaves a truncated file that looks cached
fn write_cache(cache_path: &Path, cache_dir: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let tmp = cache_path.with_extension(format!(
        "{}-{}.tmp",
        process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = fs::write(&tmp, bytes).and_then(|()| fs::rename(&tmp, cache_path));
    written.map_err(|e| {
        if is_inside_dir(&tmp, cache_dir) {
            let _ = fs::remove_file(&tmp);
        }
        format!("Failed to write preview cache: {}", e)
    })
}

/// The largest valid JPEG of `jpegs`, (offset, length) pairs in IFD order.
/// A JPEG no longer than one already found is never read; of equally long
/// ones the first wins.
fn largest_jpeg<R: Read + Seek>(
    reader: &mut R,
    jpegs: &[(u64, usize)],
    file_len: u64,
) -> Option<Vec<u8>> {
    let mut best: Option<Vec<u8>> = None;
    for &(offset, length) in jpegs {
        let best_len = best.as_ref().map_or(0, Vec::len);
        if length < 100 || length <= best_len {
            continue;
        }
        if let Ok(jpeg) = read_jpeg_at_offset(reader, offset, length, file_len) {
            if jpeg.len() > best_len {
                best = Some(jpeg);
            }
        }
    }
    best
}

/// Read and validate JPEG data at a specific offset in a file
fn read_jpeg_at_offset<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    length: usize,
    file_len: u64,
) -> Result<Vec<u8>, String> {
    let jpeg_data = read_region(reader, offset, length, file_len)?;

    // Verify JPEG signature (FFD8)
    if jpeg_data.len() >= 2 && jpeg_data[0] == 0xFF && jpeg_data[1] == 0xD8 {
        Ok(jpeg_data)
    } else {
        Err("Data at offset is not a valid JPEG".to_string())
    }
}

/// The `length` bytes at `offset`, cut short at the end of the file
fn read_region<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    length: usize,
    file_len: u64,
) -> Result<Vec<u8>, String> {
    let length = preview_read_len(offset, length, file_len)?;
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Failed to seek to JPEG offset: {}", e))?;
    let mut bytes = vec![0u8; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("Failed to read JPEG data: {}", e))?;
    Ok(bytes)
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
        cr3_with_jpegs, exif_orientation, nef_with_jpegs, noise_jpeg, opaque_raw_with_preview,
        raf_with_jpeg, tiff_raw_declaring_preview_len, tiff_raw_with_preview, write,
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
    fn preview_bytes_match_what_is_cached() {
        let dir = tempdir().unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), Some(6));
        let raw = write(dir.path(), "a.ARW", &raw);

        let bytes = raw_preview_bytes(raw.to_str().unwrap()).unwrap();

        assert_eq!(bytes, extract(&raw, dir.path()).unwrap());
    }

    #[test]
    fn falls_back_to_binary_scan_for_non_tiff_containers() {
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = write(dir.path(), "a.CR3", &opaque_raw_with_preview(&preview));

        assert_eq!(extract(&raw, dir.path()).unwrap(), preview);
    }

    /// A CR3, RAF and NEF each holding `big` (their largest JPEG) and, for
    /// CR3 and NEF, `small` where the format points to, followed by `tail`
    fn located_raws(big: &[u8], small: &[u8], tail: &[u8]) -> [(&'static str, Vec<u8>); 3] {
        let with_tail = |mut raw: Vec<u8>| {
            raw.extend(tail);
            raw
        };
        [
            ("a.CR3", with_tail(cr3_with_jpegs(big, small, true))),
            ("a.RAF", with_tail(raf_with_jpeg(big))),
            ("a.NEF", with_tail(nef_with_jpegs(&[small, big], true))),
        ]
    }

    #[test]
    fn finds_the_same_preview_as_earlier_versions_where_the_format_points() {
        let dir = tempdir().unwrap();
        let (big, small) = (noise_jpeg(320, 320), noise_jpeg(256, 256));
        for (name, bytes) in located_raws(&big, &small, b"") {
            let raw = write(dir.path(), name, &bytes);
            let raw = raw.to_str().unwrap();

            assert_eq!(raw_preview_bytes(raw).unwrap(), big, "{name}");
            assert_eq!(earlier_preview_bytes(raw).unwrap(), big, "{name}");
        }
    }

    #[test]
    fn reads_only_the_regions_the_format_points_to() {
        // Earlier versions scanned the whole file and took this larger one
        let outside = noise_jpeg(400, 400);
        let dir = tempdir().unwrap();
        let (big, small) = (noise_jpeg(320, 320), noise_jpeg(256, 256));
        for (name, bytes) in located_raws(&big, &small, &outside) {
            let raw = write(dir.path(), name, &bytes);
            let raw = raw.to_str().unwrap();

            assert_eq!(raw_preview_bytes(raw).unwrap(), big, "{name}");
            assert_eq!(earlier_preview_bytes(raw).unwrap(), outside, "{name}");
        }
    }

    #[test]
    fn picks_the_largest_jpeg_of_the_regions_and_the_earlier_of_equals() {
        let (a, b, larger) = (
            noise_jpeg(256, 256),
            noise_jpeg(256, 256),
            noise_jpeg(320, 320),
        );
        assert_eq!(a.len(), b.len());
        let file = [a.as_slice(), b"junk", &b, &larger].concat();
        let at = |offset: usize, jpeg: &[u8]| (offset as u64, jpeg.len());
        let (a_at, b_at) = (at(0, &a), at(a.len() + 4, &b));
        let larger_at = at(a.len() + 4 + b.len(), &larger);
        let unreadable = (file.len() as u64 + 10, 100_000);
        let largest = |regions: &[(u64, usize)]| {
            let mut reader = std::io::Cursor::new(&file);
            largest_in_regions(&mut reader, regions, file.len() as u64)
        };

        assert_eq!(largest(&[b_at, a_at]), Some(a.clone()));
        assert_eq!(largest(&[b_at]), Some(b));
        assert_eq!(largest(&[a_at, unreadable, larger_at, b_at]), Some(larger));
        assert_eq!(largest(&[unreadable]), None);
    }

    #[test]
    fn scans_the_whole_file_when_the_format_points_to_no_preview() {
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let mut raf = raf_with_jpeg(b"not a jpeg");
        raf.extend(&preview);
        let raw = write(dir.path(), "a.RAF", &raf);

        assert_eq!(extract(&raw, dir.path()).unwrap(), preview);
    }

    #[test]
    fn earlier_versions_extracted_the_same_preview_from_other_raws() {
        let dir = tempdir().unwrap();
        let preview = noise_jpeg(256, 256);
        let arw = write(dir.path(), "a.ARW", &tiff_raw_with_preview(&preview, None));
        let orf = write(dir.path(), "a.ORF", &opaque_raw_with_preview(&preview));

        for raw in [arw, orf] {
            assert_eq!(earlier_preview_bytes(raw.to_str().unwrap()), None);
        }
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
    fn serves_a_cached_preview_without_reading_the_raw_again() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), Some(6));
        let raw = write(dir.path(), "a.ARW", &raw);
        let cached = extract_raw_preview(raw.to_str().unwrap(), &cache).unwrap();
        fs::write(&cached, b"stand-in").unwrap();

        let again = extract_raw_preview(raw.to_str().unwrap(), &cache).unwrap();

        assert_eq!(again, cached);
        assert_eq!(fs::read(again).unwrap(), b"stand-in");
    }

    #[test]
    fn a_cache_hit_marks_the_preview_used_but_keeps_its_modification_time() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        let raw = write(dir.path(), "a.ARW", &raw);
        let cached = extract_raw_preview(raw.to_str().unwrap(), &cache).unwrap();
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
        let times = std::fs::FileTimes::new()
            .set_accessed(long_ago)
            .set_modified(long_ago);
        File::options()
            .write(true)
            .open(&cached)
            .unwrap()
            .set_times(times)
            .unwrap();

        extract_raw_preview(raw.to_str().unwrap(), &cache).unwrap();

        // Its thumbnail is keyed by the preview's modification time
        let meta = fs::metadata(&cached).unwrap();
        assert_eq!(meta.modified().unwrap(), long_ago);
        assert!(meta.accessed().unwrap() > long_ago);
    }

    #[test]
    fn extracts_again_when_the_raw_changes() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = write(dir.path(), "a.ARW", &tiff_raw_with_preview(&preview, None));
        let first = extract(&raw, &cache).unwrap();
        assert_eq!(exif_orientation(&first), None);

        // Same path, now carrying an outer orientation: the old preview is stale
        write(
            dir.path(),
            "a.ARW",
            &tiff_raw_with_preview(&preview, Some(8)),
        );
        let second = extract(&raw, &cache).unwrap();

        assert_eq!(exif_orientation(&second), Some(8));
    }

    #[test]
    fn extracts_again_when_only_the_modification_time_changes() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        let raw = write(dir.path(), "a.ARW", &raw);
        let first = extract_raw_preview(raw.to_str().unwrap(), &cache).unwrap();

        let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(3_600);
        File::options()
            .write(true)
            .open(&raw)
            .unwrap()
            .set_modified(earlier)
            .unwrap();
        let second = extract_raw_preview(raw.to_str().unwrap(), &cache).unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn leaves_no_temporary_files_in_the_cache() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        let raw = write(dir.path(), "a.ARW", &raw);

        let cached = extract_raw_preview(raw.to_str().unwrap(), &cache).unwrap();

        let names: Vec<_> = fs::read_dir(&cache)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(names, vec![Path::new(&cached).to_path_buf()]);
    }

    #[test]
    fn prefers_the_largest_jpeg_and_skips_invalid_ones() {
        let small = noise_jpeg(128, 128);
        let large = noise_jpeg(256, 256);
        let mut file = vec![0u8; 16];
        let small_at = file.len() as u64;
        file.extend(&small);
        let large_at = file.len() as u64;
        file.extend(&large);
        let junk_at = file.len() as u64;
        file.extend(vec![0x42; large.len() * 2]);
        let len = file.len() as u64;
        let mut reader = std::io::Cursor::new(file);

        let jpegs = [
            (small_at, small.len()),
            (junk_at, large.len() * 2),
            (large_at, large.len()),
            (small_at, small.len()),
        ];

        assert_eq!(largest_jpeg(&mut reader, &jpegs, len), Some(large));
        assert_eq!(largest_jpeg(&mut reader, &[(junk_at, 500)], len), None);
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
        let raw_bytes = fs::read(raw).unwrap();
        let outer = tiff_layout::read_layout(
            &mut std::io::Cursor::new(&raw_bytes),
            raw_bytes.len() as u64,
        )
        .and_then(|layout| layout.orientation)
        .unwrap_or(1);
        let embedded = exif_orientation::read_orientation(&bytes).unwrap_or(1);
        if outer > 1 && embedded != outer {
            return Err(format!("orientation {embedded}, RAW says {outer}"));
        }
        let taken = crate::media::capture_time::read_capture_time(raw).ok_or("no capture time")?;
        // Archived previews of earlier versions must still be recognized
        let read = match earlier_preview_bytes(raw_str) {
            Some(earlier) if earlier != bytes => {
                return Err(format!(
                    "located preview ({} KB) differs from the whole-file scan ({} KB)",
                    bytes.len() / 1024,
                    earlier.len() / 1024
                ))
            }
            Some(_) => "located, same as the whole-file scan",
            None => "IFD chain or whole-file scan",
        };
        Ok(format!(
            "{}x{}, {} KB, orientation {embedded}, taken at {} ms, {read}",
            img.width(),
            img.height(),
            bytes.len() / 1024,
            taken.local_ms
        ))
    }
}
