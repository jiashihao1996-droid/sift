// ============================================================
// Sift - EXIF Reading Utility
// ============================================================

use crate::models::photo::{Dimensions, ExifData};
use std::fs::{self, File};
use std::io::BufReader;

/// Read the EXIF Orientation (1=normal, 3=180°, 6=CW 90°, 8=CCW 90°, ...)
/// from the primary IFD of a JPEG or TIFF-based RAW file.
/// Returns None if the file isn't EXIF-readable or the tag is absent.
pub fn read_orientation(path: &str) -> Option<u16> {
    let file = File::open(path).ok()?;
    let exif = exif::Reader::new()
        .read_from_container(&mut BufReader::new(file))
        .ok()?;
    match &exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
        .value
    {
        exif::Value::Short(v) => v.first().copied(),
        _ => None,
    }
}

pub fn read_exif_data(jpg_path: &str) -> Result<ExifData, String> {
    let file = File::open(jpg_path).map_err(|e| format!("Failed to open file: {}", e))?;
    let file_size = fs::metadata(jpg_path).map(|m| m.len()).unwrap_or(0);
    let mut reader = BufReader::new(file);

    let exif_reader = exif::Reader::new();
    let exif = exif_reader
        .read_from_container(&mut reader)
        .map_err(|e| format!("Failed to read EXIF: {}", e))?;

    let camera = get_field_string(&exif, exif::Tag::Model);
    let lens = get_field_string(&exif, exif::Tag::LensModel);
    let iso = get_field_u32(&exif, exif::Tag::PhotographicSensitivity);
    let aperture = get_field_rational_string(&exif, exif::Tag::FNumber, "f/");
    let shutter_speed = get_shutter_speed(&exif);
    let focal_length = get_field_rational_string(&exif, exif::Tag::FocalLength, "");
    let exposure_bias = get_exposure_bias(&exif);
    let date_taken = get_field_string(&exif, exif::Tag::DateTimeOriginal);

    let width = get_field_u32(&exif, exif::Tag::PixelXDimension);
    let height = get_field_u32(&exif, exif::Tag::PixelYDimension);

    Ok(ExifData {
        camera,
        lens,
        iso,
        aperture,
        shutter_speed,
        focal_length: if focal_length.is_empty() {
            String::new()
        } else {
            format!("{}mm", focal_length)
        },
        exposure_bias,
        date_taken,
        dimensions: Dimensions { width, height },
        file_size,
    })
}

fn get_field_string(exif: &exif::Exif, tag: exif::Tag) -> String {
    exif.get_field(tag, exif::In::PRIMARY)
        .map(|f| f.display_value().to_string().trim_matches('"').to_string())
        .unwrap_or_default()
}

fn get_field_u32(exif: &exif::Exif, tag: exif::Tag) -> u32 {
    exif.get_field(tag, exif::In::PRIMARY)
        .and_then(|f| match &f.value {
            exif::Value::Short(v) => v.first().map(|&x| x as u32),
            exif::Value::Long(v) => v.first().copied(),
            _ => f.display_value().to_string().parse().ok(),
        })
        .unwrap_or(0)
}

fn get_field_rational_string(exif: &exif::Exif, tag: exif::Tag, prefix: &str) -> String {
    exif.get_field(tag, exif::In::PRIMARY)
        .map(|f| {
            let val = f.display_value().to_string();
            if val.is_empty() {
                String::new()
            } else {
                format!("{}{}", prefix, val)
            }
        })
        .unwrap_or_default()
}

/// Exposure compensation in EV, as the camera recorded it (a signed fraction such as -1/3)
fn get_exposure_bias(exif: &exif::Exif) -> Option<f64> {
    let field = exif.get_field(exif::Tag::ExposureBiasValue, exif::In::PRIMARY)?;
    let ev = match &field.value {
        exif::Value::SRational(v) => v.first().filter(|r| r.denom != 0)?.to_f64(),
        exif::Value::Rational(v) => v.first().filter(|r| r.denom != 0)?.to_f64(),
        _ => return None,
    };
    ev.is_finite().then_some(ev)
}

fn get_shutter_speed(exif: &exif::Exif) -> String {
    exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY)
        .map(|f| {
            let val = f.display_value().to_string();
            if val.is_empty() {
                String::new()
            } else {
                format!("{}s", val)
            }
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{jpeg_with_exif, noise_jpeg, write};
    use exif::experimental::Writer;
    use exif::{Field, In, Rational, SRational, Tag, Value};
    use std::io::Cursor;

    fn jpeg_with_fields(fields: &[Field]) -> Vec<u8> {
        let mut writer = Writer::new();
        for field in fields {
            writer.push_field(field);
        }
        let mut tiff = Cursor::new(Vec::new());
        writer.write(&mut tiff, true).expect("write EXIF");
        jpeg_with_exif(&noise_jpeg(8, 8), &tiff.into_inner())
    }

    fn field(tag: Tag, value: Value) -> Field {
        Field {
            tag,
            ifd_num: In::PRIMARY,
            value,
        }
    }

    fn bias_of(fields: &[Field]) -> Option<f64> {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "a.jpg", &jpeg_with_fields(fields));
        read_exif_data(path.to_str().unwrap())
            .unwrap()
            .exposure_bias
    }

    fn bias(num: i32, denom: i32) -> Field {
        field(
            Tag::ExposureBiasValue,
            Value::SRational(vec![SRational { num, denom }]),
        )
    }

    #[test]
    fn reads_negative_exposure_bias() {
        let ev = bias_of(&[bias(-2, 3)]).unwrap();
        assert!((ev + 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn reads_zero_exposure_bias_as_zero() {
        assert_eq!(bias_of(&[bias(0, 10)]), Some(0.0));
    }

    #[test]
    fn missing_exposure_bias_is_none() {
        let iso = field(Tag::PhotographicSensitivity, Value::Short(vec![100]));
        assert_eq!(bias_of(&[iso]), None);
    }

    #[test]
    fn exposure_bias_with_zero_denominator_is_none() {
        assert_eq!(bias_of(&[bias(1, 0)]), None);
    }

    #[test]
    fn reads_exposure_settings_alongside_bias() {
        let dir = tempfile::tempdir().unwrap();
        let fields = [
            field(
                Tag::FNumber,
                Value::Rational(vec![Rational { num: 9, denom: 1 }]),
            ),
            field(
                Tag::ExposureTime,
                Value::Rational(vec![Rational { num: 1, denom: 320 }]),
            ),
            field(Tag::PhotographicSensitivity, Value::Short(vec![100])),
            bias(1, 3),
        ];
        let path = write(dir.path(), "a.jpg", &jpeg_with_fields(&fields));
        let data = read_exif_data(path.to_str().unwrap()).unwrap();
        assert_eq!(data.aperture, "f/9");
        assert_eq!(data.shutter_speed, "1/320s");
        assert_eq!(data.iso, 100);
        assert!((data.exposure_bias.unwrap() - 1.0 / 3.0).abs() < 1e-9);
    }
}
