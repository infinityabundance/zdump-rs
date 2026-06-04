# zdump-rs — a bounded, independent TZif witness (companion to zic-rs)

> **`zic-rs` is the compiler. `zdump-rs` is the witness/inspection companion.** It is deliberately *not* a
> full `zdump` replacement — it is a narrow, independent Rust TZif reader that renders what a TZif file
> *means* (offset / is_dst / abbreviation) at explicit instants, as deterministic JSON witness rows, so the
> evidence court has a **second, independently-written reader** to cross-check against reference `zdump`.

Milestone: **`T23.zdump-witness.1`** (inside the existing zic-rs T23 evidence surface — not a new top-level
milestone). It will graduate to its own published crate.

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

## What it does (Phase 1)

- reads TZif **v1/v2/v3/v4** (independent, bounds-checked reader; truncated/hostile input → `Err`, never a panic)
- evaluates **UT offset / is_dst / abbreviation** at explicit UTC instants (explicit transition table)
- emits **deterministic JSON witness rows** (fixed field order)
- flags instants **beyond the explicit transition table** (footer-governed) instead of guessing

```sh
zdump-rs inspect --tzif /usr/share/zoneinfo/America/New_York --at 2026-07-01T00:00:00Z
zdump-rs inspect --tzif fixtures/America_New_York.tzif --probe-default --json
```

## Acceptance (met)

`T23.zdump-witness.1` is accepted when a Rust TZif witness can read selected TZif files, evaluate
offset/is_dst/abbrev at explicit instants, emit deterministic JSON witness rows, **compare against reference
`zdump` for a small pinned fixture set**, and explicitly refuse full `zdump` CLI/output parity.

- `tests/witness_golden.rs` — pins witness output to committed goldens for `America/New_York`, `Europe/London`, `UTC` (deterministic, no oracle).
- `tests/cross_check_zdump.rs` — parses reference `zdump -v` and confirms zdump-rs agrees at probe instants (**18/18 matched** across the three fixtures); **auto-skips with a reason** if `zdump` is absent.

Pinned fixtures + their sha256 in `fixtures/SHA256SUMS` (the `America/New_York` fixture is byte-identical to the matrix's New_York, `e9ed07d7…`).

## Explicit NON-claims (guarded as hard as the capability)

```
not a full zdump replacement     not exact stdout/stderr parity     not all flags
not locale behaviour             not a replacement oracle           not civil-time truth
does NOT yet interpret the POSIX footer for future projection      (Phase 2)
does NOT apply leap-second corrections to wall time                (Phase 2)
```

## Scope ladder

- **Phase 1 (this):** internal semantic witness reader — read TZif, evaluate at instants, emit JSON, cross-check `zdump`.
- **Phase 2:** transition listing over a window; `right/`-leap behaviour; footer-derived future projection.
- **Phase 3:** partial `zdump` CLI compatibility (`-v`, `-c lo,hi`, named zones) — *only after the witness core is correct.*
- **Phase 4:** full `zdump` replacement — *only if scope is explicitly expanded.*

Zero dependencies; `#![forbid(unsafe_code)]`; `overflow-checks` in release.
