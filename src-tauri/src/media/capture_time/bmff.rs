// ============================================================
// Sift - Canon CR3 Metadata Box
// A CR3 is an ISO base media file: moov > uuid (Canon) > CMT2 holds
// the Exif IFD. Only box headers are read on the way there.
// ============================================================

use std::io::{Read, Seek, SeekFrom};

/// usertype of the uuid box Canon keeps its metadata boxes in
const CANON_UUID: [u8; 16] = [
    0x85, 0xC0, 0xB6, 0x87, 0x82, 0x0F, 0x11, 0xE0, 0x81, 0x11, 0xF4, 0xCE, 0x46, 0x2B, 0x6A, 0x48,
];
/// The boxes before the one we want number in the tens; more means a corrupt file
const MAX_BOXES_PER_LEVEL: usize = 256;

struct BoxHeader {
    kind: [u8; 4],
    payload: u64,
    end: u64,
}

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

fn find_box<R: Read + Seek>(
    reader: &mut R,
    start: u64,
    end: u64,
    mut is_wanted: impl FnMut(&BoxHeader, &mut R) -> bool,
) -> Option<BoxHeader> {
    let mut at = start;
    for _ in 0..MAX_BOXES_PER_LEVEL {
        let header = read_box_header(reader, at, end)?;
        if is_wanted(&header, reader) {
            return Some(header);
        }
        at = header.end;
    }
    None
}

fn read_box_header<R: Read + Seek>(reader: &mut R, at: u64, end: u64) -> Option<BoxHeader> {
    reader.seek(SeekFrom::Start(at)).ok()?;
    let mut head = [0u8; 8];
    reader.read_exact(&mut head).ok()?;
    let size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
    let kind = [head[4], head[5], head[6], head[7]];
    let (header_len, size) = match size {
        0 => (8, end - at), // runs to the end of its parent
        1 => {
            let mut large = [0u8; 8];
            reader.read_exact(&mut large).ok()?;
            (16, u64::from_be_bytes(large))
        }
        _ => (8, size),
    };
    let box_end = at.checked_add(size)?;
    (size >= header_len && box_end <= end).then_some(BoxHeader {
        kind,
        payload: at + header_len,
        end: box_end,
    })
}

fn read_usertype<R: Read + Seek>(reader: &mut R, at: u64) -> Option<[u8; 16]> {
    reader.seek(SeekFrom::Start(at)).ok()?;
    let mut usertype = [0u8; 16];
    reader.read_exact(&mut usertype).ok()?;
    Some(usertype)
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
