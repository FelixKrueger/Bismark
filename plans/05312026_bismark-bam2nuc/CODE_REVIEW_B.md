# Code Review B — `rust/bismark-bam2nuc` (port of Perl `bam2nuc` v0.25.1)

**Reviewer:** B (independent, fresh context)
**Date:** 2026-05-31
**Scope:** correctness / byte-identity / panic-robustness / efficiency / structure of the new crate `rust/bismark-bam2nuc` (binary `bam2nuc_rs`), against Perl ground truth `/Users/fkrueger/Github/Bismark-bam2nuc/bam2nuc` and `SPEC.md`/`PLAN.md` (rev 1).
**Verdict:** **APPROVE with minor findings.** The byte-identity-critical paths are correct and match Perl. I independently re-derived the substr saturation, the `%.2f`/`%.3f` rounding contract (43,100 ratio cases, **0 mismatches** locally), the cache bytewise sort, the empty-count-field quirk, the PE `or 163` bug, and the genomic N-skip — all faithful. No Critical or High issues. One Medium (a documented-but-mis-implemented `None`-alignment_start corner, unreachable on real Bismark data) and several Low/test-gap items.

Build state I verified: `cargo test -p bismark-bam2nuc` = **70 lib + 12 golden + 2 sanity** pass, 1 `#[ignore]` real-data smoke; `cargo clippy --all-targets -- -D warnings` clean.

---

## Summary by review area

### 1. Logic / byte-identity — CORRECT

- **`extract_span` saturation (`count.rs:76-85`)** matches Perl `substr($chr,POS-1,len)` (`bam2nuc:133`). I verified against the live Perl interpreter:
  - normal, run-off-end-truncated, start-exactly-at-end (`""`), start-strictly-past-end (Perl `undef`, Rust `""`), and missing-chr (`""`). All behaviourally identical (Perl `undef`/`""` both yield 0 counts because `length` is 0 and the `process_sequence` loop never runs). `read_len = record.sequence().as_ref().len()` is the SEQ length (`= Perl length$sequence`), NOT the CIGAR ref-span — correct (they coincide because `[IDSN]` reads are pre-skipped). No panic path: `start = p.min(n)`, `end = p.saturating_add(read_len).min(n)`, both ≤ n, `start ≤ end`.
