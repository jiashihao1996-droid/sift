// ============================================================
// Sift - Archiving Again After Opening a Folder With 精选/
// Scan loads 精选/ as starred picks; the next archive must leave the ones
// still starred alone, move unstarred ones back out (JPG to the folder,
// RAW and XMP to RAW/) and drop only the preview it once saved itself, so
// a rescan always shows what the user last marked.
// ============================================================

use super::archive::{archive_pairs, archive_pairs_moving, PICKS_DIR, RAW_DIR};
use super::delete_batch::delete_pairs;
use super::export::export_pairs;
use super::test_trash::BinTrash;
use crate::catalog::scan::scan_folder;
use crate::models::organize::{DeletePairInput, OrganizeError, OrganizeErrorKind};
use crate::models::photo::{
    ArchivePairInput, ExportPairInput, PhotoPair, PhotoSource, PhotoStatus,
};
use crate::test_support::{noise_jpeg, tiff_raw_with_preview, write};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

/// A photo folder plus a preview cache outside it
struct Folder {
    tmp: TempDir,
}

impl Folder {
    fn new(files: &[(&str, &str)]) -> Self {
        let folder = Self {
            tmp: TempDir::new().unwrap(),
        };
        fs::create_dir_all(folder.base()).unwrap();
        fs::create_dir_all(folder.cache()).unwrap();
        for (name, contents) in files {
            folder.put(name, contents.as_bytes());
        }
        folder
    }

    fn base(&self) -> PathBuf {
        self.tmp.path().join("photos")
    }

    fn cache(&self) -> PathBuf {
        self.tmp.path().join("cache")
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.base().join(rel)
    }

    fn put(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        write(&self.base(), rel, bytes)
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap()
    }

    fn scan(&self) -> Vec<PhotoPair> {
        scan_folder(&self.base(), &self.cache()).unwrap().pairs
    }

    /// The scanned pair whose JPG (or, RAW-only, RAW) is `rel`. Paths are
    /// compared by component: on Windows `rel`'s `/` meets the scan's `\`.
    fn pair(&self, rel: &str) -> PhotoPair {
        let path = self.path(rel);
        self.scan()
            .into_iter()
            .find(|p| {
                Path::new(&p.jpg_path) == path
                    || p.raw_path.as_deref().map(Path::new) == Some(&path)
            })
            .unwrap_or_else(|| panic!("no pair for {}", rel))
    }

    fn archive(&self, pairs: &[ArchivePairInput], trash: &mut BinTrash) -> usize {
        self.try_archive(pairs, trash).unwrap()
    }

    fn try_archive(
        &self,
        pairs: &[ArchivePairInput],
        trash: &mut BinTrash,
    ) -> Result<usize, OrganizeError> {
        archive_pairs(&self.base(), pairs, &self.cache(), trash, &mut |_| {}).map(|r| r.moved_count)
    }
}

/// What the frontend sends for a scanned pair the user marked `status`
fn input(pair: &PhotoPair, status: PhotoStatus) -> ArchivePairInput {
    ArchivePairInput {
        jpg_path: pair.jpg_path.clone(),
        raw_path: pair.raw_path.clone(),
        status,
        source: pair.source.clone(),
        xmp_paths: pair.xmp_paths.clone(),
    }
}

fn file_names(paths: &[String]) -> Vec<&str> {
    let mut names: Vec<&str> = paths
        .iter()
        .map(|p| Path::new(p).file_name().unwrap().to_str().unwrap())
        .collect();
    names.sort();
    names
}

const PICK: [(&str, &str); 3] = [
    ("精选/DSC_1.JPG", "pick jpg"),
    ("精选/DSC_1.ARW", "pick raw"),
    ("精选/DSC_1.xmp", "pick xmp"),
];

