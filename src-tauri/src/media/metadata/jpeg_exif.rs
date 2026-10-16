// ============================================================
// Sift - EXIF Location in a JPEG
// Walks the JPEG's segment headers, seeking over each payload, to the
// APP1/EXIF segment. Callers then read only the parts of its TIFF they
// need: a camera's APP1 also carries a thumbnail worth tens of KB.
// ============================================================

use std::io::{Read, Seek, SeekFrom};

const EXIF_HEADER: &[u8; 6] = b"Exif\0\0";
/// Cameras put APP1 first or nearly so; this many segments in means a corrupt file
const MAX_SEGMENTS: usize = 64;

/// File offset of the EXIF TIFF in the JPEG that starts `at` bytes into
/// the reader; None if the image data starts before any EXIF
pub fn find_exif_tiff<R: Read + Seek>(reader: &mut R, at: u64) -> Option<u64> {
    reader.seek(SeekFrom::Start(at)).ok()?;
    if read_array::<2, _>(reader)? != [0xFF, 0xD8] {
        return None;
    }
    for _ in 0..MAX_SEGMENTS {
        let marker = next_marker(reader)?;
        match marker {
            0xD9 | 0xDA => return None,     // EOI / SOS: image data starts
            0x01 | 0xD0..=0xD7 => continue, // standalone markers
            _ => {}
        }
        let payload_len = u64::from(u16::from_be_bytes(read_array(reader)?).checked_sub(2)?);
        let payload_at = reader.stream_position().ok()?;
        let may_be_exif = marker == 0xE1 && payload_len >= EXIF_HEADER.len() as u64;
        if may_be_exif && &read_array::<6, _>(reader)? == EXIF_HEADER {
            return Some(payload_at + EXIF_HEADER.len() as u64);
        }
        reader
            .seek(SeekFrom::Start(payload_at + payload_len))
            .ok()?;
    }
    None
}

fn next_marker<R: Read>(reader: &mut R) -> Option<u8> {
    let [first] = read_array(reader)?;
    if first != 0xFF {
        return None;
    }
    // Any number of 0xFF fill bytes may precede the marker code
    loop {
        match read_array(reader)? {
            [0xFF] => continue,
            [code] => return Some(code),
        }
    }
}

fn read_array<const N: usize, R: Read>(reader: &mut R) -> Option<[u8; N]> {
    let mut bytes = [0u8; N];
    reader.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{jpeg_with_exif, noise_jpeg};
    use std::io::Cursor;

    fn app_segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, marker];
        out.extend(((payload.len() + 2) as u16).to_be_bytes());
        out.extend(payload);
        out
    }

    fn find(file: &[u8]) -> Option<u64> {
        find_exif_tiff(&mut Cursor::new(file), 0)
    }

    #[test]
    fn finds_the_tiff_of_the_first_app1() {
        let jpeg = jpeg_with_exif(&noise_jpeg(16, 16), b"II*\0tiff");
        let at = find(&jpeg).unwrap() as usize;
        assert_eq!(&jpeg[at..at + 4], b"II*\0");
    }

    #[test]
    fn skips_other_segments_and_non_exif_app1() {
        let mut file = vec![0xFF, 0xD8];
        file.extend(app_segment(0xE0, b"JFIF\0\x01\x02"));
        file.extend(app_segment(0xE1, b"http://ns.adobe.com/xap/1.0/\0<x/>"));
        file.extend([0xFF, 0xFF]); // fill bytes before the next marker
        file.extend(app_segment(0xE1, b"Exif\0\0MM\0*"));
        file.extend([0xFF, 0xDA]);

        let at = find(&file).unwrap() as usize;
        assert_eq!(&file[at..at + 4], b"MM\0*");
    }

    #[test]
    fn is_none_when_the_image_data_comes_first() {
        assert_eq!(find(&noise_jpeg(16, 16)), None);
    }

    #[test]
    fn is_none_for_a_segment_running_past_the_end() {
        let mut file = vec![0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF];
        file.extend([0u8; 16]);
        assert_eq!(find(&file), None);
        assert_eq!(find(b"not a jpeg"), None);
    }
}
