# Phase B Code Review — Reviewer B

**Crate:** `bismark-coverage2cytosine` (worktree `/Users/fkrueger/Github/Bismark-c2c`)
**Scope:** Phase B genome-wide cytosine report — `report.rs` (kernel + `run_report`), `cov.rs`, `summary.rs`, `error.rs`, `lib.rs`, `main.rs`, plus `tests/golden_phase_b.rs` + `tests/data/phase_b/*` (fixtures, goldens, `generate_goldens.sh`).
**Contract:** raw-byte-identical to Perl `coverage2cytosine` v0.25.1 (core report, PLAIN output).
**Mode:** RECOMMEND-ONLY (no source edits; fixes given as diffs). Independent of Reviewer A.

## Verdict: **APPROVE**

The product code is byte-faithful to the Perl. I built the binary and ran it head-to-head against the **live repo Perl** (`./coverage2cytosine`, v0.25.1, `perl -c` OK) on a battery of adversarial inputs aimed at exactly the focus areas in my brief; every product-output case was **raw-byte-identical** (report *and* cytosine-context summary), confirmed with `cmp`/`diff`, not just `from_utf8_lossy`. I also re-derived all eight committed goldens from the Perl into a scratch dir and `diff`'d them against the checked-in `.golden` files — **all eight identical**, so the goldens are genuine Perl ground truth (not hand-edited / stale). `cargo test -p bismark-coverage2cytosine` = **67 pass**; `cargo clippy -p … --all-targets` = **clean**.

