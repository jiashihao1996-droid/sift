// ============================================================
// Sift - Thumbnail Generation
// Generates upright thumbnails + dominant color. Tries the EXIF
// embedded thumbnail first, falls back to full decode.
// ============================================================

use crate::media::exif::read_orientation;
use crate::models::photo::{ThumbnailInput, ThumbnailResult};
use image::GenericImageView;
use rayon::prelude::*;
use std::path::Path;

/// Generate thumbnails for all inputs into `cache_dir`, in parallel.
/// Inputs that fail (unreadable/corrupt images) are left out of the result.
pub fn generate_thumbnails(inputs: &[ThumbnailInput], cache_dir: &Path) -> Vec<ThumbnailResult> {
    // Use rayon's default global thread pool (all cores) for maximum throughput
    inputs
        .par_iter()
        .filter_map(|input| process_thumbnail(cache_dir, input).ok())
        .collect()
}

/// Try to extract the EXIF-embedded JPEG thumbnail without full image decode.
/// Reads the EXIF data to find JPEGInterchangeFormat offset/length in IFD1,
/// then extracts the raw bytes from the file.
fn try_extract_exif_thumbnail(jpg_path: &str) -> Option<Vec<u8>> {
    let file_bytes = std::fs::read(jpg_path).ok()?;

    // Find EXIF APP1 marker and parse
    let file = std::io::Cursor::new(&file_bytes);
    let exif_reader = exif::Reader::new();
    let exif_data = exif_reader
        .read_from_container(&mut std::io::BufReader::new(file))
        .ok()?;

    // Get thumbnail offset and length from IFD1
    let offset_field = exif_data.get_field(exif::Tag::JPEGInterchangeFormat, exif::In(1))?;
    let length_field = exif_data.get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In(1))?;

    let offset = match &offset_field.value {
        exif::Value::Long(v) if !v.is_empty() => v[0] as usize,
        _ => return None,
    };
    let length = match &length_field.value {
        exif::Value::Long(v) if !v.is_empty() => v[0] as usize,
        _ => return None,
    };

    // Sanity check
    if !(1024..=1_048_576).contains(&length) {
        return None;
    }

    // The offset from EXIF is relative to the TIFF header start.
    // Find the TIFF header within the JPEG file (after APP1 marker + size).
    // Look for "Exif\0\0" followed by TIFF header ("II" or "MM")
    let exif_marker = b"Exif\x00\x00";
    let tiff_offset = file_bytes
        .windows(exif_marker.len())
        .position(|w| w == exif_marker)?
        + exif_marker.len();

    let abs_offset = tiff_offset + offset;
    let abs_end = abs_offset + length;

    if abs_end > file_bytes.len() {
        return None;
    }

    let thumb_bytes = file_bytes[abs_offset..abs_end].to_vec();

    // Verify it starts with JPEG SOI marker
    if thumb_bytes.len() < 2 || thumb_bytes[0] != 0xFF || thumb_bytes[1] != 0xD8 {
        return None;
    }

    Some(thumb_bytes)
}

/// Apply EXIF Orientation to a DynamicImage, producing pixels in the
/// natural/upright orientation so downstream encoders (which discard EXIF)
/// still render correctly.
fn apply_orientation(img: image::DynamicImage, orientation: u16) -> image::DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img, // 1 or unknown: no-op
    }
}

pub fn process_thumbnail(
    cache_dir: &Path,
    input: &ThumbnailInput,
) -> Result<ThumbnailResult, String> {
    // Cache key v2: previous cache stored sensor-native (unrotated) pixels with
    // no EXIF, which rendered sideways for portrait shots. v2 stores pixels
    // already rotated to the upright orientation.
    let thumb_filename = format!("{}_v2.jpg", input.id);
    let thumb_path = cache_dir.join(&thumb_filename);

    // Check cache — read dominant color from cached thumbnail (not original)
    if thumb_path.exists() {
        let color = extract_dominant_color_from_file(thumb_path.to_str().unwrap_or(""))?;
        return Ok(ThumbnailResult {
            id: input.id.clone(),
            path: thumb_path.to_string_lossy().to_string(),
            dominant_color: color,
        });
    }

    // Read orientation once from the main JPEG; both strategies share it.
    // The `image` crate does not apply EXIF orientation on decode, and our
    // thumbnail encoder writes no EXIF, so we must bake rotation into pixels.
    let orientation = read_orientation(&input.jpg_path).unwrap_or(1);

    // Strategy 1: Try EXIF embedded thumbnail (fast path, ~5ms)
    if let Some(thumb_bytes) = try_extract_exif_thumbnail(&input.jpg_path) {
        // Validate it's a real JPEG and decode
        if let Ok(thumb_img) =
            image::load_from_memory_with_format(&thumb_bytes, image::ImageFormat::Jpeg)
        {
            // Skip tiny embedded thumbs (typically 160x120). Camera vendors
            // letterbox/pillarbox them to fit a fixed frame, baking black bars
            // into pixels that we can't strip. Anything under 240px on the
            // short side is treated as suspicious and falls through to Strategy 2.
            let (tw, th) = thumb_img.dimensions();
            if tw.min(th) >= 240 {
                let rotated = apply_orientation(thumb_img, orientation);
                let resized = rotated.thumbnail(200, 200);
                resized
                    .save(&thumb_path)
                    .map_err(|e| format!("Failed to save EXIF thumbnail: {}", e))?;

                let color = extract_dominant_color_from_image(&resized)?;
                return Ok(ThumbnailResult {
                    id: input.id.clone(),
                    path: thumb_path.to_string_lossy().to_string(),
                    dominant_color: color,
                });
            }
        }
    }

    // Strategy 2: Full decode fallback
    let img = image::open(&input.jpg_path).map_err(|e| format!("Failed to open image: {}", e))?;

    let rotated = apply_orientation(img, orientation);
    let thumb = rotated.thumbnail(200, 200);
    thumb
        .save(&thumb_path)
        .map_err(|e| format!("Failed to save thumbnail: {}", e))?;

    let color = extract_dominant_color_from_image(&thumb)?;

    Ok(ThumbnailResult {
        id: input.id.clone(),
        path: thumb_path.to_string_lossy().to_string(),
        dominant_color: color,
    })
}

