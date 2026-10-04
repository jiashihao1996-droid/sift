// ============================================================
// Sift - Capture Time from a TIFF Structure
// Seeks straight to IFD0, the Exif IFD and the few strings that hold
// the capture time, so a 60MB RAW costs a handful of small reads.
// Offsets and counts come from the file and are bounds-checked.
// ============================================================

use super::clock;
use crate::models::photo::CaptureTime;
use std::io::{Read, Seek, SeekFrom};

const EXIF_IFD_POINTER: u16 = 0x8769;
/// (date/time, sub-seconds, UTC offset) tags: when the shutter fired, then
/// when the image was digitized for files that only record that
const CLOCK_TAGS: [(u16, u16, u16); 2] = [(0x9003, 0x9291, 0x9011), (0x9004, 0x9292, 0x9012)];

const TYPE_ASCII: u16 = 2;
const TYPE_LONG: u16 = 4;
const TYPE_UNDEFINED: u16 = 7;
const TYPE_IFD: u16 = 13;

/// Real IFDs hold a few dozen entries; more means a corrupt file
const MAX_IFD_ENTRIES: usize = 1_024;
/// The longest clock string is DateTimeOriginal's 20 bytes
const MAX_STRING_BYTES: u32 = 64;
const ENTRY_LEN: usize = 12;

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

struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    /// The 4-byte value field: the value itself when it fits, else an offset
    value: [u8; 4],
    little_endian: bool,
}

impl Entry {
    fn offset_value(&self) -> Option<u32> {
        matches!(self.kind, TYPE_LONG | TYPE_IFD).then(|| read_u32(self.value, self.little_endian))
    }
}

struct Tiff<'a, R> {
    reader: &'a mut R,
    base: u64,
    little_endian: bool,
    ifd0: u32,
}

impl<'a, R: Read + Seek> Tiff<'a, R> {
    fn open(reader: &'a mut R, base: u64) -> Option<Self> {
        let header: [u8; 8] = read_at(reader, base, 8)?.try_into().ok()?;
        let little_endian = match &header[0..2] {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        // TIFF, Panasonic RW2 (0x55) and Olympus ORF ("RO" / "RS") headers
        let magic = read_u16([header[2], header[3]], little_endian);
        if !matches!(magic, 0x002A | 0x0055 | 0x4F52 | 0x5352) {
            return None;
        }
        let ifd0 = read_u32([header[4], header[5], header[6], header[7]], little_endian);
        Some(Self {
            reader,
            base,
            little_endian,
            ifd0,
        })
    }

    fn entries(&mut self, ifd: u32) -> Option<Vec<Entry>> {
        let at = self.base + u64::from(ifd);
        let count_bytes = read_at(self.reader, at, 2)?;
        let count = usize::from(read_u16(
            [count_bytes[0], count_bytes[1]],
            self.little_endian,
        ));
        if count > MAX_IFD_ENTRIES {
            return None;
        }
        let table = read_at(self.reader, at + 2, count * ENTRY_LEN)?;
        let le = self.little_endian;
        let entries = table
            .as_chunks::<ENTRY_LEN>()
            .0
            .iter()
            .map(|e| Entry {
                tag: read_u16([e[0], e[1]], le),
                kind: read_u16([e[2], e[3]], le),
                count: read_u32([e[4], e[5], e[6], e[7]], le),
                value: [e[8], e[9], e[10], e[11]],
                little_endian: le,
            })
            .collect();
        Some(entries)
    }

    /// An ASCII value, up to its first NUL. Some cameras type the
    /// sub-second tags as UNDEFINED, so those bytes are read too.
    fn string(&mut self, entries: &[Entry], tag: u16) -> Option<String> {
        let entry = entries.iter().find(|e| e.tag == tag)?;
        if !matches!(entry.kind, TYPE_ASCII | TYPE_UNDEFINED) || entry.count > MAX_STRING_BYTES {
            return None;
        }
        let len = entry.count as usize;
        let bytes = if len <= entry.value.len() {
            entry.value[..len].to_vec()
        } else {
            let offset = read_u32(entry.value, self.little_endian);
            read_at(self.reader, self.base + u64::from(offset), len)?
        };
        let text = bytes.split(|&b| b == 0).next().unwrap_or_default();
        Some(String::from_utf8_lossy(text).into_owned())
    }
}

fn read_at<R: Read + Seek>(reader: &mut R, at: u64, len: usize) -> Option<Vec<u8>> {
    reader.seek(SeekFrom::Start(at)).ok()?;
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn read_u16(b: [u8; 2], little_endian: bool) -> u16 {
    if little_endian {
        u16::from_le_bytes(b)
    } else {
        u16::from_be_bytes(b)
    }
}

fn read_u32(b: [u8; 4], little_endian: bool) -> u32 {
    if little_endian {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    }
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
