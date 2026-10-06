// ============================================================
// Sift - Sony Focus Point
// Sony maker notes are an IFD, after a 12-byte "SONY DSC " header in
// JPGs and bare in ARWs, whose offsets count from the TIFF header.
// Only plain tags are read, which E-mount bodies write whatever the
// model (the enciphered 0x94xx blocks change layout between models):
// - 0x201B FocusMode: 0 is manual focus
// - 0x2027 FocusLocation: image width, height, then the focus x, y
//   (the image centre when the camera has no position)
// - 0x2037 FocusFrameSize: width and height of the box centred on the
//   location, then a flag that is 0 when there is no box
// - 0x202A: the phase-detect points in focus, in a 640×480 space that
//   spans the image. The 640×428 it declares is not the scale: on 800
//   ILCE-7CM2 frames FocusLocation lands exactly on the 640×480 grid.
//   ExifTool only trusts the block when its first byte is 1.
// ============================================================

use crate::media::metadata::ifd::{read_u16, Tiff};
use crate::models::af_point::{AfArea, AfInfo, AfSpot};
use std::io::{Read, Seek};

const HEADERS: [&[u8; 12]; 2] = [b"SONY DSC \0\0\0", b"SONY CAM \0\0\0"];
const FOCUS_MODE: u16 = 0x201B;
const FOCUS_LOCATION: u16 = 0x2027;
const FOCUS_FRAME_SIZE: u16 = 0x2037;
const FOCAL_PLANE_POINTS: u16 = 0x202A;

const MANUAL_FOCUS: u8 = 0;
const POINT_BLOCK_VERSION: u8 = 1;
const POINT_GRID_WIDTH: u32 = 640;
const POINT_GRID_HEIGHT: u32 = 480;
const MAX_POINTS: usize = 15;
/// Version, count, declared area (2 × u16), then up to 15 (x, y) u16 pairs
const POINTS_AT: usize = 6;
const POINT_BLOCK_LEN: usize = POINTS_AT + MAX_POINTS * 4;

/// Focus data from the Sony maker notes `at` bytes past the TIFF header;
/// None when they hold no usable position
pub fn read<R: Read + Seek>(tiff: &mut Tiff<'_, R>, at: u32, orientation: u16) -> Option<AfInfo> {
    let has_header = tiff
        .bytes_at(at, HEADERS[0].len())
        .is_some_and(|start| HEADERS.iter().any(|h| start == h[..]));
    let ifd_at = if has_header { at.checked_add(12)? } else { at };
    let entries = tiff.entries(ifd_at)?;
    let find = |tag: u16| entries.iter().find(|e| e.tag == tag);

    let focus_mode = find(FOCUS_MODE).and_then(|e| tiff.value_bytes(e, 1));
    if focus_mode.as_deref() == Some(&[MANUAL_FOCUS]) {
        return Some(AfInfo::ManualFocus);
    }

    let location = tiff.u16_values(find(FOCUS_LOCATION)?, 4)?;
    let [width, height, x, y] = <[u16; 4]>::try_from(location).ok()?.map(u32::from);
    if width == 0 || height == 0 || x > width || y > height {
        return None;
    }
    let area = find(FOCUS_FRAME_SIZE)
        .and_then(|e| tiff.u16_values(e, 3))
        .and_then(|size| focus_frame(&size, x, y));
    let points = find(FOCAL_PLANE_POINTS)
        .and_then(|e| tiff.value_bytes(e, POINT_BLOCK_LEN))
        .map(|block| focal_plane_points(&block, width, height, tiff.little_endian))
        .unwrap_or_default();
    if area.is_none() && points.is_empty() {
        return None;
    }
    Some(AfInfo::Found {
        width,
        height,
        orientation,
        area,
        points,
    })
}

fn focus_frame(size: &[u16], x: u32, y: u32) -> Option<AfArea> {
    let [width, height, flag] = <[u16; 3]>::try_from(size).ok()?;
    (flag != 0 && width > 0 && height > 0).then(|| AfArea {
        x,
        y,
        width: u32::from(width),
        height: u32::from(height),
    })
}

