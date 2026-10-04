// ============================================================
// Sift - Export
// Copies starred photos (JPG + RAW) to target directory
// Supports RAW-only pairs: exports RAW original only
// Never overwrites: colliding pairs get a shared `_N` suffix, and each
// destination is claimed with `create_new` before anything is copied
// ============================================================

use super::file_name;
use crate::models::organize::OrganizeError;
use crate::models::photo::{ArchiveProgress, ExportPairInput, PhotoSource};
use crate::paths::claim_paths_sharing_stem;
use std::fs;
use std::path::Path;

/// One source file to copy; `kind` labels errors, and only photos count as exported.
struct ExportFile<'a> {
    src: &'a Path,
    kind: &'static str,
}

impl ExportFile<'_> {
    fn is_photo(&self) -> bool {
        self.kind != "XMP"
    }
}

/// Copy every pair into `target` and return how many photos (JPG/RAW) were
/// copied. On failure the error counts the photos copied before it.
pub fn export_pairs(
    pairs: &[ExportPairInput],
    target: &Path,
    mut on_progress: impl FnMut(ArchiveProgress),
) -> Result<usize, OrganizeError> {
    fs::create_dir_all(target)
        .map_err(|e| OrganizeError::io(&e, target, "Failed to create export folder"))?;

    let total = pairs.len();
    let mut exported_count: usize = 0;

    for (i, pair) in pairs.iter().enumerate() {
        let files = files_to_export(pair);
        let Some(first) = files.first() else {
            continue;
        };

        on_progress(ArchiveProgress {
            current: i + 1,
            total,
            current_file: file_name(first.src),
        });

        exported_count += copy_group(&files, target, &pair_stem(pair))
            .map_err(|e| e.with_completed(exported_count))?;
    }

    Ok(exported_count)
}

/// Existing files of a pair in copy order. RAW-only pairs skip `jpg_path`,
/// which points at a temporary preview rather than a user file.
fn files_to_export(pair: &ExportPairInput) -> Vec<ExportFile<'_>> {
    let mut files = Vec::new();
    if pair.source != PhotoSource::RawPreview {
        files.push(ExportFile {
            src: Path::new(&pair.jpg_path),
            kind: "JPG",
        });
    }
    if let Some(raw) = &pair.raw_path {
        files.push(ExportFile {
            src: Path::new(raw),
            kind: "RAW",
        });
    }
    for xmp in &pair.xmp_paths {
        files.push(ExportFile {
            src: Path::new(xmp),
            kind: "XMP",
        });
    }
    files.retain(|f| f.src.exists());
    files
}

