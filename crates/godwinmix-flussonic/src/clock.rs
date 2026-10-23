//! UTC dates from unix time and back, for segment names, log lines and
//! `EXT-X-PROGRAM-DATE-TIME`. The standard library has no calendar, and a
//! crate for three functions would be the wrong trade.

/// Year, month, day, hour, minute, second and millisecond of a unix time
/// in milliseconds, in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utc {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub milli: u32,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Howard Hinnant's `days_from_civil`.
fn days(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn utc(ms: i64) -> Utc {
    let (day, in_day) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (year, month, d) = civil(day);
    let s = (in_day / 1000) as u32;
    Utc { year, month, day: d, hour: s / 3600, minute: s / 60 % 60, second: s % 60, milli: (in_day % 1000) as u32 }
}

pub fn unix_ms(t: &Utc) -> i64 {
    let secs = days(t.year, t.month, t.day) * 86_400 + i64::from(t.hour * 3600 + t.minute * 60 + t.second);
    secs * 1000 + i64::from(t.milli)
}

/// `2026-10-23T12:00:04.000Z`.
pub fn rfc3339(ms: i64) -> String {
    let t = utc(ms);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", t.year, t.month, t.day, t.hour, t.minute, t.second, t.milli)
}

/// Read `2026-10-23T12:00:04.000Z`, `2026-10-23T12:00:04Z` or one with a
/// `+hh:mm` offset, as a playlist's `EXT-X-PROGRAM-DATE-TIME` writes it.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |a: usize, b: usize| s.get(a..b).and_then(|v| v.parse::<u32>().ok());
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let mut rest = s.get(19..)?;
    let mut milli = 0;
    if let Some(frac) = rest.strip_prefix('.') {
        let digits: String = frac.chars().take_while(char::is_ascii_digit).collect();
        rest = &frac[digits.len()..];
        milli = format!("{digits:0<3}").get(..3)?.parse().ok()?;
    }
    let offset_min: i64 = match rest {
        "Z" | "z" | "" => 0,
        o if o.len() == 6 => {
            let sign = if o.starts_with('-') { -1 } else { 1 };
            let h: i64 = o.get(1..3)?.parse().ok()?;
            let m: i64 = o.get(4..6)?.parse().ok()?;
            sign * (h * 60 + m)
        }
        _ => return None,
    };
    let t = Utc { year: i64::from(year), month, day, hour, minute, second, milli };
    Some(unix_ms(&t) - offset_min * 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let ms = 1_792_756_804_123; // 2026-10-23T12:00:04.123Z
        assert_eq!(rfc3339(ms), "2026-10-23T12:00:04.123Z");
        assert_eq!(parse_rfc3339("2026-10-23T12:00:04.123Z"), Some(ms));
        assert_eq!(parse_rfc3339("2026-10-23T14:00:04.123+02:00"), Some(ms));
        assert_eq!(parse_rfc3339("2026-10-23T12:00:04Z"), Some(ms - 123));
        assert_eq!(utc(0), Utc { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0, milli: 0 });
        assert_eq!(rfc3339(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert!(parse_rfc3339("yesterday").is_none());
    }
}
