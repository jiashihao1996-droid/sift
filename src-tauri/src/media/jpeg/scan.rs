// ============================================================
// Sift - JPEG Stream Scanning
// Locates complete JPEG streams (SOI..EOI) inside arbitrary bytes,
// e.g. previews embedded in non-TIFF RAW containers (CR3, RAF).
// ============================================================

/// Find the largest complete JPEG stream in `data` that is at least `min_len` bytes.
///
/// Candidates start at SOI (FF D8) immediately followed by a plausible marker,
/// and end at the matching EOI (FF D9) found by walking marker segments.
pub fn find_largest_jpeg(data: &[u8], min_len: usize) -> Option<&[u8]> {
    let mut best: Option<&[u8]> = None;
    let mut i: usize = 0;

    while i < data.len().saturating_sub(3) {
        let is_soi = data[i] == 0xFF && data[i + 1] == 0xD8;
        // The byte after SOI must be FF followed by a valid JPEG marker
        if is_soi && data[i + 2] == 0xFF && is_valid_jpeg_marker(data[i + 3]) {
            if let Some(end) = find_jpeg_end(data, i) {
                let candidate = &data[i..end];
                if candidate.len() >= min_len && best.is_none_or(|b| candidate.len() > b.len()) {
                    best = Some(candidate);
                }
                // Skip past this JPEG to find potentially larger ones
                i = end;
                continue;
            }
        }
        i += 1;
    }

    best
}

/// Check if a byte is a valid JPEG marker that can appear right after SOI
fn is_valid_jpeg_marker(marker: u8) -> bool {
    matches!(
        marker,
        0xC0 // SOF0 - Baseline DCT
        | 0xC1 // SOF1 - Extended sequential DCT
        | 0xC2 // SOF2 - Progressive DCT
        | 0xC4 // DHT - Define Huffman Table
        | 0xDB // DQT - Define Quantization Table
        | 0xDD // DRI - Define Restart Interval
        | 0xE0 // APP0 (JFIF)
        | 0xE1 // APP1 (EXIF)
        | 0xE2
            ..=0xEF // APP2-APP15
        | 0xFE // COM - Comment
    )
}

/// Find the end (exclusive) of a JPEG stream starting at `start`.
/// Walks marker segments so that FF D9 bytes inside segment payloads or
/// entropy-coded data are not mistaken for EOI.
fn find_jpeg_end(data: &[u8], start: usize) -> Option<usize> {
    let len = data.len();
    let mut pos = start + 2; // Skip SOI (FF D8)

    while pos < len.saturating_sub(1) {
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }

        match data[pos + 1] {
            // EOI marker found
            0xD9 => return Some(pos + 2),

            // SOS (Start of Scan) — skip its header, then the entropy-coded data
            0xDA => {
                pos = pos + 2 + segment_len(data, pos)?;
                match skip_entropy_coded_data(data, pos) {
                    EntropyEnd::Eoi(end) => return Some(end),
                    EntropyEnd::Marker(next) => pos = next,
                }
            }

            // Standalone markers (no length field): RST0-RST7, SOI, TEM
            0xD0..=0xD7 | 0xD8 | 0x01 => pos += 2,

            // Fill bytes (FF FF)
            0xFF => pos += 1,

            // All other markers have a 2-byte length field
            _ => pos = pos + 2 + segment_len(data, pos)?,
        }
    }

    None
}

enum EntropyEnd {
    /// EOI reached; value is the exclusive end of the stream
    Eoi(usize),
    /// Another marker (or end of data) at this position
    Marker(usize),
}

/// Scan entropy-coded data after SOS, skipping byte-stuffed FF 00 and RST markers.
fn skip_entropy_coded_data(data: &[u8], mut pos: usize) -> EntropyEnd {
    while pos < data.len().saturating_sub(1) {
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }
        match data[pos + 1] {
            0x00 | 0xD0..=0xD7 => pos += 2,
            0xD9 => return EntropyEnd::Eoi(pos + 2),
            _ => break,
        }
    }
    EntropyEnd::Marker(pos)
}

/// Big-endian segment length at `pos` (marker at pos, length at pos+2..pos+4).
fn segment_len(data: &[u8], pos: usize) -> Option<usize> {
    if pos + 3 >= data.len() {
        return None;
    }
    Some(((data[pos + 2] as usize) << 8) | (data[pos + 3] as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::noise_jpeg;

    #[test]
    fn find_jpeg_end_skips_stuffed_bytes_and_restart_markers() {
        let jpeg = [
            0xFF, 0xD8, // SOI
            0xFF, 0xDB, 0x00, 0x04, 0xAA, 0xBB, // DQT, 2-byte payload
            0xFF, 0xDA, 0x00, 0x02, // SOS header
            0x11, 0xFF, 0x00, 0x22, 0xFF, 0xD0, 0x33, // entropy data, FF00 + RST0
            0xFF, 0xD9, // EOI
            0x99, 0x99, // trailing junk
        ];
        assert_eq!(find_jpeg_end(&jpeg, 0), Some(jpeg.len() - 2));
    }

    #[test]
    fn find_jpeg_end_returns_none_without_eoi() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02, 0x11, 0x22];
        assert_eq!(find_jpeg_end(&jpeg, 0), None);
    }

    #[test]
    fn find_largest_jpeg_picks_biggest_stream_between_junk() {
        let small = noise_jpeg(16, 16);
        let big = noise_jpeg(64, 64);
        let mut data = vec![0x00; 100];
        data.extend_from_slice(&small);
        data.extend_from_slice(&[0xAB; 100]);
        data.extend_from_slice(&big);
        data.extend_from_slice(&[0xCD; 100]);

        assert_eq!(find_largest_jpeg(&data, 0), Some(big.as_slice()));
    }

    #[test]
    fn find_largest_jpeg_respects_minimum_length() {
        let jpeg = noise_jpeg(16, 16);
        assert_eq!(find_largest_jpeg(&jpeg, jpeg.len() + 1), None);
        assert_eq!(find_largest_jpeg(&jpeg, jpeg.len()), Some(jpeg.as_slice()));
    }

    #[test]
    fn find_largest_jpeg_ignores_soi_without_valid_marker() {
        assert_eq!(
            find_largest_jpeg(&[0xFF, 0xD8, 0x00, 0x00, 0xFF, 0xD9], 0),
            None
        );
    }
}
