// ============================================================
// Sift - RAW-only Preview Extraction
// Each RAW-only photo is shown through its embedded JPEG preview, extracted
// into the cache. RAWs without a usable preview are reported as skipped,
// never dropped.
// ============================================================

use crate::catalog::pairing::{self, RawOnly};
use crate::media::raw_preview::extract_raw_preview;
use crate::models::photo::{PhotoPair, PhotoSource, SkipReason, SkippedFile};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::path::Path;

/// RAW files are 25-60MB each; unbounded parallel preview extraction can exhaust memory
const PREVIEW_EXTRACTION_THREADS: usize = 2;

/// Extract embedded previews for RAW-only files. RAWs whose preview can't be
/// extracted are returned as skipped so the user is told about them.
pub fn extract_previews(
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
