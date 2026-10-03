// ============================================================
// Sift - RAW/JPG/XMP Pairing
// Pure pairing rules: no filesystem access, so every rule is
// unit-testable from plain paths.
// ============================================================

use crate::catalog::file_types::{is_jpg, is_raw, raw_format_name};
use crate::models::photo::{PhotoPair, PhotoSource, PhotoStatus};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Jpg,
    Raw,
}

/// One JPG or RAW file. `name` and `stem` keep the original spelling.
struct PhotoFile {
    path: String,
    name: String,
    stem: String,
    /// Lowercase extension
    ext: String,
    kind: Kind,
}

/// Photo files found in a folder, grouped by case-folded file stem.
#[derive(Default)]
pub struct FileIndex {
    /// `fold_case(stem)` -> every JPG and RAW with that stem
    photos: HashMap<String, Vec<PhotoFile>>,
    /// (XMP file stem, path): "IMG_1.xmp" has stem "IMG_1", "IMG_1.CR3.xmp" has "IMG_1.CR3"
    xmps: Vec<(String, String)>,
}

/// A RAW file with no JPG to pair with; needs its embedded preview extracted.
pub struct RawOnly {
    pub raw_path: String,
    pub ext: String,
    pub xmp_paths: Vec<String>,
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
    /// Neither stems nor extensions are case-sensitive (see `fold_case`).
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

        let kind = if ext == "xmp" {
            self.xmps.push((stem, full_path));
            return;
        } else if is_jpg(&ext) {
            Kind::Jpg
        } else if is_raw(&ext) {
            Kind::Raw
        } else {
            return;
        };
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.photos
            .entry(fold_case(&stem))
            .or_default()
            .push(PhotoFile {
                name: name.to_string(),
                path: full_path,
                stem,
                ext,
                kind,
            });
    }

    pub fn has_jpgs(&self) -> bool {
        self.photos.values().flatten().any(|f| f.kind == Kind::Jpg)
    }

    /// Number of JPG and RAW files seen (XMP sidecars excluded)
    pub fn photo_count(&self) -> usize {
        self.photos.values().map(Vec::len).sum()
    }
}

/// Grouping key for a stem, so `img_1.jpg` pairs with `IMG_1.CR3`: cameras
/// and import tools often disagree on case. Only ASCII letters are folded.
/// Camera (DCF) names are ASCII, and ASCII folding is predictable: Unicode
/// lowercasing differs from what file systems treat as equal (ß, dotted İ,
/// NFC vs NFD) and can change a name's byte length, which `paths::insert_suffix`
/// relies on staying the same. Non-ASCII stems still pair when spelled the same.
fn fold_case(stem: &str) -> String {
    stem.to_ascii_lowercase()
}

/// One session entry for a stem: a JPG and/or a RAW plus the XMPs that go
/// with them. Always holds at least one of `jpg` and `raw`.
struct Entry<'a> {
    jpg: Option<&'a PhotoFile>,
    raw: Option<&'a PhotoFile>,
    xmps: Vec<String>,
}

impl<'a> Entry<'a> {
    fn new(jpg: Option<&'a PhotoFile>, raw: Option<&'a PhotoFile>) -> Self {
        Entry {
            jpg,
            raw,
            xmps: Vec::new(),
        }
    }

    fn files(&self) -> impl Iterator<Item = &'a PhotoFile> {
        [self.jpg, self.raw].into_iter().flatten()
    }
}

/// Pair JPGs with same-stem RAWs; RAWs without a JPG are returned separately.
///
/// Every photo file ends up in exactly one entry. When a stem has several
/// JPGs or RAWs, the first JPG and first RAW by name form a pair; each extra
/// JPG becomes a JPG-only photo and each extra RAW a RAW-only one.
pub fn pair(index: &FileIndex) -> Pairing {
    let mut groups: HashMap<&str, Vec<Entry>> = index
        .photos
        .iter()
        .map(|(stem, files)| (stem.as_str(), entries_for_stem(files)))
        .collect();
    for (xmp_stem, path) in &index.xmps {
        if let Some((stem, i)) = xmp_owner(&groups, xmp_stem) {
            if let Some(entry) = groups.get_mut(stem).and_then(|e| e.get_mut(i)) {
                entry.xmps.push(path.clone());
            }
        }
    }
    into_pairing(groups.into_values().flatten())
}

