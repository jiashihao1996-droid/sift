// ============================================================
// Sift - Saved Previews of Unstarred Picks
// Archive saves a RAW-only pick's embedded preview next to its RAW in 精选/
// as <stem>.jpg. When that pick is unstarred, the RAW goes back to RAW/ and
// the preview is dropped instead of being moved into the folder the photo
// came from, which never had a JPG for it. It goes through `Trash` (the
// trash, or for good only where there is none and the user confirmed).
// A JPG counts as that preview only when it is byte for byte the preview
// embedded in its RAW, so dropping it loses nothing: an edited copy, or any
// other JPG, is the user's and moves back like one. A preview saved by a
// version that scanned the whole RAW (CR3, RAF, NEF) counts too.
// ============================================================

use super::archive::is_already_in_dir;
use super::delete_batch::Trash;
use crate::media::raw_preview::{earlier_preview_bytes, raw_preview_bytes};
use crate::models::organize::OrganizeError;
use crate::models::photo::{ArchivePairInput, PhotoSource, PhotoStatus};
use std::fs;
use std::path::Path;

/// The JPG of `pair` when it is the preview an earlier archive saved for a
/// RAW-only pick that the user has now unstarred.
pub(super) fn saved_preview<'a>(pair: &'a ArchivePairInput, picks: &Path) -> Option<&'a Path> {
    if pair.status != PhotoStatus::Unprocessed || pair.source != PhotoSource::Jpg {
        return None;
    }
    let jpg = Path::new(&pair.jpg_path);
    let raw = Path::new(pair.raw_path.as_deref()?);
    let is_in_picks = is_already_in_dir(jpg, picks) && is_already_in_dir(raw, picks);
    if !is_in_picks || jpg.file_stem() != raw.file_stem() {
        return None;
    }
    is_embedded_preview(jpg, raw).then_some(jpg)
}

/// Whether `jpg` holds exactly the preview embedded in `raw`, as this or an
/// earlier version extracts it. Any doubt (unreadable file, no preview)
/// means no: the JPG is then kept.
fn is_embedded_preview(jpg: &Path, raw: &Path) -> bool {
    let raw = raw.to_string_lossy();
    let is_same = |preview: Vec<u8>| {
        let is_same_size = fs::metadata(jpg).is_ok_and(|m| m.len() == preview.len() as u64);
        is_same_size && fs::read(jpg).is_ok_and(|bytes| bytes == preview)
    };
    raw_preview_bytes(&raw).is_ok_and(is_same) || earlier_preview_bytes(&raw).is_some_and(is_same)
}

