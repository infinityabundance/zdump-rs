//! `zdump-rs` CLI — the bounded TZif witness.
//!
//! Usage:
//!   zdump-rs inspect     --tzif FILE --at 2026-01-01T00:00:00Z [--at ...] [--json]
//!   zdump-rs inspect     --tzif FILE --probe-default [--json]
//!   zdump-rs transitions --tzif FILE [--from YEAR] [--to YEAR] [--leaps] [--json]
//!
//! `inspect` evaluates offset/is_dst/abbreviation at explicit instants (footer-projected beyond the last
//! transition — Phase 2). `transitions` lists the explicit transitions in a year window (the `zdump -v`
//! analog), optionally with the leap-second table. Exit 0 ok; 2 usage error; 1 read/parse error.

#![forbid(unsafe_code)]

use std::process::ExitCode;
use zdump_rs::civil::{format_iso_utc, parse_iso_utc};
use zdump_rs::witness::{rows_to_json_array, transition_to_json, transition_to_text};
use zdump_rs::{parse, Tzif, WitnessRow, PROBE_DEFAULT};

fn main() -> ExitCode {
    // Exit-code contract (mirrors zic-rs): 0 = success, 2 = usage error, 1 = operational (read/parse).
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, msg)) => {
            eprintln!("zdump-rs: {msg}");
            ExitCode::from(code)
        }
    }
}

fn run(args: &[String]) -> Result<(), (u8, String)> {
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print_help();
        return Ok(());
    }
    match args[0].as_str() {
        "inspect" => run_inspect(&args[1..]),
        "transitions" => run_transitions(&args[1..]),
        other => Err((
            2,
            format!("unknown subcommand {other:?} (try `inspect` or `transitions`)"),
        )),
    }
}

/// Read+parse the TZif file named by `--tzif`, or a usage/operational error. Shared by both subcommands.
fn load(path: &Option<String>) -> Result<(String, Tzif), (u8, String)> {
    let path = path.clone().ok_or((2, "missing --tzif FILE".to_string()))?;
    let bytes = std::fs::read(&path).map_err(|e| (1, format!("cannot read {path:?}: {e}")))?;
    let z = parse(&bytes).map_err(|e| (1, format!("not valid TZif ({path:?}): {e}")))?;
    Ok((path, z))
}

fn run_inspect(args: &[String]) -> Result<(), (u8, String)> {
    let mut tzif: Option<String> = None;
    let mut ats: Vec<String> = Vec::new();
    let mut json = false;
    let mut probe_default = false;
    let mut i = 0;
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
    let (path, z) = load(&tzif)?;
    if probe_default {
        ats.extend(PROBE_DEFAULT.iter().map(|s| s.to_string()));
    }
    if ats.is_empty() {
        return Err((
            2,
            "no instants: give --at <ISO8601> (repeatable) or --probe-default".to_string(),
        ));
    }
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

fn run_transitions(args: &[String]) -> Result<(), (u8, String)> {
    let mut tzif: Option<String> = None;
    let mut from_year: i64 = 1900;
    let mut to_year: i64 = 2040;
    let mut json = false;
    let mut want_leaps = false;
    let mut i = 0;
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
            "--from" => {
                i += 1;
                from_year = args
                    .get(i)
                    .ok_or((2, "--from needs a year".to_string()))?
                    .parse()
                    .map_err(|_| (2, "--from must be an integer year".to_string()))?;
            }
            "--to" => {
                i += 1;
                to_year = args
                    .get(i)
                    .ok_or((2, "--to needs a year".to_string()))?
                    .parse()
                    .map_err(|_| (2, "--to must be an integer year".to_string()))?;
            }
            "--leaps" => want_leaps = true,
            "--json" => json = true,
            other => return Err((2, format!("unknown flag {other:?}"))),
        }
        i += 1;
    }
    let (path, z) = load(&tzif)?;
    let lo = zdump_rs::civil::days_from_civil(from_year, 1, 1) * 86400;
    let hi = zdump_rs::civil::days_from_civil(to_year + 1, 1, 1) * 86400;
    let trs = z.transitions_in(lo, hi);
    if json {
        println!("{{\"zone_file\":{:?},\"from_year\":{from_year},\"to_year\":{to_year},\"footer\":{},\"transitions\":[", path, footer_json(&z));
        for (k, tr) in trs.iter().enumerate() {
            let comma = if k + 1 < trs.len() { "," } else { "" };
            println!("  {}{comma}", transition_to_json(&path, tr));
        }
        print!("]");
        if want_leaps {
            print!(",\"leaps\":[");
            for (k, l) in z.leaps.iter().enumerate() {
                let comma = if k + 1 < z.leaps.len() { "," } else { "" };
                print!(
                    "{{\"occur_utc\":{:?},\"occur_unix\":{},\"corr\":{}}}{comma}",
                    format_iso_utc(l.occur),
                    l.occur,
                    l.corr
                );
            }
            print!("]");
        }
        println!("}}");
    } else {
        println!(
            "# {} transitions in [{from_year},{to_year}]  (footer: {})",
            trs.len(),
            z.footer.as_deref().unwrap_or("<none>")
        );
        for tr in &trs {
            println!("{}", transition_to_text(tr));
        }
        if want_leaps {
            println!("# {} leap records", z.leaps.len());
            for l in &z.leaps {
                println!("leap {}  corr={}", format_iso_utc(l.occur), l.corr);
            }
        }
    }
    Ok(())
}

fn footer_json(z: &Tzif) -> String {
    match &z.footer {
        Some(f) => format!("{f:?}"),
        None => "null".to_string(),
    }
}

fn print_help() {
    println!(
        "zdump-rs — bounded, independent TZif witness (companion to zic-rs)\n\n\
         USAGE:\n  \
         zdump-rs inspect     --tzif FILE --at 2026-01-01T00:00:00Z [--at ...] [--json]\n  \
         zdump-rs inspect     --tzif FILE --probe-default [--json]\n  \
         zdump-rs transitions --tzif FILE [--from YEAR] [--to YEAR] [--leaps] [--json]\n\n\
         Phase 2: instants beyond the last explicit transition are projected from the POSIX footer.\n  \
         NON-CLAIMS: not full zdump, not stdout parity, not all flags, not locale behaviour,\n  \
         not a replacement oracle, not civil-time truth."
    );
}
