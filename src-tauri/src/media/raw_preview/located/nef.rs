// ============================================================
// Sift - Nikon NEF Preview
// A NEF's IFD0 lists SubIFDs; the large preview (JpgFromRaw) is the JPEG
// one of them points to with JPEGInterchangeFormat / ...Length. Only
// IFD0 and the SubIFD tables are read.
// ============================================================

use crate::media::metadata::ifd::{read_u32, Tiff};
use std::io::{Read, Seek};

const SUB_IFDS: u16 = 0x014A;
const JPEG_OFFSET: u16 = 0x0201;
const JPEG_LENGTH: u16 = 0x0202;
/// NEFs have two or three SubIFDs; more means a corrupt file
const MAX_SUB_IFDS: usize = 8;

/// (offset, length) of each JPEG a SubIFD of IFD0 points to
pub fn preview_regions<R: Read + Seek>(reader: &mut R) -> Vec<(u64, usize)> {
    let Some(mut tiff) = Tiff::open(reader, 0) else {
        return Vec::new();
    };
    let sub_ifds = tiff
        .entries(tiff.ifd0)
        .and_then(|ifd0| {
            let entry = ifd0.iter().find(|e| e.tag == SUB_IFDS)?;
            tiff.value_bytes(entry, MAX_SUB_IFDS * 4)
        })
        .unwrap_or_default();
    let le = tiff.little_endian;
    sub_ifds
        .as_chunks::<4>()
        .0
        .iter()
        .filter_map(|&offset| {
            let entries = tiff.entries(read_u32(offset, le))?;
            let value = |tag| entries.iter().find(|e| e.tag == tag)?.offset_value();
            let offset = value(JPEG_OFFSET)?;
            let length = value(JPEG_LENGTH)?;
            Some((u64::from(offset), usize::try_from(length).ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::nef_with_jpegs;
    use std::io::Cursor;

    fn regions_of(file: &[u8]) -> Vec<&[u8]> {
        preview_regions(&mut Cursor::new(file))
            .into_iter()
            .map(|(at, len)| &file[at as usize..at as usize + len])
            .collect()
    }

    #[test]
    fn finds_the_jpeg_of_each_sub_ifd() {
        let nef = nef_with_jpegs(&[b"\xFF\xD8one", b"\xFF\xD8two"], true);
        assert_eq!(regions_of(&nef), [b"\xFF\xD8one", b"\xFF\xD8two"]);
    }

    #[test]
    fn reads_big_endian_files_with_a_single_sub_ifd() {
        let nef = nef_with_jpegs(&[b"\xFF\xD8one"], false);
        assert_eq!(regions_of(&nef), [b"\xFF\xD8one"]);
    }

    #[test]
    fn finds_nothing_without_sub_ifds() {
        assert!(regions_of(&nef_with_jpegs(&[], true)).is_empty());
        assert!(regions_of(b"not a tiff").is_empty());
    }
}
