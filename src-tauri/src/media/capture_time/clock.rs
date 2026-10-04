// ============================================================
// Sift - EXIF Clock Values
// Turns EXIF's date/time strings ("2026:09:15 10:23:45", "120",
// "+08:00") into a CaptureTime. The strings come from the photo file,
// so anything malformed yields None rather than a guess.
// ============================================================

use crate::models::photo::CaptureTime;

const MS_PER_SECOND: i64 = 1_000;
const SECONDS_PER_DAY: i64 = 86_400;
/// Longest real UTC offsets are +14:00 and -12:00
const MAX_OFFSET_MINUTES: i16 = 14 * 60;

/// Combine EXIF DateTimeOriginal with its optional SubSecTime and OffsetTime
pub fn parse(datetime: &str, subsec: Option<&str>, offset: Option<&str>) -> Option<CaptureTime> {
    let seconds = parse_datetime(datetime)?;
    Some(CaptureTime {
        local_ms: seconds * MS_PER_SECOND + subsec.map_or(0, parse_subsec_ms),
        offset_minutes: offset.and_then(parse_offset),
    })
}

/// "YYYY:MM:DD HH:MM:SS" as seconds since 1970-01-01 00:00. Cameras without
/// a set clock write zeros or blanks, which are not a time.
fn parse_datetime(text: &str) -> Option<i64> {
    let b = text.trim().as_bytes();
    if b.len() < 19
        || !matches!(b[4], b':' | b'-')
        || !matches!(b[7], b':' | b'-')
        || !matches!(b[10], b' ' | b'T')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let field = |at: usize, len: usize| -> Option<i64> {
        let digits = b.get(at..at + len)?;
        digits.iter().try_fold(0i64, |acc, &d| {
            d.is_ascii_digit().then(|| acc * 10 + i64::from(d - b'0'))
        })
    };
    let (year, month, day) = (field(0, 4)?, field(5, 2)?, field(8, 2)?);
    let (hour, minute, second) = (field(11, 2)?, field(14, 2)?, field(17, 2)?);
    let is_valid = year >= 1
        && (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
        && hour < 24
        && minute < 60
        && second < 60;
    if !is_valid {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(days * SECONDS_PER_DAY + hour * 3_600 + minute * 60 + second)
}

/// Leading digits of a fraction of a second ("5" = 500 ms, "0123" = 12 ms)
fn parse_subsec_ms(text: &str) -> i64 {
    let digits: Vec<i64> = text
        .trim()
        .bytes()
        .take_while(u8::is_ascii_digit)
        .take(3)
        .map(|d| i64::from(d - b'0'))
        .collect();
    let scale = [100, 10, 1];
    digits.iter().zip(scale).map(|(d, s)| d * s).sum()
}

/// "+08:00" / "-05:30" as minutes east of UTC
fn parse_offset(text: &str) -> Option<i16> {
    let b = text.trim().as_bytes();
    if b.len() != 6 || b[3] != b':' {
        return None;
    }
    let sign = match b[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let two = |at: usize| -> Option<i16> {
        let (hi, lo) = (b[at], b[at + 1]);
        (hi.is_ascii_digit() && lo.is_ascii_digit())
            .then(|| i16::from(hi - b'0') * 10 + i16::from(lo - b'0'))
    };
    let (hours, minutes) = (two(1)?, two(4)?);
    let total = hours * 60 + minutes;
    (minutes < 60 && total <= MAX_OFFSET_MINUTES).then_some(sign * total)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// days_from_civil)
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_ms(datetime: &str) -> Option<i64> {
        parse(datetime, None, None).map(|t| t.local_ms)
    }

    #[test]
    fn reads_the_camera_clock_as_milliseconds_since_the_epoch() {
        assert_eq!(local_ms("1970:01:01 00:00:00"), Some(0));
        // 2026-09-15 10:23:45 = 20,711 days after the epoch
        assert_eq!(
            local_ms("2026:09:15 10:23:45"),
            Some((20_711 * 86_400 + 10 * 3_600 + 23 * 60 + 45) * 1_000)
        );
    }

    #[test]
    fn handles_leap_days_and_dates_before_march() {
        let feb_29 = local_ms("2024:02:29 00:00:00").unwrap();
        let mar_01 = local_ms("2024:03:01 00:00:00").unwrap();
        assert_eq!(mar_01 - feb_29, 86_400_000);
        assert_eq!(local_ms("2023:02:29 00:00:00"), None);
    }

    #[test]
    fn adds_sub_seconds_as_a_decimal_fraction() {
        let base = local_ms("2026:09:15 10:23:45").unwrap();
        let with = |subsec: &str| parse("2026:09:15 10:23:45", Some(subsec), None).unwrap();
        assert_eq!(with("5").local_ms, base + 500);
        assert_eq!(with("12").local_ms, base + 120);
        assert_eq!(with("0123").local_ms, base + 12);
        assert_eq!(with(" 37 ").local_ms, base + 370);
        assert_eq!(with("").local_ms, base);
    }

    #[test]
    fn reads_the_utc_offset_in_minutes() {
        let offset = |text: &str| parse("2026:09:15 10:23:45", None, Some(text)).unwrap();
        assert_eq!(offset("+08:00").offset_minutes, Some(480));
        assert_eq!(offset("-05:30").offset_minutes, Some(-330));
        assert_eq!(offset("   :  ").offset_minutes, None);
        assert_eq!(offset("+15:00").offset_minutes, None);
    }

    #[test]
    fn accepts_dashes_and_a_t_separator_some_writers_use() {
        assert_eq!(
            local_ms("2026-09-15T10:23:45"),
            local_ms("2026:09:15 10:23:45")
        );
    }

    #[test]
    fn rejects_unset_clocks_and_malformed_values() {
        for text in [
            "0000:00:00 00:00:00",
            "                   ",
            "2026:13:01 00:00:00",
            "2026:09:31 00:00:00",
            "2026:09:15 24:00:00",
            "2026:09:15 10:23",
            "2026/09/15 10:23:45",
            "2026:09:15 1a:23:45",
        ] {
            assert_eq!(local_ms(text), None, "{text:?}");
        }
    }
}
