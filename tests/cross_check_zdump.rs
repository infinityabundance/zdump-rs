//! Cross-check the independent witness against reference `zdump` for the pinned fixtures.
//!
//! `zdump -v -c LO,HI <file>` prints, for each printed UT instant, the offset/isdst/abbreviation in effect
//! there. We parse those lines into `(unix, gmtoff, isdst, abbr)` samples, then for a set of probe instants
//! inside the window we look up the latest sample at-or-before the probe and assert zdump-rs's observation
//! matches it. If `zdump` is absent the test SKIPS with a reason (oracle-availability discipline — a test
//! must never silently weaken when the reference tool is missing).

use std::process::Command;
use zdump_rs::civil::parse_iso_utc;
use zdump_rs::parse;

fn zdump_available() -> bool {
    Command::new("zdump")
        .arg("--version")
        .output()
        .map(|o| o.status.success() || !o.stderr.is_empty())
        .unwrap_or(false)
}

fn month(m: &str) -> Option<i64> {
    Some(match m {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    })
}

/// Parse one `zdump -v` line into (unix_of_UT_instant, gmtoff, isdst, abbr) if it is a transition line.
fn parse_line(line: &str) -> Option<(i64, i32, bool, String)> {
    // ... "Www Mmm DD HH:MM:SS YYYY UT = Www Mmm DD HH:MM:SS YYYY ABBR isdst=N gmtoff=S"
    let ut = line.find(" UT = ")?;
    let left = &line[..ut];
    let toks: Vec<&str> = left.split_whitespace().collect();
    // last 5 tokens of the left side: Www Mmm DD HH:MM:SS YYYY
    if toks.len() < 5 {
        return None;
    }
    let n = toks.len();
    let mon = month(toks[n - 4])?;
    let day: i64 = toks[n - 3].parse().ok()?;
    let hms: Vec<&str> = toks[n - 2].split(':').collect();
    if hms.len() != 3 {
        return None;
    }
    let (hh, mm, ss): (i64, i64, i64) = (
        hms[0].parse().ok()?,
        hms[1].parse().ok()?,
        hms[2].parse().ok()?,
    );
    let year: i64 = toks[n - 1].parse().ok()?;
    let unix = zdump_rs::civil::days_from_civil(year, mon, day) * 86400 + hh * 3600 + mm * 60 + ss;
    let gmtoff: i32 = line
        .split("gmtoff=")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let isdst = line.split("isdst=").nth(1)?.starts_with('1');
    // abbr sits just before " isdst="
    let pre = line.split(" isdst=").next()?;
    let abbr = pre.split_whitespace().last()?.to_string();
    Some((unix, gmtoff, isdst, abbr))
}

fn cross_check(zone: &str) {
    let tzif = format!("fixtures/{zone}.tzif");
    // zdump treats a bare arg as a zone NAME relative to TZDIR; an absolute path makes it read our file.
    let abs = std::fs::canonicalize(&tzif).expect("canonicalize fixture");
    let out = Command::new("zdump")
        .args(["-v", "-c", "1902,2037"])
        .arg(&abs)
        .output()
        .expect("run zdump");
    let text = String::from_utf8_lossy(&out.stdout);
    let mut samples: Vec<(i64, i32, bool, String)> = text.lines().filter_map(parse_line).collect();
    samples.sort_by_key(|s| s.0);
    assert!(!samples.is_empty(), "no zdump samples parsed for {zone}");

    let bytes = std::fs::read(&tzif).unwrap();
    let z = parse(&bytes).unwrap();

    // probe instants strictly inside the window, away from transition seconds
    let probes = [
        "1933-06-01T12:00:00Z",
        "1970-01-01T00:00:00Z",
        "2000-06-01T00:00:00Z",
        "2026-01-15T00:00:00Z",
        "2026-07-15T00:00:00Z",
        "2030-02-01T00:00:00Z",
    ];
    let mut compared = 0;
    for p in probes {
        let t = parse_iso_utc(p).unwrap();
        // latest zdump sample at-or-before t
        let Some(s) = samples.iter().rev().find(|s| s.0 <= t) else {
            continue;
        };
        let o = z.observe(t);
        assert_eq!(
            o.utoff, s.1,
            "{zone} @ {p}: gmtoff zdump={} zdump-rs={}",
            s.1, o.utoff
        );
        assert_eq!(o.is_dst, s.2, "{zone} @ {p}: isdst mismatch");
        assert_eq!(
            o.abbr, s.3,
            "{zone} @ {p}: abbr zdump={} zdump-rs={}",
            s.3, o.abbr
        );
        compared += 1;
    }
    assert!(
        compared >= 3,
        "{zone}: too few probes compared ({compared})"
    );
    eprintln!("cross_check {zone}: {compared} probe instants matched reference zdump");
}

#[test]
fn matches_reference_zdump_or_skips() {
    if !zdump_available() {
        eprintln!("SKIP: reference `zdump` not found on PATH (oracle unavailable) — golden test still pins output");
        return;
    }
    cross_check("America_New_York");
    cross_check("Europe_London");
    cross_check("UTC");
}
