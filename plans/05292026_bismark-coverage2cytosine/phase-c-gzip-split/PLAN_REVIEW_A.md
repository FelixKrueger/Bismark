# Phase C PLAN — Reviewer A

**Plan:** `phase-c-gzip-split/PLAN.md` (rev 0)
**Contract:** byte-identical to Perl `coverage2cytosine` v0.25.1; Phase C re-routes output bytes only.
**Verdict:** **APPROVE-WITH-CHANGES** — one **Critical** filename-derivation divergence (split mode + suffixed `-o`), plus validation gaps that would let it ship undetected. The gzip and the summary-quirk modelling are otherwise correct and confirmed against the live Perl.

I ran the repo's Perl v0.25.1 against the `phase_b` fixtures (and two purpose-built fixtures) to settle every claim. Findings below cite Perl line numbers and the empirical runs.

---

## 1. Logic review

### 1.1 The `.chr` literal infix — CORRECT for the fixtures, but the derivation model is WRONG (Critical)

The plan derives the split filename as `{output_stem}.chr{CHRNAME}.{CpG|CX}_report.txt` (§3.2, §4), where `output_stem` is the **already-suffix-stripped** value Phase A stores in `ResolvedConfig.output_stem` (`cli.rs:195-198`, stripped unconditionally regardless of split).

Perl does the opposite order (`handle_filehandles:99-112`):
1. `:101` append the infix to the **raw** `-o` value first: `$cytosine_coverage_file =~ s/$/.chr${my_chr}/`.
2. `:108/:111` THEN strip the report suffix anchored at `$`: `s/\.CpG_report.txt$//`.

Because step 1 pushes any original suffix away from the `$` anchor, **in split mode the strip never fires when `-o` carried a `.CpG_report.txt`/`.CX_report.txt` suffix** — the suffix is *preserved* and a fresh one re-added. Confirmed by running Perl:

- `-o foo.CpG_report.txt --split_by_chromosome` (Test F) → `foo.CpG_report.txt.chrchr1.CpG_report.txt` and summary `foo.CpG_report.txt.chrchr1.cytosine_context_summary.txt` (suffix doubled).
- Same `-o` **without** split (Test G) → `foo.CpG_report.txt` (stripped, as Phase B already does).
- `-o bar.CX_report.txt --split --CX` (Test H) → `bar.CX_report.txt.chrchr1.CX_report.txt`.

The plan's model (using the pre-stripped stem) would emit `foo.chrchr1.CpG_report.txt` — a **different filename** from Perl. This is a byte-identity break on the file-NAME (and thus the file-SET assertion V6) for exactly the **methylation-extractor invocation path**: the extractor hands c2c an `-o` that already ends in `.CpG_report.txt`/`.CX_report.txt` (that is literally why the Perl strip exists — Perl comment `:106` "if the data came from the methylation extractor it will already end in …"). `ResolvedConfig` no longer retains the raw `-o` (`cli.rs:104-132`), so Phase C cannot reconstruct this without a change.

For manual `-o`-without-suffix usage (the current fixtures), there is **no divergence**, which is why nothing here is currently red — and why the bug would ship silently.

**Fix options** (any one):
- (a) Store the raw `-o` in `ResolvedConfig` and, in split mode, derive `report_path` as Perl does: `raw_o + ".chr{NAME}"`, then strip-suffix-anchored-at-end, then re-append the report suffix.
- (b) In `ResolvedConfig::validate`, additionally record `output_stem_split` = (raw `-o` with `.chr` placeholder logic) — uglier.
- (c) Simplest faithful model: keep `output_stem` for non-split; for split, compute the per-chr base = `apply_perl_infix_then_strip(raw_o, name, cx)`. Add a Phase-C unit test for the suffixed-`-o` × split × {CpG,CX} matrix.

This is the only Critical item; everything else is correct.

### 1.2 The split-mode context-summary quirk — CORRECT (confirmed)

