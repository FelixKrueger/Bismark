# Phase 3 `--ffs` — Code Review B

**Date:** 2026-06-01
**Reviewer:** Code Reviewer B (independent; did not read CODE_REVIEW_A.md)
**Scope:** uncommitted working tree on `rust/c2c-v1x` in worktree `/Users/fkrueger/Github/Bismark-c2c`
**Files reviewed:** `src/report.rs` (new `ffs_fields`, `emit_position` `ffs` param), `src/cli.rs`, `src/drach.rs`, `src/gpc.rs`, `tests/sanity.rs`, `tests/golden_phase3.rs`, `tests/data/phase3_ffs/*`, `src/merge.rs::parse_report_row` (unchanged, audited)

---

## Top-line verdict: **REQUEST-CHANGES**

**Critical: 1 · High: 0 · Medium: 0 · Low: 2**

The `--ffs` report-line extension itself is **excellent** — the offset table is byte-identical to Perl v0.25.1 across every mode I tested (CpG, `--CX`, `--zero_based`, `--split_by_chromosome`, `--gzip`, `--merge_CpGs`, uncovered chromosomes, N-windows, the forward-hexa negative-wrap, all empty-window cases, the all-three-empty trailing-tab line). The deviation (standalone `ffs_fields` helper) is sound and well-contained, the non-ffs hot path is untouched, and 168 tests + clippy + fmt are green.

**However**, I found ONE byte-identity divergence in a reachable flag combination that the plan never analyzed and the tests do not cover: **`--ffs --nome-seq` produces a `*.NOMe.CpG.cov` file that diverges from Perl** (Perl emits nothing; Rust emits the cov lines). Because byte-identity to Perl v0.25.1 is the prime directive, this blocks an unqualified APPROVE.

---

## CRITICAL — `--ffs --nome-seq` writes a NOMe `.cov` companion that Perl suppresses

### What Perl does
In all three extraction/emission blocks (`:396-447`, `:638-689`) the per-position emit is structured:

```perl
if ($tetra){                                   # --ffs ON
    print CYT  <10-col line>;                  #   NO CYTCOV print here
}
else{                                          # --ffs OFF
    if ($nome){
        print CYT    <7-col line>;
        print CYTCOV <chr pos pos %.6f m u>;   #   the only place CYTCOV is written
    }
    else{
        print CYT <7-col line>;
    }
}
```

All four `print CYTCOV` statements (`:405`, `:420`, `:647`, `:662`) live inside the `else → if($nome)` arm. **When `$tetra` (`--ffs`) is true, the `if($tetra)` arm runs and CYTCOV is never written.** So Perl `--ffs --nome-seq` produces an **empty** `*.NOMe.CpG.cov`.

### What Rust does
In `emit_position` (`report.rs:312`) the cov companion is gated only on `nome`, independent of `ffs`:

```rust
if nome {                       // <-- should be `nome && !ffs`
    ... cov_out.extend_from_slice(...)  // writes the NOMe .cov line
}
```

So Rust writes the cov line whenever `nome`, even under `--ffs`.

### Live-Perl evidence (my from-scratch fixtures)

Genome `>chr1 AACGTTACGAACGGT`, cov `chr1 3 .. 80 8 2` / `chr1 8 .. 50 5 5` (both NOMe-qualifying ACG-upstream CpGs, covered):

| run | `out.NOMe.CpG.cov` |
|-----|--------------------|
| Perl `--nome-seq` (no ffs) | `chr1 3 3 80.000000 8 2` / `chr1 8 8 50.000000 5 5` (46 B) |
| Perl `--ffs --nome-seq` | **0 bytes (empty)** |
| Rust `--ffs --nome-seq` | `chr1 3 3 80.000000 8 2` / `chr1 8 8 50.000000 5 5` (46 B) — **DIVERGES** |

Reproduced on a second fixture (`--ffs --nome-seq` AND `--ffs --nome-seq --zero_based`): `out.NOMe.CpG.cov` Perl=0 B vs Rust=46 B in both. The 10-col `*.NOMe.CpG_report.txt` and the `*.cytosine_context_summary.txt` ARE byte-identical — the *only* divergent file is the NOMe `.cov` companion.

### Why it matters / why it was missed
- `--ffs --nome-seq` is **reachable** — there is no Perl mutex (grep of `process_commandline` confirms `$tetra`/`$nome` are uncoupled) and the Rust `validate()` does not reject it (`--nome-seq` only rejects with `--CX`/`--merge_CpGs`). I ran it through both binaries with exit 0.
- The plan's `ffs_resolves_and_composes` test and §3.6 claim `--ffs` "composes with every flag (no mutex)" — true for *validation*, but the plan never analyzed the `--nome-seq` cov-companion suppression, and `golden_phase3.rs` has **no `--ffs --nome-seq` cell**. This is a genuine gap, not a known/accepted deviation.

