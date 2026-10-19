// ============================================================
// Sift - Embedded JPEGs of a TIFF-Based RAW
// Finds the outer Orientation and the JPEGs that the IFD chain points to
// by reading only the IFD tables, not the whole file.
// Earlier versions got these from kamadak-exif, which loads the whole
// file; this walk accepts and rejects exactly the files it does and picks
// the same values, so the preview chosen for a RAW never changes. That
// matters: archive saves the preview next to a RAW-only pick, and
// organize/saved_preview.rs recognizes it later by its bytes.
// What kamadak-exif does, and so what this does:
// - only "II*\0" / "MM\0*" headers; IFD0, IFD1, ... up to 8 IFDs
// - any value whose bytes run past the end of the file, a 9th IFD, or a
//   broken Exif / GPS / Interop pointer rejects the whole file
// - tags are read from the chain IFDs themselves, the last one wins
// ============================================================

use crate::media::metadata::ifd::{read_at, read_u16, read_u32};
use std::io::{Read, Seek};

const ORIENTATION: u16 = 0x0112;
const JPEG_OFFSET: u16 = 0x0201;
const JPEG_LENGTH: u16 = 0x0202;
const EXIF_POINTER: u16 = 0x8769;
const GPS_POINTER: u16 = 0x8825;
const INTEROP_POINTER: u16 = 0xA005;

const TYPE_BYTE: u16 = 1;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;

const MAX_IFDS: usize = 8;
const ENTRY_LEN: u64 = 12;

/// What the IFD chain says about a RAW's embedded JPEGs
#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    /// IFD0's Orientation, when it is a SHORT
    pub orientation: Option<u16>,
    /// (offset, length) of each IFD's JPEG, in IFD order
    pub jpegs: Vec<(u64, usize)>,
}

/// The layout of the TIFF at the start of `reader`, a file of `file_len`
/// bytes; None when it isn't a TIFF or kamadak-exif would reject it
pub fn read_layout<R: Read + Seek>(reader: &mut R, file_len: u64) -> Option<Layout> {
    let header = read_at(reader, 0, 8)?;
    let little_endian = match &header[..4] {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let mut walk = Walk {
        reader,
        little_endian,
        file_len,
    };
    let mut ifds = Vec::new();
    let mut next = read_u32([header[4], header[5], header[6], header[7]], little_endian);
    while next != 0 {
        if ifds.len() == MAX_IFDS {
            return None;
        }
        let (entries, after) = walk.ifd(next, Context::Tiff)?;
        ifds.push(entries);
        next = after;
    }

    let orientation = match ifds.first().and_then(|ifd| last(ifd, ORIENTATION)) {
        Some(entry) if entry.kind == TYPE_SHORT => walk.first_uint(entry).map(|v| v as u16),
        _ => None,
    };
    let jpegs = ifds
        .iter()
        .filter_map(|ifd| {
            let offset = walk.first_long(last(ifd, JPEG_OFFSET)?)?;
            let length = walk.first_long(last(ifd, JPEG_LENGTH)?)?;
            Some((u64::from(offset), length as usize))
        })
        .collect();
    Some(Layout { orientation, jpegs })
}

#[derive(Clone, Copy, PartialEq)]
enum Context {
    Tiff,
    Exif,
    Other,
}

struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    /// The value itself when it fits in 4 bytes, else where it is
    value: [u8; 4],
}

impl Entry {
    fn value_len(&self) -> u64 {
        unit_len(self.kind) * u64::from(self.count)
    }
}

struct Walk<'a, R> {
    reader: &'a mut R,
    little_endian: bool,
    file_len: u64,
}