/// Split one stem's files into entries; the primary entry comes first.
fn entries_for_stem(files: &[PhotoFile]) -> Vec<Entry<'_>> {
    let (mut jpgs, mut raws): (Vec<&PhotoFile>, Vec<&PhotoFile>) =
        files.iter().partition(|f| f.kind == Kind::Jpg);
    jpgs.sort_by(|a, b| by_name(a, b));
    raws.sort_by(|a, b| by_name(a, b));

    let mut jpgs = jpgs.into_iter();
    let mut raws = raws.into_iter();
    let primary = Entry::new(jpgs.next(), raws.next());
    std::iter::once(primary)
        .chain(jpgs.map(|jpg| Entry::new(Some(jpg), None)))
        .chain(raws.map(|raw| Entry::new(None, Some(raw))))
        .collect()
}

/// Natural order by file name, then by full path, so the primary file does
/// not depend on directory listing order.
fn by_name(a: &PhotoFile, b: &PhotoFile) -> Ordering {
    natord::compare(&a.name, &b.name).then_with(|| a.path.cmp(&b.path))
}

/// The (stem key, entry index) an XMP sidecar belongs to, so each XMP is
/// moved or trashed along with exactly one entry. Names match ignoring case:
/// - "IMG_1.CR3.xmp" goes with IMG_1.CR3;
/// - "IMG_1.xmp" goes with the primary entry of stem IMG_1;
/// - "IMG_1.NEF.xmp" with no IMG_1.NEF also goes with that primary entry.
///
/// When a stem's entries differ only in case (IMG_1.jpg and img_1.JPG on a
/// case-sensitive volume), the entry spelled exactly like the XMP wins.
fn xmp_owner<'a>(
    groups: &HashMap<&'a str, Vec<Entry>>,
    xmp_stem: &str,
) -> Option<(&'a str, usize)> {
    let key = fold_case(xmp_stem);
    let base = key.rsplit_once('.').map(|(base, _)| base);
    if let Some((&stem, entries)) = base.and_then(|b| groups.get_key_value(b)) {
        if let Some(i) = entry_with(entries, xmp_stem, |f| &f.name) {
            return Some((stem, i));
        }
    }
    if let Some((&stem, entries)) = groups.get_key_value(key.as_str()) {
        return Some((
            stem,
            entry_with(entries, xmp_stem, |f| &f.stem).unwrap_or(0),
        ));
    }
    base.and_then(|b| groups.get_key_value(b))
        .map(|(&stem, _)| (stem, 0))
}

/// First entry holding a file whose `part` (name or stem) is `target`:
/// spelled exactly the same if any, otherwise equal ignoring ASCII case.
fn entry_with(entries: &[Entry], target: &str, part: impl Fn(&PhotoFile) -> &str) -> Option<usize> {
    let find = |matches: &dyn Fn(&str) -> bool| {
        entries
            .iter()
            .position(|e| e.files().any(|f| matches(part(f))))
    };
    find(&|s| s == target).or_else(|| find(&|s| s.eq_ignore_ascii_case(target)))
}