**No Critical or High issues.** The defects I found are all in **test strength / hardening**, not in the shipping output. Two Medium (both trivial, both about making the byte-identity gate actually byte-exact and pinning a behavior the goldens can't), three Low.

---

## Differential verification against live Perl (the core of this review)

All runs were `Rust binary` vs `perl ./coverage2cytosine` on identical genome+cov, then `cmp`/`diff` byte-for-byte. **Every product-output case below was IDENTICAL** (report AND summary):

| Probe | What it exercises | Result |
|---|---|---|
| Re-derive all 8 goldens from Perl into scratch dir, diff vs committed | goldens are genuine Perl output, not stale | 8/8 IDENTICAL |
| `>chrA CGAACGTTCG` covered at **pos1** (forward C), G@pos2 | **CpG at genome position 1**, reverse partner G@i=1 (`pos-3<0`, tri=2B → dropped) | IDENTICAL |
| `>chrEndC AACGC` / `>chrEndG AACGG` | chromosome **ending in C** / **ending in G** (last-base Guard 2) | IDENTICAL |
| cov ordered `chrEndG, chrA, chrA, chrEndG, chrA` (non-sorted, non-contiguous, dup pos) | **covered emission in COV order ≠ genome order**, re-flush, last-write-wins | IDENTICAL |
| `>chrNcontext ACGNNCGAC` | `N` in tri (`CGN`) + `N` upstream base (summary skip) | IDENTICAL |
| `>chrG1 GACGT` / `>chrG2 AGCGT` / `>chrGG GGCGT` covered, `--CX` | reverse-G at **i=0 and i=1** (`i<2` branch) | IDENTICAL |
| `m/(m+u)` at 8 exact `.xx5` two-dp ties (`1/32 … 15/32`) | `%.2f` parity Perl `sprintf` vs Rust `format!("{:.2}")` | 8/8 IDENTICAL (`3.12, 9.38, 15.62, 21.88, 28.12, 34.38, 40.62, 46.88`) |
| trailing tab / 7th empty field / extra column / empty chr name | cov parse `>=6` fields, field 0 empty | IDENTICAL |

These corroborate, against ground truth, my focus items 3, 4, 5, 7, 8:

- **q3 covered-order vs genome-order**: proven byte-identical and COV-ordered. In genome (bytewise) order `chrA` < `chrEndG`, yet the binary emits `chrEndG` first because cov lists it first — confirming COV order, not genome order.
- **q4 `%.2f` rounding / large counts / strand & context literals**: every `.xx5` tie agrees; counts emit as plain `u32` decimal; `+`/`-` and `CG`/`CHG`/`CHH`/raw tri bytes match.
- **q5 write order**: report fully flushed first, then summary opened+written — matches Perl (`generate_genome_wide_cytosine_report` then `print_context_summary`, `coverage2cytosine:44,49`).
- **q7 cov parse**: tolerant where it must be (trailing tab, empty chr name); strict (rejecting) on `+5`/` 5`/`-5`/`2.0` — the documented B-I1 divergence (see L-1).
- **q8 edge fidelity**: pos-1 C, dropped reverse partner, last-base C/G all faithful.

I specifically tried to construct a divergent sequence/position/percentage that the product would get wrong, and could not.

---

## Issues by area

### Logic / correctness — no product defects
The single-kernel refactor (`report.rs:9-14`) is correct. I independently confirmed the `%processed` subtlety the module relies on: Perl seeds `$processed{$chromosome_name}=0` for **every** genome chromosome at load (`coverage2cytosine:1712` and `:1734`), so the uncovered pass `foreach my $chr (sort keys %processed)` is exactly `genome.names_sorted()` filtered by `!covered`. The Rust `names_sorted()` + `!seen` model is faithful, **not** a divergence. The uncovered pass correctly does **not** call `context_reporting` (Perl `process_unprocessed_chromosomes` has no summary call), and the Rust passes `accumulate_summary=false` there — verified by the all-`N/A`-for-uncovered summary output.

Guard order in the single kernel (`tri<3` → last-base → threshold → context → summary → emit) is provably equivalent to Perl's covered block (same order) and the last-chromosome block (threshold-first) because every guard is a side-effect-free skip and both `context_reporting` and the emit run only after *all* guards in both Perl blocks.

### Errors
`MalformedCovLine` vs genuine `Io` is cleanly separated (`cov.rs:67-72` only fails the numeric parse; `read_until` errors propagate as `Io`). `EmptyCoverageInput` fires before the uncovered pass even when `threshold==0` (Perl `:472-474`). `create_dir_all` is guarded against the empty prefix (`report.rs:217`) — necessary, since `create_dir_all("")` errors `ENOENT`; **q6 handled**.

### Efficiency
`flush_chromosome` buffers a whole chromosome's report in a `Vec<u8>` then one `write_all`. Fine and freed per chromosome, but see L-3 for the real-chromosome memory note (Phase C seam).

### Structure / style
Clear, accurate module docs that read directly against Perl line numbers. clippy `--all-targets` silent.

---

## Recommendations (prioritized) — all as diffs

### Critical / High
**None.**

### Medium

**M-1 (test strength — the byte-identity gate is not actually byte-exact): `golden_phase_b.rs` asserts on `String::from_utf8_lossy`, which can mask byte differences.**
`assert_mode_matches_golden` compares `String::from_utf8_lossy(&got) == String::from_utf8_lossy(&want)` (lines 36-40, 48-52). `from_utf8_lossy` maps *distinct* invalid bytes — and a literal U+FFFD — onto the **same** `Cow<str>`. I verified this directly: `lossy([0xFF]) == lossy([0xFE])` is `true`, and `lossy([0xFF]) == lossy([0xEF,0xBF,0xBD])` is `true`. So two reports differing only in invalid-UTF-8 or trailing-replacement bytes would pass vacuously. For a crate whose *entire contract* is raw-byte identity, the assertion should compare raw bytes.

Practical exposure today is **nil** (I confirmed all 8 goldens are pure ASCII and non-empty, and the Rust genome reader *rejects* non-UTF-8 chromosome names via `noodles` `InvalidData` → `MalformedFastaHeader` — see L-2 — so a non-ASCII byte can never reach the report; every other field is ASCII digits / `+-` / `ACGTN`). So this is a latent test-correctness smell, not an active failure — but the fix is free and removes the only way a future byte regression could slip the gate.

```diff
--- a/rust/bismark-coverage2cytosine/tests/golden_phase_b.rs
+++ b/rust/bismark-coverage2cytosine/tests/golden_phase_b.rs
@@
     let got_report = std::fs::read(tmp.path().join(format!("{mode}.{report_suffix}"))).unwrap();
     let want_report = std::fs::read(d.join(format!("{mode}.report.golden"))).unwrap();
-    assert_eq!(
-        String::from_utf8_lossy(&got_report),
-        String::from_utf8_lossy(&want_report),
-        "{mode}: report bytes differ from Perl golden"
-    );
+    assert_eq!(
+        got_report, want_report,
+        "{mode}: report bytes differ from Perl golden"
+    );
@@
     let want_sum = std::fs::read(d.join(format!("{mode}.summary.golden"))).unwrap();
-    assert_eq!(
-        String::from_utf8_lossy(&got_sum),
-        String::from_utf8_lossy(&want_sum),
-        "{mode}: context-summary bytes differ from Perl golden"
-    );
+    assert_eq!(
+        got_sum, want_sum,
+        "{mode}: context-summary bytes differ from Perl golden"
+    );
```
(Both sides are already `Vec<u8>` from `std::fs::read`; on a mismatch the panic prints byte vectors, which is adequate. If a nicer diff is wanted later, wrap each in a `BStr`-style newtype.)

**M-2 (test coverage gap — q3): no committed test proves covered chromosomes emit in COV order when that differs from genome (bytewise) order.**
The goldens' `in.cov` lists `chr1` then `chr2` — already genome/sorted order — so they cannot distinguish COV-order from genome-order. `non_contiguous_chromosome_re_emits` uses `chrA…chrB…chrA`, where first-appearance order coincides with alphabetical order, so it also doesn't pin this. I proved the binary is correct via the live-Perl cross-check above (`chrEndG` emitted before `chrA` though `chrA` < `chrEndG` bytewise), but there is no regression guard in the repo. Add a clean A-vs-B-order test:

```rust
#[test]
fn covered_chromosomes_follow_cov_order_not_genome_order() {
    // Genome (bytewise) order is chrA < chrB. The cov lists chrB FIRST, so the
    // report must emit chrB's block before chrA's (Perl flushes in arrival order).
    let genome = ">chrA\nACGT\n>chrB\nTTCGTT\n";
    let cov = "chrB\t3\t3\t100\t9\t0\nchrA\t2\t2\t100\t5\t0\n";
    let report = run_with(genome, cov, &[]);
    let chrb = report.find("chrB\t").expect("chrB present");
    let chra = report.find("chrA\t").expect("chrA present");
    assert!(chrb < chra, "chrB (cov-first) must precede chrA:\n{report}");
}
```

### Low

**L-1 (documented intentional divergence — q7): strict cov-field parsing rejects inputs Perl coerces.**
Verified vs Perl: `+5`, ` 5` (leading space), `-5`, and a float position `2.0` are all **accepted/coerced** by Perl (`+5`→5, ` 5`→5, `-5`→0, `2.0`→2) but **rejected** (`MalformedCovLine`, exit 1) by the Rust. This is the explicitly documented B-I1 divergence (`error.rs:120-130`, `cov.rs:1-8`) and cannot occur on real `bismark2bedGraph` output. The strict behavior is arguably safer (Perl silently turning `-5` into 0 is a footgun). *No change required.* The float-position case (`2.0`) is the only one a hand-edited cov could plausibly contain; the error message is clear, so this is fine.

**L-2 (Phase-A divergence, surfaces here): a non-UTF-8 chromosome name makes Rust error where Perl emits.**
I fed `>chr\xff` through both: Perl succeeds and writes `chr\xff` verbatim into the report; Rust exits 1 (`noodles` `InvalidData` → `MalformedFastaHeader`). This is the already-documented genome-reader divergence (`genome.rs:319-331`, `bare_or_nameless_header_errors`) and cannot occur on a Bowtie2-built Bismark genome. Noted only because it (helpfully) makes M-1's masking risk strictly unreachable in practice. *No change required for Phase B.*

**L-3 (memory, Phase-C seam — q5): per-chromosome report fully buffered in a `Vec<u8>`.**
`flush_chromosome` accumulates the entire chromosome's report in `out` before `write_all`. For hg38 chr1 (~248 Mbp) the CpG report is ~2.4M lines × ~30 B ≈ ~70 MB; a `--CX` report is roughly an order of magnitude larger (~hundreds of MB) — on top of the whole genome already resident in memory. It is freed after each chromosome, so it's a transient spike, not a leak, and is acceptable at Phase-B (correctness-first). The module already flags `open_report_writer` as the Phase-C seam. **Recommend Phase C stream lines straight to the `BufWriter`** (write inside `emit_position`/`flush_chromosome` rather than buffering the chromosome) so peak RSS stays flat on real genomes. *Tracked for Phase C, not a Phase-B blocker.*

(I also independently reached Reviewer A's `u32`-overflow note: `summary.accumulate`'s `cell.0 += meth` is `u32`, and the release profile has `overflow-checks` off → a context cell exceeding `u32::MAX` would *silently wrap* rather than panic. For human WGBS even at 100x the largest CG cell stays ~10x under `u32::MAX`, so it is theoretical; if ever hardened, use `u64` accumulators in `ContextSummary` and `(meth as u64)+(nonmeth as u64)` in the threshold compare. Not required for Phase B.)

---

## Golden-fixture & test adequacy
- Goldens are committed (currently untracked in git as a fresh phase — expected), **not** regenerated at build/test time (`generate_goldens.sh` is only referenced in a doc comment), and byte-identical to fresh Perl output (re-derived and diff'd, 8/8).
- No vacuous-pass risk from emptiness: every golden is non-empty; the harness `.unwrap()`s `fs::read` (panics if the binary produced no file) and `.assert().success()` guards the exit code.
- `cx.report.golden` genuinely contains CHG/CHH lines; `default.summary.golden` has a covered `.xx5` cell (`A CGT ACGT 408 400 50.50`) plus the full sorted 64-row grid; uncovered/absent/non-contiguous paths have dedicated tests.
- Gaps: M-1 (lossy compare) and M-2 (cov-order-vs-genome-order) above.

---

## Files reviewed
- `rust/bismark-coverage2cytosine/src/report.rs` (kernel + `run_report`) — primary
- `src/cov.rs`, `src/summary.rs`, `src/error.rs`, `src/lib.rs`, `src/main.rs`, `src/cli.rs`, `src/genome.rs`
- `tests/golden_phase_b.rs`, `tests/sanity.rs`, `tests/data/phase_b/*` (fixtures, 8 goldens, `generate_goldens.sh`)
- Perl ground truth `coverage2cytosine:44-49, 63-78, 89-125, 168-745, 1388-1565, 1700-1739, 1961-1988`
