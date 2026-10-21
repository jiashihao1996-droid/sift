// ============================================================
// Sift - Thumbnail Generation
// Generates upright thumbnails + dominant color. Prefers a preview the
// camera embedded in the JPEG (MPF large thumbnail, then the EXIF
// thumbnail) and only decodes the full photo when there is none.
// ============================================================

use crate::media::cache_lru;
use crate::media::jpeg::header::{self, ImageRange, JpegHeader};
use crate::models::photo::{ThumbnailInput, ThumbnailResult};
use image::{DynamicImage, GenericImageView, ImageFormat};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Long edge of a thumbnail, in pixels
const THUMB_SIZE: u32 = 200;
/// Embedded images with a shorter short side are skipped: cameras letterbox
/// their 160x120 EXIF thumbnails, baking black bars into the pixels
const MIN_EMBEDDED_SHORT_SIDE: u32 = 240;
/// An embedded preview whose aspect ratio differs from the photo's by more
/// than this is letterboxed or cropped differently, so it is skipped
const ASPECT_TOLERANCE: f64 = 0.02;
/// Larger "previews" come from corrupt offsets; reading them is not worth it
const MAX_EMBEDDED_LEN: u64 = 16 * 1024 * 1024;

/// Half the cores: thumbnails stream in while the user starts culling, and the
/// viewer needs the rest to decode photos. None if the pool cannot be built,
/// in which case rayon's global pool is used.
static POOL: Lazy<Option<ThreadPool>> = Lazy::new(|| {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    ThreadPoolBuilder::new()
        .num_threads((cores / 2).max(1))
        .thread_name(|i| format!("sift-thumbnail-{i}"))
        .build()
        .ok()
});

/// Generate thumbnails for all inputs into `cache_dir`, in parallel.
/// Inputs that fail (unreadable/corrupt images) are left out of the result.
pub fn generate_thumbnails(inputs: &[ThumbnailInput], cache_dir: &Path) -> Vec<ThumbnailResult> {
    let run = || {
        inputs
            .par_iter()
            .filter_map(|input| process_thumbnail(cache_dir, input).ok())
            .collect()
    };
    match POOL.as_ref() {
        Some(pool) => pool.install(run),
        None => run(),
    }
}

pub fn process_thumbnail(
    cache_dir: &Path,
    input: &ThumbnailInput,
) -> Result<ThumbnailResult, String> {
    let source = Path::new(&input.jpg_path);
    let thumb_path = cache_dir.join(cache_filename(source)?);

    let cached = fs::metadata(&thumb_path).ok().filter(fs::Metadata::is_file);
    let dominant_color = if let Some(cached) = cached {
        cache_lru::mark_used(&thumb_path, &cached);
        extract_dominant_color_from_file(&thumb_path)?
    } else {
        let thumb = render_thumbnail(source)?;
        save_atomically(&thumb, &thumb_path)?;
        extract_dominant_color_from_image(&thumb)?
    };

    Ok(ThumbnailResult {
        id: input.id.clone(),
        path: thumb_path.to_string_lossy().to_string(),
        dominant_color,
    })
}

/// Cache file name for `source`, keyed by the file rather than the per-scan
/// pair id: reopening a folder reuses its thumbnails, while a photo that was
/// replaced or edited (new size or modification time) gets a new one.
/// v3: v2 was keyed by pair id; v1 stored unrotated pixels.
fn cache_filename(source: &Path) -> Result<String, String> {
    let meta =
        fs::metadata(source).map_err(|e| format!("Failed to read {}: {}", source.display(), e))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let mut hasher = DefaultHasher::new();
    (source, meta.len(), modified).hash(&mut hasher);
    Ok(format!("{:016x}_v3.jpg", hasher.finish()))
}

/// Write via a temporary file so a scan running at the same time never reads
/// a half-written thumbnail of the same photo
fn save_atomically(thumb: &DynamicImage, path: &Path) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let saved = thumb
        .save_with_format(&temp, ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to save thumbnail: {}", e))
        .and_then(|()| {
            fs::rename(&temp, path).map_err(|e| format!("Failed to save thumbnail: {}", e))
        });
    if saved.is_err() {
        // Only ever our own temporary file inside the thumbnail cache
        let _ = fs::remove_file(&temp);
    }
    saved
}