fn into_pairing<'a>(entries: impl Iterator<Item = Entry<'a>>) -> Pairing {
    let mut pairing = Pairing {
        pairs: Vec::new(),
        raw_only: Vec::new(),
        paired_count: 0,
        jpg_only_count: 0,
    };
    for entry in entries {
        match (entry.jpg, entry.raw) {
            (Some(jpg), raw) => {
                match raw {
                    Some(_) => pairing.paired_count += 1,
                    None => pairing.jpg_only_count += 1,
                }
                pairing.pairs.push(new_pair(
                    jpg.path.clone(),
                    raw.map(|r| (r.path.clone(), r.ext.as_str())),
                    PhotoSource::Jpg,
                    entry.xmps,
                ));
            }
            (None, Some(raw)) => pairing.raw_only.push(RawOnly {
                raw_path: raw.path.clone(),
                ext: raw.ext.clone(),
                xmp_paths: entry.xmps,
            }),
            (None, None) => unreachable!("entries always hold a JPG or a RAW"),
        }
    }
    pairing
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

/// Natural sort by the displayed file name (IMG_2 before IMG_10), then by
/// full path so same-named files from different folders keep a stable order.
pub fn sort_naturally(pairs: &mut [PhotoPair]) {
    fn file_name(path: &str) -> &str {
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
    }
    pairs.sort_by(|a, b| {
        natord::compare(file_name(&a.jpg_path), file_name(&b.jpg_path))
            .then_with(|| a.jpg_path.cmp(&b.jpg_path))
    });
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

    /// Every path that ended up in a pair (JPG, RAW or XMP) or in `raw_only`.
    fn all_paths(pairing: &Pairing) -> Vec<String> {
        let mut paths: Vec<String> = pairing
            .pairs
            .iter()
            .flat_map(|p| {
                let mut v = vec![p.jpg_path.clone()];
                v.extend(p.raw_path.clone());
                v.extend(p.xmp_paths.clone());
                v
            })
            .chain(pairing.raw_only.iter().flat_map(|r| {
                let mut v = vec![r.raw_path.clone()];
                v.extend(r.xmp_paths.clone());
                v
            }))
            .collect();
        paths.sort();
        paths
    }

    fn sorted_names(paths: &[String]) -> Vec<&str> {
        let mut names: Vec<&str> = paths.iter().map(|p| file_name(p)).collect();
        names.sort();
        names
    }

    fn pair_with_jpg<'a>(pairing: &'a Pairing, name: &str) -> &'a PhotoPair {
        pairing
            .pairs
            .iter()
            .find(|p| file_name(&p.jpg_path) == name)
            .unwrap_or_else(|| panic!("no pair for {}", name))
    }

    fn raw_only_names(pairing: &Pairing) -> Vec<&str> {
        let mut names: Vec<&str> = pairing
            .raw_only
            .iter()
            .map(|r| file_name(&r.raw_path))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn reports_unmatched_raws_as_raw_only() {
        let pairing = pair(&index(&["IMG_1.jpg", "DSC_2.ARW"]));

        assert_eq!(pairing.jpg_only_count, 1);
        assert_eq!(raw_only_names(&pairing), ["DSC_2.ARW"]);
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
        let pairing = pair(&index(&[
            "IMG_1.JPG",
            "IMG_1.CR3",
            "IMG_1.xmp",
            "IMG_1.CR3.xmp",
        ]));

        assert_eq!(
            sorted_names(&pairing.pairs[0].xmp_paths),
            ["IMG_1.CR3.xmp", "IMG_1.xmp"]
        );
    }

    #[test]
    fn attaches_raw_named_xmp_to_the_pair_even_without_that_raw() {
        let pairing = pair(&index(&["IMG_1.jpg", "IMG_1.CR3.xmp"]));
        assert_eq!(sorted_names(&pairing.pairs[0].xmp_paths), ["IMG_1.CR3.xmp"]);
    }

    #[test]
    fn attaches_dotted_stem_xmp_only_to_its_own_photo() {
        // "IMG_1.edit.xmp" belongs to IMG_1.edit.jpg; giving it to IMG_1.jpg
        // too would trash it when IMG_1.jpg is deleted.
        let pairing = pair(&index(&["IMG_1.jpg", "IMG_1.edit.jpg", "IMG_1.edit.xmp"]));

        assert!(pair_with_jpg(&pairing, "IMG_1.jpg").xmp_paths.is_empty());
        assert_eq!(
            sorted_names(&pair_with_jpg(&pairing, "IMG_1.edit.jpg").xmp_paths),
            ["IMG_1.edit.xmp"]
        );
    }

    #[test]
    fn matches_extensions_case_insensitively() {
        let pairing = pair(&index(&["IMG_1.jpeg", "IMG_1.nef"]));
        assert_eq!(pairing.paired_count, 1);
    }

    #[test]
    fn pairs_stems_that_differ_only_in_case_keeping_original_paths() {
        let pairing = pair(&index(&["img_1.jpg", "IMG_1.CR3"]));

        assert_eq!(pairing.paired_count, 1);
        assert!(pairing.raw_only.is_empty());
        let p = &pairing.pairs[0];
        assert_eq!(p.jpg_path, "/photos/img_1.jpg");
        assert_eq!(p.raw_path.as_deref(), Some("/photos/IMG_1.CR3"));
    }

    #[test]
    fn attaches_xmps_whose_stem_case_differs_from_the_photos() {
        let pairing = pair(&index(&[
            "img_1.jpg",
            "IMG_1.CR3",
            "Img_1.xmp",
            "img_1.cr3.xmp",
            "IMG_2.NEF",
            "img_2.xmp",
        ]));

        assert_eq!(
            sorted_names(&pair_with_jpg(&pairing, "img_1.jpg").xmp_paths),
            ["Img_1.xmp", "img_1.cr3.xmp"]
        );
        assert_eq!(sorted_names(&pairing.raw_only[0].xmp_paths), ["img_2.xmp"]);
    }

    #[test]
    fn folds_only_ascii_case() {
        // Non-ASCII letters are compared byte for byte (see `fold_case`).
        let pairing = pair(&index(&["Ümlaut.jpg", "ümlaut.CR3"]));
        assert_eq!(pairing.paired_count, 0);
        assert_eq!(raw_only_names(&pairing), ["ümlaut.CR3"]);
    }

    #[test]
    fn keeps_jpgs_whose_stems_differ_only_in_case_as_separate_photos() {
        // Both can exist on a case-sensitive volume; neither may be dropped.
        let files = ["IMG_1.jpg", "img_1.JPG", "IMG_1.CR3"];
        let pairing = pair(&index(&files));

        assert_eq!(pairing.pairs.len(), 2);
        assert_eq!(pairing.paired_count, 1);
        assert_eq!(pairing.jpg_only_count, 1);
        let primary = pair_with_jpg(&pairing, "IMG_1.jpg");
        assert_eq!(file_name(primary.raw_path.as_deref().unwrap()), "IMG_1.CR3");
        assert_eq!(pair_with_jpg(&pairing, "img_1.JPG").raw_path, None);
        assert_eq!(all_paths(&pairing).len(), files.len());
    }

    #[test]
    fn gives_each_case_variant_the_xmps_named_exactly_after_it() {
        let pairing = pair(&index(&[
            "IMG_1.jpg",
            "img_1.JPG",
            "IMG_1.xmp",
            "img_1.xmp",
            "IMG_1.jpg.xmp",
            "img_1.JPG.xmp",
            "IMG_1.JPG.xmp",
        ]));

        // An exact name wins; IMG_1.JPG.xmp matches neither exactly and goes
        // to the first file with that name ignoring case.
        assert_eq!(
            sorted_names(&pair_with_jpg(&pairing, "IMG_1.jpg").xmp_paths),
            ["IMG_1.JPG.xmp", "IMG_1.jpg.xmp", "IMG_1.xmp"]
        );
        assert_eq!(
            sorted_names(&pair_with_jpg(&pairing, "img_1.JPG").xmp_paths),
            ["img_1.JPG.xmp", "img_1.xmp"]
        );
    }

    #[test]
    fn keeps_a_second_jpg_with_the_same_stem_as_its_own_pair() {
        let pairing = pair(&index(&["IMG_1.jpg", "IMG_1.jpeg", "IMG_1.CR3"]));

        assert_eq!(pairing.pairs.len(), 2);
        assert_eq!(pairing.paired_count, 1);
        assert_eq!(pairing.jpg_only_count, 1);
        // The first JPG by name pairs with the RAW
        let primary = pair_with_jpg(&pairing, "IMG_1.jpeg");
        assert_eq!(file_name(primary.raw_path.as_deref().unwrap()), "IMG_1.CR3");
        assert_eq!(pair_with_jpg(&pairing, "IMG_1.jpg").raw_path, None);
        assert!(pairing.raw_only.is_empty());
    }

    #[test]
    fn keeps_a_second_raw_with_the_same_stem_as_raw_only() {
        let pairing = pair(&index(&["IMG_1.jpg", "IMG_1.DNG", "IMG_1.CR3"]));

        assert_eq!(pairing.paired_count, 1);
        let primary = pair_with_jpg(&pairing, "IMG_1.jpg");
        assert_eq!(file_name(primary.raw_path.as_deref().unwrap()), "IMG_1.CR3");
        assert_eq!(raw_only_names(&pairing), ["IMG_1.DNG"]);
    }

    #[test]
    fn keeps_every_raw_of_a_stem_without_jpg() {
        let pairing = pair(&index(&["IMG_1.CR3", "IMG_1.DNG"]));

        assert!(pairing.pairs.is_empty());
        assert_eq!(raw_only_names(&pairing), ["IMG_1.CR3", "IMG_1.DNG"]);
    }

    #[test]
    fn picks_the_same_primary_files_whatever_the_scan_order() {
        let forward = pair(&index(&[
            "IMG_1.jpg",
            "IMG_1.jpeg",
            "IMG_1.CR3",
            "IMG_1.DNG",
        ]));
        let backward = pair(&index(&[
            "IMG_1.DNG",
            "IMG_1.CR3",
            "IMG_1.jpeg",
            "IMG_1.jpg",
        ]));

        for pairing in [&forward, &backward] {
            let primary = pair_with_jpg(pairing, "IMG_1.jpeg");
            assert_eq!(file_name(primary.raw_path.as_deref().unwrap()), "IMG_1.CR3");
            assert_eq!(raw_only_names(pairing), ["IMG_1.DNG"]);
        }
    }

    #[test]
    fn gives_file_specific_xmp_to_the_extra_file_and_shared_xmp_to_the_primary() {
        let pairing = pair(&index(&[
            "IMG_1.jpg",
            "IMG_1.CR3",
            "IMG_1.DNG",
            "IMG_1.xmp",
            "IMG_1.DNG.xmp",
        ]));

        assert_eq!(
            sorted_names(&pair_with_jpg(&pairing, "IMG_1.jpg").xmp_paths),
            ["IMG_1.xmp"]
        );
        assert_eq!(
            sorted_names(&pairing.raw_only[0].xmp_paths),
            ["IMG_1.DNG.xmp"]
        );
    }

    #[test]
    fn accounts_for_every_photo_and_xmp_exactly_once() {
        let files = [
            "IMG_1.jpg",
            "IMG_1.jpeg",
            "IMG_1.CR3",
            "IMG_1.DNG",
            "IMG_1.xmp",
            "IMG_1.CR3.xmp",
            "IMG_1.DNG.xmp",
            "IMG_1.jpeg.xmp",
            "IMG_2.NEF",
            "IMG_2.nef.xmp",
            "IMG_3.JPG",
            "img_3.jpg",
            "img_3.CR3",
            "Img_3.xmp",
        ];
        let pairing = pair(&index(&files));

        let mut expected: Vec<String> = files
            .iter()
            .map(|f| Path::new("/photos").join(f).to_string_lossy().to_string())
            .collect();
        expected.sort();
        assert_eq!(all_paths(&pairing), expected);
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
