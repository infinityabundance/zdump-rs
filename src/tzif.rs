//! An independent, bounds-checked TZif reader (RFC 9636).
//!
//! This is a SECOND implementation, intentionally sharing no code with zic-rs's `tzif` module — its value
//! as evidence is exactly that it was written separately, so agreement between the two is meaningful. It
//! reads v1/v2/v3/v4 files; for v2+ it parses the 64-bit data block (the one modern readers use) and keeps
//! the POSIX footer string for later (Phase 2) future-projection work.
//!
//! Scope (Phase 1): header + data block (transitions, transition-type indices, local-time types,
//! leap-second records, the footer string). It does NOT interpret the footer, apply leap corrections to
//! wall time, or validate to the full RFC predicate set — that is `rfc9636::validate` in zic-rs's lane.

#![forbid(unsafe_code)]

/// A local-time type (`ttinfo`): UT offset, DST flag, and abbreviation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtInfo {
    pub utoff: i32,
    pub is_dst: bool,
    pub abbr: String,
}

/// A leap-second record (occurrence in UT, cumulative correction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeapSecond {
    pub occur: i64,
    pub corr: i32,
}

/// A parsed TZif file (the data this witness reasons over).
#[derive(Debug, Clone)]
pub struct Tzif {
    pub version: u8, // 1, 2, 3, or 4
    pub transitions: Vec<i64>,
    pub type_indices: Vec<u8>,
    pub types: Vec<TtInfo>,
    pub leaps: Vec<LeapSecond>,
    pub footer: Option<String>,
}

