// ============================================================
// Sift - Canon CR3 Metadata Box
// A CR3 is an ISO base media file: moov > uuid (Canon) > CMT2 holds
// the Exif IFD. Only box headers are read on the way there
// (metadata/bmff.rs).
// ============================================================

use crate::media::metadata::bmff::{find_box, read_usertype};
use std::io::{Read, Seek, SeekFrom};

/// usertype of the uuid box Canon keeps its metadata boxes in
const CANON_UUID: [u8; 16] = [
    0x85, 0xC0, 0xB6, 0x87, 0x82, 0x0F, 0x11, 0xE0, 0x81, 0x11, 0xF4, 0xCE, 0x46, 0x2B, 0x6A, 0x48,
];

/// File offset of the TIFF structure in a CR3's CMT2 box
pub fn find_canon_cmt2<R: Read + Seek>(reader: &mut R) -> Option<u64> {
    let file_end = reader.seek(SeekFrom::End(0)).ok()?;
    let moov = find_box(reader, 0, file_end, |b, _| &b.kind == b"moov")?;
    let canon = find_box(reader, moov.payload, moov.end, |b, r| {
        &b.kind == b"uuid" && read_usertype(r, b.payload).is_some_and(|u| u == CANON_UUID)
    })?;
    let cmt2 = find_box(reader, canon.payload + 16, canon.end, |b, _| {
        &b.kind == b"CMT2"
    })?;
    Some(cmt2.payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{bmff_box, cr3_with_cmt2};
    use std::io::Cursor;

    #[test]
    fn finds_the_cmt2_payload() {
        let cr3 = cr3_with_cmt2(b"II*\0payload");
        let at = find_canon_cmt2(&mut Cursor::new(&cr3)).unwrap() as usize;
        assert_eq!(&cr3[at..at + 4], b"II*\0");
    }

    #[test]
    fn skips_uuid_boxes_of_other_vendors() {
        let mut other = vec![0u8; 16];
        other.extend(bmff_box(b"CMT2", b"wrong"));
        let mut moov = bmff_box(b"uuid", &other);
        let mut canon = CANON_UUID.to_vec();
        canon.extend(bmff_box(b"CMT2", b"right"));
        moov.extend(bmff_box(b"uuid", &canon));
        let file = bmff_box(b"moov", &moov);

        let at = find_canon_cmt2(&mut Cursor::new(&file)).unwrap() as usize;
        assert_eq!(&file[at..at + 5], b"right");
    }

    #[test]
    fn is_none_for_boxes_that_overrun_their_parent() {
        let mut file = bmff_box(b"moov", &[0u8; 8]);
        file[0..4].copy_from_slice(&1_000u32.to_be_bytes());
        assert_eq!(find_canon_cmt2(&mut Cursor::new(&file)), None);
    }

    #[test]
    fn is_none_without_a_canon_box() {
        let file = bmff_box(b"moov", &bmff_box(b"trak", b"data"));
        assert_eq!(find_canon_cmt2(&mut Cursor::new(&file)), None);
    }
}
