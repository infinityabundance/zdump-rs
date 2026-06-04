//! zdump-rs — a bounded, independent Rust TZif **witness** companion to zic-rs.
//!
//! `zic-rs` is the compiler (tzdb source → TZif). `zdump-rs` is the inspection companion: it reads TZif
//! and renders what it *means* (offset / is_dst / abbreviation) at explicit instants, as deterministic
//! JSON witness rows, so the evidence court has an **independent second reader** to cross-check against
//! reference `zdump`.
//!
//! ## Scope
//! Phase 1 (`T23.zdump-witness.1`):
//! - read TZif v1/v2/v3/v4 (independent reader; no shared code with zic-rs)
//! - evaluate offset/is_dst/abbreviation at explicit UTC instants
//! - emit deterministic JSON witness rows
//!
//! Phase 2 (`T23.zdump-witness.2`):
//! - **interpret the POSIX footer** so instants beyond the last explicit transition are *projected*
//!   (offset/is_dst/abbreviation), matching reference `zdump` far-future — `posix` module
//! - **transition listing** over a year window (the `zdump -v` analog) — `Tzif::transitions_in`
//! - **expose the leap-second table** in the listing
//!
//! Phase 3 (`T23.zdump-witness.3`):
//! - **named-zone resolution** via `$TZDIR`/`--tzdir` (use `America/New_York`, not just a path) — `zone`
//! - **`-c lo,hi`** year-cut (the `zdump -c` flag) on the transition listing
//! - **leap-second `23:59:60` rendering** for `right/` zones (TAI wall rendering near leaps) — `leap`
//!
//! ## Explicit NON-claims (guarded as hard as the capability)
//! - NOT a full `zdump` replacement · NOT exact stdout/stderr parity · NOT all flags
//! - NOT locale behaviour · NOT a replacement oracle (reference `zdump` stays the oracle)
//! - NOT civil-time truth
//! - does NOT apply leap-second corrections to the displayed wall time for `right/` zones (the per-type
//!   offset/is_dst/abbreviation are reported correctly; the TAI-scale wall rendering near leaps is a
//!   tracked Phase-3 refinement)

#![forbid(unsafe_code)]

pub mod civil;
pub mod leap;
pub mod posix;
pub mod tzif;
pub mod witness;
pub mod zone;

pub use tzif::{parse, Observation, TransitionRow, Tzif};
pub use witness::WitnessRow;

/// The fixed, declared probe-instant set (`--probe-default`): pre-epoch, the 32-bit `time_t` floor/ceiling,
/// a few civil years around the admitted 2026b release, and a far-future (footer-governed) point. Shared by
/// the CLI and the golden test so they cannot drift.
pub const PROBE_DEFAULT: &[&str] = &[
    "1901-12-13T20:45:52Z",
    "1933-01-01T00:00:00Z",
    "1970-01-01T00:00:00Z",
    "2000-06-01T00:00:00Z",
    "2026-01-01T00:00:00Z",
    "2026-07-01T00:00:00Z",
    "2038-01-19T03:14:07Z",
    "2100-01-01T00:00:00Z",
    "2200-07-01T12:00:00Z",
];