impl<R: Read + Seek> Walk<'_, R> {
    /// The plain entries of the IFD at `at` and the offset of the next IFD.
    /// Exif / GPS / Interop IFDs it points to are checked, not returned.
    fn ifd(&mut self, at: u32, context: Context) -> Option<(Vec<Entry>, u32)> {
        let le = self.little_endian;
        let at = u64::from(at);
        let count = read_at(self.reader, at, 2)?;
        let count = u64::from(read_u16([count[0], count[1]], le));
        let table = read_at(
            self.reader,
            at + 2,
            usize::try_from(count * ENTRY_LEN).ok()?,
        )?;
        let entries: Vec<Entry> = table
            .as_chunks::<12>()
            .0
            .iter()
            .map(|e| Entry {
                tag: read_u16([e[0], e[1]], le),
                kind: read_u16([e[2], e[3]], le),
                count: read_u32([e[4], e[5], e[6], e[7]], le),
                value: [e[8], e[9], e[10], e[11]],
            })
            .collect();
        let mut plain = Vec::with_capacity(entries.len());
        for entry in entries {
            if entry.value_len() > 4 {
                let end = u64::from(self.offset(&entry)) + entry.value_len();
                if end > self.file_len {
                    return None;
                }
            }
            match child_context(context, entry.tag) {
                Some(child) => self.child_ifd(&entry, child)?,
                None => plain.push(entry),
            }
        }
        let next = read_at(self.reader, at + 2 + count * ENTRY_LEN, 4)?;
        Some((plain, read_u32([next[0], next[1], next[2], next[3]], le)))
    }

    /// A pointed-to IFD must parse and must not chain to another
    fn child_ifd(&mut self, pointer: &Entry, context: Context) -> Option<()> {
        let at = self.first_uint(pointer)?;
        let (_, next) = self.ifd(at, context)?;
        (next == 0).then_some(())
    }

    /// The first value of a BYTE, SHORT or LONG entry
    fn first_uint(&mut self, entry: &Entry) -> Option<u32> {
        if entry.count == 0 {
            return None;
        }
        let le = self.little_endian;
        let bytes = if entry.value_len() <= 4 {
            entry.value
        } else {
            let at = u64::from(self.offset(entry));
            let unit = usize::try_from(unit_len(entry.kind)).ok()?;
            let mut bytes = read_at(self.reader, at, unit)?;
            bytes.resize(4, 0);
            [bytes[0], bytes[1], bytes[2], bytes[3]]
        };
        match entry.kind {
            TYPE_BYTE => Some(u32::from(bytes[0])),
            TYPE_SHORT => Some(u32::from(read_u16([bytes[0], bytes[1]], le))),
            TYPE_LONG => Some(read_u32(bytes, le)),
            _ => None,
        }
    }

    fn first_long(&mut self, entry: &Entry) -> Option<u32> {
        (entry.kind == TYPE_LONG).then(|| self.first_uint(entry))?
    }

    fn offset(&self, entry: &Entry) -> u32 {
        read_u32(entry.value, self.little_endian)
    }
}

fn last(entries: &[Entry], tag: u16) -> Option<&Entry> {
    entries.iter().rev().find(|e| e.tag == tag)
}

fn child_context(context: Context, tag: u16) -> Option<Context> {
    match (context, tag) {
        (Context::Tiff, EXIF_POINTER) => Some(Context::Exif),
        (Context::Tiff, GPS_POINTER) | (Context::Exif, INTEROP_POINTER) => Some(Context::Other),
        _ => None,
    }
}