/// Extract dominant color from an existing file (e.g. cached thumbnail)
fn extract_dominant_color_from_file(path: &str) -> Result<String, String> {
    let img = image::open(path).map_err(|e| format!("Failed to open for color: {}", e))?;
    extract_dominant_color_from_image(&img)
}

fn extract_dominant_color_from_image(img: &image::DynamicImage) -> Result<String, String> {
    let small = img.resize_exact(8, 8, image::imageops::FilterType::Lanczos3);

    let mut total_r: u64 = 0;
    let mut total_g: u64 = 0;
    let mut total_b: u64 = 0;
    let mut count: u64 = 0;

    for (_x, _y, pixel) in small.pixels() {
        total_r += pixel[0] as u64;
        total_g += pixel[1] as u64;
        total_b += pixel[2] as u64;
        count += 1;
    }

    if count == 0 {
        return Ok("#3B82F6".to_string());
    }

    let avg_r = (total_r / count) as u8;
    let avg_g = (total_g / count) as u8;
    let avg_b = (total_b / count) as u8;

    Ok(format!("#{:02X}{:02X}{:02X}", avg_r, avg_g, avg_b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::jpeg::exif_orientation::ensure_orientation;
    use crate::test_support::{solid_jpeg, write};
    use image::{DynamicImage, Rgb, RgbImage};
    use tempfile::tempdir;

    fn thumbnail_for(jpeg: &[u8]) -> (ThumbnailResult, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let src = write(dir.path(), "photo.jpg", jpeg);
        let input = ThumbnailInput {
            id: "p1".into(),
            jpg_path: src.to_string_lossy().into(),
        };
        (process_thumbnail(dir.path(), &input).unwrap(), dir)
    }

    #[test]
    fn rotates_portrait_photos_upright() {
        // Regression (1.0.2): portrait shots stored as landscape pixels + EXIF
        // Orientation=6 produced sideways thumbnails.
        let jpeg = ensure_orientation(solid_jpeg(400, 300, [90, 90, 90]), 6);
        let (result, _dir) = thumbnail_for(&jpeg);

        let thumb = image::open(&result.path).unwrap();
        assert_eq!(thumb.dimensions(), (150, 200));
    }

    #[test]
    fn keeps_landscape_photos_without_orientation() {
        let (result, _dir) = thumbnail_for(&solid_jpeg(400, 300, [90, 90, 90]));
        assert_eq!(image::open(&result.path).unwrap().dimensions(), (200, 150));
    }

    #[test]
    fn reuses_cached_thumbnail() {
        let dir = tempdir().unwrap();
        let src = write(dir.path(), "photo.jpg", &solid_jpeg(40, 30, [1, 2, 3]));
        let input = ThumbnailInput {
            id: "p1".into(),
            jpg_path: src.to_string_lossy().into(),
        };
        let first = process_thumbnail(dir.path(), &input).unwrap();
        std::fs::remove_file(&src).unwrap();

        let second = process_thumbnail(dir.path(), &input).unwrap();
        assert_eq!(second.path, first.path);
    }

    #[test]
    fn dominant_color_is_average_hex() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([200, 40, 10])));
        assert_eq!(extract_dominant_color_from_image(&img).unwrap(), "#C8280A");
    }

    #[test]
    fn apply_orientation_maps_exif_values_to_transforms() {
        // 2x1 image: red on the left, blue on the right
        let mut img = RgbImage::new(2, 1);
        img.put_pixel(0, 0, Rgb([255, 0, 0]));
        img.put_pixel(1, 0, Rgb([0, 0, 255]));
        let img = DynamicImage::ImageRgb8(img);

        let cw = apply_orientation(img.clone(), 6).to_rgb8();
        assert_eq!(cw.dimensions(), (1, 2));
        assert_eq!(cw.get_pixel(0, 0), &Rgb([255, 0, 0]), "6 = rotate 90° CW");

        let ccw = apply_orientation(img.clone(), 8).to_rgb8();
        assert_eq!(ccw.get_pixel(0, 0), &Rgb([0, 0, 255]), "8 = rotate 90° CCW");

        let flipped = apply_orientation(img.clone(), 2).to_rgb8();
        assert_eq!(flipped.get_pixel(0, 0), &Rgb([0, 0, 255]), "2 = mirror");

        assert_eq!(apply_orientation(img.clone(), 1).to_rgb8(), img.to_rgb8());
    }
}