/// Drop the saved preview of an unstarred pick, if it has one, before its
/// RAW moves. Done first so a failure leaves the pair untouched; if the move
/// fails afterwards, the RAW stays in 精选/ as a pick whose preview is
/// extracted again on the next scan.
pub(super) fn drop_saved_preview(
    pair: &ArchivePairInput,
    picks: &Path,
    trash: &mut dyn Trash,
) -> Result<(), OrganizeError> {
    match saved_preview(pair, picks) {
        Some(preview) => trash
            .trash_one(preview)
            .map_err(|e| e.context("Failed to remove the saved preview")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{cr3_with_jpegs, noise_jpeg, tiff_raw_with_preview, write};
    use tempfile::TempDir;

    /// `<tmp>/精选/DSC_1.ARW` and the JPG `name` holding its exact preview
    fn layout(jpg_name: &str) -> (TempDir, ArchivePairInput) {
        let tmp = TempDir::new().unwrap();
        let preview = noise_jpeg(256, 256);
        let raw = write(
            tmp.path(),
            "精选/DSC_1.ARW",
            &tiff_raw_with_preview(&preview, None),
        );
        let jpg = write(tmp.path(), jpg_name, &preview);
        let pair = ArchivePairInput {
            jpg_path: jpg.to_string_lossy().into_owned(),
            raw_path: Some(raw.to_string_lossy().into_owned()),
            status: PhotoStatus::Unprocessed,
            source: PhotoSource::Jpg,
            xmp_paths: vec![],
        };
        (tmp, pair)
    }

    fn picks(tmp: &TempDir) -> std::path::PathBuf {
        tmp.path().join("精选")
    }

    #[test]
    fn recognizes_the_embedded_preview_of_an_unstarred_pick() {
        let (tmp, pair) = layout("精选/DSC_1.jpg");
        assert_eq!(
            saved_preview(&pair, &picks(&tmp)),
            Some(Path::new(&pair.jpg_path))
        );
    }

    #[test]
    fn keeps_the_preview_of_a_pick_that_is_starred_or_marked_for_deletion() {
        let (tmp, mut pair) = layout("精选/DSC_1.jpg");
        for status in [PhotoStatus::Starred, PhotoStatus::Deleted] {
            pair.status = status;
            assert_eq!(saved_preview(&pair, &picks(&tmp)), None);
        }
    }

    #[test]
    fn only_looks_inside_the_picks_folder() {
        let (tmp, mut pair) = layout("DSC_1.jpg");
        assert_eq!(saved_preview(&pair, &picks(&tmp)), None);

        let raw = write(
            tmp.path(),
            "RAW/DSC_1.ARW",
            &fs::read(pair.raw_path.unwrap()).unwrap(),
        );
        pair.raw_path = Some(raw.to_string_lossy().into_owned());
        assert_eq!(saved_preview(&pair, &picks(&tmp)), None);
    }

    #[test]
    fn a_jpg_named_differently_from_its_raw_is_never_the_preview() {
        let (tmp, pair) = layout("精选/DSC_1_edit.jpg");
        assert_eq!(saved_preview(&pair, &picks(&tmp)), None);
    }

    #[test]
    fn a_jpg_that_differs_by_one_byte_is_kept() {
        let (tmp, pair) = layout("精选/DSC_1.jpg");
        let mut bytes = fs::read(&pair.jpg_path).unwrap();
        let last = bytes.len() - 3;
        bytes[last] ^= 1;
        fs::write(&pair.jpg_path, bytes).unwrap();
        assert_eq!(saved_preview(&pair, &picks(&tmp)), None);
    }

    /// `<tmp>/精选/IMG_1.CR3`, whose largest JPEG sits outside the boxes
    /// the format points to, and `<tmp>/精选/IMG_1.jpg` holding `pick(...)`
    /// of (the preview now extracted, the one earlier versions saved)
    fn cr3_layout(pick: fn(Vec<u8>, Vec<u8>) -> Vec<u8>) -> (TempDir, ArchivePairInput) {
        let tmp = TempDir::new().unwrap();
        let (full, outside) = (noise_jpeg(256, 256), noise_jpeg(320, 320));
        let mut cr3 = cr3_with_jpegs(&full, b"", true);
        cr3.extend(&outside);
        let raw = write(tmp.path(), "精选/IMG_1.CR3", &cr3);
        let jpg = write(tmp.path(), "精选/IMG_1.jpg", &pick(full, outside));
        let pair = ArchivePairInput {
            jpg_path: jpg.to_string_lossy().into_owned(),
            raw_path: Some(raw.to_string_lossy().into_owned()),
            status: PhotoStatus::Unprocessed,
            source: PhotoSource::Jpg,
            xmp_paths: vec![],
        };
        (tmp, pair)
    }

    #[test]
    fn recognizes_the_preview_of_a_cr3_as_extracted_now_or_by_earlier_versions() {
        for pick in [|now, _| now, |_, earlier| earlier] as [fn(_, _) -> _; 2] {
            let (tmp, pair) = cr3_layout(pick);
            assert_eq!(
                saved_preview(&pair, &picks(&tmp)),
                Some(Path::new(&pair.jpg_path))
            );
        }
    }

    #[test]
    fn keeps_any_other_jpg_next_to_a_cr3() {
        let (tmp, pair) = cr3_layout(|_, _| noise_jpeg(300, 300));
        assert_eq!(saved_preview(&pair, &picks(&tmp)), None);
    }

    #[test]
    fn a_raw_without_a_preview_never_matches() {
        let (tmp, pair) = layout("精选/DSC_1.jpg");
        fs::write(pair.raw_path.as_deref().unwrap(), "not a raw").unwrap();
        assert_eq!(saved_preview(&pair, &picks(&tmp)), None);
    }
}
