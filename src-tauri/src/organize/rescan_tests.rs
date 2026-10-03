// ============================================================
// Sift - Organize → Rescan Round Trips
// Archive and export must leave files that a rescan pairs up again, even
// when a pair's JPG, RAW and XMPs spell their stem in different cases.
// ============================================================

use super::archive::archive_pairs;
use super::export::export_pairs;
use crate::catalog::scan::scan_folder;
use crate::models::photo::{ArchivePairInput, ExportPairInput, PhotoPair, PhotoSource};
use std::fs;
use std::path::Path;
use tempfile::{tempdir, TempDir};

/// A JPG, RAW and both XMP styles whose stems differ only in case.
const MIXED_CASE_GROUP: [&str; 4] = ["img_1.jpg", "IMG_1.CR3", "Img_1.xmp", "img_1.cr3.xmp"];

fn write(path: &Path, contents: &str) -> String {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    path.to_string_lossy().into_owned()
}

fn file_name(path: &str) -> &str {
    Path::new(path).file_name().unwrap().to_str().unwrap()
}

fn sources(dir: &Path) -> Vec<String> {
    MIXED_CASE_GROUP
        .iter()
        .map(|name| write(&dir.join(name), name))
        .collect()
}

fn rescan(dir: &Path) -> Vec<PhotoPair> {
    let cache = tempdir().unwrap();
    scan_folder(dir, cache.path()).unwrap().pairs
}

fn assert_rescanned_as_one_pair(pairs: &[PhotoPair], jpg: &str, raw: &str, xmps: [&str; 2]) {
    let pair = pairs
        .iter()
        .find(|p| file_name(&p.jpg_path) == jpg)
        .unwrap_or_else(|| panic!("no pair for {}", jpg));
    assert_eq!(pair.raw_path.as_deref().map(file_name), Some(raw));
    let mut names: Vec<&str> = pair.xmp_paths.iter().map(|p| file_name(p)).collect();
    names.sort();
    assert_eq!(names, xmps);
}

#[test]
fn archived_mixed_case_pair_keeps_each_stem_and_one_suffix_and_pairs_on_rescan() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("photos");
    let cache = tmp.path().join("cache");
    fs::create_dir_all(&cache).unwrap();
    // An earlier archive already holds IMG_1, so the new group must move on to _1
    write(&base.join("JPG/IMG_1.JPG"), "earlier jpg");
    write(&base.join("RAW/IMG_1.CR3"), "earlier raw");
    let [jpg, raw, xmp, raw_xmp] = <[String; 4]>::try_from(sources(&base)).unwrap();
    let pair = ArchivePairInput {
        jpg_path: jpg,
        raw_path: Some(raw),
        status: "starred".to_string(),
        source: PhotoSource::Jpg,
        xmp_paths: vec![xmp, raw_xmp],
    };

    archive_pairs(&base, &[pair], &cache, &mut |_| {}).unwrap();

    assert_eq!(
        fs::read_to_string(base.join("JPG/IMG_1.JPG")).unwrap(),
        "earlier jpg"
    );
    assert_eq!(
        fs::read_to_string(base.join("RAW/IMG_1.CR3")).unwrap(),
        "earlier raw"
    );
    for (dest, original) in [
        ("JPG/img_1_1.jpg", "img_1.jpg"),
        ("RAW/IMG_1_1.CR3", "IMG_1.CR3"),
        ("RAW/Img_1_1.xmp", "Img_1.xmp"),
        ("RAW/img_1_1.cr3.xmp", "img_1.cr3.xmp"),
    ] {
        assert_eq!(fs::read_to_string(base.join(dest)).unwrap(), original);
    }
    assert_rescanned_as_one_pair(
        &rescan(&base),
        "img_1_1.jpg",
        "IMG_1_1.CR3",
        ["Img_1_1.xmp", "img_1_1.cr3.xmp"],
    );
}

#[test]
fn exported_mixed_case_pair_keeps_each_stem_and_one_suffix_and_pairs_on_rescan() {
    let src = tempdir().unwrap();
    let target = tempdir().unwrap();
    write(&target.path().join("IMG_1.CR3"), "other card");
    let [jpg, raw, xmp, raw_xmp] = <[String; 4]>::try_from(sources(src.path())).unwrap();
    let pair = ExportPairInput {
        jpg_path: jpg,
        raw_path: Some(raw),
        source: PhotoSource::Jpg,
        xmp_paths: vec![xmp, raw_xmp],
    };

    export_pairs(&[pair], target.path(), |_| {}).unwrap();

    let read = |name: &str| fs::read_to_string(target.path().join(name)).unwrap();
    assert_eq!(read("IMG_1.CR3"), "other card");
    assert_eq!(read("img_1_1.jpg"), "img_1.jpg");
    assert_eq!(read("IMG_1_1.CR3"), "IMG_1.CR3");
    assert_eq!(read("Img_1_1.xmp"), "Img_1.xmp");
    assert_eq!(read("img_1_1.cr3.xmp"), "img_1.cr3.xmp");
    assert_rescanned_as_one_pair(
        &rescan(target.path()),
        "img_1_1.jpg",
        "IMG_1_1.CR3",
        ["Img_1_1.xmp", "img_1_1.cr3.xmp"],
    );
}
