# Phase B Code Review — Reviewer A

**Crate:** `bismark-coverage2cytosine` (worktree `/Users/fkrueger/Github/Bismark-c2c`)
**Scope:** Phase B byte-identity crux — `report.rs` kernel + `run_report`, `cov.rs`, `summary.rs`, `error.rs`, `lib.rs`, `main.rs`, `tests/golden_phase_b.rs` + fixtures.
**Contract:** byte-identical to Perl `coverage2cytosine` v0.25.1, core report, PLAIN output.
**Mode:** RECOMMEND-ONLY (no source edits — concurrent reviewers/auditor).

## Verdict: **APPROVE**

The kernel is byte-faithful to the Perl. I cross-checked the Rust binary against the **live repo Perl** (`/Users/fkrueger/Github/Bismark-c2c/coverage2cytosine`, v0.25.1) on a battery of adversarial inputs targeting every risky path; all produced **raw-byte-identical** report + summary output. `cargo test -p bismark-coverage2cytosine` = 67 pass; `cargo clippy --all-targets` = clean.

**No Critical or High issues.** Three Low/informational notes below (one documented intentional divergence, two theoretical-only overflow edges). None block Phase B.

---

## Differential verification against live Perl (the heart of this review)

All runs were `Rust binary` vs `perl ./coverage2cytosine` on the same genome+cov, `diff`'d byte-for-byte. **Every case was IDENTICAL** (report *and* cytosine-context summary):

| Probe | What it exercises | Result |
|---|---|---|
| `>chr_g1 GATCGCATG` | reverse-G at **i=0** (`pos-3<0`, tri=1 byte → dropped) | IDENTICAL |
| `>chr_cg1 CGTACGTA` | forward-C at **i=0** (upstream wraps to last base, `substr(seq,-1,3)`); G at **i=1** (`pos-3<0`, tri=2 bytes → dropped) | IDENTICAL |
| `>chr_endC TTACGTTCG` | **last-base** reverse-G (tri len 3 but `len-pos==0` skip); top partner C at pos8 (tri len 2 → dropped) | IDENTICAL |
| `>chr_n ACNGCGNTCGN` | `N` in tri (`CNG`→CHG, `CGN`→CG) + `N` as upstream base (summary skip) | IDENTICAL |
| single-chr `AAACGTCG`, last base covered 7,3; pos7 C covered 8,2 | **last-chromosome Perl block** (guard order threshold→tri→last-base) vs single Rust kernel (tri→last-base→threshold) — proves reordering is emission- AND summary-equivalent | IDENTICAL |
| chrA…chrB…chrA + duplicate pos2 | **non-contiguous re-flush** (chrA emitted twice), **last-write-wins** dup, **summary double-counts both walks** (13/7 → 65%) | IDENTICAL |
| `--coverage_threshold 5` w/ uncovered chr | threshold gate (also gates summary), uncovered-chr suppression when `threshold>0` | IDENTICAL |
| soft-masked `acgt` + `chr10`/`chr2`/`chr1` + cov-only `chrZ` | uppercase-on-load, **cov-chr-absent-from-genome emits nothing**, uncovered pass in **bytewise `sort keys %processed`** order | IDENTICAL |
| 16 ratios incl. `.xx5` ties (`1/8`, `1/16`, `3/16`, `401/800`, `1/800`, `1/1600`) | `%.2f` round-half-to-even parity Perl `sprintf` vs Rust `format!("{:.2}")` | ALL IDENTICAL |

This corroborates, against ground truth, every focus item:

1. **Coordinate arithmetic** (`extract`/`perl_substr`/`revcomp`): the `i<2`≡`pos-3<0` reverse-G branch, the `substr(seq,-1,3)` upstream wrap at i=0, N pass-through, and interior/last-base all verified faithful. I specifically tried to construct a divergent sequence/position and could not.
2. **Single-kernel guard order**: the last-chromosome block (the one place Perl reorders threshold before tri-len/last-base) is proven equivalent — all guards are side-effect-free `next` skips and `context_reporting`/summary runs only after all of them in *both* Perl blocks, so reordering changes neither the emitted set nor the summary. Confirmed empirically (covered last-base + covered short-tri positions both excluded, and excluded from the summary, identically).
3. **`run_report` streaming**: non-contiguous re-flush, fresh-buffer seeding, last-write-wins, empty-input→`EmptyCoverageInput`, uncovered pass sorted + `threshold==0`-gated, cov-chr-absent — all verified.
4. **Report-line + summary bytes**: field formatting, raw chr/tri bytes, `N/A`/`%.2f`, sorted 64 rows, i=0 upstream-wrap feeding `ubase` — all byte-identical.

---

## Issues by area

### Logic / correctness
**No defects found.** The single-kernel refactor's equivalence claim (`report.rs:9-14`) is correct and now empirically verified against the Perl last-chromosome block.

The `%processed`-seeding subtlety is handled correctly: I confirmed Perl seeds `$processed{$chr}=0` for **every** genome chromosome at load (`coverage2cytosine:1712/1734`), so `sort keys %processed` ≡ all genome names. The Rust `genome.names_sorted()` + `!seen` filter is the faithful model — *not* a divergence (an earlier hypothesis that Perl would skip genome chromosomes never named in `%processed` is refuted by line 1712/1734).

### Errors
`MalformedCovLine` vs genuine I/O is cleanly separated (`cov.rs:67-72` only fails the field parse; `read_until` I/O errors propagate as `Io`). `EmptyCoverageInput` fires before the uncovered pass even when `threshold==0` — matches Perl `:472-474`. Error strings echo Perl wording.

