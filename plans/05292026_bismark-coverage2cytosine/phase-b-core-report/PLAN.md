# Phase B PLAN — Core genome-wide cytosine report

**Epic:** `05292026_bismark-coverage2cytosine/EPIC.md`, Phase B — Core genome-wide report
**Design contract:** `../SPEC.md` (rev 3) — §4 (input), §7 (algorithm), §8 (context summary), §10.4/§10.5 (ordering + writers), §11 (data structures), §16 (pitfalls).
**Status:** rev 1 — implemented + green (see Implementation notes). Awaiting dual code-review + plan-manager.

## Implementation notes (Phase B — 2026-05-29)

**Status: implemented + byte-identical to Perl v0.25.1.** New modules `src/{cov,report,summary}.rs`; `error.rs` +`EmptyCoverageInput`/`MalformedCovLine`; `lib.rs::run` + `main.rs` wired. **67 tests pass** (55 unit + 7 golden/streaming + 5 sanity); clippy `-D warnings` clean; workspace builds; siblings untouched.

**Golden validation:** `tests/data/phase_b/` holds a synthetic multi-FASTA genome (CpG-at-start/end, N-run, 2-bp `scaf_short`, uncovered `chr3uncov`) + hand-built `in.cov`; `generate_goldens.sh` runs the **repo's Perl v0.25.1** `coverage2cytosine` to produce goldens. `tests/golden_phase_b.rs` asserts raw-byte-identity for **{default CpG, --CX, --zero_based, --coverage_threshold 5}** (report + context summary) + 3 streaming edge cases (non-contiguous re-flush, empty-input error, cov-chr-absent). All match Perl byte-for-byte.

**Empirically confirmed:** the kernel anchor (`ACGTACGCGT` + cov pos3=5/0 → `chr1 3 - 5 0 CG CGT`) matched Perl exactly; `scaf_short` (2 bp) correctly emits nothing; `CGN` near the N-run classifies as CG; coverage at a non-cytosine position is silently ignored; `%.2f` summary percentages byte-match Perl.

**Iteration log:**
- #1: `clippy -D warnings` `type_complexity` on `parse_cov_line`'s `Result<Option<(Vec<u8>,u32,u32,u32)>,_>` → added `type CovRecord` alias. Clean.
- #2: golden tests initially NotFound — golden filenames (`*.report.golden`) didn't match the test's expected `{mode}.{suffix}.golden`; normalized goldens to `{mode}.report.golden` + fixed the script. (Test-harness naming, not a logic bug; the report files themselves were byte-correct.)

**No design deviations.** All §3.1–§3.6 behavior + rev-1 folds (C1 re-flush, fresh-buffer seeding, CRLF/malformed, dup/blank-line) + V1–V24 implemented.

**Post-code-review (rev 2, 2026-05-29 — dual code-review + plan-manager).** Both code reviews **APPROVE** (no Critical/High; both cross-checked the binary against live Perl v0.25.1 → byte-identical). plan-manager flagged **test-coverage gaps only** (production code complete). Closed:
- **B-M1:** golden assertions now compare raw `Vec<u8>` (was `from_utf8_lossy`, which could mask a non-UTF-8 byte regression).
- **V10/B-M2:** added `covered_chromosomes_emit_in_cov_appearance_order_not_sorted` (cov order ≠ genome/sorted order — now discriminating).
- **V21:** added `duplicate_position_last_write_wins`.
- **V23:** added `three_way_covered_then_uncovered_sorted`.
- **V22:** added `blank_and_trailing_lines_are_ignored_end_to_end`.
Result: **71 tests pass**, clippy clean. Low/optional items (u32-overflow hardening, per-chromosome `Vec` buffering → stream in Phase C) noted as non-blocking follow-ups.

## 1. Goal

Implement the core genome-wide cytosine report — the heart of the port. Read the `*.bismark.cov[.gz]`, walk the genome per chromosome with byte-exact coordinate arithmetic, classify cytosine context, and emit the per-cytosine report (CpG-only default; `--CX` all-context) **byte-identical to Perl v0.25.1**, plus the always-on `*.cytosine_context_summary.txt`. **PLAIN (uncompressed) single-file output only** — `--gzip` + `--split_by_chromosome` are Phase C; `--merge_CpGs` is Phase D.