#[test]
fn a_pick_still_starred_stays_in_the_picks_folder_untouched() {
    // Same names in the folder itself and in RAW/: nothing may be renamed
    let folder = Folder::new(&[
        PICK[0],
        PICK[1],
        PICK[2],
        ("DSC_1.JPG", "other jpg"),
        ("RAW/DSC_1.ARW", "other raw"),
    ]);
    let pick = folder.pair("精选/DSC_1.JPG");
    assert_eq!(pick.status, PhotoStatus::Starred);
    assert!(pick.is_archived_pick);

    let moved = folder.archive(&[input(&pick, PhotoStatus::Starred)], &mut BinTrash::new());

    assert_eq!(moved, 0);
    for (rel, contents) in PICK {
        assert_eq!(folder.read(rel), contents);
    }
    assert_eq!(folder.read("DSC_1.JPG"), "other jpg");
    assert!(folder.pair("精选/DSC_1.JPG").is_archived_pick);
}

#[test]
fn an_unstarred_pick_goes_back_with_its_jpg_in_place_and_raw_in_raw_folder() {
    let folder = Folder::new(&PICK);
    let pick = folder.pair("精选/DSC_1.JPG");
    let mut trash = BinTrash::new();

    let moved = folder.archive(&[input(&pick, PhotoStatus::Unprocessed)], &mut trash);

    assert_eq!(moved, 2);
    assert!(trash.trashed.is_empty(), "the user's JPG is never dropped");
    assert_eq!(folder.read("DSC_1.JPG"), "pick jpg");
    assert_eq!(folder.read("RAW/DSC_1.ARW"), "pick raw");
    assert_eq!(folder.read("RAW/DSC_1.xmp"), "pick xmp");
    assert!(!folder.path(PICKS_DIR).join("DSC_1.JPG").exists());

    let rescanned = folder.pair("DSC_1.JPG");
    assert_eq!(rescanned.status, PhotoStatus::Unprocessed);
    assert!(!rescanned.is_archived_pick);
    assert_eq!(file_names(&rescanned.xmp_paths), ["DSC_1.xmp"]);
    assert!(rescanned
        .raw_path
        .map(PathBuf::from)
        .unwrap()
        .ends_with("RAW/DSC_1.ARW"));
}

#[test]
fn an_unstarred_jpg_only_pick_takes_its_xmp_back_to_the_folder() {
    let folder = Folder::new(&[PICK[0], PICK[2]]);
    let pick = folder.pair("精选/DSC_1.JPG");

    folder.archive(
        &[input(&pick, PhotoStatus::Unprocessed)],
        &mut BinTrash::new(),
    );

    assert_eq!(folder.read("DSC_1.JPG"), "pick jpg");
    assert_eq!(folder.read("DSC_1.xmp"), "pick xmp");
    assert_eq!(
        file_names(&folder.pair("DSC_1.JPG").xmp_paths),
        ["DSC_1.xmp"]
    );
}

#[test]
fn an_unstarred_pick_never_overwrites_and_keeps_one_suffix_across_folders() {
    let folder = Folder::new(&[
        PICK[0],
        PICK[1],
        PICK[2],
        ("DSC_1.JPG", "other jpg"),
        ("RAW/DSC_1.ARW", "other raw"),
    ]);
    let pick = folder.pair("精选/DSC_1.JPG");

    folder.archive(
        &[input(&pick, PhotoStatus::Unprocessed)],
        &mut BinTrash::new(),
    );

    assert_eq!(folder.read("DSC_1.JPG"), "other jpg");
    assert_eq!(folder.read("RAW/DSC_1.ARW"), "other raw");
    assert_eq!(folder.read("DSC_1_1.JPG"), "pick jpg");
    assert_eq!(folder.read("RAW/DSC_1_1.ARW"), "pick raw");
    assert_eq!(folder.read("RAW/DSC_1_1.xmp"), "pick xmp");

    let back = folder.pair("DSC_1_1.JPG");
    assert!(back
        .raw_path
        .map(PathBuf::from)
        .unwrap()
        .ends_with("RAW/DSC_1_1.ARW"));
    assert!(folder
        .pair("DSC_1.JPG")
        .raw_path
        .map(PathBuf::from)
        .unwrap()
        .ends_with("RAW/DSC_1.ARW"));
}