/// The in-focus points of a 0x202A block, scaled to the image
fn focal_plane_points(block: &[u8], width: u32, height: u32, le: bool) -> Vec<AfSpot> {
    let [version, count, w0, w1, ..] = *block else {
        return Vec::new();
    };
    let count = usize::from(count);
    let is_known_layout = version == POINT_BLOCK_VERSION
        && count <= MAX_POINTS
        && u32::from(read_u16([w0, w1], le)) == POINT_GRID_WIDTH;
    if !is_known_layout {
        return Vec::new();
    }
    block[POINTS_AT..]
        .as_chunks::<4>()
        .0
        .iter()
        .take(count)
        .map(|p| {
            let x = u32::from(read_u16([p[0], p[1]], le));
            let y = u32::from(read_u16([p[2], p[3]], le));
            (x, y)
        })
        .filter(|&(x, y)| x <= POINT_GRID_WIDTH && y <= POINT_GRID_HEIGHT)
        .map(|(x, y)| AfSpot {
            x: x * width / POINT_GRID_WIDTH,
            y: y * height / POINT_GRID_HEIGHT,
        })
        .collect()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::test_support::{ifd_bytes, tiff_header, TiffEntry};
    use std::io::Cursor;

    const W: u16 = 7008;
    const H: u16 = 4672;
    const ORIENTATION_CCW: u16 = 8;

    /// FocusMode AF-A, a 153×156 box at (4095, 2530) and two in-focus
    /// points at grid (374, 260) and (392, 280), as an ILCE-7CM2 writes them
    fn maker_entries(le: bool) -> Vec<TiffEntry> {
        vec![
            TiffEntry::byte(FOCUS_MODE, 4),
            TiffEntry::short(FOCUS_LOCATION, &[W, H, 4095, 2530], le),
            TiffEntry::undefined(
                FOCAL_PLANE_POINTS,
                point_block(1, 640, &[(374, 260), (392, 280)], le),
            ),
            TiffEntry::undefined(FOCUS_FRAME_SIZE, u16_bytes(&[153, 156, 257], le)),
        ]
    }

    pub fn expected() -> AfInfo {
        AfInfo::Found {
            width: 7008,
            height: 4672,
            orientation: ORIENTATION_CCW,
            area: Some(AfArea {
                x: 4095,
                y: 2530,
                width: 153,
                height: 156,
            }),
            points: vec![AfSpot { x: 4095, y: 2530 }, AfSpot { x: 4292, y: 2725 }],
        }
    }

    pub fn jpg_tiff() -> Vec<u8> {
        tiff_with_make("SONY", maker_entries(true), true)
    }

    pub fn arw_tiff() -> Vec<u8> {
        tiff_with_make("SONY", maker_entries(true), false)
    }

    /// A little-endian TIFF: IFD0 (Make, Orientation 8) → Exif IFD → maker
    /// notes holding `maker` entries, after a JPG-style header if `header`
    pub fn tiff_with_make(make: &str, maker: Vec<TiffEntry>, header: bool) -> Vec<u8> {
        build(make, maker, header, true)
    }

    fn build(make: &str, maker: Vec<TiffEntry>, header: bool, le: bool) -> Vec<u8> {
        let ifd0 = |exif_at: u32| {
            vec![
                TiffEntry::ascii(0x010F, make),
                TiffEntry::short(0x0112, &[ORIENTATION_CCW], le),
                TiffEntry::long(0x8769, exif_at, le),
            ]
        };
        let exif_at = 8 + ifd_bytes(&ifd0(0), 8, le).len();
        let maker_at = exif_at + 2 + 12 + 4;
        let mut note = if header {
            HEADERS[0].to_vec()
        } else {
            Vec::new()
        };
        let ifd_at = maker_at + note.len();
        note.extend(ifd_bytes(&maker, ifd_at, le));

        let mut out = tiff_header(le);
        out.extend(ifd_bytes(&ifd0(exif_at as u32), 8, le));
        out.extend(ifd_bytes(
            &[TiffEntry::undefined(0x927C, note)],
            exif_at,
            le,
        ));
        out
    }

    fn u16_bytes(values: &[u16], le: bool) -> Vec<u8> {
        values
            .iter()
            .flat_map(|&v| if le { v.to_le_bytes() } else { v.to_be_bytes() })
            .collect()
    }

    fn point_block(version: u8, grid_width: u16, points: &[(u16, u16)], le: bool) -> Vec<u8> {
        let mut block = vec![version, points.len() as u8];
        block.extend(u16_bytes(&[grid_width, 428], le));
        for &(x, y) in points {
            block.extend(u16_bytes(&[x, y], le));
        }
        block.resize(POINT_BLOCK_LEN, 0);
        block
    }

    /// Run `read` on a bare-IFD (ARW-style) TIFF built from `maker`
    fn parse_with(maker: Vec<TiffEntry>, le: bool) -> Option<AfInfo> {
        let bytes = build("SONY", maker, false, le);
        let mut cursor = Cursor::new(bytes);
        let mut tiff = Tiff::open(&mut cursor, 0).unwrap();
        let ifd0 = tiff.entries(tiff.ifd0).unwrap();
        let exif_at = ifd0
            .iter()
            .find(|e| e.tag == 0x8769)
            .unwrap()
            .offset_value()
            .unwrap();
        let exif = tiff.entries(exif_at).unwrap();
        let at = exif[0].data_offset();
        read(&mut tiff, at, ORIENTATION_CCW)
    }

    fn replace(mut entries: Vec<TiffEntry>, entry: TiffEntry) -> Vec<TiffEntry> {
        entries.retain(|e| e.tag != entry.tag);
        entries.push(entry);
        entries.sort_by_key(|e| e.tag);
        entries
    }

    fn without(mut entries: Vec<TiffEntry>, tag: u16) -> Vec<TiffEntry> {
        entries.retain(|e| e.tag != tag);
        entries
    }

    #[test]
    fn reads_the_focus_box_and_in_focus_points() {
        assert_eq!(parse_with(maker_entries(true), true), Some(expected()));
    }

    #[test]
    fn reads_big_endian_maker_notes() {
        assert_eq!(parse_with(maker_entries(false), false), Some(expected()));
    }

    #[test]
    fn reports_manual_focus() {
        let maker = replace(
            maker_entries(true),
            TiffEntry::byte(FOCUS_MODE, MANUAL_FOCUS),
        );
        assert_eq!(parse_with(maker, true), Some(AfInfo::ManualFocus));
    }

    #[test]
    fn keeps_the_points_when_there_is_no_focus_box() {
        let no_box = TiffEntry::undefined(FOCUS_FRAME_SIZE, u16_bytes(&[0, 0, 0], true));
        let info = parse_with(replace(maker_entries(true), no_box), true);
        let Some(AfInfo::Found { area, points, .. }) = info else {
            panic!("expected focus data, got {info:?}");
        };
        assert_eq!(area, None);
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn is_none_without_a_box_or_points() {
        let maker = without(
            without(maker_entries(true), FOCAL_PLANE_POINTS),
            FOCUS_FRAME_SIZE,
        );
        assert_eq!(parse_with(maker, true), None);
    }

    #[test]
    fn is_none_for_an_unusable_focus_location() {
        for location in [[0, 0, 0, 0], [W, H, W + 1, 10], [W, H, 10, H + 1]] {
            let entry = TiffEntry::short(FOCUS_LOCATION, &location, true);
            assert_eq!(parse_with(replace(maker_entries(true), entry), true), None);
        }
        let maker = without(maker_entries(true), FOCUS_LOCATION);
        assert_eq!(parse_with(maker, true), None);
    }

    #[test]
    fn ignores_point_blocks_of_an_unknown_layout() {
        let too_many: Vec<(u16, u16)> = vec![(320, 240); MAX_POINTS + 1];
        for block in [
            point_block(110, 640, &[(320, 240)], true),
            point_block(1, 128, &[(320, 240)], true),
            point_block(1, 640, &too_many, true),
        ] {
            let entry = TiffEntry::undefined(FOCAL_PLANE_POINTS, block);
            let info = parse_with(replace(maker_entries(true), entry), true);
            let Some(AfInfo::Found { points, .. }) = info else {
                panic!("expected focus data, got {info:?}");
            };
            assert!(points.is_empty());
        }
    }

    #[test]
    fn drops_points_outside_the_grid() {
        let block = point_block(1, 640, &[(641, 240), (320, 481), (320, 240)], true);
        let entry = TiffEntry::undefined(FOCAL_PLANE_POINTS, block);
        let info = parse_with(replace(maker_entries(true), entry), true);
        let Some(AfInfo::Found { points, .. }) = info else {
            panic!("expected focus data, got {info:?}");
        };
        assert_eq!(points, vec![AfSpot { x: 3504, y: 2336 }]);
    }
}
