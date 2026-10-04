// ============================================================
// Sift - JPEG Header Reading
// Reads the segments before the compressed image (EXIF, MPF, frame
// size) without loading the rest of the file, and locates the smaller
// images a camera embeds next to the photo.
// ============================================================

use super::exif_orientation::{parse_tiff_header, read_u16, read_u32};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};

/// MPF tag listing the images in the file (CIPA DC-007)
const MP_ENTRY_TAG: u16 = 0xB002;
const MP_ENTRY_LEN: usize = 16;
/// MPF image types for "large thumbnail" previews (VGA and full-HD classes)
const MP_TYPE_LARGE_THUMBNAIL: [u32; 2] = [0x010001, 0x010002];
const MP_TYPE_MASK: u32 = 0x00FF_FFFF;

/// Byte range of an embedded image, as absolute file offsets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Default)]
pub struct JpegHeader {
    /// TIFF payload of the first APP1/EXIF segment and its file offset
    exif: Option<(u64, Vec<u8>)>,
    /// TIFF payload of the APP2/MPF segment and its file offset
    mpf: Option<(u64, Vec<u8>)>,
    /// Stored (pre-orientation) width and height from the frame header
    pub dimensions: Option<(u32, u32)>,
}

impl JpegHeader {
    /// The EXIF TIFF payload, for an EXIF parser
    pub fn exif_tiff(&self) -> Option<&[u8]> {
        self.exif.as_ref().map(|(_, tiff)| tiff.as_slice())
    }

    /// Converts an offset inside the EXIF TIFF payload to a file offset
    pub fn exif_range(&self, offset: u64, len: u64) -> Option<ImageRange> {
        let (tiff_offset, _) = self.exif.as_ref()?;
        Some(ImageRange {
            offset: tiff_offset + offset,
            len,
        })
    }

    /// MPF large-thumbnail previews, largest first
    pub fn mpf_previews(&self) -> Vec<ImageRange> {
        let Some((tiff_offset, tiff)) = &self.mpf else {
            return Vec::new();
        };
        let mut previews: Vec<ImageRange> = mp_entries(tiff)
            .into_iter()
            .filter(|e| e.offset != 0 && MP_TYPE_LARGE_THUMBNAIL.contains(&e.image_type))
            .map(|e| ImageRange {
                offset: tiff_offset + u64::from(e.offset),
                len: u64::from(e.size),
            })
            .collect();
        previews.sort_by_key(|r| std::cmp::Reverse(r.len));
        previews
    }
}

/// Read the header segments of the JPEG at the reader's start
pub fn read_header<R: Read + Seek>(reader: R) -> io::Result<JpegHeader> {
    let mut reader = BufReader::new(reader);
    let mut soi = [0u8; 2];
    reader.read_exact(&mut soi)?;
    if soi != [0xFF, 0xD8] {
        return Err(invalid("not a JPEG"));
    }

    let mut header = JpegHeader::default();
    loop {
        let marker = next_marker(&mut reader)?;
        match marker {
            0xD9 | 0xDA => return Ok(header), // EOI / SOS: image data starts
            0x01 | 0xD0..=0xD7 => continue,   // standalone markers
            _ => {}
        }
        let mut len_bytes = [0u8; 2];
        reader.read_exact(&mut len_bytes)?;
        let payload_len = u16::from_be_bytes(len_bytes)
            .checked_sub(2)
            .ok_or_else(|| invalid("bad segment length"))?;
        let payload_offset = reader.stream_position()?;
        let mut payload = vec![0u8; usize::from(payload_len)];
        reader.read_exact(&mut payload)?;
        record_segment(&mut header, marker, payload_offset, payload);
    }
}

/// Read the bytes of an embedded image
pub fn read_range<R: Read + Seek>(mut reader: R, range: ImageRange) -> io::Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(range.offset))?;
    let mut bytes = Vec::new();
    reader.take(range.len).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != range.len {
        return Err(invalid("embedded image runs past the end of the file"));
    }
    Ok(bytes)
}

