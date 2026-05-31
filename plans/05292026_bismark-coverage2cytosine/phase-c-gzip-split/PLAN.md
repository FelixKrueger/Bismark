# Phase C PLAN — `--gzip` + `--split_by_chromosome`

**Epic:** `05292026_bismark-coverage2cytosine/EPIC.md`, Phase C — `--gzip` + `--split_by_chromosome`
**Design contract:** `../SPEC.md` (rev 3) — §5 (output topology), §10.5 (writers).
**Status:** rev 1 — implemented + green (see Implementation notes). Awaiting dual code-review + plan-manager.

## Implementation notes (Phase C — 2026-05-29)

**Implemented + byte-identical to Perl v0.25.1.** Changes: `cli.rs` +`ResolvedConfig.output_raw`; `report.rs` +`ReportWriter{Plain,Gz}` (explicit `finish()`), `run_report`→`run_single`/`run_split`, `flush_split_chromosome`, new `report_name`/`summary_name`/`report_path`/`summary_path` (raw-`-o` split base, `.gz` suffix); `Cargo.toml` `flate2` dev-dep. **81 tests pass** (58 unit + 11 phase-B golden + 7 phase-C golden + 5 sanity); clippy `-D warnings` clean; workspace builds; siblings untouched.

**Golden validation** (`tests/golden_phase_c.rs` + `tests/data/phase_b/phase_c/{split,split_thr}/`, generated locally from repo Perl v0.25.1): `--gzip` (decompressed report == plain golden; summary plain), `--CX --gzip`, `--split` (whole-dir byte-identity incl. the empty-vs-last summary quirk), `--split --gzip` (per-chr decompress), suffixed-`-o` split (doubled-suffix names, content == split golden), `--split --threshold 5` (only covered chrs get files), and split re-appearance (truncate + summary-in-last-chr). All match Perl.

**Iteration log:**
- #1: clippy `unused_assignments` on `last_summary_path` (loop-body assignment overwritten before read; then the `None` initializer itself). Fixed by flushing-and-discarding non-final chrs in the loop and computing `last_summary_path: PathBuf` directly from the final-chr match (no dead `None` init).

**No design deviations** beyond #1 (a cleanup). All §3.1–§3.5 + V1–V14 implemented; the two rev-1 Criticals (raw-`-o` split filename doubling; truncate-on-reopen) are verified by goldens + the re-appearance test.

## 1. Goal

Add the two output-shaping flags on top of Phase B's byte-identical report, **without touching the per-position kernel or the streaming walk**:
- **`--gzip`**: gzip-compress the report file (`.CpG_report.txt.gz` / `.CX_report.txt.gz`); the context summary stays **plain**.
- **`--split_by_chromosome`**: one report file per chromosome (`{stem}.chr{CHRNAME}.{CpG|CX}_report.txt[.gz]`), reproducing Perl's per-chromosome context-summary quirk.

**Byte-identical to Perl v0.25.1** for `--gzip` (after decompression), `--split_by_chromosome`, and the two combined.

## 2. Context

- **Where:** `rust/bismark-coverage2cytosine/src/report.rs` — generalize the Phase-B writer seam (`open_report_writer`/`open_summary_writer`/`output_path`/`report_filename`/`summary_filename`) + `run_report`. No changes to `emit_position`, `extract`, `flush_chromosome`'s walk, `cov.rs`, `summary.rs`'s `ContextSummary` (only where its output is routed). Add `flate2::write::GzEncoder` (flate2 already a dep).
- **Depends on:** `phase-b-core-report/PLAN.md` (shipped — `run_report` streaming + kernel are the substrate).
- **Perl ground truth:** `handle_filehandles:89-165` (filename `.chr` infix `:101`; gzip pipe `:139-150`; per-chr `CONTEXTSUMMARY` reopen `:115-117`) + split call sites `generate_genome_wide_cytosine_report:200, 216-219, 457-466` + `print_context_summary:49,63-78` (written once, at the end).

