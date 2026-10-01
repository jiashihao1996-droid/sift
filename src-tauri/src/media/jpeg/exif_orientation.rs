// ============================================================
// Sift - JPEG EXIF Orientation
// Reads and writes the Orientation tag (0x0112) in a JPEG's APP1/EXIF
// segment without decoding or re-encoding the image.
// ============================================================

/// Get the Orientation tag already present in a JPEG's APP1/EXIF segment, if any.
pub fn read_orientation(jpeg: &[u8]) -> Option<u16> {
    let (start, len) = find_jpeg_app1_tiff_range(jpeg)?;
    read_tiff_orientation(&jpeg[start..start + len])
}

/// Ensure the JPEG carries `outer_orientation` in its APP1/EXIF.
///
/// Behaviour:
///   - outer == 1 (or unknown): return the JPEG unchanged.
///   - JPEG already has an APP1/EXIF with a writable Orientation tag: patch it in place.
///   - JPEG has no APP1/EXIF (or APP1 without Orientation tag): inject a minimal
///     APP1 segment containing just the Orientation tag right after SOI.
pub fn ensure_orientation(jpeg: Vec<u8>, outer_orientation: u16) -> Vec<u8> {
    if outer_orientation <= 1 {
        return jpeg;
    }

    // Try to patch an existing Orientation tag in-place.
    if let Some((tiff_start, tiff_len)) = find_jpeg_app1_tiff_range(&jpeg) {
        let mut out = jpeg;
        let tiff = &mut out[tiff_start..tiff_start + tiff_len];
        if patch_tiff_orientation(tiff, outer_orientation) {
            return out;
        }
        // APP1 exists but no Orientation tag — prepend a fresh APP1; readers pick the first.
        return inject_orientation_app1(out, outer_orientation);
    }

    // No APP1/EXIF at all: inject a minimal one.
    inject_orientation_app1(jpeg, outer_orientation)
}

/// Locate the TIFF payload inside a JPEG's first APP1/EXIF segment.
/// Returns (offset_of_tiff_in_jpeg, tiff_len).
fn find_jpeg_app1_tiff_range(jpeg: &[u8]) -> Option<(usize, usize)> {
    // JPEG must start with SOI (FF D8)
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }

    let mut pos = 2;
    while pos + 4 <= jpeg.len() {
        if jpeg[pos] != 0xFF {
            return None;
        }
        let marker = jpeg[pos + 1];

        // SOS / EOI => past the header area, no APP1 found.
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }

        // Standalone markers (no length)
        if matches!(marker, 0xD0..=0xD7 | 0xD8 | 0x01) {
            pos += 2;
            continue;
        }

        // Fill byte (FF FF)
        if marker == 0xFF {
            pos += 1;
            continue;
        }

        // Length field (big-endian, includes the 2 length bytes themselves)
        let seg_len = ((jpeg[pos + 2] as usize) << 8) | (jpeg[pos + 3] as usize);
        if seg_len < 2 || pos + 2 + seg_len > jpeg.len() {
            return None;
        }

        if marker == 0xE1 {
            // APP1: check for "Exif\0\0" signature
            let payload_start = pos + 4;
            let payload_end = pos + 2 + seg_len;
            let payload = &jpeg[payload_start..payload_end];
            if payload.len() >= 6 && &payload[0..6] == b"Exif\0\0" {
                let tiff_start = payload_start + 6;
                let tiff_len = payload_end - tiff_start;
                if tiff_len >= 8 {
                    return Some((tiff_start, tiff_len));
                }
            }
        }

        pos += 2 + seg_len;
    }
    None
}

/// Read Orientation (tag 0x0112) from a TIFF blob. Returns None if absent/malformed.
fn read_tiff_orientation(tiff: &[u8]) -> Option<u16> {
    let (le, ifd0_off) = parse_tiff_header(tiff)?;
    read_ifd_orientation(tiff, ifd0_off, le)
}

/// Patch an existing Orientation tag's value in a TIFF blob. Returns true if patched.
fn patch_tiff_orientation(tiff: &mut [u8], new_value: u16) -> bool {
    let (le, ifd0_off) = match parse_tiff_header(tiff) {
        Some(x) => x,
        None => return false,
    };
    if ifd0_off + 2 > tiff.len() {
        return false;
    }
    let count = read_u16(&tiff[ifd0_off..ifd0_off + 2], le) as usize;
    for i in 0..count {
        let entry_off = ifd0_off + 2 + i * 12;
        if entry_off + 12 > tiff.len() {
            return false;
        }
        let tag = read_u16(&tiff[entry_off..entry_off + 2], le);
        if tag == 0x0112 {
            // Orientation is SHORT, count 1 — the value sits in the first 2 bytes of the value/offset field.
            let val_off = entry_off + 8;
            write_u16(&mut tiff[val_off..val_off + 2], new_value, le);
            return true;
        }
    }
    false
}

