//! POSIX TZ-string parsing + evaluation — the TZif **footer**, used to project offset/is_dst/abbreviation
//! for instants *beyond the last explicit transition* (Phase 2).
//!
//! A TZif file's footer is a POSIX TZ string such as `EST5EDT,M3.2.0,M11.1.0` or
//! `GMT0BST,M3.5.0/1,M10.5.0` or `<-03>3<-02>,M3.5.0/-2,M10.5.0/-1`. RFC 9636 says a reader uses it to
//! determine local time after the final recorded transition. This module parses that grammar and answers
//! offset/is_dst/abbreviation at an arbitrary instant by computing the DST start/end transitions for the
//! surrounding years (the same approach glibc/Python's `zoneinfo` use).
//!
//! Grammar (POSIX, with the common extensions tzcode emits):
//! ```text
//!   std offset [dst [offset] [,start[/time],end[/time]]]
//!   name   := alpha{3,} | '<' [+-]?[A-Za-z0-9]{3,} '>'
//!   offset := [+-]hh[:mm[:ss]]        (POSIX sign: POSITIVE means WEST of UTC)
//!   rule   := 'J'n | n | 'M'm.w.d     (Jn=1..365 skip Feb29; n=0..365; Mm.w.d month.week.weekday)
//!   time   := [+-]hh[:mm[:ss]]        (LOCAL wall clock; default 02:00:00; may be negative or >24h)
//! ```

#![forbid(unsafe_code)]

use crate::civil::{civil_from_days, days_from_civil};
use crate::tzif::Observation;

/// A transition rule (when DST starts or ends within a year).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rule {
    /// `Jn` — Julian day 1..365, never counting Feb 29.
    Julian1(i64),
    /// `n` — zero-based day of year 0..365, counting Feb 29.
    ZeroBased(i64),
    /// `Mm.w.d` — month (1..12), week (1..5, 5 = last), weekday (0=Sunday..6).
    MonthWeekDay { m: i64, w: i64, d: i64 },
}

/// A parsed POSIX TZ string. `utoff_*` are seconds EAST of UTC (so `local = UTC + utoff`), already
/// sign-flipped from the POSIX west-positive convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TzString {
    pub std_abbr: String,
    pub std_utoff: i32,
    pub dst: Option<Dst>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dst {
    pub dst_abbr: String,
    pub dst_utoff: i32,
    start: Rule,
    start_time: i64,
    end: Rule,
    end_time: i64,
}

/// Parse a POSIX TZ string. Returns `None` if it is not a well-formed footer (the witness then falls back
/// to the explicit table and flags the instant as footer-governed-but-unprojected).
pub fn parse(s: &str) -> Option<TzString> {
    let b = s.as_bytes();
    let mut i = 0usize;
    let (std_abbr, ni) = parse_name(b, i)?;
    i = ni;
    let (std_posix, ni) = parse_offset(b, i)?; // required for std
    i = ni;
    let std_utoff = -std_posix;
    if i >= b.len() {
        return Some(TzString {
            std_abbr,
            std_utoff,
            dst: None,
        });
    }
    // DST abbreviation
    let (dst_abbr, ni) = parse_name(b, i)?;
    i = ni;
    // optional DST offset; default = std one hour east (utoff_dst = utoff_std + 3600)
    let (dst_utoff, ni) = if i < b.len() && b[i] != b',' {
        let (p, ni) = parse_offset(b, i)?;
        (-p, ni)
    } else {
        (std_utoff + 3600, i)
    };
    i = ni;
    // ,start[/time],end[/time]
    if i >= b.len() || b[i] != b',' {
        return None; // a DST name with no rules is not usable
    }
    i += 1;
    let (start, ni) = parse_rule(b, i)?;
    i = ni;
    let (start_time, ni) = parse_opt_time(b, i);
    i = ni;
    if i >= b.len() || b[i] != b',' {
        return None;
    }
    i += 1;
    let (end, ni) = parse_rule(b, i)?;
    i = ni;
    let (end_time, _ni) = parse_opt_time(b, i);
    Some(TzString {
        std_abbr,
        std_utoff,
        dst: Some(Dst {
            dst_abbr,
            dst_utoff,
            start,
            start_time,
            end,
            end_time,
        }),
    })
}

