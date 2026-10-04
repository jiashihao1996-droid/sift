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
