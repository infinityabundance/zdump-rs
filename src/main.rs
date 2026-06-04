//! `zdump-rs` CLI — the bounded TZif witness.
//!
//! Usage:
//!   zdump-rs inspect --tzif FILE --at 2026-01-01T00:00:00Z [--at ...] [--json]
//!   zdump-rs inspect --tzif FILE --probe-default [--json]
//!
//! `--probe-default` evaluates a fixed, declared set of RFC-9636-Appendix-A-style probe instants
//! (pre-epoch, the 2^31 boundary, a few civil years, far future) so a witness run is reproducible without
//! the caller having to remember the canonical set. Exit 0 on success; 2 on usage error; 1 on read error.

#![forbid(unsafe_code)]

use std::process::ExitCode;
use zdump_rs::civil::parse_iso_utc;
use zdump_rs::witness::rows_to_json_array;
use zdump_rs::{parse, WitnessRow, PROBE_DEFAULT};

fn main() -> ExitCode {
    // Exit-code contract (mirrors zic-rs's taxonomy so scripts can rely on it): 0 = success,
    // 2 = usage error (bad flags/instants), 1 = operational error (file unreadable / not valid TZif).
    // `run` returns the (code, message) pair so the taxonomy lives in exactly one place.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, msg)) => {
            eprintln!("zdump-rs: {msg}");
            ExitCode::from(code)
        }
    }
}

/// Parse argv, read+parse the TZif file, evaluate every requested instant, and print the rows. Hand-rolled
/// argument handling (no `clap`) keeps the crate dependency-free; the flag surface is deliberately tiny.
fn run(args: &[String]) -> Result<(), (u8, String)> {
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print_help();
        return Ok(());
    }
    if args[0] != "inspect" {
        return Err((
            2,
            format!("unknown subcommand {:?} (try `inspect`)", args[0]),
        ));
    }
    let mut tzif: Option<String> = None; // --tzif PATH (required)
    let mut ats: Vec<String> = Vec::new(); // each --at INSTANT (repeatable)
    let mut json = false; // --json: emit the deterministic witness array instead of text
    let mut probe_default = false; // --probe-default: append the canonical probe-instant set
    let mut i = 1; // start after the `inspect` subcommand
    while i < args.len() {
        match args[i].as_str() {
            "--tzif" => {
                i += 1;
                tzif = Some(
                    args.get(i)
                        .ok_or((2, "--tzif needs a path".to_string()))?
                        .clone(),
                );
            }
            "--at" => {
                i += 1;
                ats.push(
                    args.get(i)
                        .ok_or((2, "--at needs an instant".to_string()))?
                        .clone(),
                );
            }
            "--json" => json = true,
            "--probe-default" => probe_default = true,
            other => return Err((2, format!("unknown flag {other:?}"))),
        }
        i += 1;
    }
    let path = tzif.ok_or((2, "missing --tzif FILE".to_string()))?;
    if probe_default {
        ats.extend(PROBE_DEFAULT.iter().map(|s| s.to_string()));
    }
    if ats.is_empty() {
        return Err((
            2,
            "no instants: give --at <ISO8601> (repeatable) or --probe-default".to_string(),
        ));
    }
    // Read the whole file (TZif files are small) and parse with the independent reader. Both failures are
    // operational (exit 1), distinct from the usage errors above (exit 2).
    let bytes = std::fs::read(&path).map_err(|e| (1, format!("cannot read {path:?}: {e}")))?;
    let z = parse(&bytes).map_err(|e| (1, format!("not valid TZif ({path:?}): {e}")))?;

    // One witness row per requested instant, in input order (deterministic output).
    let mut rows = Vec::new();
    for a in &ats {
        let t = parse_iso_utc(a).map_err(|e| (2, format!("bad --at {a:?}: {e}")))?;
        rows.push(WitnessRow::build(&path, &z, t));
    }
    if json {
        print!("{}", rows_to_json_array(&rows));
    } else {
        for r in &rows {
            println!("{}", r.to_text());
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        "zdump-rs — bounded, independent TZif witness (companion to zic-rs)\n\n\
         USAGE:\n  \
         zdump-rs inspect --tzif FILE --at 2026-01-01T00:00:00Z [--at ...] [--json]\n  \
         zdump-rs inspect --tzif FILE --probe-default [--json]\n\n\
         NON-CLAIMS: not full zdump, not stdout parity, not all flags, not locale behaviour,\n  \
         not a replacement oracle, not civil-time truth; footer/leap projection is Phase 2."
    );
}