fn parse_name(b: &[u8], mut i: usize) -> Option<(String, usize)> {
    if i >= b.len() {
        return None;
    }
    if b[i] == b'<' {
        i += 1;
        let start = i;
        while i < b.len() && b[i] != b'>' {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        let name = std::str::from_utf8(&b[start..i]).ok()?.to_string();
        Some((name, i + 1))
    } else {
        let start = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i - start < 3 {
            return None;
        }
        Some((std::str::from_utf8(&b[start..i]).ok()?.to_string(), i))
    }
}

/// Parse `[+-]hh[:mm[:ss]]` into seconds (POSIX west-positive sign, returned as-is).
fn parse_offset(b: &[u8], mut i: usize) -> Option<(i32, usize)> {
    let mut sign = 1i32;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        if b[i] == b'-' {
            sign = -1;
        }
        i += 1;
    }
    let (hh, ni) = parse_uint(b, i)?;
    i = ni;
    let mut secs = hh * 3600;
    if i < b.len() && b[i] == b':' {
        let (mm, ni) = parse_uint(b, i + 1)?;
        i = ni;
        secs += mm * 60;
        if i < b.len() && b[i] == b':' {
            let (ss, ni) = parse_uint(b, i + 1)?;
            i = ni;
            secs += ss;
        }
    }
    Some((sign * secs as i32, i))
}

/// `/time` with default 02:00:00; the time is LOCAL wall and may be signed / exceed 24h.
fn parse_opt_time(b: &[u8], i: usize) -> (i64, usize) {
    if i < b.len() && b[i] == b'/' {
        if let Some((t, ni)) = parse_signed_time(b, i + 1) {
            return (t, ni);
        }
    }
    (7200, i)
}

fn parse_signed_time(b: &[u8], mut i: usize) -> Option<(i64, usize)> {
    let mut sign = 1i64;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        if b[i] == b'-' {
            sign = -1;
        }
        i += 1;
    }
    let (hh, ni) = parse_uint(b, i)?;
    i = ni;
    let mut secs = (hh as i64) * 3600;
    if i < b.len() && b[i] == b':' {
        let (mm, ni) = parse_uint(b, i + 1)?;
        i = ni;
        secs += (mm as i64) * 60;
        if i < b.len() && b[i] == b':' {
            let (ss, ni) = parse_uint(b, i + 1)?;
            i = ni;
            secs += ss as i64;
        }
    }
    Some((sign * secs, i))
}

fn parse_rule(b: &[u8], i: usize) -> Option<(Rule, usize)> {
    if i >= b.len() {
        return None;
    }
    match b[i] {
        b'J' => {
            let (n, ni) = parse_uint(b, i + 1)?;
            Some((Rule::Julian1(n as i64), ni))
        }
        b'M' => {
            let (m, i1) = parse_uint(b, i + 1)?;
            if b.get(i1) != Some(&b'.') {
                return None;
            }
            let (w, i2) = parse_uint(b, i1 + 1)?;
            if b.get(i2) != Some(&b'.') {
                return None;
            }
            let (d, i3) = parse_uint(b, i2 + 1)?;
            Some((
                Rule::MonthWeekDay {
                    m: m as i64,
                    w: w as i64,
                    d: d as i64,
                },
                i3,
            ))
        }
        _ => {
            let (n, ni) = parse_uint(b, i)?;
            Some((Rule::ZeroBased(n as i64), ni))
        }
    }
}

fn parse_uint(b: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let start = i;
    let mut v = 0u64;
    while i < b.len() && b[i].is_ascii_digit() {
        v = v * 10 + (b[i] - b'0') as u64;
        i += 1;
    }
    if i == start {
        None
    } else {
        Some((v, i))
    }
}