### Efficiency
`flush_chromosome` builds a per-chromosome `Vec<u8>` then one `write_all` — good. `out` is re-allocated per chromosome (re-emitted twice for a non-contiguous chr); acceptable and matches the streaming model. No concerns at Phase-B scope (Phase A loads the whole genome into memory by design).

### Structure / style
Clear, well-documented, idioms clean (clippy `--all-targets` silent). The `extract`/`perl_substr`/`revcomp`/`classify_context` decomposition reads directly against the Perl line numbers. Module docs are accurate.

---

## Recommendations (prioritized)

### Critical / High
None.

### Medium
None.

### Low

**L-1 (informational, documented intentional divergence): blank line forces a chromosome transition in Perl but is a no-op in Rust.**
A cov file with an interior blank line makes Perl `split /\t/` yield `$chr=undef`, which trips `$chr ne $last_chr` → an extra flush/re-emit of the current chromosome (and a phantom `undef` chromosome). The Rust skips blank lines (`cov.rs:51-53` → `run_report:238-240` `continue`). Demonstrated: a contrived `chrA\n<blank>\nchrA` cov emits chrA **twice** under Perl, **once** under Rust.

This is an **accepted, documented divergence** (SPEC §98 "blank lines skipped"; PLAN B-I3; the Rust is arguably the more-correct behavior). It **cannot occur on real `bismark2bedGraph` output**, so it does not threaten the byte-identity contract. *No change required.* Optional: the `cov.rs:39` doc comment "Returns `Ok(None)` for a blank line (skipped — no phantom chromosome)" is accurate; consider one extra sentence noting Perl *would* phantom-flush, so a future reader doesn't "fix" it back. Purely editorial.

**L-2 (theoretical-only): `u32` overflow in the threshold sum and summary accumulation.**
`emit_position:145` computes `meth + nonmeth` and `summary.accumulate` does `cell.0 += meth`. Both are `u32`; in a debug build a sum exceeding `u32::MAX` would panic, and in release would wrap (Perl uses arbitrary-precision arithmetic, so it would never diverge there). Real per-position coverage is tiny and the summary aggregates over a genome's CpGs with small counts, so this is unreachable in practice. If hardening is ever wanted, widen the threshold compare to `u64` (`(meth as u64) + nonmeth as u64`) and/or use `saturating_add` in `accumulate`. *Not required for Phase B.*

**L-3 (test-coverage gap, already tracked): no end-to-end blank/trailing-line test (COVERAGE.md V22 PARTIAL).**
`parse_blank_line_is_skipped` covers the parse level, and `non_contiguous_chromosome_re_emits` exercises re-flush, but no integration test drives a blank-line-containing cov through the binary to assert "no phantom chromosome, no `EmptyCoverageInput` misfire." Already flagged as optional in COVERAGE.md. Suggested addition (drop into `tests/golden_phase_b.rs`):

```rust
#[test]
fn blank_and_trailing_lines_do_not_phantom_flush() {
    // Interior blank line + no final newline: must NOT create a phantom
    // chromosome nor misfire EmptyCoverageInput; chrA emitted once.
    let genome = ">chrA\nAACGTT\n";
    let cov = "chrA\t3\t3\t0\t5\t5\n\n"; // trailing blank line, no data after
    let report = run_with(genome, cov, &[]);
    let chra: Vec<&str> = report.lines().filter(|l| l.starts_with("chrA\t")).collect();
    assert_eq!(chra.len(), 2, "chrA emitted exactly once (2 CpG lines): {chra:?}");
    assert!(report.contains("chrA\t3\t+\t5\t5\tCG"));
}
```

(Note: this pins the *Rust* contract, which intentionally diverges from Perl on the contrived blank-line input per L-1; it is a regression guard, not a Perl-parity golden.)

---

## Golden-fixture adequacy

The committed goldens are well chosen and exercise the risky paths:
- `cx.report.golden` **contains CHG and CHH lines** (`chr1 13 CHG CCG`, `chr2 4 CHH CTT`, `chr2 7 CHH CAA`) → `--CX` is genuinely tested, not just CpG.
- The genome (`test.fa`) hits **N in the trinucleotide** (`chr1 22 CG CGN`), a **2-bp scaffold** `scaf_short` (whole-chromosome tri-len drop), an **uncovered chromosome** (`chr3uncov`), and **chr2 starting with C** (i=0 upstream-wrap feeding `ubase`).
- The summary golden has a **covered, non-round-percentage cell** (`A CGT ACGT 408 400 50.50` — a `.xx5` rounding case) plus the full sorted 64-row grid with header.
- Streaming edge cases the sorted goldens can't reach are covered by dedicated tests (`non_contiguous_chromosome_re_emits`, `empty_coverage_input_errors`, `cov_chromosome_absent_from_genome_emits_nothing_for_it`).

The only adequacy gap is L-3 (e2e blank/trailing line), already tracked as optional.

---

## Files reviewed
- `rust/bismark-coverage2cytosine/src/report.rs` (kernel + `run_report`) — primary
- `src/cov.rs`, `src/summary.rs`, `src/error.rs`, `src/lib.rs`, `src/main.rs`, `src/cli.rs`, `src/genome.rs`
- `tests/golden_phase_b.rs`, `tests/sanity.rs`, `tests/data/phase_b/*` (fixtures, goldens, `generate_goldens.sh`)
- Perl ground truth `coverage2cytosine:63-78, 168-745, 1388-1565, 1700-1739, 1961-1988`
