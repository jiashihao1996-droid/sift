// ============================================================
// Sift - Metadata Readers
// Low-level pieces for reading a photo's metadata from the start of
// its file without loading the whole file: a cached head, the EXIF
// segment of a JPEG, a seeking TIFF IFD reader and ISO base media boxes.
// ============================================================

pub mod bmff;
pub mod head;
pub mod ifd;
pub mod jpeg_exif;