fn next_marker<R: BufRead>(reader: &mut R) -> io::Result<u8> {
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte)?;
    if byte[0] != 0xFF {
        return Err(invalid("expected a marker"));
    }
    // Any number of 0xFF fill bytes may precede the marker code
    while byte[0] == 0xFF {
        reader.read_exact(&mut byte)?;
    }
    Ok(byte[0])
}

fn record_segment(header: &mut JpegHeader, marker: u8, offset: u64, payload: Vec<u8>) {
    match marker {
        0xE1 if header.exif.is_none() && payload.starts_with(b"Exif\0\0") => {
            header.exif = Some((offset + 6, payload[6..].to_vec()));
        }
        0xE2 if header.mpf.is_none() && payload.starts_with(b"MPF\0") => {
            header.mpf = Some((offset + 4, payload[4..].to_vec()));
        }
        // SOF0-SOF15 except DHT (C4), JPG (C8) and DAC (CC)
        0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) && payload.len() >= 5 => {
            let height = u16::from_be_bytes([payload[1], payload[2]]);
            let width = u16::from_be_bytes([payload[3], payload[4]]);
            header.dimensions = Some((u32::from(width), u32::from(height)));
        }
        _ => {}
    }
}

struct MpEntry {
    image_type: u32,
    size: u32,
    /// Relative to the MPF TIFF header; 0 for the primary image
    offset: u32,
}

fn mp_entries(tiff: &[u8]) -> Vec<MpEntry> {
    let Some((le, ifd)) = parse_tiff_header(tiff) else {
        return Vec::new();
    };
    let Some(count) = tiff.get(ifd..ifd + 2).map(|b| read_u16(b, le)) else {
        return Vec::new();
    };
    let entry_tag = (0..usize::from(count))
        .map(|i| ifd + 2 + i * 12)
        .filter_map(|at| tiff.get(at..at + 12))
        .find(|entry| read_u16(&entry[0..2], le) == MP_ENTRY_TAG);
    let Some(entry_tag) = entry_tag else {
        return Vec::new();
    };
    let table_len = read_u32(&entry_tag[4..8], le) as usize;
    let table_at = read_u32(&entry_tag[8..12], le) as usize;
    let Some(table) = tiff.get(table_at..table_at.saturating_add(table_len)) else {
        return Vec::new();
    };
    table
        .as_chunks::<MP_ENTRY_LEN>()
        .0
        .iter()
        .map(|e| MpEntry {
            image_type: read_u32(&e[0..4], le) & MP_TYPE_MASK,
            size: read_u32(&e[4..8], le),
            offset: read_u32(&e[8..12], le),
        })
        .collect()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{jpeg_with_mpf_preview, solid_jpeg};
    use std::io::Cursor;

    #[test]
    fn reads_frame_dimensions() {
        let header = read_header(Cursor::new(solid_jpeg(64, 48, [9, 9, 9]))).unwrap();
        assert_eq!(header.dimensions, Some((64, 48)));
        assert!(header.mpf_previews().is_empty());
    }

    #[test]
    fn locates_mpf_large_thumbnail() {
        let preview = solid_jpeg(32, 24, [0, 0, 255]);
        let file = jpeg_with_mpf_preview(&solid_jpeg(64, 48, [255, 0, 0]), &preview);

        let header = read_header(Cursor::new(&file)).unwrap();
        let ranges = header.mpf_previews();

        assert_eq!(ranges.len(), 1);
        assert_eq!(read_range(Cursor::new(&file), ranges[0]).unwrap(), preview);
    }

    #[test]
    fn rejects_non_jpeg() {
        assert!(read_header(Cursor::new(b"II*\0 not a jpeg".to_vec())).is_err());
    }

    #[test]
    fn rejects_range_past_end_of_file() {
        let range = ImageRange {
            offset: 10,
            len: 1_000,
        };
        assert!(read_range(Cursor::new(vec![0u8; 100]), range).is_err());
    }
}