/// The stem that ties a pair together: the RAW's when present, since XMP sidecars follow it.
fn pair_stem(pair: &ExportPairInput) -> String {
    let anchor = pair.raw_path.as_deref().unwrap_or(&pair.jpg_path);
    Path::new(anchor)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Copy a pair's files under one shared, non-colliding suffix; returns the photo count.
fn copy_group(files: &[ExportFile], target: &Path, stem: &str) -> Result<usize, OrganizeError> {
    let candidates: Vec<_> = files
        .iter()
        .map(|f| target.join(file_name(f.src)))
        .collect();
    let dests = claim_paths_sharing_stem(&candidates, stem).map_err(|e| {
        OrganizeError::io(&e, files[0].src, "Failed to reserve export file names for")
    })?;

    // Every dest is now an empty placeholder this call created, so fs::copy
    // only ever replaces our own file. A failed copy leaves the placeholder.
    for (file, dest) in files.iter().zip(&dests) {
        fs::copy(file.src, dest).map_err(|e| {
            let action = format!("Failed to copy {} to {}:", file.kind, dest.display());
            OrganizeError::io(&e, file.src, &action)
        })?;
    }
    Ok(files.iter().filter(|f| f.is_photo()).count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::{tempdir, TempDir};

    struct Fixture {
        src: TempDir,
        target: TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                src: tempdir().unwrap(),
                target: tempdir().unwrap(),
            }
        }

        fn source(&self, name: &str, content: &str) -> String {
            let path = self.src.path().join(name);
            fs::write(&path, content).unwrap();
            path.to_string_lossy().into_owned()
        }

        fn existing(&self, name: &str, content: &str) {
            fs::write(self.target.path().join(name), content).unwrap();
        }

        fn read(&self, name: &str) -> String {
            fs::read_to_string(self.target.path().join(name)).unwrap()
        }

        fn has(&self, name: &str) -> bool {
            self.target.path().join(name).exists()
        }

        fn export(&self, pairs: &[ExportPairInput]) -> usize {
            export_pairs(pairs, self.target.path(), |_| {}).unwrap()
        }
    }

    fn pair(jpg: &str, raw: Option<&str>, xmps: &[&str], source: PhotoSource) -> ExportPairInput {
        ExportPairInput {
            jpg_path: jpg.to_string(),
            raw_path: raw.map(str::to_string),
            source,
            xmp_paths: xmps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn copies_pair_with_original_names_when_no_collision() {
        let fx = Fixture::new();
        let jpg = fx.source("DSC_0001.JPG", "jpg");
        let raw = fx.source("DSC_0001.ARW", "raw");
        let xmp = fx.source("DSC_0001.xmp", "xmp");

        let count = fx.export(&[pair(&jpg, Some(&raw), &[&xmp], PhotoSource::Jpg)]);

        assert_eq!(count, 2);
        assert_eq!(fx.read("DSC_0001.JPG"), "jpg");
        assert_eq!(fx.read("DSC_0001.ARW"), "raw");
        assert_eq!(fx.read("DSC_0001.xmp"), "xmp");
    }

    #[test]
    fn collision_gets_unique_name_and_leaves_existing_file_unchanged() {
        let fx = Fixture::new();
        fx.existing("DSC_0001.ARW", "other card");
        let jpg = fx.source("DSC_0001.JPG", "jpg");
        let raw = fx.source("DSC_0001.ARW", "raw");

        let count = fx.export(&[pair(&jpg, Some(&raw), &[], PhotoSource::Jpg)]);

        assert_eq!(count, 2);
        assert_eq!(fx.read("DSC_0001.ARW"), "other card");
        assert_eq!(fx.read("DSC_0001_1.ARW"), "raw");
        assert_eq!(fx.read("DSC_0001_1.JPG"), "jpg");
        assert!(!fx.has("DSC_0001.JPG"));
    }

    #[test]
    fn raw_only_keeps_raw_and_xmp_on_matching_stem_when_only_xmp_collides() {
        let fx = Fixture::new();
        fx.existing("DSC_0001.ARW.xmp", "other edits");
        let preview = fx.source("preview-cache.jpg", "temp preview");
        let raw = fx.source("DSC_0001.ARW", "raw");
        let xmp = fx.source("DSC_0001.ARW.xmp", "xmp");

        let count = fx.export(&[pair(&preview, Some(&raw), &[&xmp], PhotoSource::RawPreview)]);

        assert_eq!(count, 1);
        assert_eq!(fx.read("DSC_0001.ARW.xmp"), "other edits");
        assert_eq!(fx.read("DSC_0001_1.ARW"), "raw");
        assert_eq!(fx.read("DSC_0001_1.ARW.xmp"), "xmp");
        assert!(!fx.has("DSC_0001.ARW"));
        assert!(!fx.has("preview-cache.jpg"));
    }

    #[test]
    fn same_name_from_two_source_folders_does_not_overwrite_within_one_export() {
        let fx = Fixture::new();
        let card_a = fx.source("DSC_0001.JPG", "card a");
        let card_b_dir = tempdir().unwrap();
        let card_b: PathBuf = card_b_dir.path().join("DSC_0001.JPG");
        fs::write(&card_b, "card b").unwrap();
        let card_b = card_b.to_string_lossy().into_owned();

        let count = fx.export(&[
            pair(&card_a, None, &[], PhotoSource::Jpg),
            pair(&card_b, None, &[], PhotoSource::Jpg),
        ]);

        assert_eq!(count, 2);
        assert_eq!(fx.read("DSC_0001.JPG"), "card a");
        assert_eq!(fx.read("DSC_0001_1.JPG"), "card b");
    }

    #[test]
    fn exporting_into_source_folder_does_not_clobber_originals() {
        let fx = Fixture::new();
        let jpg = fx.source("DSC_0001.JPG", "original");

        export_pairs(
            &[pair(&jpg, None, &[], PhotoSource::Jpg)],
            fx.src.path(),
            |_| {},
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&jpg).unwrap(), "original");
        assert!(fx.src.path().join("DSC_0001_1.JPG").exists());
    }

    #[test]
    fn skips_missing_files_and_reports_progress_per_exported_pair() {
        let fx = Fixture::new();
        let jpg = fx.source("DSC_0002.JPG", "jpg");
        let missing = fx
            .src
            .path()
            .join("gone.JPG")
            .to_string_lossy()
            .into_owned();
        let mut progress = Vec::new();

        let pairs = [
            pair(&missing, None, &[], PhotoSource::Jpg),
            pair(&jpg, None, &[], PhotoSource::Jpg),
        ];
        let count = export_pairs(&pairs, fx.target.path(), |p| progress.push(p)).unwrap();

        assert_eq!(count, 1);
        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0].current, 2);
        assert_eq!(progress[0].total, 2);
        assert_eq!(progress[0].current_file, "DSC_0002.JPG");
    }
}
