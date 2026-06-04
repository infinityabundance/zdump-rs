//! Named-zone resolution (Phase 3) — accept a zone NAME like `America/New_York` (the way `zdump` is used)
//! and resolve it to a TZif file under the zoneinfo directory, while still accepting an explicit path.
//!
//! Resolution order: if the argument names an existing file, use it directly; otherwise join it onto the
//! zoneinfo dir (`--tzdir`, else `$TZDIR`, else `/usr/share/zoneinfo`). Keeps the witness usable as a
//! drop-in `zdump <zone>` for inspection without weakening the explicit-path mode.

#![forbid(unsafe_code)]

use std::path::PathBuf;

/// Default zoneinfo directory (POSIX/Linux convention).
pub const DEFAULT_TZDIR: &str = "/usr/share/zoneinfo";

/// Resolve a zone name or path to a TZif file path. `tzdir` overrides `$TZDIR`/the default.
pub fn resolve(arg: &str, tzdir: Option<&str>) -> Result<PathBuf, String> {
    // 1) explicit existing file (absolute or relative path)
    let direct = PathBuf::from(arg);
    if direct.is_file() {
        return Ok(direct);
    }
    // 2) join onto the zoneinfo dir
    let base = tzdir
        .map(|s| s.to_string())
        .or_else(|| std::env::var("TZDIR").ok())
        .unwrap_or_else(|| DEFAULT_TZDIR.to_string());
    let joined = PathBuf::from(&base).join(arg);
    if joined.is_file() {
        return Ok(joined);
    }
    Err(format!(
        "could not resolve zone {arg:?}: not a file, and {base}/{arg} does not exist (try --tzif PATH or --tzdir DIR)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_explicit_path() {
        assert!(resolve("fixtures/UTC.tzif", None).is_ok());
    }
    #[test]
    fn resolves_name_under_tzdir() {
        // treat the fixtures dir as a TZDIR; the bare name resolves under it
        let p = resolve("UTC.tzif", Some("fixtures")).unwrap();
        assert!(p.is_file());
    }
    #[test]
    fn unknown_zone_errors() {
        assert!(resolve("No/Such/Zone", Some("fixtures")).is_err());
    }
}