/// A RAW-only photo archived as a pick: its RAW plus the preview archive saved
fn raw_only_pick(folder: &Folder) -> (PathBuf, PathBuf) {
    let preview = noise_jpeg(256, 256);
    let raw = folder.put("精选/DSC_2.ARW", &tiff_raw_with_preview(&preview, None));
    let jpg = folder.put("精选/DSC_2.jpg", &preview);
    (raw, jpg)
}

#[test]
fn an_unstarred_raw_only_pick_drops_its_saved_preview_and_rescans_raw_only() {
    let folder = Folder::new(&[]);
    let (_, preview) = raw_only_pick(&folder);
    let pick = folder.pair("精选/DSC_2.jpg");
    let mut trash = BinTrash::new();

    let moved = folder.archive(&[input(&pick, PhotoStatus::Unprocessed)], &mut trash);

    assert_eq!(moved, 1, "only the RAW moved");
    assert_eq!(trash.trashed, [preview]);
    assert!(folder.path("RAW/DSC_2.ARW").exists());
    assert!(
        !folder.path("DSC_2.jpg").exists(),
        "no JPG appears in the folder"
    );
    assert!(fs::read_dir(folder.path(PICKS_DIR))
        .unwrap()
        .next()
        .is_none());

    let back = folder.pair("RAW/DSC_2.ARW");
    assert_eq!(back.source, PhotoSource::RawPreview);
    assert!(!back.is_archived_pick);
}

#[test]
fn a_jpg_that_differs_from_the_embedded_preview_is_the_users_and_goes_back() {
    let folder = Folder::new(&[]);
    let (_, jpg) = raw_only_pick(&folder);
    fs::write(&jpg, "edited in another app").unwrap();
    let pick = folder.pair("精选/DSC_2.jpg");
    let mut trash = BinTrash::new();

    folder.archive(&[input(&pick, PhotoStatus::Unprocessed)], &mut trash);

    assert!(trash.trashed.is_empty());
    assert_eq!(folder.read("DSC_2.jpg"), "edited in another app");
    assert!(folder.path("RAW/DSC_2.ARW").exists());
}

#[test]
fn a_starred_raw_only_pick_keeps_its_saved_preview() {
    let folder = Folder::new(&[]);
    let (raw, jpg) = raw_only_pick(&folder);
    let pick = folder.pair("精选/DSC_2.jpg");
    let mut trash = BinTrash::new();

    folder.archive(&[input(&pick, PhotoStatus::Starred)], &mut trash);

    assert!(trash.trashed.is_empty());
    assert!(raw.exists() && jpg.exists());
}

#[test]
fn a_preview_that_cannot_be_dropped_stops_the_archive_with_nothing_moved() {
    // A volume without a trash, permanent deletion not confirmed
    let folder = Folder::new(&[]);
    let (raw, jpg) = raw_only_pick(&folder);
    let pick = folder.pair("精选/DSC_2.jpg");

    let err = folder
        .try_archive(
            &[input(&pick, PhotoStatus::Unprocessed)],
            &mut BinTrash::refusing(),
        )
        .unwrap_err();

    assert_eq!(err.kind, OrganizeErrorKind::TrashUnsupported);
    assert!(raw.exists() && jpg.exists());
    assert!(!folder.path(RAW_DIR).exists());
}

#[test]
fn a_raw_that_fails_to_move_after_its_preview_went_stays_a_pick() {
    let folder = Folder::new(&[]);
    let (raw, jpg) = raw_only_pick(&folder);
    let pick = folder.pair("精选/DSC_2.jpg");
    let mut move_never = |src: &Path, _: &Path| -> Result<(), OrganizeError> {
        Err(OrganizeError::other(format!(
            "card pulled out: {}",
            src.display()
        )))
    };

    let result = archive_pairs_moving(
        &folder.base(),
        &[input(&pick, PhotoStatus::Unprocessed)],
        &folder.cache(),
        &mut BinTrash::new(),
        &mut |_| {},
        &mut move_never,
    );

    assert!(result.is_err());
    assert!(raw.exists(), "the RAW is back in 精选/");
    assert!(!jpg.exists());
    let rescanned = folder.pair("精选/DSC_2.ARW");
    assert_eq!(rescanned.status, PhotoStatus::Starred);
    assert_eq!(rescanned.source, PhotoSource::RawPreview);
}