### Recommended fix (do NOT apply — recommend-only)
Gate the cov-companion write in `emit_position` on `nome && !ffs` to mirror Perl's `if($tetra){…}else{if($nome){…CYTCOV…}}` structure:

```rust
if nome && !ffs {
    ... // write NOMe .cov companion
}
```

This is a one-line change, fully local to `emit_position`. It is plausibly a Perl quirk (the NOMe cov is "lost" when `--ffs` is requested), but byte-identity to v0.25.1 is the contract, so the Rust must reproduce it. **Add a `--ffs --nome-seq` golden** (and ideally a unit/integration assertion that the cov is empty) to pin it.

---

## Findings by area

### Logic / correctness — the offset table (independently re-derived from scratch)

I re-derived the §3.2 table directly from the three Perl blocks (`:262-330`, `:507-585`, `:1421-1493` — all three are byte-for-byte identical, so the dual-driver-drift risk is structurally absent) and confirmed each against a live `--CX --ffs` run. **All correct:**

| field | strand | Perl | Rust (`ffs_fields`) | verified |
|-------|--------|------|---------------------|----------|
| tetra | + | `substr(pos-1,4)` guard `len≥pos-1+4` | `perl_substr(i,4)` guard `len≥i+4` | ✓ |
| penta | + | `substr(pos-1,5)` guard `len≥pos-1+5` | `perl_substr(i,5)` guard `len≥i+5` | ✓ |
| hexa  | + | `substr(pos-3,6)` guard `len≥pos-3+6` | `perl_substr(i-2,6)` guard `len≥i+4` (signed → negative-wrap) | ✓ |
| tetra | − | `revcomp(substr(pos-4,4))` guard `pos-4≥0` | `revcomp(perl_substr(i-3,4))` guard `i≥3` | ✓ |
| penta | − | `revcomp(substr(pos-5,5))` guard `pos-5≥0` | `revcomp(perl_substr(i-4,5))` guard `i≥4` | ✓ |
| hexa  | − | `revcomp(substr(pos-4,6))` guard `pos-4≥0` | `revcomp(perl_substr(i-3,6))` guard `i≥3` | ✓ |

Live `--CX --ffs` on `chr1=GCCGTGAAACACGGCTTT, chrC=CGTAAACCC, chrN=AACNGCCAAGGCC` was **byte-identical** Rust≡Perl, exercising:
- **forward-hexa negative-wrap**: `chr1 i=1` → `T` (`substr(-1,6)`), `chrC i=0` → `CC` (`substr(-2,6)`) — NOT clamped to empty. ✓
- **chr-end empties**: `chr1 i=14` penta `""`; `chrC i=6` all three `""` rendered as `…CCC\t\t\t\n`. ✓
- **reverse fields + empty penta**: `chr1 i=3 (-)` → `CGGC`/`""`/`CACGGC`. ✓
- **reverse-hexa offset is `i-3` (not `i-2`)** — confirmed (`CACGGC` at `chr1 i=3`). ✓
- **N-windows verbatim**, both strands (`chrN` rows: `CNGC`, `AACNGC`, revcomp `GGCNGT` etc.) — the false `--help` "Ns ignored" is correctly NOT implemented. ✓

The `if(seq[i]=='C')` C/G strand discrimination is done by `extract`, and `ffs_fields` receives the already-resolved `strand` byte — consistent (a C is always `+`, a G always `-`; the helper never re-classifies).

### Append-only / column integrity
`emit_position` appends `\t{tetra}\t{penta}\t{hexa}` after the `tri` field and before `\n`, only when `ffs`. I confirmed:
- non-ffs `--CX` Rust == Perl (regression intact); non-ffs goldens (Phase 1/2/B/C/D) all green.
- cols 1–7 are byte-unchanged in ffs mode (the V10 `--zero_based` discriminator and direct diffs confirm pos shifts while cols 8–10 are frozen).
- empty fields render as nothing-between-tabs; all-three-empty → `…\t\t\t\n` (verified via `cat -A`).

### Scope (CpG-only / `--CX` / covered / uncovered)
Byte-identical in both CpG-only and `--CX`. The uncovered chromosome (`chrC`, absent from cov) emits 10-col `0 0` lines, byte-identical to Perl. ✓

