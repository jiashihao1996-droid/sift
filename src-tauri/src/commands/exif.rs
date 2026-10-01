// ============================================================
// Sift - EXIF Command
// For RAW-only photos, reads EXIF from the RAW original instead
// of the temporary preview JPG
// ============================================================

use crate::media::exif::read_exif_data;
use crate::models::photo::ExifData;

#[tauri::command]
pub fn read_exif(
    jpg_path: String,
    raw_path: Option<String>,
    source: Option<String>,
) -> Result<ExifData, String> {
    if source.as_deref() == Some("rawPreview") {
        if let Some(rp) = &raw_path {
            if let Ok(exif) = read_exif_data(rp) {
                return Ok(exif);
            }
        }
    }
    read_exif_data(&jpg_path)
}