#[test]
fn a_pick_marked_for_deletion_is_trashed_from_the_picks_folder() {
    let folder = Folder::new(&PICK);
    let pick = folder.pair("精选/DSC_1.JPG");
    let mut trash = BinTrash::new();
    let request = DeletePairInput {
        id: pick.id.clone(),
        jpg_path: pick.jpg_path.clone(),
        raw_path: pick.raw_path.clone(),
        source: pick.source.clone(),
        xmp_paths: pick.xmp_paths.clone(),
    };

    let result = delete_pairs(&[request], &folder.cache(), &mut trash, &mut |_| {});

    assert_eq!(result.trashed, [pick.id]);
    assert_eq!(trash.trashed.len(), 3);
    assert!(folder.scan().is_empty());
}

#[test]
fn export_copies_archived_picks_too() {
    let folder = Folder::new(&PICK);
    let pick = folder.pair("精选/DSC_1.JPG");
    let target = tempdir().unwrap();
    let request = ExportPairInput {
        jpg_path: pick.jpg_path.clone(),
        raw_path: pick.raw_path.clone(),
        source: pick.source.clone(),
        xmp_paths: pick.xmp_paths.clone(),
    };

    let copied = export_pairs(&[request], target.path(), |_| {}).unwrap();

    assert_eq!(copied, 2);
    for (rel, contents) in PICK {
        assert_eq!(folder.read(rel), contents, "the pick itself stays");
    }
    assert!(target.path().join("DSC_1.ARW").exists());
}

#[test]
fn archive_open_again_unstar_archive_and_rescan_round_trip() {
    let folder = Folder::new(&[
        ("DSC_1.JPG", "jpg 1"),
        ("DSC_1.ARW", "raw 1"),
        ("DSC_2.JPG", "jpg 2"),
        ("DSC_2.ARW", "raw 2"),
    ]);
    let first: Vec<ArchivePairInput> = folder
        .scan()
        .iter()
        .map(|p| input(p, PhotoStatus::Starred))
        .collect();
    folder.archive(&first, &mut BinTrash::new());

    let reopened = folder.scan();
    assert!(reopened
        .iter()
        .all(|p| p.status == PhotoStatus::Starred && p.is_archived_pick));
    let second: Vec<ArchivePairInput> = reopened
        .iter()
        .map(|p| match p.jpg_path.ends_with("DSC_2.JPG") {
            true => input(p, PhotoStatus::Unprocessed),
            false => input(p, PhotoStatus::Starred),
        })
        .collect();
    folder.archive(&second, &mut BinTrash::new());

    assert_eq!(folder.read("精选/DSC_1.JPG"), "jpg 1");
    assert_eq!(folder.read("DSC_2.JPG"), "jpg 2");
    assert_eq!(folder.read("RAW/DSC_2.ARW"), "raw 2");
    let one = folder.pair("精选/DSC_1.JPG");
    let two = folder.pair("DSC_2.JPG");
    assert_eq!(one.status, PhotoStatus::Starred);
    assert_eq!(two.status, PhotoStatus::Unprocessed);
    assert!(two
        .raw_path
        .map(PathBuf::from)
        .unwrap()
        .ends_with("RAW/DSC_2.ARW"));
}

/// What a rescan pairs: (JPG or RAW-only RAW, RAW) file names relative to the folder
fn pairing_of(folder: &Folder) -> Vec<(String, Option<String>)> {
    // Joined with `/` on every platform, like the expected paths
    let rel = |p: &str| {
        let rel = Path::new(p).strip_prefix(folder.base()).unwrap();
        let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
        parts.join("/")
    };
    let mut pairs: Vec<(String, Option<String>)> = folder
        .scan()
        .iter()
        .map(|p| match p.source {
            PhotoSource::Jpg => (rel(&p.jpg_path), p.raw_path.as_deref().map(rel)),
            PhotoSource::RawPreview => (rel(p.raw_path.as_deref().unwrap()), None),
        })
        .collect();
    pairs.sort();
    pairs
}