/// A big-endian cursor with explicit bounds checks — a malformed/truncated TZif yields `Err`, never a
/// panic (the same hostile-input discipline zic-rs holds itself to).
struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cur<'a> {
    fn new(b: &'a [u8]) -> Self {
        Cur { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.p.checked_add(n).ok_or("offset overflow")?;
        if end > self.b.len() {
            return Err(format!("truncated: need {n} bytes at {}", self.p));
        }
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    fn i64(&mut self) -> Result<i64, String> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(i64::from_be_bytes(a))
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
}

#[derive(Clone, Copy)]
struct Counts {
    isutcnt: u32,
    isstdcnt: u32,
    leapcnt: u32,
    timecnt: u32,
    typecnt: u32,
    charcnt: u32,
}

fn read_header(c: &mut Cur) -> Result<(u8, Counts), String> {
    let magic = c.take(4)?;
    if magic != b"TZif" {
        return Err("bad magic (not TZif)".into());
    }
    let ver = c.u8()?;
    let version = match ver {
        0 => 1,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        other => return Err(format!("unknown version byte {other:#x}")),
    };
    c.take(15)?; // reserved
    let counts = Counts {
        isutcnt: c.u32()?,
        isstdcnt: c.u32()?,
        leapcnt: c.u32()?,
        timecnt: c.u32()?,
        typecnt: c.u32()?,
        charcnt: c.u32()?,
    };
    Ok((version, counts))
}

/// The decoded contents of one TZif data block.
struct Block {
    transitions: Vec<i64>,
    type_indices: Vec<u8>,
    types: Vec<TtInfo>,
    leaps: Vec<LeapSecond>,
}

/// Parse a TZif data block. `wide` selects 64-bit (`true`, v2+ block) vs 32-bit (`false`, v1 block)
/// transition/leap-occurrence widths.
fn read_block(c: &mut Cur, n: Counts, wide: bool) -> Result<Block, String> {
    if n.typecnt == 0 {
        return Err("typecnt == 0".into());
    }
    let mut transitions = Vec::with_capacity(n.timecnt as usize);
    for _ in 0..n.timecnt {
        transitions.push(if wide { c.i64()? } else { c.i32()? as i64 });
    }
    let mut type_indices = Vec::with_capacity(n.timecnt as usize);
    for _ in 0..n.timecnt {
        let ti = c.u8()?;
        if ti as u32 >= n.typecnt {
            return Err(format!(
                "transition type index {ti} >= typecnt {}",
                n.typecnt
            ));
        }
        type_indices.push(ti);
    }
    // ttinfo: int32 utoff, u8 isdst, u8 desigidx
    let mut raw_types = Vec::with_capacity(n.typecnt as usize);
    for _ in 0..n.typecnt {
        let utoff = c.i32()?;
        let isdst = c.u8()? != 0;
        let desigidx = c.u8()?;
        if desigidx as u32 >= n.charcnt {
            return Err(format!(
                "designation index {desigidx} >= charcnt {}",
                n.charcnt
            ));
        }
        raw_types.push((utoff, isdst, desigidx));
    }
    let desig = c.take(n.charcnt as usize)?;
    let mut types = Vec::with_capacity(n.typecnt as usize);
    for (utoff, isdst, di) in raw_types {
        let start = di as usize;
        let end = desig[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|z| start + z)
            .unwrap_or(desig.len());
        let abbr = String::from_utf8_lossy(&desig[start..end]).into_owned();
        types.push(TtInfo {
            utoff,
            is_dst: isdst,
            abbr,
        });
    }
    let mut leaps = Vec::with_capacity(n.leapcnt as usize);
    for _ in 0..n.leapcnt {
        let occur = if wide { c.i64()? } else { c.i32()? as i64 };
        let corr = c.i32()?;
        leaps.push(LeapSecond { occur, corr });
    }
    for _ in 0..n.isstdcnt {
        c.u8()?;
    }
    for _ in 0..n.isutcnt {
        c.u8()?;
    }
    Ok(Block {
        transitions,
        type_indices,
        types,
        leaps,
    })
}

/// Parse a complete TZif file.
pub fn parse(bytes: &[u8]) -> Result<Tzif, String> {
    let mut c = Cur::new(bytes);
    let (version, counts) = read_header(&mut c)?;
    if version == 1 {
        let b = read_block(&mut c, counts, false)?;
        return Ok(Tzif {
            version,
            transitions: b.transitions,
            type_indices: b.type_indices,
            types: b.types,
            leaps: b.leaps,
            footer: None,
        });
    }
    // v2+: skip the v1 block, then read the second header + 64-bit block + footer.
    let _ = read_block(&mut c, counts, false)?;
    let (v2, counts2) = read_header(&mut c)?;
    if v2 != version {
        return Err(format!("v1/v2 header version mismatch: {version} vs {v2}"));
    }
    let b = read_block(&mut c, counts2, true)?;
    // footer: <newline> TZ-string <newline>
    let footer = read_footer(&mut c);
    Ok(Tzif {
        version,
        transitions: b.transitions,
        type_indices: b.type_indices,
        types: b.types,
        leaps: b.leaps,
        footer,
    })
}

fn read_footer(c: &mut Cur) -> Option<String> {
    let rest = &c.b[c.p.min(c.b.len())..];
    let s = String::from_utf8_lossy(rest);
    let s = s.trim_matches(|ch| ch == '\n' || ch == '\r' || ch == '\0');
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

impl Tzif {
    /// The local-time type index in effect *before the first transition* (RFC 9636: the first non-DST
    /// type, else type 0). Used when a probe instant precedes every recorded transition.
    fn pre_first_type(&self) -> usize {
        self.types.iter().position(|t| !t.is_dst).unwrap_or(0)
    }

    /// The type in effect at Unix instant `t`.
    ///
    /// Within the explicit transition table this is a binary-search lookup. **Beyond** the last explicit
    /// transition (Phase 2) the POSIX footer governs: if it parses, the answer is projected from it (so
    /// far-future instants now match reference `zdump` instead of being pinned to the last type). Without a
    /// footer, the last type is used and `beyond_explicit` flags it.
    pub fn observe(&self, t: i64) -> Observation {
        if self.beyond_explicit(t) {
            if let Some(f) = &self.footer {
                if let Some(tz) = crate::posix::parse(f) {
                    return tz.observe(t);
                }
            }
        }
        self.observe_explicit(t)
    }

    /// The explicit-table-only lookup (no footer projection) — used within the recorded range and as the
    /// fallback when there is no parseable footer.
    pub fn observe_explicit(&self, t: i64) -> Observation {
        let idx = match self.transitions.binary_search(&t) {
            Ok(i) => Some(i),      // a transition begins exactly at t
            Err(0) => None,        // before the first transition
            Err(i) => Some(i - 1), // last transition strictly before t
        };
        let ti = match idx {
            Some(i) => self.type_indices[i] as usize,
            None => self.pre_first_type(),
        };
        let tt = &self.types[ti];
        Observation {
            utoff: tt.utoff,
            is_dst: tt.is_dst,
            abbr: tt.abbr.clone(),
        }
    }

    /// True when `t` is at or beyond the last explicit transition, so the answer is footer-governed (now
    /// footer-projected in Phase 2 when a footer is present; the field still records the factual provenance
    /// "this value came from the footer, not the explicit table").
    pub fn beyond_explicit(&self, t: i64) -> bool {
        match self.transitions.last() {
            Some(&last) => t >= last,
            None => true,
        }
    }

    /// Every explicit transition with `lo <= at < hi`, each with the type in effect just before and at it
    /// (the `zdump -v` analog, restricted to the explicit table). Phase 2.
    pub fn transitions_in(&self, lo: i64, hi: i64) -> Vec<TransitionRow> {
        let mut out = Vec::new();
        for (i, &at) in self.transitions.iter().enumerate() {
            if at < lo || at >= hi {
                continue;
            }
            let before = if i == 0 {
                let tt = &self.types[self.pre_first_type()];
                Observation {
                    utoff: tt.utoff,
                    is_dst: tt.is_dst,
                    abbr: tt.abbr.clone(),
                }
            } else {
                let tt = &self.types[self.type_indices[i - 1] as usize];
                Observation {
                    utoff: tt.utoff,
                    is_dst: tt.is_dst,
                    abbr: tt.abbr.clone(),
                }
            };
            let at_t = &self.types[self.type_indices[i] as usize];
            let after = Observation {
                utoff: at_t.utoff,
                is_dst: at_t.is_dst,
                abbr: at_t.abbr.clone(),
            };
            out.push(TransitionRow { at, before, after });
        }
        out
    }
}

/// One explicit transition: the instant, and the local-time type just before and at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionRow {
    pub at: i64,
    pub before: Observation,
    pub after: Observation,
}

/// What the witness observed at one instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub utoff: i32,
    pub is_dst: bool,
    pub abbr: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    // A minimal hand-built v1 TZif: one type (UTC, "UTC"), no transitions.
    fn utc_v1() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"TZif");
        b.push(0); // v1
        b.extend_from_slice(&[0u8; 15]);
        for v in [0u32, 0, 0, 0, 1, 4] {
            b.extend_from_slice(&v.to_be_bytes()); // isut,isstd,leap,time,type,char
        }
        // one ttinfo: utoff 0, isdst 0, desigidx 0
        b.extend_from_slice(&0i32.to_be_bytes());
        b.push(0);
        b.push(0);
        b.extend_from_slice(b"UTC\0");
        b
    }
    #[test]
    fn parses_utc_and_observes() {
        let z = parse(&utc_v1()).unwrap();
        assert_eq!(z.types.len(), 1);
        let o = z.observe(1_767_225_600); // 2026-01-01Z
        assert_eq!(o.utoff, 0);
        assert!(!o.is_dst);
        assert_eq!(o.abbr, "UTC");
    }
    #[test]
    fn rejects_truncated() {
        assert!(parse(b"TZif").is_err());
        assert!(parse(b"XXXX").is_err());
    }
    #[test]
    fn rejects_oob_type_index() {
        // timecnt=1 transition pointing at type 5 with typecnt=1 -> must be rejected, not panic
        let mut b = Vec::new();
        b.extend_from_slice(b"TZif");
        b.push(0);
        b.extend_from_slice(&[0u8; 15]);
        for v in [0u32, 0, 0, 1, 1, 4] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.extend_from_slice(&0i32.to_be_bytes()); // transition time
        b.push(5); // bad type index
        b.extend_from_slice(&0i32.to_be_bytes());
        b.push(0);
        b.push(0);
        b.extend_from_slice(b"UTC\0");
        assert!(parse(&b).is_err());
    }
}
