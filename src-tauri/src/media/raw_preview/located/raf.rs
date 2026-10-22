// ============================================================
// Sift - Fujifilm RAF Preview
// The RAF header records where its embedded JPEG is: a big-endian
// offset at byte 84 and length at byte 88.
// ============================================================

use crate::media::metadata::ifd::read_at;
use std::io::{Read, Seek};

const MAGIC: &[u8; 16] = b"FUJIFILMCCD-RAW ";
const JPEG_OFFSET_AT: usize = 84;
const HEADER_LEN: usize = JPEG_OFFSET_AT + 8;

/// (offset, length) of the JPEG the header points to
pub fn preview_region<R: Read + Seek>(reader: &mut R) -> Option<(u64, usize)> {
    let header = read_at(reader, 0, HEADER_LEN)?;
    if &header[..MAGIC.len()] != MAGIC {
        return None;
    }
    let field = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap_or_default());
    let offset = field(JPEG_OFFSET_AT);
    let length = field(JPEG_OFFSET_AT + 4);
    Some((u64::from(offset), usize::try_from(length).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::raf_with_jpeg;
    use std::io::Cursor;

    #[test]
    fn reads_the_jpeg_offset_and_length_from_the_header() {
        let raf = raf_with_jpeg(b"\xFF\xD8jpeg");
        let (offset, length) = preview_region(&mut Cursor::new(&raf)).unwrap();
        assert_eq!(
            &raf[offset as usize..offset as usize + length],
            b"\xFF\xD8jpeg"
        );
    }

    #[test]
    fn is_none_for_other_files() {
        let mut raf = raf_with_jpeg(b"\xFF\xD8jpeg");
        raf[0] = b'X';
        assert_eq!(preview_region(&mut Cursor::new(&raf)), None);
        assert_eq!(preview_region(&mut Cursor::new(b"FUJIFILM")), None);
    }
}
