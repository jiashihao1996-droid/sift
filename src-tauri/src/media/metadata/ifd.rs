// ============================================================
// Sift - TIFF IFD Reader
// Seeks to one IFD at a time and reads only its entry table and the
// values asked for, so walking a 60MB RAW costs a handful of small reads.
// Offsets and counts come from the file and are bounds-checked.
// ============================================================

use std::io::{Read, Seek, SeekFrom};

pub const EXIF_IFD_POINTER: u16 = 0x8769;

pub const TYPE_ASCII: u16 = 2;
pub const TYPE_LONG: u16 = 4;
pub const TYPE_UNDEFINED: u16 = 7;
pub const TYPE_IFD: u16 = 13;

/// Real IFDs hold a few dozen entries; more means a corrupt file
const MAX_IFD_ENTRIES: usize = 1_024;
/// The longest clock string is DateTimeOriginal's 20 bytes
const MAX_STRING_BYTES: u32 = 64;
const ENTRY_LEN: usize = 12;

pub struct Entry {
    pub tag: u16,
    pub kind: u16,
    pub count: u32,
    /// The 4-byte value field: the value itself when it fits, else an offset
    pub value: [u8; 4],
    pub little_endian: bool,
}

impl Entry {
    pub fn offset_value(&self) -> Option<u32> {
        matches!(self.kind, TYPE_LONG | TYPE_IFD).then(|| read_u32(self.value, self.little_endian))
    }
}

pub struct Tiff<'a, R> {
    reader: &'a mut R,
    pub base: u64,
    pub little_endian: bool,
    pub ifd0: u32,
}

impl<'a, R: Read + Seek> Tiff<'a, R> {
    /// The TIFF structure that starts `base` bytes into `reader`
    pub fn open(reader: &'a mut R, base: u64) -> Option<Self> {
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

    /// The entries of the IFD `ifd` bytes past the TIFF header
    pub fn entries(&mut self, ifd: u32) -> Option<Vec<Entry>> {
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
    pub fn string(&mut self, entries: &[Entry], tag: u16) -> Option<String> {
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

pub fn read_at<R: Read + Seek>(reader: &mut R, at: u64, len: usize) -> Option<Vec<u8>> {
    reader.seek(SeekFrom::Start(at)).ok()?;
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

pub fn read_u16(b: [u8; 2], little_endian: bool) -> u16 {
    if little_endian {
        u16::from_le_bytes(b)
    } else {
        u16::from_be_bytes(b)
    }
}

pub fn read_u32(b: [u8; 4], little_endian: bool) -> u32 {
    if little_endian {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    }
}
