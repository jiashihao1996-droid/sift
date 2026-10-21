// ============================================================
// Sift - RAW-only Preview Extraction
// Each RAW-only photo is shown through its embedded JPEG preview, extracted
// into the cache. RAWs without a usable preview are reported as skipped,
// never dropped. Extraction reports its progress and stops, before the next
// photo, once the scan is cancelled.
// ============================================================

use crate::catalog::pairing::{self, RawOnly};
use crate::catalog::scan_control::{PreviewProgress, ScanControl, SCAN_CANCELLED};
use crate::media::raw_preview::extract_raw_preview;
use crate::models::photo::{PhotoPair, PhotoSource, SkipReason, SkippedFile};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::path::Path;

/// RAW files are 25-60MB each; unbounded parallel preview extraction can exhaust memory
const PREVIEW_EXTRACTION_THREADS: usize = 2;

/// Extract embedded previews for RAW-only files. RAWs whose preview can't be
/// extracted are returned as skipped so the user is told about them. Once
/// the scan is cancelled the rest are left alone and the whole call errs:
/// a partial list would silently leave photos out of the session.
pub fn extract_previews(
    raw_only: Vec<RawOnly>,
    cache_dir: &Path,
    control: &ScanControl,
    progress: &PreviewProgress,
) -> Result<(Vec<PhotoPair>, Vec<SkippedFile>), String> {
    if raw_only.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let pool = ThreadPoolBuilder::new()
        .num_threads(PREVIEW_EXTRACTION_THREADS)
        .build()
        .map_err(|e| format!("Failed to build thread pool: {}", e))?;

    let results: Vec<Option<Result<PhotoPair, SkippedFile>>> = pool.install(|| {
        raw_only
            .into_par_iter()
            .map(|raw| {
                control.check().ok()?;
                let result = raw_preview_pair(raw, cache_dir);
                progress.finish_one();
                Some(result)
            })
            .collect()
    });
    // A photo left out means the scan was cancelled, which it stays
    let results: Vec<_> = results
        .into_iter()
        .collect::<Option<_>>()
        .ok_or_else(|| SCAN_CANCELLED.to_string())?;
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
    use crate::test_support::{noise_jpeg, tiff_raw_with_preview, write};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tempfile::tempdir;

    fn write_raws(dir: &Path, names: &[&str], bytes: &[u8]) -> Vec<String> {
        names
            .iter()
            .map(|name| write(dir, name, bytes).to_string_lossy().into_owned())
            .collect()
    }

    fn raw_only(paths: &[String]) -> Vec<RawOnly> {
        paths
            .iter()
            .map(|path| RawOnly {
                raw_path: path.clone(),
                ext: "ARW".to_string(),
                xmp_paths: Vec::new(),
            })
            .collect()
    }

    fn cached_previews(cache: &Path) -> usize {
        std::fs::read_dir(cache).unwrap().count()
    }

    #[test]
    fn stops_before_the_next_photo_once_cancelled_and_keeps_extracted_previews() {
        let dir = tempdir().unwrap();
        let cache = tempdir().unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        let names = ["A.ARW", "B.ARW", "C.ARW", "D.ARW", "E.ARW", "F.ARW"];
        let paths = write_raws(dir.path(), &names, &raw);
        let raws = raw_only(&paths);
        // Cancelled after the first two photos started, one per thread
        let checks = AtomicUsize::new(0);
        let is_cancelled = || checks.fetch_add(1, Ordering::SeqCst) >= PREVIEW_EXTRACTION_THREADS;
        let control = ScanControl {
            is_cancelled: &is_cancelled,
            on_progress: &|_, _| {},
        };
        let progress = PreviewProgress::start(&control, raws.len());

        let result = extract_previews(raws, cache.path(), &control, &progress);

        assert_eq!(result.unwrap_err(), SCAN_CANCELLED);
        assert_eq!(cached_previews(cache.path()), PREVIEW_EXTRACTION_THREADS);
        // A later scan finds them cached and extracts only the rest
        let raws = raw_only(&paths);
        let control = ScanControl::UNCONTROLLED;
        let progress = PreviewProgress::start(&control, raws.len());
        let (pairs, _) = extract_previews(raws, cache.path(), &control, &progress).unwrap();
        assert_eq!(pairs.len(), names.len());
        assert_eq!(cached_previews(cache.path()), names.len());
    }

    #[test]
    fn counts_every_photo_including_those_without_a_preview() {
        let dir = tempdir().unwrap();
        let cache = tempdir().unwrap();
        let raw = tiff_raw_with_preview(&noise_jpeg(256, 256), None);
        let mut paths = write_raws(dir.path(), &["A.ARW", "B.ARW"], &raw);
        paths.extend(write_raws(dir.path(), &["broken.ARW"], b"not a raw"));
        let raws = raw_only(&paths);
        let events = Mutex::new(Vec::new());
        let on_progress = |done, total| events.lock().unwrap().push((done, total));
        let control = ScanControl {
            is_cancelled: &|| false,
            on_progress: &on_progress,
        };
        let progress = PreviewProgress::start(&control, raws.len());

        let (pairs, skipped) = extract_previews(raws, cache.path(), &control, &progress).unwrap();

        assert_eq!((pairs.len(), skipped.len()), (2, 1));
        let events = events.into_inner().unwrap();
        assert_eq!(events.first(), Some(&(0, 3)));
        assert_eq!(events.last(), Some(&(3, 3)));
    }
}