**Acceptance:** on a synthetic genome + hand-built `.cov`, the Rust report + context summary are byte-identical to Perl-v0.25.1 goldens across {CpG, `--CX`, `--zero_based`, `--coverage_threshold`} and covered+uncovered chromosome ordering; all unit tests green; clippy clean.

## 2. Context

- **Where:** new modules in `rust/bismark-coverage2cytosine/src/`: `cov.rs` (coverage-file parse), `report.rs` (genome walk + per-position kernel + emit), `summary.rs` (context summary). Orchestrated by a new `pub fn run(&ResolvedConfig) -> Result<(), BismarkC2cError>` in `lib.rs`, called from `main.rs` (replacing the Phase-A "genome load + stub" body).
- **Consumes Phase A (shipped):** `ResolvedConfig` (`cov_infile`, `output_stem`, `output_dir`, `cpg_only`, `cx_context`, `zero_based`, `threshold`), `Genome` (`get`, `contains`, `names_sorted` — the only name iterator), `BismarkC2cError`. **Depends on:** `phase-a-scaffold-cli-genome/PLAN.md` (done).
- **Perl ground truth:** `coverage2cytosine` — `generate_genome_wide_cytosine_report:168-745`, `process_unprocessed_chromosomes:1388-1565`, `reset_context_summary:1961-1975`, `context_reporting:1977-1988`, `print_context_summary:63-78`, `handle_filehandles:89-165` (filename derivation).
- **Reuse:** the `flate2::MultiGzDecoder` + `BufReader` gz-detection pattern already in `genome.rs::read_one_fasta`.

## 3. Behavior

### 3.1 Coverage-file parse + per-chromosome streaming (mirrors Perl `:184-468`)
1. Open `cov_infile`; if name ends `.gz` → `MultiGzDecoder`, else plain; `BufReader`.
2. Read line by line (as **bytes** — chr names may be non-UTF-8 and must byte-match `Genome` keys). **Strip a trailing `\r`** from the line first (CRLF cov files; rev 1 B-I1). **Skip an empty line** (e.g. a trailing `\n` at EOF) — it must NOT create a phantom chromosome (rev 1 B-I3). Split on `\t`: field 0 = `chr` (`Vec<u8>`), field 1 = `start` (1-based `u32`), **field 2 (end) and field 3 (%) discarded**, field 4 = `meth` (`u32`), field 5 = `nonmeth` (`u32`).
   - **Numeric-field policy (rev 1 B-I1, accepted divergence):** `start`/`meth`/`nonmeth` are parsed as strict `u32`. Perl coerces leniently (`"123\r"`→123, `"abc"`→0, non-fatal); the Rust port instead errors (`BismarkC2cError::MalformedCovLine { line_no }`) on a non-numeric/short line. This cannot occur on real `bismark2bedGraph` output; erroring loud beats silently emitting `0` (fail-explicitly principle). Documented divergence; tested.
3. **Stream one chromosome at a time** (Perl buffers `%chr` for the current chromosome only): accumulate `start → (meth, nonmeth)` into a `HashMap<u32,(u32,u32)>` via `insert` (**last-write-wins on a duplicate position**, matching Perl's `%chr` hash overwrite at `:224-225`; rev 1 B-I2). When `chr` **differs from the previous line's chr**, **flush** the just-finished chromosome (run §3.2 on it), **clear the buffer, and seed the fresh buffer with the *triggering* line's `(start → meth,nonmeth)`** (Perl `:450-455`; rev 1 A — without this, the first covered position of every non-first chromosome is dropped). Add the chr to `seen: HashSet<Vec<u8>>`. Flush the final chromosome after EOF.
4. **`seen` drives ONLY the uncovered pass — NEVER flush suppression (rev 1 C1, Critical).** Flush happens on *every* chr-transition (+ EOF), driven solely by `chr != prev_chr`. So a **non-contiguous** cov (`chrA…chrB…chrA`) re-flushes — re-walks and **re-emits chrA's full report a second time** (with only the second block's coverage) — exactly as Perl does (its `%processed` is idempotent and does not suppress re-flush; flush is purely the `:227` transition). Do NOT use `seen` to dedup a re-seen chromosome at flush time. Never fires on real sorted cov, but byte-identity is asserted unconditionally; pinned by a test.
5. **Empty-input guard (Perl `:472-474`):** if EOF is reached with no chromosome ever started (zero data lines), return `BismarkC2cError::EmptyCoverageInput` — **before** any uncovered-chromosome pass (Perl dies here even when threshold==0).
6. **Chromosome ordering** = flush order = **coverage-file appearance order** (pitfall P1: this is why we stream rather than collect into a `BTreeMap`).

