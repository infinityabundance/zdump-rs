//! `zdump-rs` CLI — the bounded TZif witness.
//!
//! Usage:
//!   zdump-rs inspect     [<zone>|--tzif FILE] --at 2026-01-01T00:00:00Z [--at ...] [--probe-default] [--json] [--tzdir DIR]
//!   zdump-rs transitions [<zone>|--tzif FILE] [--from Y --to Y | -c Y,Y] [--leaps] [--json] [--tzdir DIR]
//!
//! A `<zone>` positional (e.g. `America/New_York`, `right/Etc/UTC`) resolves under `$TZDIR`/`--tzdir`
//! (default `/usr/share/zoneinfo`), the way `zdump` is used; an explicit `--tzif PATH` still works. `-c
//! lo,hi` is the `zdump -c` year-cut. For `right/` zones, `transitions --leaps` renders the inserted leap
//! seconds as `…23:59:60`. Exit 0 ok; 2 usage error; 1 read/parse/resolve error.

#![forbid(unsafe_code)]

use std::process::ExitCode;
use zdump_rs::civil::parse_iso_utc;
use zdump_rs::witness::{rows_to_json_array, transition_to_json, transition_to_text};
use zdump_rs::{leap, parse, zone, Tzif, WitnessRow, PROBE_DEFAULT};

