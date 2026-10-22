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

pub fn tiff_header(le: bool) -> Vec<u8> {
    let mut out = if le { b"II".to_vec() } else { b"MM".to_vec() };
    push_u16(&mut out, 0x2A, le);
    push_u32(&mut out, 8, le);
    out
}

/// An IFD at file offset `at` of ASCII entries, followed by the strings
/// too long to sit in an entry's value field
fn ascii_ifd(tags: &[(u16, &str)], at: u32, le: bool) -> Vec<u8> {
    let entries: Vec<TiffEntry> = tags
        .iter()
        .map(|&(tag, value)| TiffEntry::ascii(tag, value))
        .collect();
    ifd_bytes(&entries, at as usize, le)
}

/// One entry of a synthetic IFD; `data` is the value in the file's byte order
pub struct TiffEntry {
    pub tag: u16,
    pub kind: u16,
    pub count: u32,
    pub data: Vec<u8>,
}

impl TiffEntry {
    pub fn ascii(tag: u16, value: &str) -> Self {
        let mut data = value.as_bytes().to_vec();
        data.push(0);
        Self::raw(tag, 2, data.len() as u32, data)
    }

    pub fn byte(tag: u16, value: u8) -> Self {
        Self::raw(tag, 1, 1, vec![value])
    }

    pub fn short(tag: u16, values: &[u16], le: bool) -> Self {
        let mut data = Vec::new();
        values.iter().for_each(|&v| push_u16(&mut data, v, le));
        Self::raw(tag, 3, values.len() as u32, data)
    }

    pub fn long(tag: u16, value: u32, le: bool) -> Self {
        let mut data = Vec::new();
        push_u32(&mut data, value, le);
        Self::raw(tag, 4, 1, data)
    }

    pub fn undefined(tag: u16, data: Vec<u8>) -> Self {
        Self::raw(tag, 7, data.len() as u32, data)
    }

    pub fn raw(tag: u16, kind: u16, count: u32, data: Vec<u8>) -> Self {
        Self {
            tag,
            kind,
            count,
            data,
        }
    }
}

/// An IFD at TIFF offset `at`, followed by the values too long to sit in
/// an entry's value field
pub fn ifd_bytes(entries: &[TiffEntry], at: usize, le: bool) -> Vec<u8> {
    let mut data_at = at + 2 + entries.len() * 12 + 4;
    let (mut ifd, mut data): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
    push_u16(&mut ifd, entries.len() as u16, le);
    for entry in entries {
        push_u16(&mut ifd, entry.tag, le);
        push_u16(&mut ifd, entry.kind, le);
        push_u32(&mut ifd, entry.count, le);
        if entry.data.len() <= 4 {
            let mut inline = entry.data.clone();
            inline.resize(4, 0);
            ifd.extend(inline);
        } else {
            push_u32(&mut ifd, data_at as u32, le);
            data_at += entry.data.len();
            data.extend(&entry.data);
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

/// A Canon CR3 skeleton holding `full` as the first track's sample in mdat
/// (its offset in co64, or in stco when `is_co64` is false) and `small` in
/// the PRVW uuid box, laid out like a real CR3: ftyp, moov, uuid, mdat
pub fn cr3_with_jpegs(full: &[u8], small: &[u8], is_co64: bool) -> Vec<u8> {
    const PREVIEW_UUID: [u8; 16] = [
        0xEA, 0xF4, 0x2B, 0x5E, 0x1C, 0x98, 0x4B, 0x88, 0xB9, 0xFB, 0xB7, 0xDC, 0x40, 0x6E, 0x4D,
        0x16,
    ];
    let head_with = |mdat_at: u64| {
        let mut stsz = vec![0u8; 4];
        stsz.extend((full.len() as u32).to_be_bytes());
        stsz.extend(1u32.to_be_bytes());
        let mut chunks = vec![0, 0, 0, 0, 0, 0, 0, 1];
        let offsets = if is_co64 {
            chunks.extend((mdat_at + 8).to_be_bytes());
            bmff_box(b"co64", &chunks)
        } else {
            chunks.extend(((mdat_at + 8) as u32).to_be_bytes());
            bmff_box(b"stco", &chunks)
        };
        let mut stbl = bmff_box(b"stsd", b"CRAW");
        stbl.extend(bmff_box(b"stsz", &stsz));
        stbl.extend(offsets);
        let trak = bmff_box(b"mdia", &bmff_box(b"minf", &bmff_box(b"stbl", &stbl)));
        let mut moov = bmff_box(b"mvhd", &[0; 8]);
        moov.extend(bmff_box(b"trak", &trak));
        moov.extend(bmff_box(b"trak", &bmff_box(b"mdia", b"raw track")));

        let mut prvw = vec![0u8; 4];
        prvw.extend([0x06, 0x54, 0x04, 0x38, 0, 1]); // 1620 x 1080
        prvw.extend((small.len() as u32).to_be_bytes());
        prvw.extend(small);
        let mut uuid = PREVIEW_UUID.to_vec();
        uuid.extend([0u8; 8]);
        uuid.extend(bmff_box(b"PRVW", &prvw));

        let mut out = bmff_box(b"ftyp", b"crx \0\0\0\x01crx isom");
        out.extend(bmff_box(b"moov", &moov));
        out.extend(bmff_box(b"uuid", &uuid));
        out
    };
    let mdat_at = head_with(0).len() as u64;
    let mut mdat = full.to_vec();
    mdat.extend([0x5A; 4096]); // raw data
    let mut out = head_with(mdat_at);
    out.extend(bmff_box(b"mdat", &mdat));
    out
}

/// A Nikon NEF skeleton: IFD0 lists one SubIFD per JPEG, each pointing at
/// its JPEG with JPEGInterchangeFormat / ...Length
pub fn nef_with_jpegs(jpegs: &[&[u8]], le: bool) -> Vec<u8> {
    const SUB_IFD_LEN: usize = 2 + 2 * 12 + 4;
    let ifd0_with = |offsets: &[u32]| {
        let mut data = Vec::new();
        offsets.iter().for_each(|&o| push_u32(&mut data, o, le));
        let entries: Vec<TiffEntry> = match offsets.len() {
            0 => vec![TiffEntry::short(0x0112, &[1], le)],
            n => vec![TiffEntry::raw(0x014A, 4, n as u32, data)],
        };
        ifd_bytes(&entries, 8, le)
    };
    let sub_ifds_at = 8 + ifd0_with(&vec![0; jpegs.len()]).len();
    let jpegs_at = sub_ifds_at + jpegs.len() * SUB_IFD_LEN;
    let offsets: Vec<u32> = (0..jpegs.len())
        .map(|i| (sub_ifds_at + i * SUB_IFD_LEN) as u32)
        .collect();

    let mut out = tiff_header(le);
    out.extend(ifd0_with(&offsets));
    let mut jpeg_at = jpegs_at;
    for (i, jpeg) in jpegs.iter().enumerate() {
        let entries = [
            TiffEntry::long(0x0201, jpeg_at as u32, le),
            TiffEntry::long(0x0202, jpeg.len() as u32, le),
        ];
        out.extend(ifd_bytes(&entries, offsets[i] as usize, le));
        jpeg_at += jpeg.len();
    }
    jpegs.iter().for_each(|jpeg| out.extend(*jpeg));
    out.extend([0x5A; 4096]); // raw data
    out
}