fn read_ifd_orientation(tiff: &[u8], ifd_off: usize, le: bool) -> Option<u16> {
    if ifd_off + 2 > tiff.len() {
        return None;
    }
    let count = read_u16(&tiff[ifd_off..ifd_off + 2], le) as usize;
    for i in 0..count {
        let entry_off = ifd_off + 2 + i * 12;
        if entry_off + 12 > tiff.len() {
            return None;
        }
        let tag = read_u16(&tiff[entry_off..entry_off + 2], le);
        if tag == 0x0112 {
            return Some(read_u16(&tiff[entry_off + 8..entry_off + 10], le));
        }
    }
    None
}

/// Parse TIFF header. Returns (little_endian, ifd0_offset).
fn parse_tiff_header(tiff: &[u8]) -> Option<(bool, usize)> {
    if tiff.len() < 8 {
        return None;
    }
    let le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let magic = read_u16(&tiff[2..4], le);
    if magic != 0x002A {
        return None;
    }
    let ifd0 = read_u32(&tiff[4..8], le) as usize;
    Some((le, ifd0))
}

fn read_u16(b: &[u8], le: bool) -> u16 {
    if le {
        u16::from_le_bytes([b[0], b[1]])
    } else {
        u16::from_be_bytes([b[0], b[1]])
    }
}

fn read_u32(b: &[u8], le: bool) -> u32 {
    if le {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    }
}

fn write_u16(b: &mut [u8], v: u16, le: bool) {
    let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
    b[0] = bytes[0];
    b[1] = bytes[1];
}

/// Inject a minimal APP1/EXIF segment (carrying just Orientation) right after SOI.
fn inject_orientation_app1(jpeg: Vec<u8>, orientation: u16) -> Vec<u8> {
    // Build a minimal TIFF:
    //   "II" 2A 00  08 00 00 00           TIFF header, IFD0 @ offset 8 (little-endian)
    //   01 00                             entry count = 1
    //   12 01 03 00 01 00 00 00 <O> 00 00 00   Orientation entry (tag=0x0112, type=SHORT, count=1)
    //   00 00 00 00                       next IFD offset = 0
    let o = orientation.to_le_bytes();
    let tiff: [u8; 26] = [
        b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01,
        0x00, 0x00, 0x00, o[0], o[1], 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // APP1 payload = "Exif\0\0" + tiff
    // APP1 length field = 2 (length bytes) + 6 ("Exif\0\0") + tiff.len()
    let app1_len = 2 + 6 + tiff.len();
    let len_be = (app1_len as u16).to_be_bytes();

    let mut out = Vec::with_capacity(jpeg.len() + 4 + app1_len);
    // SOI
    out.extend_from_slice(&jpeg[0..2]);
    // APP1 marker + length
    out.push(0xFF);
    out.push(0xE1);
    out.push(len_be[0]);
    out.push(len_be[1]);
    // "Exif\0\0"
    out.extend_from_slice(b"Exif\0\0");
    // TIFF
    out.extend_from_slice(&tiff);
    // Rest of the JPEG (everything after SOI)
    out.extend_from_slice(&jpeg[2..]);

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{exif_orientation, noise_jpeg};

    #[test]
    fn ensure_orientation_is_noop_for_normal_orientation() {
        let jpeg = noise_jpeg(32, 32);
        assert_eq!(ensure_orientation(jpeg.clone(), 1), jpeg);
    }

    #[test]
    fn ensure_orientation_injects_app1_when_absent() {
        let out = ensure_orientation(noise_jpeg(32, 32), 6);

        assert_eq!(read_orientation(&out), Some(6));
        assert_eq!(exif_orientation(&out), Some(6));
        assert!(image::load_from_memory(&out).is_ok());
    }

    #[test]
    fn ensure_orientation_patches_existing_tag_in_place() {
        let with_tag = ensure_orientation(noise_jpeg(32, 32), 6);
        let out = ensure_orientation(with_tag.clone(), 8);

        assert_eq!(out.len(), with_tag.len());
        assert_eq!(exif_orientation(&out), Some(8));
    }

    #[test]
    fn ensure_orientation_patches_big_endian_tiff() {
        // APP1 with a Motorola-order TIFF carrying Orientation = 1
        let tiff: [u8; 26] = [
            b'M', b'M', 0x00, 0x2A, 0x00, 0x00, 0x00, 0x08, // header
            0x00, 0x01, // 1 entry
            0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
            0x00, // Orientation
            0x00, 0x00, 0x00, 0x00, // next IFD
        ];
        let jpeg = noise_jpeg(32, 32);
        let app1_len = (2 + 6 + tiff.len()) as u16;
        let mut input = vec![0xFF, 0xD8, 0xFF, 0xE1];
        input.extend_from_slice(&app1_len.to_be_bytes());
        input.extend_from_slice(b"Exif\0\0");
        input.extend_from_slice(&tiff);
        input.extend_from_slice(&jpeg[2..]);

        let out = ensure_orientation(input.clone(), 3);

        assert_eq!(out.len(), input.len());
        assert_eq!(exif_orientation(&out), Some(3));
    }

    #[test]
    fn read_orientation_is_none_for_non_jpeg() {
        assert_eq!(read_orientation(b"not a jpeg"), None);
    }
}
