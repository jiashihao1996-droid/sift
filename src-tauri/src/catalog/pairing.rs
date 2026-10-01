// ============================================================
// Sift - RAW/JPG/XMP Pairing
// Pure pairing rules: no filesystem access, so every rule is
// unit-testable from plain paths.
// ============================================================

use crate::catalog::file_types::{is_jpg, is_raw, raw_format_name};
use crate::models::photo::{PhotoPair, PhotoSource, PhotoStatus};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Photo files found in a folder, keyed by file stem.
#[derive(Default)]
pub struct FileIndex {
    jpgs: HashMap<String, String>,
    /// stem -> (path, lowercase extension)
    raws: HashMap<String, (String, String)>,
    xmps: HashMap<String, Vec<String>>,
    photo_count: usize,
}

/// A RAW file with no JPG of the same stem; needs its embedded preview extracted.
pub struct RawOnly {
    pub stem: String,
    pub raw_path: String,
    pub ext: String,
}

pub struct Pairing {
    /// JPG+RAW pairs and JPG-only photos
    pub pairs: Vec<PhotoPair>,
    pub raw_only: Vec<RawOnly>,
    pub paired_count: usize,
    pub jpg_only_count: usize,
}

impl FileIndex {
    /// Classify one file by extension. Unknown extensions are ignored.
    ///
    /// Stems are case-sensitive, and a later JPG/RAW with an already-seen stem
    /// replaces the earlier one (it still counts towards `photo_count`).
    pub fn add(&mut self, path: &Path) {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return;
        };
        let ext = ext.to_lowercase();
        let full_path = path.to_string_lossy().to_string();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if ext == "xmp" {
            self.add_xmp(stem, full_path);
        } else if is_jpg(&ext) {
            self.photo_count += 1;
            self.jpgs.insert(stem, full_path);
        } else if is_raw(&ext) {
            self.photo_count += 1;
            self.raws.insert(stem, (full_path, ext));
        }
    }

    /// XMP sidecars come as "IMG_001.xmp" or "IMG_001.CR3.xmp"; index the
    /// latter under both "IMG_001.CR3" and "IMG_001".
    fn add_xmp(&mut self, stem: String, full_path: String) {
        let base_stem = Path::new(&stem)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&stem)
            .to_string();

        self.xmps
            .entry(base_stem.clone())
            .or_default()
            .push(full_path.clone());
        if stem != base_stem {
            self.xmps.entry(stem).or_default().push(full_path);
        }
    }

    pub fn has_jpgs(&self) -> bool {
        !self.jpgs.is_empty()
    }

    /// Number of JPG and RAW files seen (XMP sidecars excluded)
    pub fn photo_count(&self) -> usize {
        self.photo_count
    }

    /// Deduplicated XMP sidecar paths for a stem
    pub fn xmps_for(&self, stem: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        self.xmps
            .get(stem)
            .into_iter()
            .flatten()
            .filter(|p| seen.insert(p.as_str()))
            .cloned()
            .collect()
    }
}

/// Pair JPGs with same-stem RAWs; RAWs without a JPG are returned separately.
pub fn pair(index: &FileIndex) -> Pairing {
    let mut pairs = Vec::new();
    let mut paired_count = 0;
    let mut jpg_only_count = 0;

    for (stem, jpg_path) in &index.jpgs {
        let raw = index.raws.get(stem);
        match raw {
            Some(_) => paired_count += 1,
            None => jpg_only_count += 1,
        }
        pairs.push(new_pair(
            jpg_path.clone(),
            raw.map(|(path, ext)| (path.clone(), ext.as_str())),
            PhotoSource::Jpg,
            index.xmps_for(stem),
        ));
    }

    let raw_only = index
        .raws
        .iter()
        .filter(|(stem, _)| !index.jpgs.contains_key(*stem))
        .map(|(stem, (raw_path, ext))| RawOnly {
            stem: stem.clone(),
            raw_path: raw_path.clone(),
            ext: ext.clone(),
        })
        .collect();

    Pairing {
        pairs,
        raw_only,
        paired_count,
        jpg_only_count,
    }
}

/// Build an unprocessed PhotoPair with a fresh id.
pub fn new_pair(
    jpg_path: String,
    raw: Option<(String, &str)>,
    source: PhotoSource,
    xmp_paths: Vec<String>,
) -> PhotoPair {
    let (raw_path, raw_format) = match raw {
        Some((path, ext)) => (Some(path), Some(raw_format_name(ext))),
        None => (None, None),
    };
    PhotoPair {
        id: uuid::Uuid::new_v4().to_string(),
        jpg_path,
        raw_path,
        raw_format,
        status: PhotoStatus::Unprocessed,
        thumbnail_path: None,
        dominant_color: None,
        source,
        xmp_paths,
    }
}