### Empirically observed (local Perl v0.25.1 — ground truth)
1. **`--gzip`**: report → `{stem}.CpG_report.txt.gz` (gzip); `{stem}.cytosine_context_summary.txt` is **plain ASCII, never gzipped**.
2. **`--split_by_chromosome`** (genome chr1/chr2/scaf_short/chr3uncov, cov covers chr1/chr2): files `split.chr{CHRNAME}.CpG_report.txt` — the infix is **literal `.chr` + the chromosome name** (so chr `chr1` → `split.chrchr1.CpG_report.txt`). A chromosome that emits nothing (e.g. 2-bp `scaf_short`) still gets a **0-byte** report file.
3. **Context-summary-in-split quirk**: each chromosome gets a `{stem}.chr{CHRNAME}.cytosine_context_summary.txt`, but **only the LAST-processed chromosome's is non-empty** (full 64-row genome summary); all others are **0 bytes**. (Perl reopens the global `CONTEXTSUMMARY` per chr (truncate) but writes the summary once at the end.) Last-processed = last in {covered cov-order …, then uncovered bytewise-sorted} — in the fixture, `scaf_short` (sorts last) holds the summary.

## 3. Behavior

### 3.1 `--gzip` (non-split)
- Report writer wraps the output `File` in `flate2::write::GzEncoder` (default compression) when `config.gzip`; filename gains `.gz`. The encoder is **explicitly `finish()`ed** at end-of-run (gzip trailer).
- The **context-summary writer is NEVER gzipped** — plain `File`, filename unchanged.
- **Byte-identity:** the gzip *container* is not asserted equal to Perl's `gzip -c` (impl-dependent); the **decompressed report content is** (== the plain report). Test = decompress the Rust `.gz` and compare to the plain `default`/`cx` golden (§9).

### 3.2 `--split_by_chromosome`
Per Perl, in split mode the single shared writer is replaced by a report writer opened **fresh (truncating) on every chromosome transition** — exactly mirroring `handle_filehandles` being re-invoked per chr (`:457-466`):

- **Filename — uses the RAW `-o`, NOT the stripped stem** (rev 1 C1, reviewer A vs Perl `:99-112`). Perl appends `.chr{NAME}` to the *raw* `-o` **first**, then runs the suffix-strip anchored at `$` — which now **no-ops** (the string ends in the chr name, not the report suffix). So:
  - split base = `{raw_-o}` + `.chr{CHRNAME}` (literal `.chr` + name; **no suffix stripping**).
  - report = `{split_base}.{CpG|CX}_report.txt[.gz]`; summary = `{split_base}.cytosine_context_summary.txt`.
  - Consequence: a **suffixed** `-o` (the extractor passes `-o …CpG_report.txt`) **doubles** the suffix: `-o foo.CpG_report.txt --split` → `foo.CpG_report.txt.chrchr1.CpG_report.txt`. A bare `-o split` → `split.chrchr1.CpG_report.txt`. **Both confirmed against live Perl.** This requires `ResolvedConfig` to retain the **raw `-o`** (see §3.5).
- **Truncate-on-reopen (rev 1 C1, reviewer B vs Perl `:146`/`:457-466`):** open the per-chr report file with `File::create` (truncate) on **every** transition; do **NOT** cache/reuse a per-chr-name writer. So a **non-contiguous** re-appearance (`chrA…chrB…chrA`) leaves chrA's file holding **only the last segment** (the reopen truncates) — *unlike* non-split mode, which appends/re-emits to the single file (Phase B's tested behavior). Verified against live Perl.
- Each covered chromosome (cov-appearance order, as streamed) and each uncovered chromosome (sorted, threshold==0 only) opens its writer, gets its report walk written, then `finish()`/close. **A chromosome emitting zero lines still produces its report file** — a 0-byte plain file, or (under `--gzip`) a valid ~20-byte empty-gzip stream that decompresses to 0 bytes (Perl pipes through `gzip -c`; confirmed, rev 1 §10 Q1). So under `--gzip`, even zero-emitting chrs must `GzEncoder::finish()` an unwritten encoder.
- **Context summary (the quirk):** on each chromosome transition, **create/truncate** its `{split_base}.cytosine_context_summary.txt` (empty). After ALL chromosomes, write the **full** `ContextSummary` (whole-genome, unchanged from Phase B) **only to the summary path of the LAST chromosome reopened** (= the last flush call's chr — for `chrA…chrB…chrA` that's **chrA**, not chrB; for full coverage with no uncovered pass, the last covered chr in cov order — rev 1 §10 Q2 confirmed). Net: N empty summary files + 1 full. Summaries are **never gzipped**, and the full one is **byte-identical to the non-split summary** (the accumulation is genome-wide regardless of split).

### 3.3 `--gzip` + `--split_by_chromosome`
Per-chr report writers are gz-wrapped (`{stem}.chr{CHRNAME}.CpG_report.txt.gz`); the per-chr summary files stay plain; the last-chr summary quirk holds.

