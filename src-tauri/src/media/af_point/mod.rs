// ============================================================
// Sift - Focus Point
// Reads where the camera focused from its maker notes, touching only
// the start of the file: a JPEG's EXIF segment, or IFD0 and the Exif
// IFD of a TIFF-based RAW. Each brand lays its maker notes out its own
// way, so each has its own file; other brands are Unavailable.
// ============================================================

mod sony;

use crate::media::metadata::head::HeadCached;
use crate::media::metadata::ifd::{Tiff, EXIF_IFD_POINTER, TYPE_UNDEFINED};
use crate::media::metadata::jpeg_exif;
use crate::models::af_point::AfInfo;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

/// Sony's maker notes (about 38KB) end within the first 64KB of both its
/// JPGs and ARWs, so a photo costs a single read
const HEAD_LEN: usize = 64 * 1024;
const MAKE: u16 = 0x010F;
const ORIENTATION: u16 = 0x0112;
const MAKER_NOTE: u16 = 0x927C;

/// Focus data for a photo pair. The JPG is read first; the RAW when the
/// JPG has none (an edited export) or is Sift's own preview of the RAW.
pub fn read_for_pair(jpg: &Path, raw: Option<&Path>, jpg_is_preview: bool) -> AfInfo {
    if !jpg_is_preview {
        let info = read_af_info(jpg);
        if info != AfInfo::Unavailable {
            return info;
        }
    }
    raw.map_or(AfInfo::Unavailable, read_af_info)
}

/// Focus data of one JPEG or TIFF-based RAW
pub fn read_af_info(path: &Path) -> AfInfo {
    File::open(path)
        .and_then(|file| HeadCached::new(file, HEAD_LEN))
        .ok()
        .and_then(|mut file| read(&mut file))
        .unwrap_or(AfInfo::Unavailable)
}

fn read<R: Read + Seek>(file: &mut R) -> Option<AfInfo> {
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).ok()?;
    let base = match magic {
        [0xFF, 0xD8] => jpeg_exif::find_exif_tiff(file, 0)?,
        [b'I', b'I'] | [b'M', b'M'] => 0,
        _ => return None,
    };
    let mut tiff = Tiff::open(file, base)?;
    let ifd0 = tiff.entries(tiff.ifd0)?;
    let make = tiff.string(&ifd0, MAKE)?;
    let orientation = ifd0
        .iter()
        .find(|e| e.tag == ORIENTATION)
        .and_then(|e| tiff.u16_values(e, 1))
        .and_then(|v| v.first().copied())
        .filter(|o| (1..=8).contains(o))
        .unwrap_or(1);
    let exif_ifd = ifd0.iter().find(|e| e.tag == EXIF_IFD_POINTER)?;
    let exif = tiff.entries(exif_ifd.offset_value()?)?;
    let maker_note = exif
        .iter()
        .find(|e| e.tag == MAKER_NOTE && e.kind == TYPE_UNDEFINED)?;

    if make.trim().eq_ignore_ascii_case("SONY") {
        return sony::read(&mut tiff, maker_note.data_offset(), orientation);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{jpeg_with_exif, noise_jpeg, write};
    use tempfile::tempdir;

    #[test]
    fn reads_a_sony_jpeg_and_arw() {
        let dir = tempdir().unwrap();
        let jpg = write(
            dir.path(),
            "DSC_0001.JPG",
            &jpeg_with_exif(&noise_jpeg(16, 16), &sony::tests::jpg_tiff()),
        );
        let arw = write(dir.path(), "DSC_0001.ARW", &sony::tests::arw_tiff());

        assert_eq!(read_af_info(&jpg), sony::tests::expected());
        assert_eq!(read_af_info(&arw), sony::tests::expected());
    }

    #[test]
    fn falls_back_to_the_raw_when_the_jpg_has_no_focus_data() {
        let dir = tempdir().unwrap();
        let plain = write(dir.path(), "edited.jpg", &noise_jpeg(16, 16));
        let arw = write(dir.path(), "DSC_0001.ARW", &sony::tests::arw_tiff());

        assert_eq!(
            read_for_pair(&plain, Some(&arw), false),
            sony::tests::expected()
        );
        assert_eq!(read_for_pair(&plain, None, false), AfInfo::Unavailable);
    }

    #[test]
    fn reads_only_the_raw_for_a_raw_preview() {
        let dir = tempdir().unwrap();
        let preview = write(
            dir.path(),
            "preview.jpg",
            &jpeg_with_exif(&noise_jpeg(16, 16), &sony::tests::jpg_tiff()),
        );
        assert_eq!(read_for_pair(&preview, None, true), AfInfo::Unavailable);
    }

    #[test]
    fn is_unavailable_for_other_files() {
        let dir = tempdir().unwrap();
        let other = sony::tests::tiff_with_make("Canon", Vec::new(), false);
        for (name, bytes) in [
            ("IMG_0001.CR2", other),
            ("plain.jpg", noise_jpeg(16, 16)),
            ("unknown.x3f", vec![0x42; 64]),
            ("empty.jpg", Vec::new()),
        ] {
            let path = write(dir.path(), name, &bytes);
            assert_eq!(read_af_info(&path), AfInfo::Unavailable, "{name}");
        }
        assert_eq!(
            read_af_info(&dir.path().join("gone.jpg")),
            AfInfo::Unavailable
        );
    }

    /// Checks the parser against real photos: point SIFT_AF_SAMPLES at a
    /// folder of camera JPGs and RAWs, then
    /// `cargo test af_point::tests::reads_real_samples -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reads_real_samples() {
        let dir = std::env::var("SIFT_AF_SAMPLES").expect("set SIFT_AF_SAMPLES");
        let mut found = 0;
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let info = read_af_info(&entry.path());
            println!("{}: {info:?}", entry.path().display());
            found += usize::from(matches!(info, AfInfo::Found { .. }));
        }
        assert!(found > 0, "no sample had focus data");
    }
}
