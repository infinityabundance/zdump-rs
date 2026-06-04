//! Deterministic JSON witness rows — the evidence surface this tool exists to produce.
//!
//! A *witness row* records, for one TZif file at one explicit instant, what an independent Rust reader
//! observed: UT offset, DST flag, abbreviation, and whether the answer fell beyond the explicit transition
//! table (i.e. would depend on the not-yet-interpreted POSIX footer). Rows are stable and order-preserving
//! so they can be diffed against reference `zdump` for a pinned fixture set.

#![forbid(unsafe_code)]

use crate::civil::format_iso_utc;
use crate::tzif::Tzif;

/// One observation row.
pub struct WitnessRow {
    pub zone_file: String,
    pub at_unix: i64,
    pub utoff: i32,
    pub is_dst: bool,
    pub abbr: String,
    pub beyond_explicit: bool,
}

impl WitnessRow {
    pub fn build(zone_file: &str, z: &Tzif, at_unix: i64) -> WitnessRow {
        let o = z.observe(at_unix);
        WitnessRow {
            zone_file: zone_file.to_string(),
            at_unix,
            utoff: o.utoff,
            is_dst: o.is_dst,
            abbr: o.abbr,
            beyond_explicit: z.beyond_explicit(at_unix),
        }
    }

    /// One canonical JSON object (no trailing newline). Field order is fixed for deterministic diffs.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"zone_file\":{},\"at_utc\":{},\"at_unix\":{},\"utoff_seconds\":{},\"is_dst\":{},\"abbr\":{},\"beyond_explicit_transitions\":{}}}",
            json_str(&self.zone_file),
            json_str(&format_iso_utc(self.at_unix)),
            self.at_unix,
            self.utoff,
            self.is_dst,
            json_str(&self.abbr),
            self.beyond_explicit,
        )
    }

    /// A compact human line (not a stability contract — the JSON is the contract).
    pub fn to_text(&self) -> String {
        let sign = if self.utoff < 0 { '-' } else { '+' };
        let a = self.utoff.unsigned_abs();
        format!(
            "{}  {}  UT{}{:02}:{:02}:{:02}  isdst={}  {}{}",
            self.zone_file,
            format_iso_utc(self.at_unix),
            sign,
            a / 3600,
            (a % 3600) / 60,
            a % 60,
            if self.is_dst { 1 } else { 0 },
            self.abbr,
            if self.beyond_explicit {
                "  [beyond-explicit/footer-governed]"
            } else {
                ""
            },
        )
    }
}

/// Render a slice of rows as a stable, pretty JSON array (the witness artifact). Shared by the CLI and the
/// golden test so the committed goldens and the live output never diverge.
pub fn rows_to_json_array(rows: &[WitnessRow]) -> String {
    let mut s = String::from("[\n");
    for (k, r) in rows.iter().enumerate() {
        let comma = if k + 1 < rows.len() { "," } else { "" };
        s.push_str("  ");
        s.push_str(&r.to_json());
        s.push_str(comma);
        s.push('\n');
    }
    s.push_str("]\n");
    s
}

/// Minimal RFC-8259 string escaper (the only JSON values we emit that need escaping are file paths and
/// abbreviations — both ASCII-ish in practice, but we escape correctly regardless).
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_escapes() {
        assert_eq!(json_str("EST"), "\"EST\"");
        assert_eq!(json_str("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
