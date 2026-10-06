// ============================================================
// Sift - Capture Time from a TIFF Structure
// Seeks straight to IFD0, the Exif IFD and the few strings that hold
// the capture time, so a 60MB RAW costs a handful of small reads.
// ============================================================

use super::clock;
use crate::media::metadata::ifd::{Tiff, EXIF_IFD_POINTER};
use crate::models::photo::CaptureTime;
use std::io::{Read, Seek};

/// (date/time, sub-seconds, UTC offset) tags: when the shutter fired, then
/// when the image was digitized for files that only record that
const CLOCK_TAGS: [(u16, u16, u16); 2] = [(0x9003, 0x9291, 0x9011), (0x9004, 0x9292, 0x9012)];

/// Read the capture time from a TIFF structure that starts `base` bytes
/// into `reader`. Exif tags normally sit in the Exif IFD that IFD0 points
/// to; a CR3's CMT2 box is an Exif IFD itself, so they may sit in IFD0.
pub fn read_capture_time<R: Read + Seek>(reader: &mut R, base: u64) -> Option<CaptureTime> {
    let mut tiff = Tiff::open(reader, base)?;
    let ifd0 = tiff.entries(tiff.ifd0)?;
    let exif_ifd = match ifd0.iter().find(|e| e.tag == EXIF_IFD_POINTER) {
        Some(pointer) => tiff.entries(pointer.offset_value()?)?,
        None => ifd0,
    };
    CLOCK_TAGS.iter().find_map(|&(datetime, subsec, offset)| {
        let datetime = tiff.string(&exif_ifd, datetime)?;
        let subsec = tiff.string(&exif_ifd, subsec);
        let offset = tiff.string(&exif_ifd, offset);
        clock::parse(&datetime, subsec.as_deref(), offset.as_deref())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{exif_tiff, flat_exif_tiff, ExifByteOrder};
    use std::io::Cursor;

    const DATETIME: u16 = 0x9003;
    const SUBSEC: u16 = 0x9291;
    const OFFSET: u16 = 0x9011;

    fn capture_time(tiff: &[u8]) -> Option<CaptureTime> {
        read_capture_time(&mut Cursor::new(tiff), 0)
    }

    fn local_ms(datetime: &str) -> i64 {
        clock::parse(datetime, None, None).unwrap().local_ms
    }

    #[test]
    fn reads_date_sub_seconds_and_offset_from_the_exif_ifd() {
        let tiff = exif_tiff(
            &[
                (DATETIME, "2026:09:15 10:23:45"),
                (SUBSEC, "37"),
                (OFFSET, "+08:00"),
            ],
            ExifByteOrder::Little,
        );

        let time = capture_time(&tiff).unwrap();

        assert_eq!(time.local_ms, local_ms("2026:09:15 10:23:45") + 370);
        assert_eq!(time.offset_minutes, Some(480));
    }

    #[test]
    fn reads_big_endian_files() {
        let tiff = exif_tiff(&[(DATETIME, "2026:09:15 10:23:45")], ExifByteOrder::Big);
        assert_eq!(
            capture_time(&tiff).map(|t| t.local_ms),
            Some(local_ms("2026:09:15 10:23:45"))
        );
    }

    #[test]
    fn reads_exif_tags_kept_in_ifd0() {
        let tiff = flat_exif_tiff(&[(DATETIME, "2026:09:15 10:23:45")]);
        assert!(capture_time(&tiff).is_some());
    }

    #[test]
    fn falls_back_to_the_digitized_time() {
        let tiff = exif_tiff(
            &[(0x9004, "2026:09:15 10:23:45"), (0x9292, "5")],
            ExifByteOrder::Little,
        );
        assert_eq!(
            capture_time(&tiff).map(|t| t.local_ms),
            Some(local_ms("2026:09:15 10:23:45") + 500)
        );
    }

    #[test]
    fn honours_a_tiff_that_starts_partway_into_the_file() {
        let mut file = vec![0xAB; 100];
        file.extend(exif_tiff(
            &[(DATETIME, "2026:09:15 10:23:45")],
            ExifByteOrder::Little,
        ));
        assert!(read_capture_time(&mut Cursor::new(file), 100).is_some());
    }

    #[test]
    fn is_none_without_a_capture_time() {
        let tiff = exif_tiff(&[(0x010F, "Camera maker")], ExifByteOrder::Little);
        assert_eq!(capture_time(&tiff), None);
        assert_eq!(capture_time(b"not a tiff at all"), None);
    }

    #[test]
    fn is_none_when_offsets_point_past_the_end() {
        let mut tiff = exif_tiff(&[(DATETIME, "2026:09:15 10:23:45")], ExifByteOrder::Little);
        // Point IFD0 far past the end of the file
        tiff[4..8].copy_from_slice(&0x00FF_FFFFu32.to_le_bytes());
        assert_eq!(capture_time(&tiff), None);

        let truncated = exif_tiff(&[(DATETIME, "2026:09:15 10:23:45")], ExifByteOrder::Little);
        assert_eq!(capture_time(&truncated[..truncated.len() - 10]), None);
    }
}