/// Natural sort by the displayed file name (IMG_2 before IMG_10).
pub fn sort_naturally(pairs: &mut [PhotoPair]) {
    fn file_name(path: &str) -> &str {
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
    }
    pairs.sort_by(|a, b| natord::compare(file_name(&a.jpg_path), file_name(&b.jpg_path)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(files: &[&str]) -> FileIndex {
        let mut index = FileIndex::default();
        for f in files {
            index.add(&Path::new("/photos").join(f));
        }
        index
    }

    fn file_name(path: &str) -> &str {
        Path::new(path).file_name().unwrap().to_str().unwrap()
    }

    #[test]
    fn pairs_jpg_with_raw_of_same_stem() {
        let pairing = pair(&index(&["IMG_1.JPG", "IMG_1.CR3"]));

        assert_eq!(pairing.paired_count, 1);
        let p = &pairing.pairs[0];
        assert_eq!(file_name(p.raw_path.as_deref().unwrap()), "IMG_1.CR3");
        assert_eq!(p.raw_format.as_deref(), Some("CR3"));
        assert_eq!(p.source, PhotoSource::Jpg);
        assert_eq!(p.status, PhotoStatus::Unprocessed);
        assert!(pairing.raw_only.is_empty());
    }

    #[test]
    fn reports_unmatched_raws_as_raw_only() {
        let pairing = pair(&index(&["IMG_1.jpg", "DSC_2.ARW"]));

        assert_eq!(pairing.jpg_only_count, 1);
        assert_eq!(pairing.raw_only.len(), 1);
        assert_eq!(pairing.raw_only[0].stem, "DSC_2");
        assert_eq!(pairing.raw_only[0].ext, "arw");
    }

    #[test]
    fn counts_only_jpg_and_raw_files() {
        let idx = index(&[
            "IMG_1.jpg",
            "IMG_1.CR3",
            "IMG_1.xmp",
            "notes.txt",
            "a.png",
            "README",
        ]);
        assert_eq!(idx.photo_count(), 2);
    }

    #[test]
    fn attaches_both_xmp_naming_styles_without_duplicates() {
        let idx = index(&["IMG_1.JPG", "IMG_1.CR3", "IMG_1.xmp", "IMG_1.CR3.xmp"]);
        let xmps = idx.xmps_for("IMG_1");
        let mut names: Vec<&str> = xmps.iter().map(|p| file_name(p)).collect();
        names.sort();

        assert_eq!(names, ["IMG_1.CR3.xmp", "IMG_1.xmp"]);
    }

    #[test]
    fn matches_extensions_case_insensitively() {
        let pairing = pair(&index(&["IMG_1.jpeg", "IMG_1.nef"]));
        assert_eq!(pairing.paired_count, 1);
    }

    #[test]
    fn stem_matching_is_case_sensitive() {
        // Current behaviour: differently-cased stems do not pair.
        let pairing = pair(&index(&["img_1.jpg", "IMG_1.CR3"]));

        assert_eq!(pairing.paired_count, 0);
        assert_eq!(pairing.jpg_only_count, 1);
        assert_eq!(pairing.raw_only.len(), 1);
    }

    #[test]
    fn keeps_only_one_file_per_stem_and_kind() {
        // Current behaviour: a second JPG or RAW with the same stem is dropped
        // from pairing (the later one wins) but still counted.
        let idx = index(&["IMG_1.jpg", "IMG_1.jpeg", "IMG_1.CR3", "IMG_1.DNG"]);
        let pairing = pair(&idx);

        assert_eq!(idx.photo_count(), 4);
        assert_eq!(pairing.pairs.len(), 1);
        assert!(pairing.raw_only.is_empty());
    }

    #[test]
    fn sorts_pairs_naturally_by_file_name() {
        let mut pairs: Vec<PhotoPair> = ["/b/IMG_10.jpg", "/a/IMG_2.jpg", "/c/IMG_1.jpg"]
            .iter()
            .map(|p| new_pair(p.to_string(), None, PhotoSource::Jpg, vec![]))
            .collect();

        sort_naturally(&mut pairs);

        let names: Vec<&str> = pairs.iter().map(|p| file_name(&p.jpg_path)).collect();
        assert_eq!(names, ["IMG_1.jpg", "IMG_2.jpg", "IMG_10.jpg"]);
    }

    #[test]
    fn new_pair_assigns_unique_ids() {
        let a = new_pair("a.jpg".into(), None, PhotoSource::Jpg, vec![]);
        let b = new_pair("a.jpg".into(), None, PhotoSource::Jpg, vec![]);
        assert_ne!(a.id, b.id);
    }
}
