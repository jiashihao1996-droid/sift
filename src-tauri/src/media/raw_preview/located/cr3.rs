// ============================================================
// Sift - Canon CR3 Previews
// A CR3 keeps two JPEGs: the full-size one, the first track's only
// sample (moov > trak > mdia > minf > stbl: its size in stsz, its offset
// in co64 or stco), and a smaller one in the PRVW uuid box. Only box
// headers and those two tables are read.
// ============================================================

use crate::media::metadata::bmff::{find_box, find_child, read_usertype, BoxHeader};
use crate::media::metadata::ifd::read_at;
use std::io::{Read, Seek};

/// usertype of the uuid box holding the PRVW preview
const PREVIEW_UUID: [u8; 16] = [
    0xEA, 0xF4, 0x2B, 0x5E, 0x1C, 0x98, 0x4B, 0x88, 0xB9, 0xFB, 0xB7, 0xDC, 0x40, 0x6E, 0x4D, 0x16,
];
const USERTYPE_LEN: u64 = 16;
/// version and flags of a full box
const FULL_BOX_LEN: u64 = 4;

/// (offset, length) of each region that holds one of the CR3's JPEGs
pub fn preview_regions<R: Read + Seek>(reader: &mut R, file_len: u64) -> Vec<(u64, usize)> {
    let full_size = find_box(reader, 0, file_len, |b, _| &b.kind == b"moov")
        .and_then(|moov| first_track_sample(reader, &moov));
    let preview = find_box(reader, 0, file_len, |b, r| {
        &b.kind == b"uuid" && read_usertype(r, b.payload).is_some_and(|u| u == PREVIEW_UUID)
    })
    .and_then(|uuid| {
        let start = uuid.payload + USERTYPE_LEN;
        Some((start, usize::try_from(uuid.end.checked_sub(start)?).ok()?))
    });
    full_size.into_iter().chain(preview).collect()
}

/// Where the first track's first sample is, and how long it is
fn first_track_sample<R: Read + Seek>(reader: &mut R, moov: &BoxHeader) -> Option<(u64, usize)> {
    let trak = find_child(reader, moov, b"trak")?;
    let mdia = find_child(reader, &trak, b"mdia")?;
    let minf = find_child(reader, &mdia, b"minf")?;
    let stbl = find_child(reader, &minf, b"stbl")?;
    let stsz = find_child(reader, &stbl, b"stsz")?;
    let size = first_sample_size(reader, &stsz)?;
    let offset = match find_child(reader, &stbl, b"co64") {
        Some(co64) => first_chunk_offset(reader, &co64, 8)?,
        None => {
            let stco = find_child(reader, &stbl, b"stco")?;
            first_chunk_offset(reader, &stco, 4)?
        }
    };
    Some((offset, usize::try_from(size).ok()?))
}

/// stsz: one size for every sample, or 0 and then a size per sample
fn first_sample_size<R: Read + Seek>(reader: &mut R, stsz: &BoxHeader) -> Option<u32> {
    let table = read_at(reader, stsz.payload + FULL_BOX_LEN, 8)?;
    let size = u32::from_be_bytes(table[..4].try_into().ok()?);
    let count = u32::from_be_bytes(table[4..].try_into().ok()?);
    match (size, count) {
        (_, 0) => None,
        (0, _) => {
            let first = read_at(reader, stsz.payload + FULL_BOX_LEN + 8, 4)?;
            Some(u32::from_be_bytes(first.try_into().ok()?))
        }
        (size, _) => Some(size),
    }
}

/// co64 / stco: a count, then 8- / 4-byte chunk offsets
fn first_chunk_offset<R: Read + Seek>(
    reader: &mut R,
    table: &BoxHeader,
    width: usize,
) -> Option<u64> {
    let at = table.payload + FULL_BOX_LEN;
    let bytes = read_at(reader, at, 4 + width)?;
    let count = u32::from_be_bytes(bytes[..4].try_into().ok()?);
    if count == 0 {
        return None;
    }
    let offset = &bytes[4..];
    Some(match width {
        8 => u64::from_be_bytes(offset.try_into().ok()?),
        _ => u64::from(u32::from_be_bytes(offset.try_into().ok()?)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{bmff_box, cr3_with_jpegs};
    use std::io::Cursor;

    fn regions_of(file: &[u8]) -> Vec<&[u8]> {
        preview_regions(&mut Cursor::new(file), file.len() as u64)
            .into_iter()
            .map(|(at, len)| &file[at as usize..at as usize + len])
            .collect()
    }

    #[test]
    fn finds_the_full_size_jpeg_and_the_prvw_preview() {
        let cr3 = cr3_with_jpegs(b"\xFF\xD8full", b"\xFF\xD8small", true);
        let regions = regions_of(&cr3);
        assert_eq!(regions[0], b"\xFF\xD8full");
        assert!(regions[1].ends_with(b"\xFF\xD8small"));
    }

    #[test]
    fn reads_32_bit_chunk_offsets_too() {
        let cr3 = cr3_with_jpegs(b"\xFF\xD8full", b"\xFF\xD8small", false);
        assert_eq!(regions_of(&cr3)[0], b"\xFF\xD8full");
    }

    #[test]
    fn finds_nothing_in_other_files() {
        let file = bmff_box(b"moov", &bmff_box(b"trak", b"data"));
        assert!(regions_of(&file).is_empty());
    }
}
