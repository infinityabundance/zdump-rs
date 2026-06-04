# zdump-rs — a bounded, independent TZif witness (companion to zic-rs)

> **`zic-rs` is the compiler. `zdump-rs` is the witness/inspection companion.** It is deliberately *not* a
> full `zdump` replacement — it is a narrow, independent Rust TZif reader that renders what a TZif file
> *means* (offset / is_dst / abbreviation) at explicit instants, as deterministic JSON witness rows, so the
> evidence court has a **second, independently-written reader** to cross-check against reference `zdump`.

Milestone: **`T23.zdump-witness.1`** (Phase 1) · **`.2`** (Phase 2) · **`.3`** (Phase 3, current) — inside
the zic-rs T23 evidence surface, not a new top-level milestone.

**Published crate:** <https://crates.io/crates/zdump-rs> · source: <https://github.com/infinityabundance/zdump-rs>
(GitHub "No releases published" refers to GitHub *Releases* only — the crate is published on crates.io.)

## Why it exists

Today zic-rs uses reference `zdump` as its semantic oracle (correct). A separate Rust reader adds an
*independent* witness — sharing **no code** with zic-rs's own `tzif` module, so agreement between the two is
real evidence, not a tautology. It is especially useful for `right/`-leap rendering, future Appendix-A
microcases, golden TZif inspection, and eventual no-reference-`zdump` environments.

The evidence court now has four distinct roles:

```
compiler:                 zic-rs
format validator:         zic-rs  rfc9636::validate
oracle comparison:        reference zdump
independent Rust witness: zdump-rs   <-- this crate
```

## What it does

**Phase 1:**
- reads TZif **v1/v2/v3/v4** (independent, bounds-checked reader; truncated/hostile input → `Err`, never a panic)
- evaluates **UT offset / is_dst / abbreviation** at explicit UTC instants
- emits **deterministic JSON witness rows** (fixed field order)

**Phase 2:**
- **interprets the POSIX footer** — instants beyond the last explicit transition are *projected*
  (offset/is_dst/abbreviation), matching reference `zdump` far-future instead of guessing
- **lists transitions** over a year window (the `zdump -v` analog: instant + before/after type)
- **exposes the leap-second table** (`--leaps`)

**Phase 3 (partial `zdump` CLI compatibility):**
- **named-zone resolution** — pass `America/New_York` (no `--tzif`), resolved under `$TZDIR`/`--tzdir`
- **`-c lo,hi`** year-cut (the `zdump -c` flag)
- **leap-second `23:59:60` rendering** for `right/` zones — the inserted leaps display as `…23:59:60`,
  matching reference `zdump` (the TAI wall rendering near leaps)

```sh
zdump-rs inspect     America/New_York --at 2200-07-01T00:00:00Z          # named zone, footer-projected
zdump-rs inspect     --tzif fixtures/America_New_York.tzif --probe-default --json
zdump-rs transitions America/New_York -c 2035,2037 --json                # zdump-style -c year-cut
zdump-rs transitions right/Etc/UTC --leaps                               # leap seconds as ...23:59:60
```

## Acceptance (Phase 1 + 2 + 3, met)

Accepted when a Rust TZif witness can read selected TZif files (by path **or named zone**), evaluate
offset/is_dst/abbrev at instants, **project footer-governed future instants without guessing**, **list
explicit transitions over a bounded `-c lo,hi` window**, **render `right/` inserted leap seconds as
`23:59:60`**, **cross-check against reference `zdump` on pinned fixtures**, and continue refusing full
`zdump` CLI/output parity.

- `tests/witness_golden.rs` — pins witness + transition output to committed goldens for `America/New_York`,
  `Europe/London`, `UTC`, **`America/Vancouver`** (the real 2026a→2026b release-diff zone), and the
  **`right/`** profiles (deterministic, no oracle); asserts `right/UTC` carries the full monotonic leap table.
- `tests/cross_check_zdump.rs` — parses reference `zdump -v` and confirms zdump-rs agrees at probe instants
  **including footer-projected far-future (2200) and `right/` zones** (**48/48**), and that the **`right/UTC`
  leap-second `:60` instants match zdump** (**27/27**); **auto-skips with a reason** if `zdump` is absent.
- unit: POSIX footer parsing (incl. southern-hemisphere wrap, angle-bracket numeric names), leap `:60`
  rendering, named-zone resolution.

Pinned fixtures + their sha256 in `fixtures/SHA256SUMS` (the `America/New_York` fixture is byte-identical to the matrix's New_York, `e9ed07d7…`).

## Explicit NON-claims (guarded as hard as the capability)

```
not a full zdump replacement     not exact stdout/stderr parity     not all flags
not locale behaviour             not a replacement oracle           not civil-time truth
```
Leap scope (honest): the inserted leap-second **instants** are rendered (`…23:59:60`, cross-checked vs
`zdump`); zdump-rs does **not** convert every arbitrary instant on the TAI scale to leap-aware wall time —
it surfaces offset/is_dst/abbreviation (leap-independent) + the leap *table* + the `:60` instants.

## Scope ladder

- **Phase 1 (done):** witness reader — read TZif, evaluate at instants, emit JSON, cross-check `zdump`.
- **Phase 2 (done):** transition listing over a window; footer-derived future projection; leap-table exposure.
- **Phase 3 (done):** partial `zdump` CLI compatibility — named zones (`$TZDIR`), `-c lo,hi`, and `right/`
  leap-second `23:59:60` rendering, all cross-checked against reference `zdump`.
- **Phase 4:** full `zdump` replacement — *only if scope is explicitly expanded.*

Zero dependencies; `#![forbid(unsafe_code)]`; `overflow-checks` in release.
