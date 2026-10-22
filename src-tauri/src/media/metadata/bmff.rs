// ============================================================
// Sift - ISO Base Media Boxes
// Canon CR3 is an ISO base media file: a tree of boxes, each a size and
// a four-letter kind. Only box headers are read while walking it; sizes
// come from the file and are checked against the parent box.
// ============================================================

use std::io::{Read, Seek, SeekFrom};

/// The boxes before the one we want number in the tens; more means a corrupt file
const MAX_BOXES_PER_LEVEL: usize = 256;

pub struct BoxHeader {
    pub kind: [u8; 4],
    /// File offset of the box's contents, past its header
    pub payload: u64,
    /// File offset just past the box
    pub end: u64,
}

/// The first box between `start` and `end` (the contents of a parent box,
/// or the whole file) that `is_wanted`
pub fn find_box<R: Read + Seek>(
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

/// The 16-byte usertype that opens the contents of a `uuid` box at `at`
pub fn read_usertype<R: Read + Seek>(reader: &mut R, at: u64) -> Option<[u8; 16]> {
    reader.seek(SeekFrom::Start(at)).ok()?;
    let mut usertype = [0u8; 16];
    reader.read_exact(&mut usertype).ok()?;
    Some(usertype)
}

fn read_box_header<R: Read + Seek>(reader: &mut R, at: u64, end: u64) -> Option<BoxHeader> {
    reader.seek(SeekFrom::Start(at)).ok()?;
    let mut head = [0u8; 8];
    reader.read_exact(&mut head).ok()?;
    let size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
    let kind = [head[4], head[5], head[6], head[7]];
    let (header_len, size) = match size {
        0 => (8, end.checked_sub(at)?), // runs to the end of its parent
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::bmff_box;
    use std::io::Cursor;

    #[test]
    fn finds_a_child_past_its_siblings() {
        let mut moov = bmff_box(b"mvhd", b"header");
        moov.extend(bmff_box(b"trak", b"track"));
        let mut file = bmff_box(b"ftyp", b"crx ");
        file.extend(bmff_box(b"moov", &moov));
        let mut reader = Cursor::new(&file);
        let end = file.len() as u64;

        let moov = find_box(&mut reader, 0, end, |b, _| &b.kind == b"moov").unwrap();
        let trak = find_box(&mut reader, moov.payload, moov.end, |b, _| {
            &b.kind == b"trak"
        });

        let trak = trak.unwrap();
        assert_eq!(&file[trak.payload as usize..trak.end as usize], b"track");
    }

    #[test]
    fn reads_64_bit_box_sizes() {
        let mut file = 1u32.to_be_bytes().to_vec();
        file.extend(b"mdat");
        file.extend(20u64.to_be_bytes());
        file.extend(b"data");
        let end = file.len() as u64;

        let mdat = find_box(&mut Cursor::new(&file), 0, end, |_, _| true).unwrap();

        assert_eq!((mdat.payload, mdat.end), (16, 20));
    }

    #[test]
    fn is_none_for_a_box_that_overruns_its_parent() {
        let mut file = bmff_box(b"moov", &[0u8; 8]);
        file[0..4].copy_from_slice(&1_000u32.to_be_bytes());
        let end = file.len() as u64;
        assert!(find_box(&mut Cursor::new(&file), 0, end, |_, _| true).is_none());
    }
}