fn raw_with_preview() -> Vec<u8> {
    tiff_raw_with_preview(&noise_jpeg(256, 256), None)
}

#[test]
fn an_unstarred_raw_never_pairs_with_a_different_jpg_of_its_name() {
    // A loose JPG-only photo and a RAW-only pick that happen to share a name
    let folder = Folder::new(&[("dsc_2.jpg", "loose jpg")]);
    raw_only_pick(&folder);
    let pick = folder.pair("精选/DSC_2.jpg");

    folder.archive(
        &[input(&pick, PhotoStatus::Unprocessed)],
        &mut BinTrash::new(),
    );

    assert_eq!(
        pairing_of(&folder),
        [
            ("RAW/DSC_2_1.ARW".to_string(), None),
            ("dsc_2.jpg".to_string(), None),
        ]
    );
}

#[test]
fn an_unstarred_jpg_never_pairs_with_a_different_raw_of_its_name() {
    let folder = Folder::new(&[("精选/DSC_3.JPG", "pick jpg")]);
    folder.put("RAW/DSC_3.ARW", &raw_with_preview());
    let pick = folder.pair("精选/DSC_3.JPG");

    folder.archive(
        &[input(&pick, PhotoStatus::Unprocessed)],
        &mut BinTrash::new(),
    );

    assert_eq!(folder.read("DSC_3_1.JPG"), "pick jpg");
    assert_eq!(
        pairing_of(&folder),
        [
            ("DSC_3_1.JPG".to_string(), None),
            ("RAW/DSC_3.ARW".to_string(), None),
        ]
    );
}

#[test]
fn a_new_pick_never_pairs_with_a_different_photo_already_in_the_picks_folder() {
    // A RAW-only pick whose saved preview is gone, and a new JPG-only pick
    let folder = Folder::new(&[("DSC_4.JPG", "new pick")]);
    folder.put("精选/DSC_4.ARW", &raw_with_preview());
    let new_pick = folder.pair("DSC_4.JPG");
    let old_pick = folder.pair("精选/DSC_4.ARW");

    folder.archive(
        &[
            input(&new_pick, PhotoStatus::Starred),
            input(&old_pick, PhotoStatus::Starred),
        ],
        &mut BinTrash::new(),
    );

    // The old pick keeps its name and gets its preview back next to it
    assert_eq!(folder.read("精选/DSC_4_1.JPG"), "new pick");
    assert_eq!(
        pairing_of(&folder),
        [
            (
                "精选/DSC_4.jpg".to_string(),
                Some("精选/DSC_4.ARW".to_string())
            ),
            ("精选/DSC_4_1.JPG".to_string(), None),
        ]
    );
}

#[test]
fn two_groups_moving_into_one_scope_in_the_same_archive_stay_apart() {
    // Both are unstarred: a RAW-only pick and a JPG-only pick named alike
    let folder = Folder::new(&[("精选/DSC_5.JPG", "jpg-only pick")]);
    folder.put("精选/dsc_5.ARW", &raw_with_preview());
    let jpg_pick = folder.pair("精选/DSC_5.JPG");
    assert_eq!(
        jpg_pick
            .raw_path
            .as_deref()
            .map(|r| r.ends_with("dsc_5.ARW")),
        Some(true)
    );
    // They were one pair in 精选/; split them as two requests to model the
    // order in which archive meets two groups bound for the same scope
    let raw_request = ArchivePairInput {
        jpg_path: String::new(),
        raw_path: jpg_pick.raw_path.clone(),
        status: PhotoStatus::Unprocessed,
        source: PhotoSource::RawPreview,
        xmp_paths: vec![],
    };
    let jpg_request = ArchivePairInput {
        raw_path: None,
        ..input(&jpg_pick, PhotoStatus::Unprocessed)
    };

    folder.archive(&[raw_request, jpg_request], &mut BinTrash::new());

    assert_eq!(
        pairing_of(&folder),
        [
            ("DSC_5_1.JPG".to_string(), None),
            ("RAW/dsc_5.ARW".to_string(), None),
        ]
    );
}
