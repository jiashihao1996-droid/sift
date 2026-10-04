// ============================================================
// Sift - Organize → Rescan Round Trips
// Archive and export must leave files that a rescan pairs up again, even
// when a pair's JPG, RAW and XMPs spell their stem in different cases, and
// an archive that fails halfway must leave the folder rescanning as before.
// ============================================================

use super::archive::{archive_pairs, archive_pairs_moving, PICKS_DIR};
use super::export::export_pairs;
use super::transfer::move_file;
use crate::catalog::scan::scan_folder;
use crate::models::photo::{
    ArchivePairInput, ExportPairInput, PhotoPair, PhotoSource, PhotoStatus,
};
use std::fs;
use std::path::Path;
use tempfile::{tempdir, TempDir};
use walkdir::WalkDir;

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

fn group_input(base: &Path, status: PhotoStatus) -> ArchivePairInput {
    let [jpg, raw, xmp, raw_xmp] = <[String; 4]>::try_from(sources(base)).unwrap();
    ArchivePairInput {
        jpg_path: jpg,
        raw_path: Some(raw),
        status,
        source: PhotoSource::Jpg,
        xmp_paths: vec![xmp, raw_xmp],
    }
}

fn archive_group(base: &Path, status: PhotoStatus) {
    let cache = tempdir().unwrap();
    let pair = group_input(base, status);
    archive_pairs(base, &[pair], cache.path(), &mut |_| {}).unwrap();
}

/// Archive the mixed-case group with its RAW failing to move, as on a card
/// that fills up or is pulled out halfway. Returns the error.
fn archive_group_failing_raw(base: &Path, status: PhotoStatus) -> String {
    let cache = tempdir().unwrap();
    let pair = group_input(base, status);
    let raw = pair.raw_path.clone().unwrap();
    let mut move_unless_raw = |src: &Path, dest: &Path| {
        if src == Path::new(&raw) {
            return Err("No space left on device".to_string());
        }
        move_file(src, dest)
    };
    archive_pairs_moving(
        base,
        &[pair],
        cache.path(),
        &mut |_| {},
        &mut move_unless_raw,
    )
    .unwrap_err()
}

/// What a rescan of `dir` shows: each entry's files, then the skipped files.
fn scan_view(dir: &Path) -> (Vec<Vec<String>>, Vec<String>) {
    let cache = tempdir().unwrap();
    let result = scan_folder(dir, cache.path()).unwrap();
    let mut entries: Vec<Vec<String>> = result
        .pairs
        .iter()
        .map(|p| {
            let mut files = vec![p.jpg_path.clone()];
            files.extend(p.raw_path.clone());
            files.extend(p.xmp_paths.iter().cloned());
            files.sort();
            files
        })
        .collect();
    entries.sort();
    let skipped = result.skipped.into_iter().map(|f| f.path).collect();
    (entries, skipped)
}

fn empty_files(dir: &Path) -> Vec<String> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.metadata().unwrap().len() == 0)
        .map(|e| e.path().display().to_string())
        .collect()
}

fn assert_contents(dir: &Path, moved: [(&str, &str); 4]) {
    for (dest, original) in moved {
        assert_eq!(fs::read_to_string(dir.join(dest)).unwrap(), original);
    }
}

#[test]
fn archived_mixed_case_pick_keeps_each_stem_and_one_suffix_and_pairs_on_rescan() {
    let base = TempDir::new().unwrap();
    let picks = base.path().join(PICKS_DIR);
    // An earlier archive already holds IMG_1, so the new group must move on to _1
    write(&picks.join("IMG_1.JPG"), "earlier jpg");

    archive_group(base.path(), PhotoStatus::Starred);

    assert_eq!(
        fs::read_to_string(picks.join("IMG_1.JPG")).unwrap(),
        "earlier jpg"
    );
    assert_contents(
        &picks,
        [
            ("img_1_1.jpg", "img_1.jpg"),
            ("IMG_1_1.CR3", "IMG_1.CR3"),
            ("Img_1_1.xmp", "Img_1.xmp"),
            ("img_1_1.cr3.xmp", "img_1.cr3.xmp"),
        ],
    );
    assert_rescanned_as_one_pair(
        &rescan(&picks),
        "img_1_1.jpg",
        "IMG_1_1.CR3",
        ["Img_1_1.xmp", "img_1_1.cr3.xmp"],
    );
}

#[test]
fn archived_unmarked_pair_pairs_its_jpg_in_place_with_its_raw_on_rescan() {
    let base = TempDir::new().unwrap();
    // RAW/ already holds another card's IMG_1, so the group moves on to _1
    // and the JPG left in place is renamed to match
    write(&base.path().join("RAW/IMG_1.CR3"), "earlier raw");

    archive_group(base.path(), PhotoStatus::Unprocessed);

    assert_contents(
        base.path(),
        [
            ("img_1_1.jpg", "img_1.jpg"),
            ("RAW/IMG_1_1.CR3", "IMG_1.CR3"),
            ("RAW/Img_1_1.xmp", "Img_1.xmp"),
            ("RAW/img_1_1.cr3.xmp", "img_1.cr3.xmp"),
        ],
    );
    assert_rescanned_as_one_pair(
        &rescan(base.path()),
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

#[test]
fn a_pick_whose_raw_fails_to_move_is_put_back_whole() {
    let base = TempDir::new().unwrap();
    sources(base.path());
    let before = scan_view(base.path());

    let err = archive_group_failing_raw(base.path(), PhotoStatus::Starred);

    assert!(err.contains("No space left on device"), "{}", err);
    // The JPG had already moved into 精选/ when the RAW failed
    assert!(err.contains("put 1 moved file(s) back"), "{}", err);
    assert_eq!(scan_view(base.path()), before, "JPG and RAW pair up again");
    let picks: Vec<_> = WalkDir::new(base.path().join(PICKS_DIR))
        .min_depth(1)
        .into_iter()
        .collect();
    assert!(picks.is_empty(), "nothing left in 精选/: {:?}", picks);
}

#[test]
fn a_renamed_group_that_fails_leaves_no_empty_placeholder_behind() {
    let base = TempDir::new().unwrap();
    // RAW/ already holds another card's IMG_1, so the group moves on to _1:
    // the JPG is renamed in place and the RAW claims RAW/IMG_1_1.CR3
    write(&base.path().join("RAW/IMG_1.CR3"), "earlier raw");
    sources(base.path());
    let before = scan_view(base.path());

    archive_group_failing_raw(base.path(), PhotoStatus::Unprocessed);

    assert_eq!(scan_view(base.path()), before);
    assert_eq!(empty_files(base.path()), Vec::<String>::new());
    assert_eq!(
        fs::read_to_string(base.path().join("img_1.jpg")).unwrap(),
        "img_1.jpg",
        "the JPG renamed in place got its name back"
    );
}
