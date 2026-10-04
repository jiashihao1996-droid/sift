// ============================================================
// Sift - Organize Errors
// How archive, export and delete report a failure to the frontend: a
// category the UI turns into advice, the file it got stuck on, the original
// message for the details view, and how much was done before it stopped.
// ============================================================

use super::photo::PhotoSource;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What went wrong, as far as the user can do something about it
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum OrganizeErrorKind {
    /// No permission to read, write or remove the file or folder
    PermissionDenied,
    /// The destination volume is full
    DiskFull,
    /// The volume is read-only, e.g. an SD card with its lock switch on
    ReadOnly,
    /// The file is gone, e.g. moved or deleted by another program
    NotFound,
    /// The volume has no trash (FAT/exFAT cards on Windows, network shares)
    TrashUnsupported,
    /// macOS refused to let Sift control Finder, which moves files to the Trash
    AutomationDenied,
    Other,
}

/// Why an organize command (archive, export, delete) failed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeError {
    pub kind: OrganizeErrorKind,
    /// The file the operation was stuck on, when known
    pub path: Option<String>,
    /// The original (English) message, for the details view and bug reports
    pub detail: String,
    /// How much was done before the failure: photos (JPG/RAW) moved or copied
    /// by archive and export; files of the pair already trashed by delete
    pub completed: usize,
}

/// A pair to delete, as the frontend knows it
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeletePairInput {
    /// The session's pair id, echoed back in the result
    pub id: String,
    pub jpg_path: String,
    pub raw_path: Option<String>,
    pub source: PhotoSource,
    #[serde(default)]
    pub xmp_paths: Vec<String>,
}

/// A pair that could not be deleted; files of it may already be in the trash
/// (`error.completed` counts them)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFailure {
    pub id: String,
    pub error: OrganizeError,
}

/// What deleting a batch of pairs did, pair by pair. A pair whose files were
/// already gone counts as trashed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// Ids of pairs whose files are all gone now
    pub trashed: Vec<String>,
    pub failed: Vec<DeleteFailure>,
}