/// Bytes per value of a TIFF type; kamadak-exif counts unknown types as 0
fn unit_len(kind: u16) -> u64 {
    match kind {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ifd_bytes, tiff_header, TiffEntry};
    use std::io::{BufReader, Cursor};

    /// What earlier versions read through kamadak-exif from the whole file
    fn kamadak_layout(file: &[u8]) -> Option<Layout> {
        let exif = exif::Reader::new()
            .read_from_container(&mut BufReader::new(Cursor::new(file)))
            .ok()?;
        let long = |tag, ifd| match &exif.get_field(tag, exif::In(ifd))?.value {
            exif::Value::Long(v) => v.first().copied(),
            _ => None,
        };
        let orientation = match &exif
            .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
            .map(|f| &f.value)
        {
            Some(exif::Value::Short(v)) => v.first().copied(),
            _ => None,
        };
        let jpegs = (0..10)
            .filter_map(|ifd| {
                let offset = long(exif::Tag::JPEGInterchangeFormat, ifd)?;
                let length = long(exif::Tag::JPEGInterchangeFormatLength, ifd)?;
                Some((u64::from(offset), length as usize))
            })
            .collect();
        Some(Layout { orientation, jpegs })
    }

    fn layout(file: &[u8]) -> Option<Layout> {
        read_layout(&mut Cursor::new(file), file.len() as u64)
    }

    /// Both readers agree on `file`; returns what they read
    fn agreed(file: &[u8]) -> Option<Layout> {
        let ours = layout(file);
        assert_eq!(ours, kamadak_layout(file));
        ours
    }

    /// A TIFF whose IFDs are chained in order, each laid out after the last;
    /// `next_of_last` lets a test point the last IFD somewhere
    fn chain(ifds: &[Vec<TiffEntry>], le: bool, next_of_last: Option<u32>) -> Vec<u8> {
        let mut file = tiff_header(le);
        for (i, entries) in ifds.iter().enumerate() {
            let at = file.len();
            let mut ifd = ifd_bytes(entries, at, le);
            let next_at = 2 + entries.len() * 12;
            let next = if i + 1 == ifds.len() {
                next_of_last.unwrap_or(0)
            } else {
                (at + ifd.len()) as u32
            };
            let next = if le {
                next.to_le_bytes()
            } else {
                next.to_be_bytes()
            };
            ifd[next_at..next_at + 4].copy_from_slice(&next);
            file.extend(ifd);
        }
        file
    }

    fn jpeg_entries(offset: u32, length: u32, le: bool) -> Vec<TiffEntry> {
        vec![
            TiffEntry::long(JPEG_OFFSET, offset, le),
            TiffEntry::long(JPEG_LENGTH, length, le),
        ]
    }

    fn padded(mut file: Vec<u8>, len: usize) -> Vec<u8> {
        file.resize(len, 0xAB);
        file
    }

    #[test]
    fn reads_orientation_and_the_jpeg_of_every_ifd() {
        for le in [true, false] {
            let mut ifd0 = vec![TiffEntry::short(ORIENTATION, &[6], le)];
            ifd0.extend(jpeg_entries(4_000, 2_000, le));
            let file = padded(
                chain(&[ifd0, jpeg_entries(1_000, 500, le)], le, None),
                8_000,
            );

            let read = agreed(&file).unwrap();

            assert_eq!(read.orientation, Some(6));
            assert_eq!(read.jpegs, vec![(4_000, 2_000), (1_000, 500)]);
        }
    }

    #[test]
    fn skips_ifds_without_a_jpeg() {
        let le = true;
        let ifds = [vec![TiffEntry::byte(0x00FE, 1)], jpeg_entries(500, 100, le)];
        let read = agreed(&padded(chain(&ifds, le, None), 1_000)).unwrap();
        assert_eq!(read.orientation, None);
        assert_eq!(read.jpegs, vec![(500, 100)]);
    }

    #[test]
    fn reads_only_long_offsets_and_short_orientation() {
        let le = true;
        let ifd0 = vec![
            TiffEntry::long(ORIENTATION, 6, le),
            TiffEntry::short(JPEG_OFFSET, &[500], le),
            TiffEntry::long(JPEG_LENGTH, 100, le),
        ];
        let read = agreed(&padded(chain(&[ifd0], le, None), 1_000)).unwrap();
        assert_eq!(
            read,
            Layout {
                orientation: None,
                jpegs: vec![]
            }
        );
    }

    #[test]
    fn the_last_of_repeated_tags_wins() {
        let le = true;
        let mut ifd0 = jpeg_entries(500, 100, le);
        ifd0.extend(jpeg_entries(600, 200, le));
        ifd0.push(TiffEntry::short(ORIENTATION, &[3], le));
        ifd0.push(TiffEntry::long(ORIENTATION, 8, le));
        let read = agreed(&padded(chain(&[ifd0], le, None), 1_000)).unwrap();
        assert_eq!(
            read,
            Layout {
                orientation: None,
                jpegs: vec![(600, 200)]
            }
        );
    }

    #[test]
    fn reads_values_stored_away_from_the_entry() {
        let le = false;
        let ifd0 = vec![
            TiffEntry::short(ORIENTATION, &[8, 1, 1], le),
            TiffEntry::raw(JPEG_OFFSET, TYPE_LONG, 2, [0, 0, 1, 0, 0, 0, 0, 9].to_vec()),
            TiffEntry::raw(
                JPEG_LENGTH,
                TYPE_LONG,
                2,
                [0, 0, 0, 50, 0, 0, 0, 9].to_vec(),
            ),
        ];
        let read = agreed(&padded(chain(&[ifd0], le, None), 1_000)).unwrap();
        assert_eq!(read.orientation, Some(8));
        assert_eq!(read.jpegs, vec![(256, 50)]);
    }

    #[test]
    fn checks_exif_gps_and_interop_ifds() {
        let le = true;
        let interop = ifd_bytes(&[TiffEntry::ascii(0x0001, "R98")], 300, le);
        let exif = ifd_bytes(&[TiffEntry::long(INTEROP_POINTER, 300, le)], 200, le);
        let gps = ifd_bytes(&[TiffEntry::byte(0x0000, 2)], 400, le);
        let mut ifd0 = jpeg_entries(600, 100, le);
        ifd0.push(TiffEntry::long(EXIF_POINTER, 200, le));
        ifd0.push(TiffEntry::long(GPS_POINTER, 400, le));
        let mut file = padded(chain(&[ifd0], le, None), 1_000);
        file[200..200 + exif.len()].copy_from_slice(&exif);
        file[300..300 + interop.len()].copy_from_slice(&interop);
        file[400..400 + gps.len()].copy_from_slice(&gps);

        assert_eq!(agreed(&file).unwrap().jpegs, vec![(600, 100)]);

        // An Exif IFD that chains to another IFD rejects the file
        let mut chained = file.clone();
        let next_at = 200 + 2 + 12;
        chained[next_at..next_at + 4].copy_from_slice(&300u32.to_le_bytes());
        assert_eq!(agreed(&chained), None);

        // So does an Interop pointer past the end
        let mut broken = file;
        let pointer_value_at = 200 + 2 + 8;
        broken[pointer_value_at..pointer_value_at + 4].copy_from_slice(&5_000u32.to_le_bytes());
        assert_eq!(agreed(&broken), None);
    }

    #[test]
    fn rejects_a_value_running_past_the_end_of_the_file() {
        let le = true;
        let mut ifd0 = jpeg_entries(500, 100, le);
        ifd0.push(TiffEntry::undefined(0x927C, vec![0; 64]));
        let mut file = padded(chain(&[ifd0], le, None), 1_000);
        assert!(agreed(&file).is_some());

        // Point the maker note at the last 32 bytes
        let value_at = 8 + 2 + 2 * 12 + 8;
        file[value_at..value_at + 4].copy_from_slice(&968u32.to_le_bytes());
        assert_eq!(agreed(&file), None);
    }

    #[test]
    fn rejects_more_than_eight_ifds() {
        let le = true;
        let ifds: Vec<_> = (0..9).map(|i| jpeg_entries(i * 10, 5, le)).collect();
        assert_eq!(agreed(&padded(chain(&ifds, le, None), 2_000)), None);
        assert_eq!(
            agreed(&padded(chain(&ifds[..8], le, None), 2_000))
                .unwrap()
                .jpegs
                .len(),
            8
        );
    }

    #[test]
    fn rejects_truncated_and_looping_chains() {
        let le = true;
        let file = chain(&[jpeg_entries(500, 100, le)], le, Some(8));
        assert_eq!(
            agreed(&padded(file, 1_000)),
            None,
            "IFD0 points back at itself"
        );

        let file = chain(&[jpeg_entries(500, 100, le)], le, None);
        assert_eq!(
            agreed(&file[..file.len() - 2]),
            None,
            "next-IFD offset cut off"
        );
        assert_eq!(agreed(&file[..20]), None, "entry table cut off");
    }

    #[test]
    fn rejects_files_that_are_not_plain_tiff() {
        assert_eq!(agreed(b"FUJIFILMCCD-RAW 0201"), None);
        assert_eq!(agreed(b"II"), None);
        // Panasonic RW2 / Olympus ORF magic numbers
        let mut rw2 = padded(chain(&[jpeg_entries(500, 100, true)], true, None), 1_000);
        rw2[2] = 0x55;
        assert_eq!(agreed(&rw2), None);
    }

    /// Opt-in check that real camera files read the same as through
    /// kamadak-exif: `SIFT_RAW_SAMPLES=/path/to/raws npm run test:raw-samples`
    #[test]
    #[ignore = "needs SIFT_RAW_SAMPLES pointing at a folder of real RAW files"]
    fn real_raw_layouts_match_kamadak_exif() {
        let samples = std::env::var("SIFT_RAW_SAMPLES").expect("set SIFT_RAW_SAMPLES");
        let mut checked = 0;
        for entry in std::fs::read_dir(&samples).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            let is_raw_file = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(crate::catalog::file_types::is_raw);
            if !is_raw_file {
                continue;
            }
            checked += 1;
            let read = agreed(&std::fs::read(&path).unwrap());
            println!("ok   {}: {read:?}", path.display());
        }
        assert!(checked > 0, "no RAW files found in {samples}");
    }
}