### Merge interaction
- `--ffs --merge_CpGs` byte-identical to Perl across all 3 output files.
- `--ffs --merge_CpGs` merged cov == `--merge_CpGs` (no ffs) merged cov (V6 invariance — re-confirmed on live Rust).
- `merge::parse_report_row` requires `f.len() ≥ 6` and indexes only `f[0..6]` — tolerant of the 10-col line; no `merge.rs` change needed. ✓

### Orthogonality
`--ffs --gc` (GpC without nome): byte-identical (the GpC report is always 7-col — `print GC` at `:925/:937/:1045/:1057` never includes ffs cols — and the CpG report gets the 10 cols). The GpC module is untouched and correct. `--zero_based`, `--split`, `--gzip` all orthogonal and byte-identical.

### Efficiency
`ffs_fields` is O(1) per cytosine (≤6-byte slices + ≤3 small `revcomp` allocs on the reverse strand), computed **only when `ffs`** — the default hot path is untouched. Fine for byte-identity-first; the `Vec<u8>` allocations mirror the existing `tri`/`upstream` and could later be stack arrays, but that is out of scope.

### Structure / the sanctioned deviation
The standalone `ffs_fields(seq, i, strand)` helper (vs extending `extract` to return an `Extracted` struct) is **sound**: the plan §4 explicitly left the struct-vs-tuple choice to the implementer, `extract`'s shipped semantics are untouched, and the offset table is directly unit-testable. The doc-comment accurately captures the negative-wrap subtlety and the N-window note. Well-contained, no duplication concern.

---

## LOW findings

- **L1 — duplicated guard expression for forward tetra & hexa.** Both use `if len >= i + 4` (intentional — Perl's hexa guard `len ≥ pos-3+6 = pos+3 = i+4` happens to equal the tetra guard). Correct, but a one-line comment that the two `i+4`s are independently-derived-but-equal (not a copy-paste error) would prevent a future "fix." Cosmetic.

- **L2 — `cli.rs` retains the `UnsupportedFlag` error variant with no remaining caller.** The comment notes it is "retained for the error-display contract + any future deferral." Now that all v1.x flags are supported, `UnsupportedFlag` is dead in `validate()`. Acceptable (it may still be referenced by the error enum's Display impl/tests), but worth a grep to confirm it is not now `#[allow(dead_code)]`-bait. Not blocking — I confirmed the suite + clippy `-D warnings` are clean, so it is not flagged by the compiler.

---

## Byte-identity claims I independently re-verified (live Perl v0.25.1)

| check | fixture | result |
|-------|---------|--------|
| offset table (full) | `--CX --ffs` on chr1/chrC/chrN | **IDENTICAL** (from-scratch re-derivation matched) |
| forward-hexa negative-wrap | `chr1 i=1`→`T`, `chrC i=0`→`CC` | **IDENTICAL** (not clamped) |
| chr-end empties / `\t\t\t\n` | `chrC i=6` | **IDENTICAL** |
| reverse fields + empty penta | `chr1 i=3 (-)` | **IDENTICAL** (`CGGC`/`""`/`CACGGC`) |
| reverse-hexa `i-3` (not `i-2`) | `chr1 i=3` | **IDENTICAL** |
| N-windows verbatim, both strands | `chrN` | **IDENTICAL** |
| CpG / `--zero_based` / `--split` / `--gzip` | main fixture | **IDENTICAL** (all files) |
| `--merge_CpGs` (3 files) + V6 invariance | merge fixture | **IDENTICAL** |
| uncovered-chromosome 10-col `0 0` | chrC | **IDENTICAL** |
| context-summary invariance | ffs vs no-ffs | **IDENTICAL** |
| `--gc` (GpC, no nome) | covered fixture | **IDENTICAL** |
| **`--ffs --nome-seq` NOMe.CpG.cov** | covered ACG CpGs | **DIVERGES** (Perl 0 B, Rust 46 B) — Critical |

Also re-ran: `cargo test -p bismark-coverage2cytosine` → **168 passed** (97 lib + 18 P1 + 12 P2 + 9 P3 + 11 B + 7 C + 10 D + 4 sanity); `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

---

## Bottom line

The core `--ffs` extension is byte-identical and ready. The single blocker is the `--ffs --nome-seq` cov-companion divergence: gate the `cov_out` write on `nome && !ffs` and add a `--ffs --nome-seq` golden pinning the empty `*.NOMe.CpG.cov`. Recommend **REQUEST-CHANGES** until that one path matches Perl.