fn main() -> ExitCode {
    // Exit-code contract (mirrors zic-rs): 0 = success, 2 = usage error, 1 = operational (read/parse/resolve).
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

/// Resolve the source (positional zone name OR `--tzif` path) to a parsed TZif. Returns the label to show
/// in output (the original argument) and the parsed file. Named zones go through `$TZDIR`/`--tzdir`.
fn load(source: &Option<String>, tzdir: &Option<String>) -> Result<(String, Tzif), (u8, String)> {
    let label = source
        .clone()
        .ok_or((2, "no zone: give a <zone> name or --tzif FILE".to_string()))?;
    let path = zone::resolve(&label, tzdir.as_deref()).map_err(|e| (1, e))?;
    let bytes = std::fs::read(&path).map_err(|e| (1, format!("cannot read {path:?}: {e}")))?;
    let z = parse(&bytes).map_err(|e| (1, format!("not valid TZif ({path:?}): {e}")))?;
    Ok((label, z))
}

/// Capture a leading positional argument (a zone name) that is not a flag.
fn take_positional(args: &[String], src: &mut Option<String>, i: &mut usize) -> bool {
    if src.is_none() && !args[*i].starts_with('-') {
        *src = Some(args[*i].clone());
        *i += 1;
        true
    } else {
        false
    }
}

fn run_inspect(args: &[String]) -> Result<(), (u8, String)> {
    let mut source: Option<String> = None;
    let mut tzdir: Option<String> = None;
    let mut ats: Vec<String> = Vec::new();
    let mut json = false;
    let mut probe_default = false;
    let mut i = 0;
    while i < args.len() {
        if take_positional(args, &mut source, &mut i) {
            continue;
        }
        match args[i].as_str() {
            "--tzif" => {
                i += 1;
                source = Some(
                    args.get(i)
                        .ok_or((2, "--tzif needs a path".to_string()))?
                        .clone(),
                );
            }
            "--tzdir" => {
                i += 1;
                tzdir = Some(
                    args.get(i)
                        .ok_or((2, "--tzdir needs a dir".to_string()))?
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
    let (label, z) = load(&source, &tzdir)?;
    if probe_default {
        ats.extend(PROBE_DEFAULT.iter().map(|s| s.to_string()));
    }
    if ats.is_empty() {
        return Err((
            2,
            "no instants: give --at <ISO8601> (repeatable) or --probe-default".to_string(),
        ));
    }
    let mut rows = Vec::new();
    for a in &ats {
        let t = parse_iso_utc(a).map_err(|e| (2, format!("bad --at {a:?}: {e}")))?;
        rows.push(WitnessRow::build(&label, &z, t));
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
    let mut source: Option<String> = None;
    let mut tzdir: Option<String> = None;
    let mut from_year: i64 = 1900;
    let mut to_year: i64 = 2040;
    let mut json = false;
    let mut want_leaps = false;
    let mut i = 0;
    while i < args.len() {
        if take_positional(args, &mut source, &mut i) {
            continue;
        }
        match args[i].as_str() {
            "--tzif" => {
                i += 1;
                source = Some(
                    args.get(i)
                        .ok_or((2, "--tzif needs a path".to_string()))?
                        .clone(),
                );
            }
            "--tzdir" => {
                i += 1;
                tzdir = Some(
                    args.get(i)
                        .ok_or((2, "--tzdir needs a dir".to_string()))?
                        .clone(),
                );
            }
            "--from" => {
                i += 1;
                from_year = year_arg(args.get(i), "--from")?;
            }
            "--to" => {
                i += 1;
                to_year = year_arg(args.get(i), "--to")?;
            }
            "-c" | "--cut" => {
                // zdump-style year-cut: -c lo,hi
                i += 1;
                let v = args.get(i).ok_or((2, "-c needs lo,hi".to_string()))?;
                let (a, b) = v
                    .split_once(',')
                    .ok_or((2, "-c expects lo,hi (comma-separated years)".to_string()))?;
                from_year = a
                    .trim()
                    .parse()
                    .map_err(|_| (2, "-c lo must be a year".to_string()))?;
                to_year = b
                    .trim()
                    .parse()
                    .map_err(|_| (2, "-c hi must be a year".to_string()))?;
            }
            "--leaps" => want_leaps = true,
            "--json" => json = true,
            other => return Err((2, format!("unknown flag {other:?}"))),
        }
        i += 1;
    }
    let (label, z) = load(&source, &tzdir)?;
    let lo = zdump_rs::civil::days_from_civil(from_year, 1, 1) * 86400;
    let hi = zdump_rs::civil::days_from_civil(to_year + 1, 1, 1) * 86400;
    let trs = z.transitions_in(lo, hi);
    if json {
        println!(
            "{{\"zone\":{:?},\"from_year\":{from_year},\"to_year\":{to_year},\"footer\":{},\"transitions\":[",
            label,
            footer_json(&z)
        );
        for (k, tr) in trs.iter().enumerate() {
            let comma = if k + 1 < trs.len() { "," } else { "" };
            println!("  {}{comma}", transition_to_json(&label, tr));
        }
        print!("]");
        if want_leaps {
            print!(",\"leaps\":[");
            for (k, l) in z.leaps.iter().enumerate() {
                let comma = if k + 1 < z.leaps.len() { "," } else { "" };
                print!(
                    "{{\"leap_second_utc\":{:?},\"occur_unix\":{},\"corr\":{}}}{comma}",
                    leap::displayed_leap(l.occur, l.corr),
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
            println!("# {} leap records (right/ profile)", z.leaps.len());
            for l in &z.leaps {
                println!(
                    "leap {}  (corr={})",
                    leap::displayed_leap(l.occur, l.corr),
                    l.corr
                );
            }
        }
    }
    Ok(())
}

fn year_arg(v: Option<&String>, flag: &str) -> Result<i64, (u8, String)> {
    v.ok_or((2, format!("{flag} needs a year")))?
        .parse()
        .map_err(|_| (2, format!("{flag} must be an integer year")))
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
         zdump-rs inspect     [<zone>|--tzif FILE] --at 2026-01-01T00:00:00Z [--at ...] [--json] [--tzdir DIR]\n  \
         zdump-rs inspect     [<zone>|--tzif FILE] --probe-default [--json]\n  \
         zdump-rs transitions [<zone>|--tzif FILE] [--from Y --to Y | -c Y,Y] [--leaps] [--json]\n\n\
         <zone> like America/New_York or right/Etc/UTC resolves under $TZDIR/--tzdir (default /usr/share/zoneinfo).\n  \
         Instants beyond the last explicit transition are projected from the POSIX footer.\n  \
         right/ zones: --leaps renders inserted leap seconds as ...23:59:60.\n  \
         NON-CLAIMS: not full zdump, not stdout parity, not all flags, not locale, not an oracle, not civil-time truth."
    );
}