Verified against Perl (`print_context_summary:49,63-78` runs once after `generate_genome_wide_cytosine_report` returns; `CONTEXTSUMMARY` is reopened/truncated per chr at `handle_filehandles:117`, from the covered call sites `:217`/`:465` and the uncovered call site `process_unprocessed_chromosomes:1396`). The last reopen wins; the full summary lands there. Empirically (Test A, fixture covers chr1,chr2; uncovered sorted chr3uncov,scaf_short):

- All summary files are **0 bytes** except the **last-processed** chromosome (`scaf_short`, last in {covered cov-order, then uncovered bytewise-sorted}), which holds the full 1310-byte 64-row summary.
- The split last-chr summary is **byte-identical** to the non-split full summary (`cmp` clean; 65 lines). The plan's "accumulated across the whole genome, unchanged from Phase B, written full only to the LAST chr" is exactly right.

Both §10 open questions are now **resolved by running Perl** (so they should be downgraded from Open to Resolved in the plan):

- **No-uncovered case (threshold>0)** (Test B): uncovered chrs get **no files at all** (no report, no summary) — only the covered chrs (chr1,chr2). The full summary lands on the **last covered chr in cov order** = `chr2`. Plan's assumption CORRECT.
- **Fully-covered case** (Test C, cov touches all four chrs in order chr1,chr2,scaf_short,chr3uncov): full summary lands on **chr3uncov** = last in cov-appearance order (NOT scaf_short, which is earlier in the cov). Plan's "last covered chr (cov-appearance order)" CORRECT.

The plan's `last_summary_path` model holds in all three regimes (uncovered present; threshold>0 no-uncovered; full-coverage no-uncovered). Note for the implementer: "last-processed" must track the **same iteration order the report walk uses** (covered cov-order, then — only if `threshold==0` — uncovered bytewise-sorted). Do not derive it independently, or it can desync from the report pass.

### 1.3 gzip — CORRECT (confirmed)

- **Non-split `--gzip`** (Test D): the Perl `gunzip`-decompressed report is byte-identical to the plain `default.report.golden`; the summary is plain ASCII and byte-identical to `default.summary.golden`. The plan's decompress-then-compare (V3) and never-gzip-summary (V4) are sound.
- **Summary never gzipped** even under `--gzip` (Tests A, D): confirmed (`file` reports ASCII text). Perl never wraps `CONTEXTSUMMARY` in the gzip pipe (`:117` is a plain `open '>'`; only `CYT`/`CYTCOV` get the `| gzip -c` pipe at `:139-150`). CORRECT.
- **Zero-emitting chr under `--split --gzip`** (Test A, §10 open question 1): Perl emits a **valid 20-byte empty-gzip stream** (decompresses to 0 bytes), NOT a 0-byte file, because `gzip -c` writes a header+trailer even for empty input. The plan's assumption is CORRECT — downgrade to Resolved. **Implementer note:** `flate2::write::GzEncoder::finish()` on a writer that received zero bytes likewise emits a valid empty-gzip stream, so a zero-emit chr must still go through `ReportWriter::Gz` + `finish()` (do not short-circuit to a 0-byte file).
- **Explicit `finish()`** is correct and matches the established extractor precedent (`bismark-extractor/src/output.rs:239,253,301,365` explicitly warns the gzip trailer must be emitted at close, NOT via `flush`). V1's round-trip test guards a forgotten trailer. CORRECT.

### 1.4 Kernel/walk left byte-identical — CONFIRMED

The plan touches only sink routing (`open_report_writer` → `ReportWriter` enum; per-chr multiplexing; `summary_path(chr)`). `emit_position`, `extract`, `perl_substr`, `revcomp`, `classify_context`, `flush_chromosome`'s walk, the cov streaming, the covered/uncovered ordering, and `ContextSummary` accumulation are all unchanged (report.rs:92-305). The split per-chr report bytes equal the corresponding chr's slice of the non-split golden (Test E: `sp.chrchr1.CpG_report.txt` == `grep '^chr1' default.report.golden`), confirming the walk is reused verbatim. V11 regression-guards Phase B against the existing `golden_phase_b.rs`. CORRECT.

---

## 2. Assumptions

