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
fn utc_is_always_zero_offset() {
    let bytes = std::fs::read("fixtures/UTC.tzif").unwrap();
    let z = parse(&bytes).unwrap();
    for a in PROBE_DEFAULT {
        let o = z.observe(parse_iso_utc(a).unwrap());
        assert_eq!(o.utoff, 0, "UTC must be 0 at {a}");
        assert!(!o.is_dst);
    }
}
