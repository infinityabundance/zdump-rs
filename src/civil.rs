//! Proleptic-Gregorian civil-time arithmetic, dependency-free.
//!
//! We need to turn an ISO-8601 UTC instant (`YYYY-MM-DDTHH:MM:SSZ`) into a Unix timestamp and back,
//! without pulling in `chrono`/`time` (the zic-rs minimal-dependency doctrine). The algorithm is Howard
//! Hinnant's `days_from_civil` / `civil_from_days` (public domain), which is exact over the proleptic
//! Gregorian calendar for the full `i64` range we care about — well beyond the 1901..2038 and far-future
//! probe instants the witness uses.

#![forbid(unsafe_code)]

/// Days since the Unix epoch (1970-01-01) for a proleptic-Gregorian date. `m` is 1..=12.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Inverse of [`days_from_civil`]: returns `(year, month, day)`.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse `YYYY-MM-DDTHH:MM:SSZ` (or trailing `Z` optional, space or `T` separator) into a Unix timestamp.
/// Deliberately strict and minimal — a witness input, not a general date parser.
pub fn parse_iso_utc(s: &str) -> Result<i64, String> {
    let s = s.trim().trim_end_matches('Z');
    let (date, time) = match s.split_once(['T', ' ']) {
        Some((d, t)) => (d, t),
        None => (s, "00:00:00"),
    };
    let dp: Vec<&str> = date.split('-').collect();
    if dp.len() != 3 {
        return Err(format!("bad date (want YYYY-MM-DD): {date:?}"));
    }
    let neg = dp[0].starts_with('-');
    let y: i64 = date_num(dp[0])?;
    let mo: i64 = date_num(dp[1])?;
    let da: i64 = date_num(dp[2])?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) {
        return Err(format!("month/day out of range in {date:?}"));
    }
    let tp: Vec<&str> = time.split(':').collect();
    if tp.is_empty() || tp.len() > 3 {
        return Err(format!("bad time (want HH:MM:SS): {time:?}"));
    }
    let hh: i64 = date_num(tp[0])?;
    let mm: i64 = if tp.len() > 1 { date_num(tp[1])? } else { 0 };
    let ss: i64 = if tp.len() > 2 { date_num(tp[2])? } else { 0 };
    if hh > 23 || mm > 59 || ss > 60 {
        return Err(format!("time component out of range in {time:?}"));
    }
    let _ = neg;
    let days = days_from_civil(y, mo, da);
    Ok(days * 86400 + hh * 3600 + mm * 60 + ss)
}

fn date_num(s: &str) -> Result<i64, String> {
    s.parse::<i64>()
        .map_err(|_| format!("not an integer: {s:?}"))
}

/// Format a Unix timestamp as `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub fn format_iso_utc(t: i64) -> String {
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn epoch_roundtrips() {
        assert_eq!(parse_iso_utc("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(format_iso_utc(0), "1970-01-01T00:00:00Z");
    }
    #[test]
    fn known_instants() {
        // 2026-01-01T00:00:00Z — independently checkable
        let t = parse_iso_utc("2026-01-01T00:00:00Z").unwrap();
        assert_eq!(format_iso_utc(t), "2026-01-01T00:00:00Z");
        // round-trip a far-future and a pre-epoch instant
        for s in [
            "2038-01-19T03:14:07Z",
            "1901-12-13T20:45:52Z",
            "2200-07-01T12:00:00Z",
        ] {
            assert_eq!(format_iso_utc(parse_iso_utc(s).unwrap()), s);
        }
    }
}