- §8 / §3.2 assumption 2 ("last-processed chr gets the full summary") — **validated** against Perl in all three regimes (§1.2). Good.
- §8 assumption 3 ("zero-emitting chr still gets its report file") — **validated** (0-byte plain; 20-byte empty-gzip under `--gzip`). Good.
- §8 assumption 1 (".chr infix is literal `.chr`+name") — **validated** as far as the infix string itself (`chrchr1`), but the assumption silently presumes the infix is applied to the **stripped stem**, which is the Critical §1.1 error. The assumption text should be corrected to "applied to the raw `-o` value, then strip-anchored-at-end, then suffix re-added" — i.e. it interacts with Phase A's strip.
- §8 assumption that `output_stem` is a sufficient base for split filenames — **false** for suffixed `-o` (§1.1).
- The gzip-container-not-asserted / decompress-compare assumption — **validated**; note the Perl gz header carries a non-zero mtime + `from Unix` OS byte (Test A `file` output), which is exactly why container compare is unsafe. flate2 default sets mtime=0 / no FNAME — both differ from Perl's container but agree after decompression. Correctly handled.

---

## 3. Efficiency

No concerns. One file handle per chromosome in split mode is bounded (§6); the O(genome) walk is unchanged. gz is streaming. Note a minor structural deviation: the plan nests `GzEncoder<BufWriter<File>>` (§4), whereas SPEC §10.5 and the extractor precedent use `BufWriter<GzEncoder<File>>` (`output.rs:389-397`). Both yield identical decompressed bytes; the extractor's nesting buffers raw bytes before compression (slightly better — fewer, larger compressor calls), the plan's buffers compressed output. Functionally fine for byte-identity; flagged for consistency (§Important-2).

---

## 4. Validation sufficiency

V1–V11 cover the happy paths well (gz round-trip, decompress-compare, the file SET, the summary quirk, the combined mode, Phase-B regression). **Gaps:**

- **G1 (Critical-linked):** No test exercises a **suffixed `-o` (`foo.CpG_report.txt`) in split mode**, so the §1.1 filename divergence is invisible. `generate_goldens.sh` uses `-o "$mode"` (no suffix); the planned Phase-C goldens inherit this. Add a golden case: `-o foo.CpG_report.txt --split_by_chromosome` (and the `--CX` analogue) and assert the file SET includes the doubled suffix (`foo.CpG_report.txt.chrchr1.CpG_report.txt`). This is the single test that protects the extractor invocation path.
- **G2:** V8 ("only the LAST-processed chr's summary is non-empty") should be split into the three regimes I verified: (a) uncovered present → last is the last uncovered (bytewise-sorted); (b) threshold>0 no-uncovered → last is the last **covered** chr AND uncovered chrs produce **no files at all**; (c) full-coverage no-uncovered → last is the last covered (cov-order). The plan's §10 question 2 is exactly regime (b)/(c); fold it into V8 as explicit sub-assertions rather than leaving it "Open".
- **G3:** V6's file-SET wording ("for every genome chr incl. zero-emitting scaf_short") is **only true for `threshold==0`**. Under `--split --coverage_threshold N` the uncovered chrs are entirely absent (Test B) — the file set is just the covered chrs. Add a threshold>0 split file-SET assertion (no extra/missing files), and tighten V6's wording.
- **G4:** No explicit assertion that the per-chr **summary filename** also carries the `.chr` infix (it does — `split.chrchr1.cytosine_context_summary.txt`). V2 lists the report-name combos but should add the split summary name, especially under the §1.1 fix (suffixed-`-o` split summary = `foo.CpG_report.txt.chrchr1.cytosine_context_summary.txt`).
- **G5 (minor):** V9 should assert the zero-emit chr's `.gz` decompresses to **0 bytes** (a valid empty-gzip stream), not merely "decompresses to per-chr golden" — to pin the §10-Q1 behavior against a future short-circuit-to-0-byte regression.

With G1 + G2 + G3 added, the validation set is sufficient. The colossal real-data gate (SPEC §12.3) will also catch G1 if the matrix includes a suffixed `-o`, but the gate should not be the first line of defense for a deterministic local quirk.

