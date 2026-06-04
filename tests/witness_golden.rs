//! Deterministic golden replay: the committed `fixtures/<zone>.witness.json` files are the contract. We
//! re-read the pinned TZif bytes, evaluate the declared probe-default instants, and assert the regenerated
//! JSON array is byte-identical to the golden. No oracle needed — this pins the witness output forever.

use std::path::Path;
use zdump_rs::civil::parse_iso_utc;
use zdump_rs::witness::rows_to_json_array;
use zdump_rs::{parse, WitnessRow, PROBE_DEFAULT};

fn regen(zone: &str) -> String {
    let tzif = format!("fixtures/{zone}.tzif");
    let bytes = std::fs::read(&tzif).unwrap_or_else(|e| panic!("read {tzif}: {e}"));
    let z = parse(&bytes).expect("parse fixture");
    let rows: Vec<WitnessRow> = PROBE_DEFAULT
        .iter()
        .map(|a| WitnessRow::build(&tzif, &z, parse_iso_utc(a).unwrap()))
        .collect();
    rows_to_json_array(&rows)
}

fn check(zone: &str) {
    let golden_path = format!("fixtures/{zone}.witness.json");
    let golden =
        std::fs::read_to_string(&golden_path).unwrap_or_else(|e| panic!("read {golden_path}: {e}"));
    let got = regen(zone);
    assert_eq!(got, golden, "witness output drifted from golden for {zone}");
}

#[test]
fn golden_new_york() {
    assert!(Path::new("fixtures/America_New_York.witness.json").exists());
    check("America_New_York");
}

#[test]
fn golden_london() {
    check("Europe_London");
}

#[test]
fn golden_utc() {
    check("UTC");
}

#[test]
fn golden_vancouver() {
    // the zone that actually changed in the real 2026a->2026b release-diff
    check("America_Vancouver");
}

#[test]
fn golden_right_zones() {
    // right/ (leap) profile fixtures — offset/is_dst/abbr are leap-independent, so the witness rows pin
    check("right_America_New_York");
    check("right_Europe_London");
    check("right_Etc_UTC");
}

#[test]
fn golden_transitions_new_york() {
    let golden = std::fs::read_to_string("fixtures/America_New_York.transitions.json").unwrap();
    let bytes = std::fs::read("fixtures/America_New_York.tzif").unwrap();
    let z = parse(&bytes).unwrap();
    let lo = zdump_rs::civil::days_from_civil(2035, 1, 1) * 86400;
    let hi = zdump_rs::civil::days_from_civil(2038, 1, 1) * 86400;
    let trs = z.transitions_in(lo, hi);
    // reproduce main's transitions JSON shell for the same window
    let mut s = format!(
        "{{\"zone\":{:?},\"from_year\":2035,\"to_year\":2037,\"footer\":{:?},\"transitions\":[\n",
        "fixtures/America_New_York.tzif",
        z.footer.as_deref().unwrap()
    );
    for (k, tr) in trs.iter().enumerate() {
        let comma = if k + 1 < trs.len() { "," } else { "" };
        s.push_str(&format!(
            "  {}{comma}\n",
            zdump_rs::witness::transition_to_json("fixtures/America_New_York.tzif", tr)
        ));
    }
    s.push_str("]}\n");
    assert_eq!(s, golden, "transitions golden drifted");
}

#[test]
fn right_utc_exposes_27_plus_leaps() {
    let bytes = std::fs::read("fixtures/right_Etc_UTC.tzif").unwrap();
    let z = parse(&bytes).unwrap();
    assert!(
        z.leaps.len() >= 27,
        "right/UTC should carry the full leap table, got {}",
        z.leaps.len()
    );
    // corrections are cumulative and monotonically increasing
    for w in z.leaps.windows(2) {
        assert!(
            w[1].corr >= w[0].corr && w[1].occur > w[0].occur,
            "leap table not monotonic"
        );
    }
}

#[test]
fn utc_is_always_zero_offset() {
    let bytes = std::fs::read("fixtures/UTC.tzif").unwrap();
    let z = parse(&bytes).unwrap();
    for a in PROBE_DEFAULT {
        let o = z.observe(parse_iso_utc(a).unwrap());
        assert_eq!(o.utoff, 0, "UTC must be 0 at {a}");
        assert!(!o.is_dst);
    }
}