/// Upright thumbnail of the photo at `source`
fn render_thumbnail(source: &Path) -> Result<DynamicImage, String> {
    let file = File::open(source).map_err(|e| format!("Failed to open image: {}", e))?;
    // A header we cannot parse just means no embedded preview: decode fully
    let header = header::read_header(&file).ok();
    let exif = header.as_ref().and_then(parse_exif);
    // The `image` crate ignores EXIF orientation and our thumbnails carry no
    // EXIF, so the rotation is baked into the pixels
    let orientation = exif.as_ref().and_then(orientation_of).unwrap_or(1);

    let embedded = header
        .as_ref()
        .and_then(|h| embedded_preview(&file, h, exif.as_ref()));
    let image = match embedded {
        Some(preview) => preview,
        None => image::open(source).map_err(|e| format!("Failed to open image: {}", e))?,
    };
    Ok(apply_orientation(image, orientation).thumbnail(THUMB_SIZE, THUMB_SIZE))
}

/// The best preview the camera embedded: the MPF large thumbnail (~1616x1080
/// on most cameras, a fraction of the photo's pixels), then the EXIF thumbnail
fn embedded_preview(
    file: &File,
    header: &JpegHeader,
    exif: Option<&exif::Exif>,
) -> Option<DynamicImage> {
    header
        .mpf_previews()
        .into_iter()
        .chain(exif.and_then(|e| exif_thumbnail(e, header)))
        .filter(|range| range.len > 0 && range.len <= MAX_EMBEDDED_LEN)
        .filter_map(|range| header::read_range(file, range).ok())
        .filter_map(|bytes| image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).ok())
        .find(|preview| fits_photo(preview.dimensions(), header.dimensions))
}

/// Whether an embedded image is big enough and shows the whole photo
fn fits_photo((width, height): (u32, u32), photo: Option<(u32, u32)>) -> bool {
    if width.min(height) < MIN_EMBEDDED_SHORT_SIDE {
        return false;
    }
    let Some((photo_width, photo_height)) = photo.filter(|&(w, h)| w > 0 && h > 0) else {
        return true;
    };
    let ratio =
        (f64::from(width) / f64::from(height)) / (f64::from(photo_width) / f64::from(photo_height));
    (ratio - 1.0).abs() <= ASPECT_TOLERANCE
}

fn parse_exif(header: &JpegHeader) -> Option<exif::Exif> {
    exif::Reader::new()
        .read_raw(header.exif_tiff()?.to_vec())
        .ok()
}

fn orientation_of(exif: &exif::Exif) -> Option<u16> {
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    field.value.get_uint(0).and_then(|v| u16::try_from(v).ok())
}

/// The JPEG thumbnail in IFD1, as a range of the file
fn exif_thumbnail(exif: &exif::Exif, header: &JpegHeader) -> Option<ImageRange> {
    let uint = |tag| exif.get_field(tag, exif::In::THUMBNAIL)?.value.get_uint(0);
    let offset = uint(exif::Tag::JPEGInterchangeFormat)?;
    let len = uint(exif::Tag::JPEGInterchangeFormatLength)?;
    header.exif_range(u64::from(offset), u64::from(len))
}