### 3.2 Per-chromosome genome walk + kernel (mirrors Perl `:242-448`)
For chromosome `name` with sequence `seq = genome.get(name)`:
- If `genome.get(name)` is `None` (cov chr absent from genome): emit nothing (Perl's `while(undef =~ /[CG]/g)` yields zero matches). Optionally one stderr note. Continue.
- Walk every byte `i` in `seq` where `seq[i] == b'C'` or `b'G'`. Set `pos = (i + 1) as u32` (1-based — Perl `pos()`). Apply the **shared kernel** (§3.3).

**Single shared kernel** for all three Perl blocks (covered-chr `:262-448`, last-chr `:507-690`, uncovered-chr `:1421-1560`). Verified 2026-05-29: the two Perl blocks differ in guard *order* (covered: `len<3`→last-base→lookup→threshold; last: lookup→threshold→`len<3`→last-base) but produce an **identical emitted set and identical STDERR-warn set** (all guards are skip-guards; threshold precedes context-classify in both, so sub-threshold positions never reach the unclassifiable-warn). One kernel is therefore byte-identical to all three and avoids Perl's triple duplication (cf. the dual-driver back-port memory).

### 3.3 The per-position kernel (guard order = Perl covered-chr block `:262-447`)
Given `seq`, `i`, `pos`, the base `seq[i]`, the chromosome's coverage buffer, and `accumulate_summary: bool`:
1. **Extract** `tri_nt` + `upstream` (5'→3' oriented), per strand:
   - **`C` (forward, strand `+`):** `tri_nt = seq[i .. min(i+3, len)]`; `upstream = perl_substr(seq, i as isize - 1, 3)`.
   - **`G` (reverse, strand `-`):** if `i < 2` (Perl `pos-3 < 0`): `tri_nt = seq[0 .. i+1]` (will be <3 → dropped); else `tri_nt = revcomp(seq[i-2 .. i+1])`. `upstream = revcomp(perl_substr(seq, i as isize - 1, 3))`.
   - `revcomp` = reverse then complement via `tr/ACTG/TGAC/` (A↔T, C↔G; **all other bytes incl. `N` pass through unchanged**).
2. **Guard:** `tri_nt.len() < 3` → skip (chromosome edge).
3. **Guard (last-base):** `(seq.len() as u32 - pos) == 0` → skip (the final genome base; its bottom-strand partner needs the following base — Perl `:347`).
4. **Coverage lookup:** `(meth, nonmeth) = buffer.get(&pos).copied().unwrap_or((0, 0))`.
5. **Guard (threshold):** `meth + nonmeth < threshold` → skip. (Default `threshold == 0` never skips → uncovered positions emit `0 0`.)
6. **Classify context** on `tri_nt` (byte regex, Perl `:365-377`): starts `CG` → `CG`; `^C.G$` (len 3) → `CHG`; `^C..$` (len 3) → `CHH`; else → stderr warn + skip. (`.` matches any byte incl. `N`.)
7. **Accumulate summary** (only if `accumulate_summary`; Perl `context_reporting` `:381`): `ubase = upstream[0]`; if `tri_nt` and `ubase` are pure `ACTG`, add `meth`→m, `nonmeth`→u at `summary[tri_nt][ubase]`. (Runs **before** the CpG-only emit filter, so the summary reflects all contexts even in default mode.)
8. **Emit** (Perl `:384-447`): if `cpg_only` → emit only when `context == CG`; if `cx_context` → emit every classified position. Report line (§3.4).

### 3.4 Report line format (Perl `:408` etc.)
Tab-separated bytes + trailing `\n`, written via a byte buffer (chr + tri_nt are raw bytes):
```
<chr>\t<pos>\t<strand>\t<meth>\t<nonmeth>\t<context>\t<tri_nt>\n
```
- `pos` = `pos` (1-based), or `pos - 1` under `--zero_based` (Perl `:397`).
- `strand` = `+` (C) / `-` (G); `context` = `CG`/`CHG`/`CHH`; `tri_nt` = the 5'→3' uppercased bytes.

### 3.5 Uncovered chromosomes (Perl `:718-728`, `process_unprocessed_chromosomes`)
**Only when `threshold == 0`** (a positive threshold skips uncovered chromosomes entirely — Perl `:714`). After all covered chromosomes: for each `name` in `genome.names_sorted()` not in `seen`, run §3.2 with an **empty** coverage buffer and `accumulate_summary = false` (Perl's uncovered pass does NOT call `context_reporting`). Every emitted position has `meth = nonmeth = 0`.

**Equivalence to Perl (rev 1 A#3 — verified, do not "simplify" away):** Perl iterates `sort keys %processed` (`:722`) and emits those with `$processed{chr}==0`. Perl seeds `$processed{chr}=0` for **every genome chromosome** at load (`read_genome_into_memory:1712`/`:1734`) and sets it to 1 when a chr is flushed. So Perl's uncovered set = `{all genome chrs} − {flushed chrs}`, bytewise-sorted — exactly `genome.names_sorted()` filtered by `!seen.contains(name)`. (A cov chr absent from the genome is never in `names_sorted()`, so it cannot appear in the uncovered pass — matching Perl, where it was never seeded into `%processed`.)

### 3.6 Context summary file (Perl `:63-78`, `:1961-1988`) — ALWAYS written, uncompressed
- **Init**: 16 trinucleotides `C{A,C,G,T}{A,C,G,T}` × 4 upstream bases `{A,C,G,T}` = 64 cells, counts 0.
- **Output** (after the report): header `upstream\tC-context\tfull context\tcount methylated\tcount unmethylated\tpercent methylation\n`; then rows sorted by `(tri_nt, ubase)` bytewise: `<ubase>\t<tri_nt>\t<ubase><tri_nt>\t<m>\t<u>\t<perc>\n` where `perc = format!("{:.2}", m as f64/(m+u) as f64*100.0)` if `m+u>0` else literal `N/A`.
- Filename = `{output_dir}{output_stem}.cytosine_context_summary.txt` (Perl `:115-117`).

## 4. Signatures

```rust
// report.rs
pub fn run_report(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError>;

/// Faithful Perl substr(seq, offset, want): negative offset counts from the
/// end; result truncated at string end; empty if start is out of range.
fn perl_substr(seq: &[u8], offset: isize, want: usize) -> &[u8];

/// Reverse-complement via tr/ACTG/TGAC/ (A<->T, C<->G; other bytes incl. N unchanged).
fn revcomp(seq: &[u8]) -> Vec<u8>;

#[derive(Clone, Copy, PartialEq)]
enum Context { Cg, Chg, Chh }
fn classify_context(tri_nt: &[u8]) -> Option<Context>;   // None = unclassifiable

// summary.rs
struct ContextSummary { /* 16x4 grid, keyed (tri_nt, ubase) */ }
impl ContextSummary {
    fn new() -> Self;                                    // 64 zeroed cells
    fn accumulate(&mut self, tri_nt: &[u8], ubase: u8, meth: u32, nonmeth: u32);
    fn write_to(&self, w: &mut impl Write) -> io::Result<()>;
}

// cov.rs
fn open_cov(path: &Path) -> Result<Box<dyn BufRead>, BismarkC2cError>;   // gz-aware

// lib.rs
pub fn run(config: &ResolvedConfig) -> Result<(), BismarkC2cError>;      // load genome + run_report
```

## 5. Implementation outline (TDD-friendly)

1. **`perl_substr` + `revcomp` + `classify_context`** (pure, table-driven) with unit tests first (the crux primitives).
2. **`ContextSummary`** (new/accumulate/write) + unit tests (64-row order, `%.2f` vs `N/A`, pure-ACTG gating).
3. **`cov.rs::open_cov`** + a line parser (bytes → `(chr, start, meth, nonmeth)`); unit tests incl. gz.
4. **The kernel** (`extract` + guards + classify + accumulate + emit into a byte buffer) as a testable function over `(seq, i, buffer, cpg_only, zero_based, &mut summary, accumulate, &mut out)`; unit tests at i=0/1, chr-end, interior, both strands, N-handling, CpG vs CX, zero_based.
5. **`run_report`**: open cov + writers; stream-flush per chromosome (covered, appearance order; flush on every chr-transition incl. re-seen chrs per §3.1.4; seed fresh buffer with the triggering line per §3.1.3); empty-input guard; uncovered pass (sorted, threshold==0); write summary. Add `EmptyCoverageInput` and `MalformedCovLine { line_no: usize }` to `BismarkC2cError`.
6. **Filename derivation**: `{output_dir}{output_stem}.CpG_report.txt` / `.CX_report.txt`; summary file. (Output writer = `BufWriter<File>` for Phase B; structure it so Phase C can swap in `GzEncoder` + per-chromosome multiplexing — e.g. a `fn open_report_writer(&config) -> Box<dyn Write>` seam.)
7. **`lib.rs::run`** + wire `main.rs`.
8. **Integration test**: tiny synthetic genome (CpG-at-start, CpG-at-end, N run, short scaffold, an uncovered chromosome) + hand-built `.cov`; compare Rust output to committed **Perl-v0.25.1 goldens** for CpG / `--CX` / `--zero_based` / `--coverage_threshold`.

## 6. Efficiency
- O(genome length) walk + O(1) per-position lookup; one chromosome's coverage buffer in memory at a time (Perl-equivalent). Genome held whole (Phase A). Report written through an 8 KiB `BufWriter`. No premature parallelism (byte-identity gate first; parallel walk is a v1.x candidate, SPEC §10.7).

## 7. Integration
- **Reads:** `cov_infile` + the `Genome` (Phase A). **Writes:** `{stem}.CpG_report.txt`/`.CX_report.txt` + `{stem}.cytosine_context_summary.txt` (plain).
- **Downstream:** Phase C wraps the report/cov writers in gzip + per-chromosome split (the `open_report_writer` seam). Phase D re-reads the emitted CpG report. So Phase B's exact report-line bytes are the contract for C, D, and the eventual extractor inline-switch.

## 8. Assumptions

**From epic (shared):** byte-identity to Perl v0.25.1 (STDERR exempt); cov is 1-based, tab-separated, sorted by chr-then-pos, col 4 discarded; report is genome-driven (covered = cov-appearance order via streaming, **never `BTreeMap`**; uncovered = `names_sorted()`; uncovered emitted only when threshold==0); genome uppercased + in RAM; `u32` positions/counts; `#![forbid(unsafe_code)]`; all work in `../Bismark-c2c`, never touch `bismark-extractor`/`bismark-bedgraph`.

**Phase-B specific:**
1. `pos = i + 1`; the §3.3 substr/revcomp arithmetic is the single source of coordinate truth.
2. Only `upstream` uses `perl_substr`'s negative-wrap (forward-C `i=0` → `substr(seq,-1,3)` = trailing 1 byte); `tri_nt` never uses a negative offset (the reverse-G `i<2` branch avoids it).
3. One shared kernel is byte-identical to all three Perl blocks (verified §3.2).
4. A cov chromosome absent from the genome emits nothing (Perl empty-walk).
5. Empty cov input → `EmptyCoverageInput` (Perl dies before the uncovered pass), even when threshold==0.
6. `f64` for the summary `%.2f` matches Perl `sprintf "%.2f"` (Perl uses C `double`; verify on goldens — see V-rows).

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | `perl_substr` semantics | unit: interior, `offset=-1` wrap, out-of-range, end-truncation | matches Perl substr |
| V2 | `revcomp` | unit: `ACGT`→`ACGT`(rev-comp), `N`/other bytes unchanged | correct; N untouched |
| V3 | `classify_context` | unit: `CGx`→CG, `CAG`→CHG, `CAA`→CHH, `CNG`→CHG, `CNN`→CHH, `len<3`/non-`C` → None | per Perl regex |
| V4 | forward-C extraction | unit: interior i, i=0 (upstream wrap), chr-end (`tri_nt`<3 skip) | exact `tri_nt`/`upstream` bytes |
| V5 | reverse-G extraction | unit: interior, i=0/1 (`tri_nt`<3), revcomp correctness, upstream revcomp | exact bytes |
| V6 | last-base exclusion | unit: position `pos==len` skipped both strands | no emit |
| V7 | threshold guard | unit: `--coverage_threshold 5` drops <5 coverage; default 0 emits `0 0` | per Perl |
| V8 | CpG-only vs `--CX` | unit: default emits only CG; `--CX` emits CG+CHG+CHH | correct filter |
| V9 | `--zero_based` | unit: emitted pos = `pos-1` | correct |
| V10 | covered order = cov appearance | unit: cov chrs `chrB,chrA` → report emits chrB before chrA | appearance order (not sorted) |
| V11 | uncovered order = sorted, threshold-gated | unit: uncovered chrs emitted in `names_sorted()` order; none when threshold>0 | per Perl |
| V12 | empty cov → error | unit: empty cov file → `EmptyCoverageInput` (no output) | error, even threshold 0 |
| V13 | cov chr absent from genome | unit: cov has `chrZ` not in genome → emits nothing, no panic | empty walk |
| V14 | context summary | unit: 64-row sorted output, header bytes, `%.2f` vs `N/A`, pure-ACTG gating, i=0 wrap ubase | per Perl |
| V15 | **byte-identity integration** | synthetic **mixed covered+uncovered** genome (incl. 1-bp + 2-bp degenerate scaffolds for the `i=0`-wrap × `len<3` boundary, an N-run, CpG-at-start/end) + hand-built `.cov` → Rust vs **Perl-v0.25.1 golden** for {CpG, --CX, --zero_based, --threshold} | raw-byte-identical report + summary (+ a non-round percentage like `403/803→50.19`) |
| V16 | **exact report-line bytes** (rev 1 A#4/B-I4) | unit: one position → assert the exact 7-tab-field line, single trailing `\n`, no trailing tab, raw chr+tri_nt bytes | byte-exact |
| V17 | **non-contiguous chr re-flush** (rev 1 C1) | unit: cov `chrA,chrB,chrA` → chrA's report emitted **twice** (2nd with only the 2nd block's coverage); `seen` does not dedup | matches Perl re-flush |
| V18 | **fresh-buffer seeding** (rev 1 A) | unit: first covered position of a non-first chromosome is present in output (not dropped on the transition) | position emitted |
| V19 | **CRLF cov line** (rev 1 B-I1) | unit: `chr\t1\t1\t100\t3\t0\r\n` parses to `(chr,1,3,0)` | trailing `\r` stripped |
| V20 | **malformed cov line** (rev 1 B-I1) | unit: non-numeric/short line → `MalformedCovLine { line_no }` | error (documented divergence) |
| V21 | **duplicate position last-write-wins** (rev 1 B-I2) | unit: two cov lines same chr+pos → 2nd `(meth,nonmeth)` used | last wins (Perl `:224`) |
| V22 | **blank/trailing line** (rev 1 B-I3) | unit: cov ending in `\n` / containing a blank line → no phantom chromosome, no `EmptyCoverageInput` misfire | clean |
| V23 | **three-way interleaved ordering** (rev 1 A#4) | unit: covered chr appears mid-genome between uncovered ones → covered emitted in cov order, uncovered bytewise-sorted around it | per Perl |
| V24 | **threshold>0 suppresses uncovered pass** (rev 1 A-opt) | unit: `--coverage_threshold 5` → uncovered chromosomes emit nothing | per Perl `:714` |

## 10. Questions or ambiguities
| Priority | Question | Assumption |
|----------|----------|------------|
| **Resolved** | Does Rust `format!("{:.2}", f64)` match Perl `sprintf "%.2f"` rounding? | **Non-issue** — both plan reviewers empirically verified (one compiled both): Rust and Perl/C both round-half-to-even on `f64` across half-way doubles AND realistic `m/(m+u)*100` percentages, byte-identical. V14 + a non-round golden row (e.g. `403/803 → 50.19`) lock it. |
| Open | stderr note when a cov chromosome is absent from the genome? | Emit a one-line note (STDERR not byte-identity-gated); harmless. |

No **Critical** ambiguities — SPEC §7/§8 + the verified guard-order analysis pin the behavior. (The rev-1 Critical C1 was a plan-clarity fix — the *behavior* was already correct; see §3.1.4.)

## 11. Self-Review
- **Logic:** guard order taken from the covered-chr block; proven equivalent to the last-chr + uncovered blocks (§3.2). Summary accumulation gated to covered chromosomes only (matches Perl). Empty-cov error precedes uncovered pass (matches Perl die-before-720).
- **Edge cases:** i=0 upstream wrap (V1/V4/V14), chr-start reverse-G `tri_nt`<3 (V5), last-base exclusion (V6), N in flanks (V2/V3), cov-chr-not-in-genome (V13), empty cov (V12), uncovered-only-when-threshold-0 (V11).
- **Efficiency:** one-chromosome-at-a-time buffer; O(genome) walk.
- **Integration:** `open_report_writer` seam designed for Phase C; report-line bytes pinned for Phase D + extractor.
- **Adjusted from SPEC:** none — Phase B implements SPEC §7/§8 as written.

**Folded from dual plan-review (rev 1, 2026-05-29 — both APPROVE-WITH-CHANGES; single-kernel + coordinate arithmetic verified correct by both):**
- **C1 (Critical, B): non-contiguous chr re-flush** — `seen` drives only the uncovered pass, never flush suppression; flush on every chr-transition re-emits a re-seen chr (matches Perl). §3.1.4 + V17.
- **(A) fresh-buffer seeding** — the transition's triggering line seeds the new buffer (Perl `:450-455`). §3.1.3 + V18.
- **(B-I1) CRLF + malformed cov** — strip trailing `\r`; strict `u32` parse → `MalformedCovLine` (documented accepted divergence vs Perl's lenient coercion). §3.1.2 + V19/V20.
- **(B-I2/I3) duplicate-position last-write-wins; blank/trailing-line no phantom flush.** §3.1.3/§3.1.2 + V21/V22.
- **(A#3) `names_sorted() \ seen` ≡ `sort keys %processed`** documented (Perl seeds all genome chrs at load). §3.5.
- **%.2f rounding → Resolved** (both reviewers verified byte-identical round-half-to-even). §10.
- **Test gaps:** exact-report-line-bytes (V16), mixed covered+uncovered + degenerate scaffolds golden (V15), three-way ordering (V23), threshold>0 uncovered-suppression (V24).

**Remaining risks:** the byte-identity golden (V15) is the ultimate check — coordinate arithmetic + summary are unit-verified, but only the Perl-v0.25.1 golden on a multi-chromosome fixture confirms the whole pipeline. To be generated on first implementation.

## Revision history
- **rev 0** (2026-05-29): initial Phase B plan from EPIC + SPEC rev 3 + Phase-A shipped code + Perl ground-truth verification (guard-order equivalence confirmed). Awaiting manual review → dual plan-review.
- **rev 1** (2026-05-29): dual plan-review folded (A+B both APPROVE-WITH-CHANGES; single-kernel claim + coordinate arithmetic + `%.2f` parity all verified correct by both). 1 Critical (C1 non-contiguous re-flush — plan-clarity, behavior was already correct) + Important (fresh-buffer seeding, CRLF/malformed parse, duplicate/blank-line, names_sorted≡processed doc) + 9 new test rows (V16–V24).