### 3.4 Unchanged from Phase B
The cov streaming, the per-position kernel (`emit_position`), `extract`/`perl_substr`/`revcomp`/`classify_context`, the covered-appearance + uncovered-sorted ordering, the empty-input guard, the `ContextSummary` accumulation — all identical. Phase C only changes **where bytes are routed** (which file, gz or not).

### 3.5 Phase-A `ResolvedConfig` change — retain the raw `-o` (rev 1 C1)
C1 requires the **raw `-o` value** (split derivation uses it un-stripped). Phase A's `ResolvedConfig` currently keeps only the suffix-stripped `output_stem`. **Add a field `output_raw: String`** (the verbatim `-o` value) alongside `output_stem`; populate both in `Cli::validate`. This is a small, additive change to the shipped `cli.rs` (same crate/branch; Phase A not yet merged to `rust/iron-chancellor` — it's in open PR #892). `output_stem` (context-conditional strip) remains the source for **non-split** filenames (Phase B behavior unchanged); `output_raw` feeds **split** filenames. Phase B's existing tests must stay green (regression guard).

## 4. Signatures

```rust
// report.rs — replace the Box<dyn Write> seam with an explicit-finish wrapper.
enum ReportWriter {
    Plain(BufWriter<File>),
    Gz(flate2::write::GzEncoder<BufWriter<File>>),
}
impl ReportWriter {
    fn create(path: &Path, gzip: bool) -> Result<Self, BismarkC2cError>;
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()>;
    fn finish(self) -> Result<(), BismarkC2cError>;   // Gz: encoder.finish(); Plain: flush()
}

// Filename derivation (extended). The BASE differs by mode (rev 1 C1):
//   base(config, chr) = match chr {
//       Some(name) => format!("{}.chr{}", config.output_raw, name),  // split: RAW -o, NO strip
//       None       => config.output_stem,                            // non-split: stripped stem
//   }
//   report_path(config, chr) -> PathBuf  // base + ".{CpG|CX}_report.txt" + (gzip? ".gz")
//   summary_path(config, chr) -> PathBuf // base + ".cytosine_context_summary.txt"  (never .gz)

pub fn run_report(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError>; // unchanged sig
```

## 5. Implementation outline (TDD-friendly)

1. **`ReportWriter` enum + `create`/`write_all`/`finish`** (gz vs plain, explicit finish). Unit test: write bytes via each variant → plain file equals input; gz file decompresses to input.
2. **`ResolvedConfig.output_raw`** (rev 1 C1): add the verbatim `-o` field in `cli.rs`; populate in `validate`. Keep `output_stem` for non-split. (Phase B tests unaffected.)
3. **Filename derivation**: `base(config, chr)` per §4 — split uses `output_raw` + `.chr{name}` (no strip), non-split uses `output_stem`; then `+ suffix + (gzip? ".gz")` for report, `+ ".cytosine_context_summary.txt"` (never gz) for summary. Unit tests incl. the **suffixed-`-o` doubling**: `-o foo.CpG_report.txt` split → `foo.CpG_report.txt.chrchr1.CpG_report.txt`; bare `-o split` split → `split.chrchr1.CpG_report.txt`; non-split `-o foo.CpG_report.txt` → `foo.CpG_report.txt` (stripped); `.gz` + `.CX` combos.
3. **Non-split `--gzip`**: in `run_report`, build the single `ReportWriter` with `config.gzip`; `finish()` it before writing the (plain) summary. Keep the Phase-B flush/uncovered flow.
4. **Split mode**: branch in `run_report`. For each chromosome processed (covered as streamed — **including a re-appearing chr**; uncovered sorted), call `flush_chromosome_to_own_file(name, …)` that: opens a per-chr `ReportWriter` via **`File::create` (truncate) every time — NO per-chr-name caching** (so a re-appearance truncates, keeping only the last segment), using `base = output_raw + ".chr" + name` (§4); runs the same walk (`emit_position` into a `Vec<u8>`); writes + `finish()`es it (under `--gzip`, `finish()` even on an empty `Vec` → valid empty-gzip stream). Then **create/truncate the per-chr summary file empty** and record its path as `last_summary_path`. After all chromosomes, write the full `ContextSummary` to `last_summary_path` (the last chr reopened). Do not special-case zero-emitting chrs — they still get their (empty/empty-gzip) report file.
5. **Wire** the split vs non-split branch in `run_report`; the streaming loop + uncovered pass are shared (only the per-chromosome sink differs).
6. **Goldens + tests** (§9): extend `tests/data/phase_b/generate_goldens.sh` (or a `phase_c/` dir) to emit `--gzip`, `--split_by_chromosome`, and combined goldens from the repo Perl; add `tests/` assertions.

