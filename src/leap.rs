//! Leap-second wall rendering for `right/` zones (Phase 3) — the `23:59:60` display.
//!
//! A `right/` TZif stores transition/leap times on a continuous (leap-inclusive) scale. Reference `zdump`
//! renders the inserted leap second as `…23:59:60`. Empirically (verified against `zdump -v` on
//! `right/Etc/UTC` across the 1972..2020 leaps), for a positive leap entry `(occur, corr)` the value
//! `occur - corr` always lands on `HH:MM:59` of the leap day, and the inserted second is shown as
//! `HH:MM:60`. This module reproduces that display and the cumulative-correction lookup.
//!
//! Scope: positive (+1) leaps — every leap second inserted to date. Negative leaps (none have ever
//! occurred) are not rendered specially and are flagged by the caller if encountered.

#![forbid(unsafe_code)]

use crate::civil::civil_from_days;
use crate::tzif::LeapSecond;

/// Cumulative leap correction in effect at a `right/`-scale instant `t` (the corr of the latest leap whose
/// occurrence is at or before `t`). Lets a caller map a `right/` stored time to pure UTC: `utc = t - corr`.
pub fn corr_at(leaps: &[LeapSecond], t: i64) -> i32 {
    let mut c = 0;
    for l in leaps {
        if l.occur <= t {
            c = l.corr;
        } else {
            break;
        }
    }
    c
}

/// The displayed UTC leap-second instant for a positive leap entry, as `YYYY-MM-DDTHH:MM:60Z`.
/// `occur - corr` is the `HH:MM:59` second immediately before the day boundary; the leap is the next
/// (inserted) second, shown with a seconds field of 60.
pub fn displayed_leap(occur: i64, corr: i32) -> String {
    let t = occur - corr as i64;
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    // For a +1 leap, ss == 59 and the inserted second is :60. (The fallback keeps output well-formed if a
    // future table ever lands off a :59 boundary — that case is not a claimed capability.)
    let shown = if ss == 59 { 60 } else { ss };
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{shown:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_two_leaps_render_60() {
        // occur/corr taken from right/Etc/UTC; verified against zdump -v
        assert_eq!(displayed_leap(78796800, 1), "1972-06-30T23:59:60Z");
        assert_eq!(displayed_leap(94694401, 2), "1972-12-31T23:59:60Z");
        assert_eq!(displayed_leap(126230402, 3), "1973-12-31T23:59:60Z");
    }
    #[test]
    fn corr_lookup_is_monotone() {
        let leaps = vec![
            LeapSecond {
                occur: 78796800,
                corr: 1,
            },
            LeapSecond {
                occur: 94694401,
                corr: 2,
            },
        ];
        assert_eq!(corr_at(&leaps, 0), 0);
        assert_eq!(corr_at(&leaps, 78796800), 1);
        assert_eq!(corr_at(&leaps, 94694401), 2);
        assert_eq!(corr_at(&leaps, 99999999999), 2);
    }
}
