// ============================================================
// Sift - Previews Located by Format
// CR3, RAF and NEF don't point to their preview from the TIFF IFD chain,
// so earlier versions scanned the whole RAW (25-60MB) for the largest
// JPEG. Each format records where its JPEGs are, though; scanning just
// those regions the same way finds the same JPEG, as long as the file's
// largest JPEG sits in one of them, which it does in the files the
// cameras write. organize/saved_preview.rs still recognizes the preview
// of a file where it doesn't.
// ============================================================

mod cr3;
mod nef;
mod raf;

use std::io::{Read, Seek};
use std::path::Path;

/// (offset, length) of the regions a RAW's format says hold its JPEGs;
/// empty for other formats or when the structure can't be read
pub fn preview_regions<R: Read + Seek>(
    reader: &mut R,
    raw_path: &Path,
    file_len: u64,
) -> Vec<(u64, usize)> {
    let ext = raw_path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("cr3") => cr3::preview_regions(reader, file_len),
        Some("raf") => raf::preview_region(reader).into_iter().collect(),
        Some("nef" | "nrw") => nef::preview_regions(reader),
        _ => Vec::new(),
    }
}
