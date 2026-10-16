// ============================================================
// Sift - Capture Time
// Reads when a photo was taken from just the bytes that record it,
// never the whole file (scanning a folder reads every photo):
// - JPEG: the TIFF in its APP1/EXIF segment, read the same way as a RAW
// - TIFF-based RAW (ARW, NEF, CR2, DNG, ORF, RW2, PEF, ...): IFD0 and
//   the Exif IFD, by seeking
// - Fujifilm RAF: the EXIF of the JPEG embedded at the header's offset
// - Canon CR3: the CMT2 box (an Exif IFD) inside the Canon uuid box
// The start of the file is read once and the walk runs in memory, so a
// photo usually costs a single read.
// Other formats, and files without a set clock, have no capture time.
// ============================================================

mod bmff;
mod clock;
mod tiff;

use crate::media::metadata::head::HeadCached;
use crate::media::metadata::jpeg_exif;
use crate::models::photo::CaptureTime;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const RAF_MAGIC: &[u8; SNIFF_LEN] = b"FUJIFILMCCD-RAW ";
/// RAF header field holding the embedded JPEG's offset (big-endian u32)
const RAF_JPEG_OFFSET_AT: u64 = 84;
const SNIFF_LEN: usize = 16;
/// Covers the EXIF of JPEGs and the IFDs of TIFF-based RAWs in practice;
/// anything further is still read, just with extra reads
const HEAD_LEN: usize = 16 * 1024;

/// When the photo at `path` was taken; None when the file doesn't record
/// it or can't be read (both just leave the photo without a time)
pub fn read_capture_time(path: &Path) -> Option<CaptureTime> {
    let mut file = HeadCached::new(File::open(path).ok()?, HEAD_LEN).ok()?;
    let mut head = [0u8; SNIFF_LEN];
    file.read_exact(&mut head).ok()?;
    match head {
        [0xFF, 0xD8, ..] => from_jpeg(&mut file, 0),
        [b'I', b'I', ..] | [b'M', b'M', ..] => tiff::read_capture_time(&mut file, 0),
        _ if &head == RAF_MAGIC => from_raf(&mut file),
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => {
            let cmt2 = bmff::find_canon_cmt2(&mut file)?;
            tiff::read_capture_time(&mut file, cmt2)
        }
        _ => None,
    }
}

/// The EXIF of a JPEG that starts `at` bytes into the reader
fn from_jpeg<R: Read + Seek>(reader: &mut R, at: u64) -> Option<CaptureTime> {
    let tiff_at = jpeg_exif::find_exif_tiff(reader, at)?;
    tiff::read_capture_time(reader, tiff_at)
}

fn from_raf<R: Read + Seek>(reader: &mut R) -> Option<CaptureTime> {
    reader.seek(SeekFrom::Start(RAF_JPEG_OFFSET_AT)).ok()?;
    let mut offset = [0u8; 4];
    reader.read_exact(&mut offset).ok()?;
    from_jpeg(reader, u64::from(u32::from_be_bytes(offset)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        cr3_with_cmt2, exif_tiff, flat_exif_tiff, jpeg_with_exif, noise_jpeg, raf_with_jpeg,
        tiff_raw_with_preview, write, ExifByteOrder,
    };
    use tempfile::tempdir;

    const TAKEN: &str = "2026:09:15 10:23:45";

    fn taken_ms() -> i64 {
        clock::parse(TAKEN, None, None).unwrap().local_ms
    }

    fn exif() -> Vec<u8> {
        exif_tiff(&[(0x9003, TAKEN)], ExifByteOrder::Little)
    }

    fn read(name: &str, bytes: &[u8]) -> Option<i64> {
        let dir = tempdir().unwrap();
        read_capture_time(&write(dir.path(), name, bytes)).map(|t| t.local_ms)
    }

    #[test]
    fn reads_a_jpeg() {
        let jpeg = jpeg_with_exif(&noise_jpeg(64, 64), &exif());
        assert_eq!(read("DSC_0001.JPG", &jpeg), Some(taken_ms()));
    }

    #[test]
    fn reads_a_tiff_based_raw() {
        assert_eq!(read("DSC_0001.ARW", &exif()), Some(taken_ms()));
    }

    #[test]
    fn reads_a_raf_through_its_embedded_jpeg() {
        let jpeg = jpeg_with_exif(&noise_jpeg(64, 64), &exif());
        assert_eq!(
            read("DSCF0001.RAF", &raf_with_jpeg(&jpeg)),
            Some(taken_ms())
        );
    }

    #[test]
    fn reads_a_cr3_through_its_cmt2_box() {
        let cr3 = cr3_with_cmt2(&flat_exif_tiff(&[(0x9003, TAKEN)]));
        assert_eq!(read("IMG_0001.CR3", &cr3), Some(taken_ms()));
    }

    #[test]
    fn is_none_for_photos_that_do_not_record_it() {
        assert_eq!(read("plain.jpg", &noise_jpeg(64, 64)), None);
        assert_eq!(
            read(
                "no_exif.nef",
                &tiff_raw_with_preview(&noise_jpeg(64, 64), None)
            ),
            None
        );
        assert_eq!(read("unknown.x3f", &[0x42; 64]), None);
        assert_eq!(read("tiny.jpg", &[0xFF, 0xD8]), None);
    }

    #[test]
    fn is_none_for_a_missing_file() {
        let dir = tempdir().unwrap();
        assert_eq!(read_capture_time(&dir.path().join("gone.jpg")), None);
    }
}
