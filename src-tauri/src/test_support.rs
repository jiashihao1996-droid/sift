// ============================================================
// Sift - Test Fixtures
// Synthetic JPEG / RAW builders so tests need no binary files.
// ============================================================

use image::codecs::jpeg::JpegEncoder;
use image::{ImageBuffer, Rgb};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Encode a deterministic noise image as a baseline JPEG (no APP1/EXIF).
/// Noise defeats compression, so 256x256 comfortably exceeds the 50KB
/// minimum preview size used by RAW preview extraction.
pub fn noise_jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut seed: u32 = 0x1234_5678;
    let img = ImageBuffer::from_fn(width, height, |_, _| {
        // xorshift32: deterministic and dependency-free
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let [r, g, b, _] = seed.to_le_bytes();
        Rgb([r, g, b])
    });
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(&img)
        .expect("encode noise JPEG");
    out
}

/// Encode a single-colour JPEG.
pub fn solid_jpeg(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = ImageBuffer::from_pixel(width, height, Rgb(rgb));
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(&img)
        .expect("encode solid JPEG");
    out
}

/// Build a minimal little-endian TIFF-based "RAW" whose IFD0 points at an
/// embedded preview JPEG, like NEF/ARW/DNG. `orientation` mirrors the
/// outer IFD0 Orientation tag that Sony ARW relies on.
pub fn tiff_raw_with_preview(preview: &[u8], orientation: Option<u16>) -> Vec<u8> {
    tiff_raw(preview, preview.len() as u32, orientation)
}

/// Like `tiff_raw_with_preview`, but IFD0 claims the preview is
/// `declared_len` bytes long, as a corrupt or hostile file might.
pub fn tiff_raw_declaring_preview_len(preview: &[u8], declared_len: u32) -> Vec<u8> {
    tiff_raw(preview, declared_len, None)
}

fn tiff_raw(preview: &[u8], declared_len: u32, orientation: Option<u16>) -> Vec<u8> {
    const SHORT: u16 = 3;
    const LONG: u16 = 4;
    let mut entries: Vec<(u16, u16, u32)> = Vec::new();
    if let Some(o) = orientation {
        entries.push((0x0112, SHORT, o as u32));
    }
    let entry_count = entries.len() + 2;
    let ifd_len = 2 + entry_count * 12 + 4;
    let preview_offset = (8 + ifd_len) as u32;
    entries.push((0x0201, LONG, preview_offset));
    entries.push((0x0202, LONG, declared_len));

    let mut out = Vec::new();
    out.extend_from_slice(b"II\x2A\x00");
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&(entry_count as u16).to_le_bytes());
    for (tag, kind, value) in entries {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        // SHORT values sit left-aligned in the 4-byte value field
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(preview);
    out
}

/// A non-TIFF container (like CR3/RAF) with a JPEG buried between junk bytes,
/// so only the SOI/EOI binary scan can find it.
pub fn opaque_raw_with_preview(preview: &[u8]) -> Vec<u8> {
    let mut out = b"FAKERAW\x00".repeat(64);
    out.extend_from_slice(preview);
    out.extend_from_slice(&[0xAB; 4096]);
    out
}

/// Append `preview` to `main` as an MPF large thumbnail, the way cameras
/// embed a ~1616x1080 preview after the full-size photo.
pub fn jpeg_with_mpf_preview(main: &[u8], preview: &[u8]) -> Vec<u8> {
    // SOI + APP2 marker/length + "MPF\0": where the MPF TIFF header starts
    const MPF_TIFF_AT: usize = 10;
    const TABLE_AT: u32 = 26; // TIFF header (8) + IFD with one entry (18)
    const TIFF_LEN: usize = TABLE_AT as usize + 32; // + two 16-byte MP entries
    let preview_at = MPF_TIFF_AT + TIFF_LEN + (main.len() - 2);

    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
    tiff.extend_from_slice(&1u16.to_le_bytes()); // one entry: MPEntry
    tiff.extend_from_slice(&0xB002u16.to_le_bytes());
    tiff.extend_from_slice(&7u16.to_le_bytes()); // UNDEFINED
    tiff.extend_from_slice(&32u32.to_le_bytes()); // two 16-byte entries
    tiff.extend_from_slice(&TABLE_AT.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    for (attr, size, offset) in [
        (0x2003_0000u32, main.len(), 0),
        (0x0001_0002, preview.len(), preview_at - MPF_TIFF_AT),
    ] {
        tiff.extend_from_slice(&attr.to_le_bytes());
        tiff.extend_from_slice(&(size as u32).to_le_bytes());
        tiff.extend_from_slice(&(offset as u32).to_le_bytes());
        tiff.extend_from_slice(&[0; 4]); // dependent images
    }

    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE2];
    out.extend_from_slice(&((2 + 4 + tiff.len()) as u16).to_be_bytes());
    out.extend_from_slice(b"MPF\0");
    out.extend_from_slice(&tiff);
    out.extend_from_slice(&main[2..]);
    assert_eq!(out.len(), preview_at, "MPF fixture offsets out of sync");
    out.extend_from_slice(preview);
    out
}