/// Apply EXIF Orientation to a DynamicImage, producing pixels in the
/// natural/upright orientation so downstream encoders (which discard EXIF)
/// still render correctly.
fn apply_orientation(img: DynamicImage, orientation: u16) -> DynamicImage {
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

/// Extract dominant color from an existing file (e.g. cached thumbnail)
fn extract_dominant_color_from_file(path: &Path) -> Result<String, String> {
    let img = image::open(path).map_err(|e| format!("Failed to open for color: {}", e))?;
    extract_dominant_color_from_image(&img)
}

fn extract_dominant_color_from_image(img: &DynamicImage) -> Result<String, String> {
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
    use crate::test_support::{jpeg_with_mpf_preview, solid_jpeg, write};
    use image::{DynamicImage, Rgb, RgbImage};
    use tempfile::tempdir;

    /// Times generating thumbnails for every JPG in a folder, then again
    /// from the cache (`SIFT_BENCH_CACHE` keeps the cache in that folder
    /// across runs of the benchmark, like a restart):
    /// `SIFT_THUMB_BENCH_DIR=~/photos cargo test --release thumbnail_benchmark -- --ignored --nocapture`
    #[test]
    #[ignore = "needs SIFT_THUMB_BENCH_DIR"]
    fn thumbnail_benchmark() {
        let dir = std::env::var("SIFT_THUMB_BENCH_DIR").expect("set SIFT_THUMB_BENCH_DIR");
        let temp_cache = tempdir().unwrap();
        let cache = std::env::var("SIFT_BENCH_CACHE")
            .map_or(temp_cache.path().into(), std::path::PathBuf::from);
        fs::create_dir_all(&cache).unwrap();
        let inputs: Vec<ThumbnailInput> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg")))
            .enumerate()
            .map(|(i, p)| ThumbnailInput {
                id: i.to_string(),
                jpg_path: p.to_string_lossy().into(),
            })
            .collect();
        for run in 1..=2 {
            let started = std::time::Instant::now();
            let done = generate_thumbnails(&inputs, &cache).len();
            println!("run {run}: {done} thumbnails in {:?}", started.elapsed());
        }
    }

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

    fn input(id: &str, path: &Path) -> ThumbnailInput {
        ThumbnailInput {
            id: id.into(),
            jpg_path: path.to_string_lossy().into(),
        }
    }

    /// (red, blue) of the thumbnail's centre pixel
    fn centre_red_blue(result: &ThumbnailResult) -> (u8, u8) {
        let thumb = image::open(&result.path).unwrap().to_rgb8();
        let Rgb([r, _, b]) = *thumb.get_pixel(thumb.width() / 2, thumb.height() / 2);
        (r, b)
    }

    const RED: [u8; 3] = [255, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    #[test]
    fn uses_the_mpf_preview_instead_of_decoding_the_photo() {
        let jpeg = jpeg_with_mpf_preview(&solid_jpeg(600, 400, RED), &solid_jpeg(450, 300, BLUE));
        let (result, _dir) = thumbnail_for(&jpeg);

        let (r, b) = centre_red_blue(&result);
        assert!(
            b > 200 && r < 50,
            "thumbnail came from the photo, not the preview"
        );
    }

    #[test]
    fn rotates_the_mpf_preview_by_the_photo_orientation() {
        let photo = ensure_orientation(solid_jpeg(600, 400, RED), 6);
        let jpeg = jpeg_with_mpf_preview(&photo, &solid_jpeg(450, 300, BLUE));
        let (result, _dir) = thumbnail_for(&jpeg);

        assert!(
            centre_red_blue(&result).1 > 200,
            "thumbnail came from the photo"
        );
        assert_eq!(image::open(&result.path).unwrap().dimensions(), (133, 200));
    }

    #[test]
    fn ignores_a_preview_with_another_aspect_ratio() {
        let jpeg = jpeg_with_mpf_preview(&solid_jpeg(600, 400, RED), &solid_jpeg(300, 300, BLUE));
        let (result, _dir) = thumbnail_for(&jpeg);

        let (r, b) = centre_red_blue(&result);
        assert!(r > 200 && b < 50, "letterboxed preview was used");
    }

    #[test]
    fn ignores_a_preview_too_small_for_a_thumbnail() {
        let jpeg = jpeg_with_mpf_preview(&solid_jpeg(600, 400, RED), &solid_jpeg(150, 100, BLUE));
        let (result, _dir) = thumbnail_for(&jpeg);

        let (r, b) = centre_red_blue(&result);
        assert!(r > 200 && b < 50, "tiny preview was used");
    }

    #[test]
    fn reuses_the_thumbnail_of_the_same_file_across_scans() {
        let photos = tempdir().unwrap();
        let cache = tempdir().unwrap();
        let src = write(photos.path(), "photo.jpg", &solid_jpeg(40, 30, [1, 2, 3]));

        // Every scan gives the pair a new id
        let first = process_thumbnail(cache.path(), &input("scan-1", &src)).unwrap();
        let second = process_thumbnail(cache.path(), &input("scan-2", &src)).unwrap();

        assert_eq!(second.id, "scan-2");
        assert_eq!(second.path, first.path);
        assert_eq!(fs::read_dir(cache.path()).unwrap().count(), 1);
    }

    #[test]
    fn makes_a_new_thumbnail_when_the_photo_changes() {
        let photos = tempdir().unwrap();
        let cache = tempdir().unwrap();
        let src = write(photos.path(), "photo.jpg", &solid_jpeg(40, 30, RED));
        let before = process_thumbnail(cache.path(), &input("p1", &src)).unwrap();

        write(photos.path(), "photo.jpg", &solid_jpeg(60, 40, BLUE));
        let after = process_thumbnail(cache.path(), &input("p1", &src)).unwrap();

        assert_ne!(after.path, before.path);
        assert!(centre_red_blue(&after).1 > 200);
    }

    #[test]
    fn embedded_images_must_be_big_enough_and_match_the_photo() {
        assert!(fits_photo((1616, 1080), Some((4608, 3072))));
        assert!(!fits_photo((160, 120), Some((4608, 3072))), "too small");
        assert!(
            !fits_photo((1920, 1080), Some((4608, 3072))),
            "16:9 frame of a 3:2 photo"
        );
        assert!(fits_photo((640, 480), None), "photo size unknown");
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
