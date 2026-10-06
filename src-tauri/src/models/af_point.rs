// ============================================================
// Sift - Focus Point Models
// Where the camera focused, as its maker notes record it
// ============================================================

use serde::Serialize;
use ts_rs::TS;

/// What a photo's maker notes say about where the camera focused.
/// Coordinates are pixels of the stored image before EXIF Orientation is
/// applied (`width` × `height`, origin top-left, y down); the viewer
/// rotates them with the photo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AfInfo {
    /// The camera recorded where it focused
    Found {
        width: u32,
        height: u32,
        /// EXIF Orientation of the file the coordinates came from (1–8)
        orientation: u16,
        /// The area the camera focused on (an eye, a face, an AF point)
        area: Option<AfArea>,
        /// Phase-detect points that were in focus, in wide-area modes
        points: Vec<AfSpot>,
    },
    /// The photo was focused by hand
    ManualFocus,
    /// No focus data Sift can read: another camera brand, or a file
    /// whose maker notes were dropped or moved by editing software
    Unavailable,
}

/// A box centred on (`x`, `y`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AfArea {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AfSpot {
    pub x: u32,
    pub y: u32,
}