## 6. Efficiency
- gz: streaming compression over the per-run (or per-chr) byte buffer; negligible overhead. Split mode opens/closes one file handle per chromosome (fine — genomes have ≤ thousands of contigs; `--gazillion`-scale many-scaffold genomes are a Perl `bismark2bedGraph` concern, not c2c). No change to the O(genome) walk.

## 7. Integration
- **Reads:** unchanged (cov + genome). **Writes:** report(s) (plain or gz; single or per-chr) + summary file(s).
- **Downstream:** Phase D (`--merge_CpGs`) re-reads the **CpG report**; it is mutually exclusive with `--split_by_chromosome` and engages its own gz handling (Phase A validation already forbids `--merge_CpGs --split_by_chromosome` and `--merge_CpGs --CX`). Phase C's `ReportWriter`/`create(path, gzip)` is reused by Phase D for the merged-cov writer.
- **Extractor inline switch (future):** the extractor drives `--gzip`/`--split_by_chromosome` via the subprocess argv today; the inline API gains nothing new (same `run`).

## 8. Assumptions

**From epic (shared):** byte-identity to Perl v0.25.1 (STDERR exempt); gzip compared **after decompression** (SPEC §15 resolved — container not asserted); summary never gzipped; covered = cov-appearance order, uncovered = sorted; all work in `../Bismark-c2c`, never touch `bismark-extractor`/`bismark-bedgraph`.

**Phase-C specific:**
1. `.chr` infix is **literal** (`.chr` + chromosome name); chromosome name rendered as a string in the filename (`from_utf8_lossy`; real names are ASCII).
2. Split-mode summary: every chr gets an empty summary file; the **last-processed** chr's gets the full summary (faithful Perl quirk, §3.2).
3. A zero-emitting chromosome still gets its report file (0-byte plain, or empty-gzip-stream `.gz`).
4. gz uses `flate2`'s default compression; only the **decompressed** bytes are contractually equal to Perl.
5. `--gzip` and `--split_by_chromosome` are orthogonal and compose; both are independent of `--CX`/`--zero_based`/`--coverage_threshold` (already handled in the kernel).

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | `ReportWriter` gz round-trip | unit: write via `Gz`, decompress | equals input; `Plain` equals input |
| V2 | filename derivation incl. **suffixed-`-o` doubling** | unit: bare `-o split` split → `split.chrchr1.CpG_report.txt`; suffixed `-o foo.CpG_report.txt` split → `foo.CpG_report.txt.chrchr1.CpG_report.txt`; non-split suffixed → `foo.CpG_report.txt` (stripped); `.gz`/`.CX` combos; summary never `.gz` | exact strings |
| V3 | `--gzip` report = plain (decompressed) | golden: run `--gzip`, gunzip the `.gz`, compare to `default.report.golden` | byte-identical |
| V4 | `--gzip` summary is plain + matches | golden: `gz.cytosine_context_summary.txt` vs `default.summary.golden`; assert NOT gzipped | identical + plain |
| V5 | `--gzip --CX` | decompress vs `cx.report.golden` | identical |
| V6 | `--split` file SET (**bidirectional**, threshold==0) | run `--split`; assert the output dir's file set EXACTLY equals Perl's — `{base}.chr{CHRNAME}.CpG_report.txt` + summary for **every** genome chr (incl. zero-emitting `scaf_short`), no spurious/missing files | matches Perl file set exactly |
| V7 | `--split` per-chr report bytes | each per-chr report vs its Perl golden | byte-identical each |
| V8 | `--split` summary quirk | assert all-but-last chr summary files are 0 bytes AND the last-processed chr's summary is **byte-identical to the non-split summary golden** (not merely non-empty) | matches Perl |
| V9 | `--split --gzip` combined | per-chr `.gz` reports decompress to per-chr goldens; per-chr summaries plain; last-chr summary full | byte-identical |
| V10 | `--split` covered/uncovered order preserved | the per-chr files exist for covered (cov order) + uncovered (sorted); content correct | per Perl |
| V11 | regression: Phase B unaffected | existing `golden_phase_b.rs` (default/cx/zero/thr) still pass | green |
| V12 | **split re-appearance truncates** (rev 1 B-C1) | `--split` with cov `chrA,chrB,chrA` → `…chrchrA.CpG_report.txt` holds ONLY the last segment (pos-2 coverage shows `0 0`); the full summary lands in **chrA** (last reopened), not chrB | matches live Perl |
| V13 | **suffixed-`-o` split golden** (rev 1 A-C1, extractor path) | `-o foo.CpG_report.txt --split` → per-chr files named `foo.CpG_report.txt.chrCHRNAME.CpG_report.txt`, bytes vs Perl golden | byte-identical incl. doubled-suffix names |
| V14 | **`--split --coverage_threshold N`** file set | uncovered chrs get **NO files** (no report, no summary); only covered chrs get files; summary in last covered chr | matches Perl (no uncovered pass) |