- **PE flag `or 163` bug (`count.rs:120-126`)** faithfully replicated: `flag ∈ {99,147}` → forward, else → revcomp, **never errors**. Matches Perl `elsif ($flag == 83 or 163)` parsing as `($flag==83) or 163` → always-true. Proven end-to-end by golden cell `pe_noncanonical` (flag 65 revcomp'd). Correct.
- **SE flag logic (`count.rs:105-111`)**: 0 → as-is, 16 → revcomp, else → `InvalidSeFlag` (reachable die, matches Perl `calc_single_end`). Correct.
- **`revcomp` (`count.rs:89-100`)** = reverse + `tr/GATC/CTAG/`; N and IUPAC bytes pass through (`other => other`). Verified `ARGY → YCRT` (G→C, A→T, R/Y untouched). Direction correct (reverse the iterator, then complement each surviving base). Matches Perl `reverse` then `tr`.
- **`process_sequence` (`freqs.rs:109-122`)**: mono skips only literal `N`; di skips a window iff either char is `N` (equivalent to Perl `index($di,'N') < 0`); IUPAC counted; `len-1` overlapping windows (`i+2 <= len`); per-chromosome (no cross-chr di — verified by `compute_genomic_sums_all_chromosomes` and the `chr2` N-run in the golden). All correct. The shared `m` for the mono check and the di's first char mirrors Perl exactly.
- **Cache (`freqs.rs:77-103`)**: bytewise `sort_unstable_by(|x,y| x.0.cmp(&y.0))` puts 1-char words before their 2-char extensions (prefix sorts less) — matches Perl `sort keys` under `LC_ALL=C`. Zero-count words omitted (`n > 0` guard). Reuse-if-present checks **only** `genome_folder` (`get_genomic_frequencies:147-148`), write precedence genome_folder → output_dir → warn (`write_cache:166-190`), read parse 1-byte→mono / 2-byte→di (`read_cache:212-216`). IUPAC placement verified by unit test (`R`=0x52 between the G-group and `T`; `CR`/`RG`). Cache golden (ACGTN + IUPAC + Mus-empty) byte-identical to Perl. Correct.
- **`report.rs:61-137`**: header text exact; mono `A,C,G,T` then 16 di in Perl's fixed order; **separate** mono/di totals; `%.2f` pct / `%.3f` coverage; empty count field for an absent word (`cs == 0 → String::new()`, but `0` used in arithmetic); ZeroDivision checks ordered `total_s` → `total_g` → `cg` so the header + prior rows are already written when it fires (matches Perl dying mid-`calculate_averages`). The `se_stats.golden` confirms the literal `\t\t` empty field. Correct.
- **`output_name.rs:25-39`**: case-sensitive, no-dot-anchor `strip_suffix("bam")`/`("cram")`; `weird.BAM → Err`, `a.bam.bam → a.bam.nucleotide_stats.txt`, `foosubbam → foosubnucleotide_stats.txt`. Matches Perl `s/(bam|cram)$/.../` exactly (verified the regex is case-sensitive). Correct.
- **`cli.rs` / `lib.rs::run`**: arg validation (mandatory `-g`, no-input-without-`--genomic_composition_only`), `--samtools_path` accepted-but-dropped, output-dir trailing-slash-not-absolute, `--genomic_composition_only` compute+write+exit, per-file loop with cache reuse, `.sam`/`.cram` reject via `AlignmentKind::from_path`, output path `format!("{output_dir}{name}")`. All match Perl's flow. Correct.

### 2. Panics / robustness — SAFE (one corner, see M-1)

- `#![forbid(unsafe_code)]`. No reachable `unwrap`/`expect` on adversarial-but-valid input in the hot path.
  - `report.rs:134` `from_utf8(word).expect("nucleotide word is ASCII")` — `word` is only ever a `MONO`/`DI` constant (ASCII), genuinely infallible.
  - di index `(a<<8)|b` with `a,b: u8` → ≤ 0xFFFF, in-bounds of the 65536 array. `mono[b as usize]` with `b: u8` ≤ 255. No OOB.
  - `usize::from(Position)` is infallible; `u16::from(record.flags())` is total.
  - `extract_span` clamps both ends; no slice panic.
- **`read_cache` on non-UTF-8** (`freqs.rs:200-203`): `reader.lines()` returns an `io::Error(InvalidData)` on a non-UTF-8 byte → propagates as `BismarkBam2nucError::Io`, **does not panic**. (See L-1 for the write/read asymmetry this creates with `cache_bytes()`'s raw-byte writer.)

### 3. Efficiency — GOOD

- Hot path (`bump_mono`/`bump_di`) is allocation-free array indexing — correct call (a `HashMap<Vec<u8>,_>` would allocate per bump). The two `NucCounts` (sample+genomic) ≈ 1 MiB, boxed di array off the stack.
- Per-read `extract_span(...).to_vec()` + `revcomp(...).collect()` = up to 2 short Vec allocations per kept read. For 55 M reads that's ~110 M small allocations; mimalloc is present and this is a QC step off the alignment hot path — acceptable per the PLAN's O-5 note. A `SmallVec`/reused scratch buffer is a possible future micro-opt, not needed for v1.0. (See L-3.)
- Cache `cache_bytes()` collect+sort runs once over ≤ a few hundred entries — negligible. No quadratic surprises. Single-threaded BGZF decode is fine for the QC step.

### 4. Structure / idiom / tests — GOOD, a few gaps

- Module split (`cli/error/genome/freqs/count/output_name/report` + `lib/main`) is clean and mirrors siblings. `count_records` factored out of `count_reads_in_file` for synthetic-`RecordBuf` testing — good. Doc comments cite the exact Perl line for each behaviour.
- Genome reader is a faithful c2c twin + the new `seqs()` accessor; the stale "no-iterator" invariant comment was correctly dropped (PLAN O-4).
- Test gaps are minor (see T-1, T-2). The golden harness is sound and reproducible (`generate_goldens.sh` pins `LC_ALL=C` at line 23, records samtools+Perl versions, regenerates fixtures+goldens deterministically). The "all 16 di-words in the genome" requirement (de Bruijn chr1) is correctly documented (Perl itself div-by-zeros otherwise).

---

## Prioritized recommendations

### Critical
*None.*

### High
*None.*

### Medium

**M-1 — `None` alignment_start does NOT yield an empty span (comment is wrong; PE counts garbage). `count.rs:176-181`.**
The code maps an unmapped record's `alignment_start() == None` to `pos1 = 0`:
```rust
// 1-based POS; None (unmapped) → 0 so the span is empty AND the flag
// still flows to the strand correction (keeps SE die-on-stray-flag).
let pos1 = record.alignment_start().map_or(0, usize::from);
```
But `extract_span(genome, chr, 0, read_len)` computes `p = 0.saturating_sub(1) = 0` → `start = 0` → it returns the chromosome's **first `read_len` bases**, NOT an empty span (verified: a present chr yields `ACGT…`, not `""`). So the inline comment ("the span is empty") and the PLAN C-2/C3 intent ("treat the None case as the missing-chr case → empty span", PLAN lines 238/280) are **not implemented** — the code instead extracts a bogus front-of-chromosome span.

Consequence by branch, for an unmapped record whose `reference_sequence_id` still points at a real genome chromosome (so `genome.get(chr)` is `Some`):
- **SE:** an unmapped read carries FLAG `0x4` → `correct_se` returns `InvalidSeFlag` and the bogus span is discarded by the error → **no byte impact** (the SE die-faithfulness the comment cares about is preserved, but for the wrong reason: the flag check saves it, not the span).
- **PE:** FLAG ≠ 99/147 → `correct_pe` revcomps the bogus front-of-chromosome span and **silently tallies it into the sample**. This is a real (silent) miscount.

Note this *also* diverges from Perl, which is itself "wrong" but differently: a real unmapped read has POS=0 in the SAM, so Perl computes `substr($chr, 0-1, len) = substr($chr, -1, len)` = the chromosome's **LAST** char (verified live: returns `T`). So for this corner Perl counts the last base, the PLAN intended empty, and the code counts the first `read_len` bases — three different answers.

**Reachability / gate impact: NONE.** Real Bismark BAMs contain only mapped alignments (no `None` alignment_start). The oxy gate and all goldens use real/synthetic mapped reads, so byte-identity is unaffected. This is a latent robustness/faithfulness gap on adversarial (non-Bismark) PE input + a misleading comment.

Recommended fix (caller's choice, must be re-tested for byte-identity — trivially neutral since unreachable on goldens): make the `None` case actually empty, e.g. branch in `count_records` to pass a sentinel that `extract_span` treats as "missing chr", or special-case `pos1 == 0 → return Vec::new()` inside `extract_span` (Perl's POS is never 0 for a mapped read, so a 0 there is always the unmapped sentinel). At minimum, fix the comment to state the truth: "`None → pos1 = 0`, which extracts the chromosome's first `read_len` bases; harmless because (a) real Bismark BAMs have no unmapped reads and (b) SE unmapped reads error on the flag before the span is used." Prefer the empty-span fix so PE adversarial input doesn't silently miscount.

### Low

**L-1 — `cache_bytes()` (raw bytes, D-impl-1) and `read_cache()` (UTF-8 `lines()`) round-trip asymmetry on a non-ASCII genome. `freqs.rs:77-103` vs `199-219`.**
`cache_bytes()` writes a raw non-ASCII genome byte verbatim (the D-impl-1 rationale: don't panic on it). But a subsequent reuse run's `read_cache` uses `reader.lines()`, which **errors** (`InvalidData`) on that same non-ASCII byte (verified). So a write succeeds but the reuse-read fails. Unreachable on a Bowtie2-built Bismark genome (uppercased ACGTN only, plus IUPAC which is still ASCII), and it fails loudly (not silently), so it's harmless to the gate. Worth a one-line note in the `read_cache` doc that the raw-write/UTF-8-read pair is only round-trip-safe for ASCII words (which is the only thing a real genome produces).

**L-2 — `cram_input_is_rejected` golden test passes a 7-byte stub that may exercise `TooShortToDetect` rather than the CRAM path. `tests/golden.rs:238`.**
The fixture is `b"CRAM\x03\x00\x00\x00"` (8 bytes). `from_path` peeks byte 0 (`C`) then `detect_cram_magic` reads the next 3 (`RAM`) → classifies `Cram` → `CramNotSupported`. The test asserts only `stderr contains "CRAM"`, which both the `CramNotSupported` message AND any CRAM-mentioning error would satisfy. It happens to hit the right path (8 bytes is enough), but the assertion is weak. Consider asserting on the distinctive `not yet supported` substring to pin the `CramNotSupported` variant specifically.

**L-3 — per-read double allocation (`extract_span` `.to_vec()` + `revcomp` `.collect()`). `count.rs:84,89-100,181-187`.**
Each kept read allocates a span `Vec` and (for reverse reads) a second revcomp `Vec`. Acceptable for a QC step with mimalloc (PLAN O-5), but a reusable scratch buffer + in-place revcomp would remove ~110 M small allocations on a 55 M-read PE sample. Future micro-opt only; flag for awareness, not for v1.0.

**L-4 — `report.rs` operation order vs Perl is `100.0 * cs / total` (matches), but worth a pinned comment.** `report.rs:124` computes `100.0 * cs as f64 / total_s as f64` (multiply-then-divide), matching Perl `100*$freqs{$word}/$total`. I cross-checked 43,100 ratio cases (pct over t∈1..200, cov over g∈1..150) Rust-vs-Perl: **0 mismatches** on macOS. Genomic counts in the billions stay < 2^53 after ×100, so no precision loss. No action needed; the oxy gate confirms Linux libc. Recording for the file.

### Test gaps (Low)

**T-1 — PLAN E2 cell #13 (content-BAM-named-`.sam`) is not implemented as a golden/integration test.** `sam_input_is_rejected` (`golden.rs:213`) feeds *text* SAM content (leading `@`), exercising the `from_path → Sam` classification, not the `derive_output_name` reject path for a real-BAM-named-`.sam`. The two-path-to-error reconciliation (I-1: `from_path` may accept a `.BAM`-named real BAM, but `derive_output_name` then errors) is unit-tested only via `derive_output_name("weird.BAM")`. Add an integration cell that feeds a real BAM file named `*.sam` to confirm the end-to-end reject (gate accepts the content but `derive_output_name` rejects → no stats file), matching Perl's read-then-die-at-naming.

**T-2 — no test for `None` alignment_start.** All `count_records` tests use `Some(pos)`. Given M-1, add a synthetic `RecordBuf` with `alignment_start == None` on (a) an SE file (expect `InvalidSeFlag` if flag has 0x4) and (b) a PE file with `reference_sequence_id = Some(<present chr>)` and flag 83 — currently this would tally the chromosome's front bases (the M-1 bug); after the fix it should tally nothing. This is the regression guard for M-1.

**T-3 — chr-end truncation has no dedicated golden cell (PLAN E2 #11).** It is exercised indirectly by `se.bam` reads `r4`/`r6` running off chr2 (LN 12), folded into `se_stats.golden`. Adequate but not isolated; optional to split out for clarity.

---

## Items independently verified as CORRECT (no action)

- substr saturation (5 cases) vs live Perl — identical behaviour.
- `%.2f`/`%.3f` round-half-to-even parity, 43,100 cases vs Perl `sprintf` — 0 mismatches.
- PE `or 163` always-true bug — replicated, golden-proven (flag 65).
- empty-count-field `\t\t` — present in `se_stats.golden` (`CT`, `GG`, `TA`, `TG`, `TT` rows).
- bytewise cache sort incl. IUPAC `R` placement — unit + golden.
- genomic N-skip across the `chr2 ACGTNNNNACGT` boundary — reflected in `cache_acgtn.golden` (AC/CG/GT = 3).
- cache reuse checks only `genome_folder`; planted ×1000 cache wins (golden `reuse_stats`).
- per-file `%freqs` reset (fresh `NucCounts` per `count_reads_in_file`) — golden `two_input_files`.
- single transitive `noodles-sam 0.85.0` / `noodles-bam 0.89.0` in the lock (the `record_bufs(&header)` type-check constraint holds); two `noodles-bgzf` (0.42.0 transitive + 0.47.0 direct) is benign.
- `H`/`P`/`=`/`X` CIGAR ops kept (not `[IDSN]`); `I/D/S/N` skipped — matches Perl regex, unit-tested.
- empty BAM (0 records) → all-zero sample → `ZeroDivision` exit 1, matching Perl's mid-`calculate_averages` die.
- SE/PE detection `None → SePeUndetermined`, matching Perl's no-`@PG` → die. The `detect_paired_from_header` (ID:Bismark-scoped) vs Perl `test_file` (first-`@PG`-any-ID) divergence is documented (I-3) and byte-neutral on raw Bismark BAMs; F1 covers the samtools-reprocessed case.