/// Weekday of a day-count since the epoch, 0=Sunday..6=Saturday (1970-01-01 was a Thursday = 4).
fn weekday(days: i64) -> i64 {
    (days.rem_euclid(7) + 4).rem_euclid(7)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Convert a `Jn` (1..365, no Feb 29) to (month, day).
fn julian1_to_md(mut n: i64) -> (i64, i64) {
    const ML: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 1;
    while m <= 12 && n > ML[(m - 1) as usize] {
        n -= ML[(m - 1) as usize];
        m += 1;
    }
    (m, n)
}

/// The UTC instant of a rule's transition in `year`, given the offset in effect *just before* it.
fn rule_to_unix(r: &Rule, time: i64, year: i64, utoff_before: i32) -> i64 {
    let (m, d) = match *r {
        Rule::Julian1(n) => julian1_to_md(n),
        Rule::ZeroBased(n) => {
            // 0-based day of year, leap-aware
            let mut rem = n;
            let mut mm = 1;
            loop {
                let dim = days_in_month(year, mm);
                if rem < dim {
                    break;
                }
                rem -= dim;
                mm += 1;
                if mm > 12 {
                    mm = 12;
                    rem = days_in_month(year, 12) - 1;
                    break;
                }
            }
            (mm, rem + 1)
        }
        Rule::MonthWeekDay { m, w, d } => {
            let first = days_from_civil(year, m, 1);
            let wd_first = weekday(first);
            // day-of-month of the first weekday `d`
            let mut dom = 1 + (d - wd_first).rem_euclid(7);
            dom += (w - 1) * 7;
            let dim = days_in_month(year, m);
            while dom > dim {
                dom -= 7; // w == 5 ("last") overshoot
            }
            (m, dom)
        }
    };
    // local wall instant (civil date at midnight + the rule's local time), then to UTC.
    let local = days_from_civil(year, m, d) * 86400 + time;
    local - utoff_before as i64
}

impl TzString {
    fn std_obs(&self) -> Observation {
        Observation {
            utoff: self.std_utoff,
            is_dst: false,
            abbr: self.std_abbr.clone(),
        }
    }
    fn dst_obs(&self, d: &Dst) -> Observation {
        Observation {
            utoff: d.dst_utoff,
            is_dst: true,
            abbr: d.dst_abbr.clone(),
        }
    }

    /// offset/is_dst/abbreviation at UTC instant `t`, via the POSIX rule. Robust across the year boundary:
    /// it materialises the start/end transitions for the years around `t`, sorts them, and takes the state
    /// set by the latest transition at-or-before `t`.
    pub fn observe(&self, t: i64) -> Observation {
        let Some(d) = &self.dst else {
            return self.std_obs();
        };
        // local year of t (std offset is a fine approximation for picking which years to materialise)
        let (y, _, _) = civil_from_days((t + self.std_utoff as i64).div_euclid(86400));
        // (instant, becomes_dst) for the surrounding years
        let mut tr: Vec<(i64, bool)> = Vec::with_capacity(6);
        for yr in (y - 1)..=(y + 1) {
            tr.push((
                rule_to_unix(&d.start, d.start_time, yr, self.std_utoff),
                true,
            ));
            tr.push((rule_to_unix(&d.end, d.end_time, yr, d.dst_utoff), false));
        }
        tr.sort_by_key(|x| x.0);
        // state set by the latest transition <= t
        let mut state_dst = None;
        for (inst, becomes_dst) in &tr {
            if *inst <= t {
                state_dst = Some(*becomes_dst);
            } else {
                break;
            }
        }
        // before the earliest materialised transition, the state is the opposite of that earliest one
        let dst_now = match state_dst {
            Some(s) => s,
            None => !tr.first().map(|x| x.1).unwrap_or(false),
        };
        if dst_now {
            self.dst_obs(d)
        } else {
            self.std_obs()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_est5edt() {
        let z = parse("EST5EDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(z.std_abbr, "EST");
        assert_eq!(z.std_utoff, -5 * 3600);
        let d = z.dst.as_ref().unwrap();
        assert_eq!(d.dst_abbr, "EDT");
        assert_eq!(d.dst_utoff, -4 * 3600); // default std+1h east
    }

    #[test]
    fn est_winter_summer() {
        let z = parse("EST5EDT,M3.2.0,M11.1.0").unwrap();
        // 2027-01-15 UTC -> EST; 2027-07-15 UTC -> EDT (far beyond any explicit table)
        let jan = crate::civil::parse_iso_utc("2027-01-15T12:00:00Z").unwrap();
        let jul = crate::civil::parse_iso_utc("2027-07-15T12:00:00Z").unwrap();
        let a = z.observe(jan);
        assert_eq!((a.utoff, a.is_dst, a.abbr.as_str()), (-18000, false, "EST"));
        let b = z.observe(jul);
        assert_eq!((b.utoff, b.is_dst, b.abbr.as_str()), (-14400, true, "EDT"));
    }

    #[test]
    fn london_and_angle_names() {
        let z = parse("GMT0BST,M3.5.0/1,M10.5.0").unwrap();
        assert_eq!(z.std_utoff, 0);
        assert_eq!(z.dst.as_ref().unwrap().dst_utoff, 3600);
        // angle-bracket numeric names (e.g. -03/-02)
        let z2 = parse("<-03>3<-02>,M3.5.0/-2,M10.5.0/-1").unwrap();
        assert_eq!(z2.std_abbr, "-03");
        assert_eq!(z2.std_utoff, -3 * 3600);
        assert_eq!(z2.dst.as_ref().unwrap().dst_utoff, -2 * 3600);
    }

    #[test]
    fn fixed_zone_no_dst() {
        let z = parse("UTC0").unwrap();
        assert!(z.dst.is_none());
        let o = z.observe(0);
        assert_eq!((o.utoff, o.is_dst), (0, false));
    }

    #[test]
    fn southern_hemisphere_wraps_year() {
        // Sydney-like: DST Oct..Apr (start month > end month)
        let z = parse("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        let jan = crate::civil::parse_iso_utc("2030-01-15T00:00:00Z").unwrap(); // summer (DST)
        let jul = crate::civil::parse_iso_utc("2030-07-15T00:00:00Z").unwrap(); // winter (std)
        assert!(z.observe(jan).is_dst);
        assert!(!z.observe(jul).is_dst);
    }
}