/// Orientation as seen by an independent parser (kamadak-exif), used to
/// cross-check our own byte-level APP1 handling.
pub fn exif_orientation(jpeg: &[u8]) -> Option<u16> {
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(jpeg))
        .ok()?;
    match exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
        .value
    {
        exif::Value::Short(ref v) => v.first().copied(),
        _ => None,
    }
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture dir");
    }
    fs::write(&path, bytes).expect("write fixture");
    path
}

#[derive(Clone, Copy)]
pub enum ExifByteOrder {
    Little,
    Big,
}

/// A TIFF whose IFD0 points at an Exif IFD holding `tags` as ASCII values,
/// the way cameras store DateTimeOriginal and its companions. On its own it
/// also passes for a TIFF-based RAW.
pub fn exif_tiff(tags: &[(u16, &str)], order: ExifByteOrder) -> Vec<u8> {
    const EXIF_IFD_AT: u32 = 8 + 18; // header + IFD0 with one entry
    let le = matches!(order, ExifByteOrder::Little);
    let mut out = tiff_header(le);
    push_u16(&mut out, 1, le);
    push_u16(&mut out, 0x8769, le); // ExifIFD pointer
    push_u16(&mut out, 4, le); // LONG
    push_u32(&mut out, 1, le);
    push_u32(&mut out, EXIF_IFD_AT, le);
    push_u32(&mut out, 0, le); // no next IFD
    out.extend(ascii_ifd(tags, EXIF_IFD_AT, le));
    out
}

/// A little-endian TIFF with `tags` directly in IFD0, like a CR3's CMT2 box
pub fn flat_exif_tiff(tags: &[(u16, &str)]) -> Vec<u8> {
    let mut out = tiff_header(true);
    out.extend(ascii_ifd(tags, 8, true));
    out
}

fn tiff_header(le: bool) -> Vec<u8> {
    let mut out = if le { b"II".to_vec() } else { b"MM".to_vec() };
    push_u16(&mut out, 0x2A, le);
    push_u32(&mut out, 8, le);
    out
}

/// An IFD at file offset `at` of ASCII entries, followed by the strings
/// too long to sit in an entry's value field
fn ascii_ifd(tags: &[(u16, &str)], at: u32, le: bool) -> Vec<u8> {
    let mut data_at = at as usize + 2 + tags.len() * 12 + 4;
    let (mut ifd, mut data) = (Vec::new(), Vec::new());
    push_u16(&mut ifd, tags.len() as u16, le);
    for &(tag, value) in tags {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        push_u16(&mut ifd, tag, le);
        push_u16(&mut ifd, 2, le); // ASCII
        push_u32(&mut ifd, bytes.len() as u32, le);
        if bytes.len() <= 4 {
            bytes.resize(4, 0);
            ifd.extend(bytes);
        } else {
            push_u32(&mut ifd, data_at as u32, le);
            data_at += bytes.len();
            data.extend(bytes);
        }
    }
    push_u32(&mut ifd, 0, le); // no next IFD
    ifd.extend(data);
    ifd
}

fn push_u16(out: &mut Vec<u8>, v: u16, le: bool) {
    out.extend(if le { v.to_le_bytes() } else { v.to_be_bytes() });
}

fn push_u32(out: &mut Vec<u8>, v: u32, le: bool) {
    out.extend(if le { v.to_le_bytes() } else { v.to_be_bytes() });
}

/// Insert `tiff` into `jpeg` as its APP1/EXIF segment
pub fn jpeg_with_exif(jpeg: &[u8], tiff: &[u8]) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend(((2 + 6 + tiff.len()) as u16).to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(tiff);
    out.extend(&jpeg[2..]);
    out
}

/// A Fujifilm RAF: a header whose offset field points at an embedded JPEG
pub fn raf_with_jpeg(jpeg: &[u8]) -> Vec<u8> {
    const JPEG_AT: usize = 160;
    let mut out = b"FUJIFILMCCD-RAW 0201FF129502".to_vec();
    out.resize(84, 0);
    out.extend((JPEG_AT as u32).to_be_bytes());
    out.extend((jpeg.len() as u32).to_be_bytes());
    out.resize(JPEG_AT, 0);
    out.extend(jpeg);
    out
}

/// An ISO base media box
pub fn bmff_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = ((8 + payload.len()) as u32).to_be_bytes().to_vec();
    out.extend(kind);
    out.extend(payload);
    out
}

/// A Canon CR3 skeleton: ftyp, then moov > Canon uuid > CMT1, CMT2
pub fn cr3_with_cmt2(cmt2: &[u8]) -> Vec<u8> {
    const CANON_UUID: [u8; 16] = [
        0x85, 0xC0, 0xB6, 0x87, 0x82, 0x0F, 0x11, 0xE0, 0x81, 0x11, 0xF4, 0xCE, 0x46, 0x2B, 0x6A,
        0x48,
    ];
    let mut canon = CANON_UUID.to_vec();
    canon.extend(bmff_box(b"CMT1", b"II*\0\x08\0\0\0\0\0"));
    canon.extend(bmff_box(b"CMT2", cmt2));
    let mut out = bmff_box(b"ftyp", b"crx \0\0\0\x01crx isom");
    out.extend(bmff_box(b"moov", &bmff_box(b"uuid", &canon)));
    out
}