Goldens generated locally from the repo Perl v0.25.1 (extend `generate_goldens.sh`) — incl. a **suffixed-`-o`** split run (V13) and a **threshold>0** split run (V14).

## 10. Questions or ambiguities
| Priority | Question | Resolution |
|----------|----------|------------|
| **Resolved** | zero-emitting chr in `--split --gzip` — 0-byte file or empty-gzip stream? | **Valid ~20-byte empty-gzip stream** (`1f8b…`, decompresses to 0 bytes) — both reviewers verified via live Perl. So `GzEncoder::finish()` an unwritten encoder; do NOT short-circuit to a 0-byte file. (§3.2) |
| **Resolved** | last-processed chr (gets the summary) when there are no uncovered chrs? | **Last covered chr in cov-appearance order** — verified via live Perl for both threshold>0 and full-coverage. The `last_summary_path = most-recent flush` model holds. (§3.2) |

No **Critical** ambiguities remain — both rev-1 Criticals (raw-`-o` split filename; truncate-on-reopen) are folded, and the Perl behavior is fully observed.

## 11. Self-Review
- **Logic:** kernel/walk/ordering unchanged (Phase B byte-identity preserved — V11 regression guard). Only sink routing + filename + the summary quirk are new. gz `finish()` is explicit (no Drop-reliance).
- **Edge cases:** zero-emitting chr report file (V6); split summary quirk incl. the empty files (V8); `--gzip` summary stays plain (V4); combined mode (V9); the `.chrchr1` double-`chr` infix (V2).
- **Efficiency:** one FH per chr in split (bounded); gz streaming.
- **Integration:** `ReportWriter::create(path, gzip)` is the reusable seam for Phase D's merged-cov writer; `--merge_CpGs`+`--split`/`--CX` already rejected in Phase A.
**Folded from dual plan-review (rev 1, 2026-05-29 — both APPROVE-WITH-CHANGES; both verified against live Perl; gzip half confirmed correct):**
- **C1 (A): split filename uses RAW `-o`** (Perl appends `.chr` before the no-op strip → suffix doubled for suffixed `-o`, the extractor path). Added `ResolvedConfig.output_raw` (§3.5); split derivation uses it un-stripped (§3.2/§4). + V2/V13.
- **C1 (B): split truncate-on-reopen** — fresh `File::create` per transition, no per-chr-name caching; re-appearance keeps last segment; summary → last reopened chr (§3.2). + V12.
- **§10 Q1/Q2 resolved** (empty-gzip stream for zero-emit; last-covered-chr for no-uncovered).
- **A-I3:** `--split --threshold N` → uncovered chrs get NO files; V6 made threshold-aware + V14.
- **B:** V8 now diffs last-chr summary vs the **non-split** summary golden.
- **Intentional SPEC-wording deviations (both reviewers):** the PLAN diverges *correctly* from SPEC §5 (suffix shown after `.chr<NAME>` — real Perl puts `.chr` after the suffix-position, hence the doubling) and SPEC §10.5 (`BufWriter<GzEncoder<File>>` — the PLAN's `GzEncoder<BufWriter<File>>` is correct). Will sync the SPEC wording at next rev so it isn't reversed.

**Remaining risks:** the byte-identity goldens (incl. the suffixed-`-o` and threshold>0 split runs) are the ultimate check — generated at implementation time from the repo Perl.

## Revision history
- **rev 0** (2026-05-29): initial Phase C plan from EPIC + SPEC rev 3 + Phase-B writer seam + empirically-observed Perl `--gzip`/`--split_by_chromosome` behavior (incl. the `.chr` literal infix + the last-chr summary quirk).
- **rev 1** (2026-05-29): dual plan-review folded (both APPROVE-WITH-CHANGES; gzip half verified correct). 2 Criticals (A: raw-`-o` split filename → new `ResolvedConfig.output_raw`; B: split truncate-on-reopen) + Important (threshold>0 split file-set, V8 non-split-summary diff) + §10 Q1/Q2 resolved + 3 new test rows (V12–V14) + SPEC-deviation notes.
