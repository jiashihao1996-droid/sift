// ============================================================
// Sift - Rust Models
// Data structures shared between Rust backend and Vue frontend
// ============================================================

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Status of a photo pair in the culling workflow
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum PhotoStatus {
    Unprocessed,
    Starred,
    Deleted,
}

/// Source of the JPG preview image
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum PhotoSource {
    /// Original JPG file
    Jpg,
    /// Preview extracted from RAW file
    RawPreview,
}

/// A paired photo: one JPG + optional RAW
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPair {
    pub id: String,
    pub jpg_path: String,
    pub raw_path: Option<String>,
    pub raw_format: Option<String>,
    pub status: PhotoStatus,
    pub thumbnail_path: Option<String>,
    pub dominant_color: Option<String>,
    pub source: PhotoSource,
    #[serde(default)]
    pub xmp_paths: Vec<String>,
    /// When the photo was taken, from EXIF; None when the file doesn't say
    #[serde(default)]
    pub capture_time: Option<CaptureTime>,
    /// Found in the 精选/ folder a previous archive filled: starred by
    /// default, and moved back out if the user unstars it
    #[serde(default)]
    pub is_archived_pick: bool,
}

/// When a photo was taken, as the camera recorded it in EXIF
/// (DateTimeOriginal, SubSecTimeOriginal, OffsetTimeOriginal)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CaptureTime {
    /// The camera clock's reading in milliseconds since 1970-01-01 00:00,
    /// with no time zone applied
    pub local_ms: i64,
    /// Minutes east of UTC, when the camera recorded its time zone
    pub offset_minutes: Option<i16>,
}

/// Result of scanning a folder
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub pairs: Vec<PhotoPair>,
    pub total_files: usize,
    pub paired_count: usize,
    pub jpg_only_count: usize,
    pub raw_only_count: usize,
    /// Photo files found but not loaded into the session
    pub skipped: Vec<SkippedFile>,
}

/// Why a photo file was left out of the session
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// RAW-only file whose embedded preview could not be extracted
    NoPreview,
    /// A folder or folder entry that could not be read (permissions, I/O
    /// error); `path` may be a folder whose photos are all unaccounted for
    Unreadable,
    /// Empty (0-byte) photo file, e.g. left by an interrupted copy
    Empty,
}

/// A photo file that was found but not loaded into the session
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub path: String,
    pub reason: SkipReason,
}

/// Thumbnail generation result for a single photo
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailResult {
    pub id: String,
    pub path: String,
    pub dominant_color: String,
}

/// EXIF metadata extracted from a JPG
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExifData {
    pub camera: String,
    pub lens: String,
    pub iso: u32,
    pub aperture: String,
    pub shutter_speed: String,
    pub focal_length: String,
    /// Exposure compensation in EV (ExposureBiasValue); None when not recorded
    pub exposure_bias: Option<f64>,
    pub date_taken: String,
    pub dimensions: Dimensions,
    pub file_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

/// Archive operation result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveResult {
    pub moved_count: usize,
    pub picks_folder: String,
    pub raw_folder: String,
}

/// Archive/Export progress event payload
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: String,
}

/// Export result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub exported_count: usize,
    pub export_folder: String,
}

/// Saved culling progress of one folder, keyed by each photo's user file path
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FolderProgress {
    pub folder: String,
    /// Photo to resume at
    pub current_key: Option<String>,
    /// Marked photos only; unprocessed ones are left out
    pub marks: BTreeMap<String, PhotoStatus>,
}

/// Input for archive command
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePairInput {
    pub jpg_path: String,
    pub raw_path: Option<String>,
    pub status: PhotoStatus,
    #[serde(default = "default_source")]
    pub source: PhotoSource,
    #[serde(default)]
    pub xmp_paths: Vec<String>,
}

/// Input for export command
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExportPairInput {
    pub jpg_path: String,
    pub raw_path: Option<String>,
    #[serde(default = "default_source")]
    pub source: PhotoSource,
    #[serde(default)]
    pub xmp_paths: Vec<String>,
}

/// Default source is Jpg for backward compatibility
fn default_source() -> PhotoSource {
    PhotoSource::Jpg
}

/// Input for thumbnail generation
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailInput {
    pub id: String,
    pub jpg_path: String,
}