---

## 5. Alternatives

- **Filename derivation (the §1.1 fix):** the cleanest faithful port is a single helper `split_report_base(raw_o, chr, cx) -> String` that literally mirrors Perl: append `.chr{name}`, then `strip_suffix`-at-end, then the caller adds the report suffix (+`.gz`). This keeps the Perl `:99-112` ordering visible in one place and is unit-testable in isolation against the Test F/G/H bytes I captured. Preferred over threading two stems through `ResolvedConfig`.
- **Summary-quirk implementation:** rather than "create empty file per chr + record last_summary_path + write full at end", a simpler model that is provably equivalent: each per-chr step `File::create`s (truncates) its summary path (leaving it empty) and the run records the most-recent path; after the loop, write the full summary to that path. This is exactly the plan's §5 step 4 — fine. Just ensure the "record last" happens in the **same** loop as the report emission so the order can't drift (§1.2 note).

---

## 6. Action items

### Critical
- **C1.** Fix the split-mode filename derivation to match Perl's append-then-strip-anchored ordering (`handle_filehandles:99-112`). The plan's pre-stripped `output_stem` base produces `foo.chrchr1.CpG_report.txt` where Perl produces `foo.CpG_report.txt.chrchr1.CpG_report.txt` for a suffixed `-o` — the extractor's real invocation path. Requires retaining the raw `-o` (or an equivalent split base) in `ResolvedConfig`. Applies to the report **and** the per-chr summary filename. (Plan §3.2/§4/§8-assumption-1; Perl `:101,:108,:111,:106`; empirical Tests F/G/H.)

### Important
- **I1.** Add a Phase-C golden/unit case for **suffixed `-o` × split × {CpG, CX}** (the test that would have caught C1). Extend `generate_goldens.sh` with one suffixed-`-o` split run and assert the doubled-suffix file SET. (Plan §9 V2/V6; closes validation gap G1.)
- **I2.** Resolve the two §10 "Open" questions in the plan to **Resolved** with the empirical answers: (Q1) zero-emit chr under `--split --gzip` = valid 20-byte empty-gzip stream (decompresses to 0 bytes), confirmed; (Q2) no-uncovered → full summary on the last **covered** chr in cov order, confirmed (threshold>0 AND full-coverage). Fold Q2 into V8 as explicit sub-assertions. (Plan §10; Tests A/B/C.)
- **I3.** Tighten V6 to note the file SET is "every genome chr" **only when `threshold==0`**; add a `--split --coverage_threshold N` file-SET assertion (uncovered chrs produce NO files — no report, no empty summary). (Plan §9 V6; Test B; closes G3.)

### Optional
- **O1.** Align the gz nesting with SPEC §10.5 / the extractor precedent (`BufWriter<GzEncoder<File>>`) rather than `GzEncoder<BufWriter<File>>`, for consistency; both are byte-identical after decompression. (Plan §4; `bismark-extractor/src/output.rs:389-397`.)
- **O2.** Add the per-chr **summary** filename to the V2 derivation matrix (it carries the `.chr` infix too), and add G5's "empty-gzip decompresses to 0 bytes" assertion to V9.
- **O3.** State in §1.2/§5 that "last-processed" MUST be tracked within the same loop as the report emission (covered cov-order, then uncovered sorted only if `threshold==0`) so the summary target cannot desync from the report pass.

---

## 7. Bottom line

The plan correctly characterizes the gzip behavior, the never-gzip-summary rule, the empty-gzip-stream-for-zero-emit-chr, and the last-chr summary quirk in all three coverage regimes — I verified every one against live Perl. It correctly leaves the kernel/walk untouched. The **one Critical defect** is the split-mode filename derivation: applying the `.chr` infix to Phase A's already-stripped `output_stem` diverges from Perl's append-then-strip-at-end ordering whenever `-o` carries a report suffix — i.e. the methylation-extractor path. The current fixtures (suffix-free `-o`) mask it. Add C1's fix + I1's test and the plan is byte-identical-sound and ready to implement.
