# Plan review — Phase C.2 (Reviewer A)

**Plan under review:** `plans/05262026_bismark-extractor/PHASE_C2_PLAN.md` (rev 0)
**Scope:** #864 (splitting-report format), #865 (empty-file deletion), #863 (won't-fix + SPEC rewrite)
**Reviewer:** A (independent dual-review pass)
**Verdict:** **Needs revision** — three Critical byte-identity findings and one Critical SPEC-target error must be fixed before implementation; otherwise the plan will produce a non-byte-identical splitting report and will edit the wrong SPEC section.

---

## 1. Logic Review

### 1.1 Splitting-report byte-shape — three discrete defects (Critical)

I read Perl `bismark_methylation_extractor:2476-2559` and `:4985-5048` side-by-side with the plan's §3.1 26-step write order. Three byte-level defects are baked into the plan as written:

#### CRIT-1 — Trailing-newline over-count at EOF

Perl `:2553` writes the CHH percentage line as `"C methylated in CHH context:\t${percent_meCHH}%\n\n\n"` — the CHH (i.e. the last line in 3-context output) line itself **carries the three trailing newlines**. The plan's §3.1 step 24 emits each percentage line with a single `\n` (`C methylated in {ctx} context:\t{pct:.1}%\n`) and then **step 25 adds an additional `\n\n\n`**. Net result: CHH gets `\n + \n\n\n = \n\n\n\n` = **four newlines at EOF**. Perl emits three. Plan will fail raw `cmp` on the splitting report — which is the *strict* gate per the plan's own §3.4.2 case-block.

Fix: either (a) write the first two percentage lines with `\n` and the last with `\n\n\n` (position-aware), or (b) emit all three with `\n` and a closing `\n\n` (not `\n\n\n`). The same position-aware fix applies to the zero-denominator fallback path (`:2556` — Perl line 2556 also ends in `\n\n\n` for the CHH fallback). The plan's step 24 zero-denominator branch is position-blind.

#### CRIT-2 — Missing blank line between header and body

Perl's header block closes with `print REPORT "\n";` (line 5047) and the body block opens with `print REPORT "\nProcessed..."` (line 2482). The combined effect after the last header line (e.g. `Methylation in CHG and CHH context will be merged...\n`) is: `\n` (line content) + `\n` (5047) + `\n` (2482 leading) = **three newlines = two blank lines** before "Processed".

The plan's reference output at §2.4 lines 161-164 correctly shows this (two visible blank lines between `No overlapping methylation calls specified` and `Processed 7699136 lines in total`), but the plan's **step 12** says "Write blank line `\n`" — singular. That emits only one newline between the last header line and `Processed`, giving one blank line, not two. Off-by-one of 1 byte → raw `cmp` fails.

Fix: step 12 should emit `\n\n` (or be split into "blank line then leading newline of body").

#### CRIT-3 — `methylation_call_strings_processed` line ends with `\n\n`, not `\n`

Perl `:2483`: `print REPORT "Total number of methylation call strings processed: $counting{methylation_call_strings}\n\n";`. The Perl line has its own trailing blank line baked in.

Plan §3.1 step 14 emits `...\n` and step 15 emits a blank line `\n`. Combined `\n\n` = matches Perl. **This one is OK** — but the doc comment in step 14 reads as if only one `\n` is emitted; this is just bookkeeping clarity. Not a defect.

#### Net effect of CRIT-1 + CRIT-2

Even if everything else in steps 1-26 is perfect, the plan as written produces a splitting report that is **2 bytes longer than Perl's** (one extra `\n` at step 25, one missing `\n` at step 12 — wait, CRIT-2 says missing, CRIT-1 says extra, so net might be zero bytes but two byte *positions* differ). Either way, raw `cmp` fails. Per the plan's own §3.4.2 case-block, splitting-report mismatch → harness fails. The whole purpose of #864 closing is defeated.

### 1.2 Empty-file sweep — stdout vs stderr (Critical)

Plan §3.3 + §5.3 step 3 + §11 self-review say `println!` (stdout). Plan §10 open-question table justifies this with "Matches Perl exactly".

Perl source contradicts this. Lines 607 and 615:
```perl
warn "$sorting_files[$index] contains data ->\tkept\n";
warn "$sorting_files[$index] was empty ->\tdeleted\n";
```

`warn` writes to **STDERR**, not STDOUT. The plan's "verified from the user's earlier `--parallel 8` direct run output where lines appeared in captured shell output" rationale is invalid — captured shell output without `2>/dev/null` or `>` redirection catches both streams. The actual Perl stream is stderr.

Fix: change `println!` → `eprintln!` for both log lines. Update §3.3, §5.3 step 3, §10 open-questions, §11 self-review. This is a one-line code change but a clearly-marked plan defect because the rationale is explicitly stated and wrong.

Also note: Perl line 625 emits `warn "\n\n";` at the end of the sweep — two trailing blank lines on stderr. Plan does not mention this. Whether to match this exact stderr-formatting is a downstream-tooling-compatibility question, but for completeness the plan should either match or explicitly omit-with-rationale.

### 1.3 SPEC §9.7 target is the wrong section (Critical)

The plan §3.4.1 says "Current SPEC §9.7 (presumed): 'Rust output must be byte-identical to Perl Bismark v0.25.1's output for every supported flag combination on the 10M PE WGBS test dataset.'" and proposes to rewrite §9.7 with the 6-point invariant.

Actual SPEC §9.7 (file `rust/bismark-extractor/SPEC.md`, lines 725-727) is titled **"Speedup expectation"** and discusses the ≥4× speedup target at N=4. It has *nothing* to do with byte-identity.

The actual byte-identity invariant lives at:
- **§8.3** (lines 653-665): "Real-data byte-identity gate (10M + 55M PE WGBS)" — the harness-level invariant.
- **§9** intro (line 690): "`--multicore N` MUST produce output byte-identical to `--multicore 1` for any N ≥ 1" — the parallelism invariant.
- **§9.4** (lines 713-715): "Output ordering" — defines BTreeMap-input-order semantics.

The plan's 6-point invariant text correctly captures the #863 won't-fix decision, but it should be inserted as a new **§8.3 sub-clause** (or §9.4 sub-clause), not by overwriting §9.7. Rewriting §9.7 would delete the speedup target — orthogonal to the byte-identity question and still wanted as a separate invariant.

Fix: re-target the SPEC edit. Likely the cleanest split is: add the 6-point invariant as new section **§8.3.1** ("Byte-identity invariant (rev 3, post-#863)") and leave §9.7 intact. Or add as a new §9.8 immediately after speedup. The plan needs to pick a real section.

### 1.4 `paired-end (SAM format)` literal — verified correct

I verified Perl `:5000` — the literal is hard-coded as `"Bismark result file: paired-end (SAM format)\n"` regardless of whether the input file is `.bam` or `.cram`. Plan §A4 / §R1 correctly identify this; risk is genuinely low. No action.

### 1.5 33-char `=` separator — verified correct

Perl `:2510` is `print REPORT "Final Cytosine Methylation Report\n",'='x33,"\n";` — exactly 33 `=` chars, then `\n`. Plan §3.1 step 17 specifies 33 `=` chars then `\n`. Correct.

### 1.6 Conditional `Ignoring …` lines — verified correct

Perl `:5006-5028` confirms the SE-branch (`first $ignore bp`, `last $ignore_3prime bp`) and PE-branch (`first … bp of Read 1/Read 2`, `last … bp of Read 1/Read 2`) variants. Plan §2.3.2 + §3.1 step 7 enumerate these correctly. Default (all four zero) emits no `Ignoring` line — correct.

### 1.7 `records_processed` semantics — current Rust audit-during-impl is necessary AND the fix is in scope

I confirmed in `src/pipeline.rs`:
- Line 163: SE path increments by 1 per record (correct: matches Perl `sequences_count`).
- Line 254: PE path increments by 2 per pair, with the doc comment at lines 188-191 explicitly saying "Increments by **2 per pair** to match Perl line 2451" — but **Perl :2451 increments `methylation_call_strings_processed`, NOT `sequences_count`**. Perl `sequences_count` is set from `$line_count` at `:2459` and `$line_count` is the outer-loop counter incremented once per pair (PE) or per record (SE).

So Phase B's PE increment-by-2 is using the wrong Perl counter as its model. The plan's §3.2 correctly flags this needs to flip to per-pair AND a new `call_strings_processed += 2` per pair must be added. Plan is correct on substance; just be aware the existing doc comment in pipeline.rs is misleading and will need updating too.

### 1.8 The "Final Cytosine Methylation Report" stderr mirror (Optional but worth noting)

Perl `:2562-2580` mirrors the report content via `warn` (i.e. stderr) in addition to writing to REPORT. The plan only addresses the file content. The Rust implementation currently does **not** mirror to stderr — neither the existing Phase B code nor the plan adds this. This is downstream-tooling-compat-irrelevant (no nf-core pipelines parse `warn` output), so leaving it out is defensible. Worth a one-line note in the plan ("Rust does not mirror the report to stderr; this is deliberate — `warn` mirror is purely user-readable progress in Perl, not consumed by tooling") to make the omission deliberate, not accidental.

### 1.9 Plan's reference to `src/run.rs` (Important — file name drift)

§2.2's table cites `src/run.rs` + `src/parallel.rs` for the sweep wire-up. The current crate has `src/pipeline.rs` (not `run.rs`) and `src/state.rs::ExtractState::finalize`. The wire-up actually goes in `state.rs::finalize` (lines 111-114). `pipeline.rs` is the SE/PE record-loop, not the finalize path. Plan should reference the correct file names so implementation doesn't get diverted.

### 1.10 Phase F parallel collector — `SplittingReport::add` needs the new field

Already flagged in plan §11 R5. Confirmed: `src/output.rs::SplittingReport::add` at lines 284-295 currently sums 8 fields. The new `call_strings_processed` field MUST be added or per-worker sums will silently drop. Plan correctly addresses this in §5.2 step 1. ✓

---

## 2. Assumptions

### A1 — `config.paired_mode` accessibility (Optional)

Plan §A1 flags this as "verify during impl". I checked `src/output.rs:306-386`'s existing signature accepts `&ResolvedConfig` — the field should be reachable. Low risk.

### A5 — Banker's-rounding hazard (Important)

The plan correctly identifies that `format!("{:.1}", ...)` in Rust uses round-half-to-even and Perl `sprintf("%.1f", ...)` uses round-half-away-from-zero. The plan's mitigation (a 50/50 split fixture) is **not sufficient** — 50.0 has no half-digit to round.

The actual disambiguators:
- 0.05 → Rust: `0.0` (round-half-to-even rounds 0.05 down because 0 is even); Perl: `0.1` (always rounds half away from zero).
- 0.15 → Rust: `0.2` (next-even); Perl: `0.2` — same.
- 0.25 → Rust: `0.2` (rounds down to even); Perl: `0.3` — DIFFERENT.
- 0.35 → Rust: `0.4`; Perl: `0.4` — same.
- 0.45 → Rust: `0.4` (rounds down to even); Perl: `0.5` — DIFFERENT.

So the bias is on quarter-percent values, not half-percent. Plan needs a fixture that produces 25.5% / 50.5% / 75.5% (these will round 26/51/76 in Perl but 26/50/76 in Rust depending on which is even). Add a test for an explicit synthetic case where Perl rounds 0.25 → "0.3" and Rust rounds 0.25 → "0.2". If this is observed in real-data (uncommon but possible with small denominators), a custom formatter is needed.

Note: floating-point representation also matters. 100.0 * 17 / 67 ≈ 25.3731... not 25.25. The bias only bites on exact-quarter values, which require specific numerator/denominator pairs — unlikely on the 10M PE dataset but **possible**. Plan should add at least one fixture verifying round-half-away-from-zero behaviour.

### A6 — `methylation_call_strings == 2 × sequences_count` for PE (Verified correct)

Perl `:2451` unconditionally `+= 2` per pair, and `sequences_count` is `$line_count` which is incremented once per pair at the outer-loop level. So `call_strings = 2 × sequences_count` exactly. Plan assumption is right.

### A7 — 33 `=` chars, no trailing space (Verified correct)

See §1.5 above.

### A8 — `records_processed` semantics (Verified correct in plan's framing)

See §1.7 above.

### Implicit (unflagged) assumption — `flush_all` propagates `GzEncoder` trailer (Important)

The plan §3.3 / §4.2 says "drop the writer (closes the file + flushes gzip trailer if applicable)". The current `OutputFileMap::flush_all` at `output.rs:189-194` only calls `writer.flush()`, which on `BufWriter<GzEncoder<File>>` propagates to GzEncoder's flush but **does not** write the gzip trailer (the trailer is written by GzEncoder::drop). The `finalize_with_empty_sweep`'s explicit `drop(writer)` per §3.3 will correctly trigger the trailer-write.

But the plan implies the sweep runs only on the empty subset; for the non-empty subset (kept files), the timing of when `GzEncoder::drop` runs is unclear from the plan. Looking at the plan's pseudo-code in §3.3 (the `drain()` + `for entry` loop), every entry — empty or kept — is drained out of the map and the writer is dropped. So the kept files get their gzip trailer written by the sweep call, not by `flush_all`. This is actually **correct** for gzip mode but the plan should explicitly note: after `finalize_with_empty_sweep`, the `OutputFileMap` is empty; the subsequent `flush_all` (if any) would be a no-op. Confirm the wiring leaves no `flush_all` call AFTER the sweep.

(Re-reading §5.3 step 4: "Wire the sweep into `ExtractState::finalize` so it runs AFTER `flush_all` and BEFORE `write_splitting_report`". So flush_all happens BEFORE sweep — good. But that means flush_all only does the buffered-flush, not the gz trailer; the gz trailer is written by drop-in-sweep. For *.gz files this means the file on disk is incomplete between flush_all and sweep — irrelevant in practice because the process is in the same thread, but worth noting as a non-obvious sequencing detail.)

---

## 3. Efficiency Analysis

The plan's §6 table is accurate. Specific notes:

- **`records_written` counter**: u64 increment per `write_call`. This is in the hot path for every methylation call. Branch prediction handles it trivially. ✓ No regression.
- **`finalize_with_empty_sweep`**: O(N_files) with N ≤ 12. Trivial. ✓
- **Harness sorted-MD5 fallback**: 4 GB → 30 s/file → 6 files = ~3 min added to harness. For a CI-grade per-PR gate this is significant on top of the existing 60+ s smoke run. Consider using `LC_ALL=C sort -S 1G` or even `sort -u | md5sum` (if uniqueness is guaranteed per row). Or memoize `sort` output to a temp file so the comparison + the `wc -l` line-count check share work. Optional optimization, not blocking.
- **Per-call `records_written` bump timing (R4)**: The plan §5.3 step 2 says "bump after the successful `writer.write_all(b"\n")?`". This is correct — but the current `write_call` body (lines 165-176) has 7 sequential `write_all` calls (8 if yacht) with `?` early-returns on each. The plan's "bump after the final `\n`" is the only safe place; intermediate failure between any field-write means the row is corrupt anyway, so the counter staying 0 is fine (the file is partial, the row didn't fully land). ✓

---

## 4. Validation Sufficiency

### 4.1 Coverage gaps in §5.5 (Important)

The 17 enumerated tests are well-distributed across header conditional branches, but I see four gaps:

#### Gap V1 — `--mbias_only` + splitting-report content (Important)

Plan §3.5 says "`--mbias_only` mode: Splitting report still emitted (with zero call counts if all calls were silenced by `--mbias_only_silence`)". No test in §5.5 explicitly exercises the `--mbias_only` × splitting-report combination. The risk is that the zero-call-counts path triggers all three zero-denominator fallback branches, which is the most fragile area of the new format code.

Suggested test: `splitting_report_format_mbias_only_all_zero` — all three percentages take the fallback line.

#### Gap V2 — gzip mode × sweep × splitting-report ordering (Important)

Plan §5.5 has `output_file_map_empty_sweep_gzip_empty_is_deleted` but no test that exercises the full sequence: gzip mode → flush_all → sweep (which writes gz trailer to kept files) → write_splitting_report. The concern is that for kept gzip files, the disk content needs to include the gz trailer before any downstream tool reads them. An integration test that reads back a gzip-mode kept file post-`finalize` and verifies the gunzip succeeds would close this gap.

#### Gap V3 — Phase F parallel-vs-sequential `call_strings_processed` parity (Important)

Plan §11 R5 flags this risk but §5.5 has no test for it. The existing `parallel_phase_f.rs` test (assumed to live in `tests/`) checks parallel-vs-sequential parity on existing fields; the new field needs the same coverage. Otherwise a regression on `SplittingReport::add` would land silently.

Suggested test: an extension to the existing parallel parity test that asserts `call_strings_processed` matches across N=1, N=2, N=4 runs.

#### Gap V4 — Round-half-away-from-zero behaviour (Important)

Per §A5 above. The 50/50 fixture in §5.5.1 won't disambiguate banker's vs round-half-away. A test with a precisely-constructed numerator/denominator pair producing 0.25 or 0.45 is needed (or accept that the production fix uses a custom formatter mirroring Perl's `sprintf` behaviour).

### 4.2 Integration-test coverage (Acceptable)

The 4 integration tests in §5.5.3 are reasonable:
1. SE Perl-compliant report ✓
2. PE Perl-compliant report ✓
3. PE empty-file deletion (12 → 6 files) ✓
4. Stdout (should be stderr per CRIT-2 above) log-line capture ✓

After fixing CRIT-2 (stderr), test 4 becomes a stderr capture; structural change is trivial.

### 4.3 Validation runs (§5.8) — sufficient

The 5 pre-merge gates are appropriate. The re-run of `oxy_phase_h_smoke.sh` on the 10M PE BAM is the definitive test. If CRIT-1, CRIT-2, CRIT-3 are fixed, the harness should report PASS.

### 4.4 Missing: regression-suite verification on existing tests (Important)

The plan §5.5 adds ~17 new tests but doesn't enumerate the existing tests that might break due to the SplittingReport format change. Any test currently asserting "expect `Bismark methylation extractor version v0.25.1` as line 1" or "expect `--ignore: 0` in the report" will fail. Plan should list which existing test files need updating (and which assertions specifically), so reviewers can check the implementation doesn't silently delete assertions.

Candidates I'd expect to touch:
- Existing `tests/output_*.rs` or `tests/integration_*.rs` files asserting on splitting report content
- `src/output.rs` inline tests using `write_splitting_report` with a fixture (likely exists for Phase B's report)

A pre-impl audit ("grep for `Bismark methylation extractor` and `--ignore:` across `tests/` and `src/output.rs` to identify assertions needing rewrite") would tighten the plan.

---

## 5. Alternatives Considered

### 5.1 Position-aware percentage writer vs sentinel-based

The plan's §5.2 step 4 offers two options for the zero-denominator branch:
1. `percent_meth` returns `Option<f64>`
2. Add a `write_percent_line(w, ctx_name, meth, unmeth)` helper

Both work. Option 2 is better because it encapsulates the position-aware trailing-newline logic (CRIT-1 fix): the helper can take an `is_last_context: bool` argument and choose between `\n` and `\n\n\n`. This makes the per-context branch self-contained and testable. Option 1 forces the caller to handle position-awareness outside the helper.

Recommendation: use a `write_percent_line(w, ctx_name, meth, unmeth, is_last) -> io::Result<()>` helper, called three times with `is_last=false, false, true` (3-context) or twice with `false, true` (merge_non_CpG 2-context).

### 5.2 Sweep via existing `cleanup_all` extension vs new method

Plan adds a new `finalize_with_empty_sweep`. The existing `cleanup_all` (lines 205-224) drains the map and removes ALL files. The semantics differ enough that a separate method is correct — `cleanup_all` is for error paths (delete everything), `finalize_with_empty_sweep` is for the success path (delete only empties). ✓

### 5.3 Harness sorted-MD5 caching

For the 4 GB data files, `LC_ALL=C sort` reads + writes the full file. If both Perl and Rust outputs need sorting, that's 2 × 4 GB → ~60 s. Caching the sort to a tempfile and reusing it across `md5sum` + a future `wc -l` invocation would halve the cost. Not blocking.

### 5.4 Atomic empty-file delete (Optional)

The current plan uses `std::fs::remove_file(&path)` after `drop(writer)`. On POSIX this is fine, but if a downstream process is reading the file concurrently (unlikely in this single-process flow), the unlink would race. The plan's note that `drop(writer)` is required for Windows is correct; for POSIX-only Bismark this is moot. Optional defensive note in the plan.

---

## 6. Self-Determinism Claim in §9.7 / §3.4.1

The plan claims Rust's "self-determinism" (same input → same output regardless of `--parallel N`) is a *stronger* invariant than byte-identity to Perl. This is correct for one direction (Rust's invariant doesn't depend on N; Perl's does), but **weaker** in another direction: it doesn't guarantee correctness against Perl's data content, only against Rust's own past runs. Sorted-content MD5 equivalence on data files closes the correctness gap (data content matches Perl, just ordering differs).

So the combined invariant ("sorted-content equality with Perl AND self-determinism") is genuinely both stronger and broader than the raw cmp check. The plan's framing is defensible. The 6-point invariant text is well-written and worth keeping intact when re-targeted to the correct SPEC section per CRIT-3.

---

## 7. Action Items (prioritised)

### Critical (must fix before implementation)

- **C1** — Fix splitting-report trailing-newline byte-shape (§3.1 step 24/25). Make the percentage-trio writer position-aware so the last context gets `\n\n\n` and the others get `\n`. Apply the same fix to the zero-denominator fallback. (Reviewer §1.1 CRIT-1)
- **C2** — Fix the blank-line gap between header and body in §3.1 step 12. Emit `\n\n` after the last header line (or split into "header trailing `\n`" + "body leading `\n`"). (Reviewer §1.1 CRIT-2)
- **C3** — Change `println!` → `eprintln!` for the sweep log lines in §3.3, §5.3 step 3, §10 open-questions, §11 self-review. Perl uses `warn` (stderr), not `print` (stdout). Update §5.5 test 4 and §5.5.3 test 4 to capture stderr accordingly. (Reviewer §1.2)
- **C4** — Re-target the SPEC edit. §9.7 is "Speedup expectation", not byte-identity. The 6-point invariant must be inserted as a new section (suggest §8.3.1 or new §9.8). Update plan §3.4.1 + §5.1 to reference the correct section. (Reviewer §1.3)

### Important (worth fixing before implementation, but unblocking is possible without)

- **I1** — Add a position-aware `write_percent_line` helper (Reviewer §5.1) — this cleanly resolves CRIT-1.
- **I2** — Update plan file-name references from `src/run.rs` to `src/pipeline.rs` / `src/state.rs` (Reviewer §1.9).
- **I3** — Strengthen the banker's-rounding test fixture: add a 0.25-rounding or 0.45-rounding case, not just 50/50. Or commit to a custom round-half-away-from-zero formatter and test it directly. (Reviewer §A5)
- **I4** — Add the four missing tests per §4.1 gaps V1-V4: mbias_only × splitting-report; gzip × sweep × kept-file-reads-cleanly; Phase F parity for `call_strings_processed`; quarter-percent rounding. (Reviewer §4.1)
- **I5** — Enumerate which existing tests need updating to match the new splitting-report format, so the implementation review can verify no assertions get silently deleted. (Reviewer §4.4)
- **I6** — Document that Rust does not mirror the splitting report to stderr (Perl does via `warn`). Make this omission deliberate, not accidental. (Reviewer §1.8)
- **I7** — Update the misleading doc comment at `src/pipeline.rs:188-191` ("Increments by 2 per pair to match Perl line 2451") since :2451 is `methylation_call_strings_processed`, not `sequences_count`. The new `records_processed = pairs` semantics need a fresh comment. (Reviewer §1.7)

### Optional

- **O1** — Match Perl's `warn "\n\n";` at the end of the sweep (line 625) or document the omission. (Reviewer §1.2)
- **O2** — Sequencing note: `OutputFileMap::flush_all` doesn't write the gzip trailer (Gzencoder::drop does); the trailer-write happens during the sweep's `drop(writer)`. Worth a one-line comment in the plan. (Reviewer §A "Implicit assumption")
- **O3** — Sorted-MD5 caching optimisation in harness (Reviewer §5.3).

---

## 8. Summary

The plan correctly identifies the three sub-issues, scopes them well, and the high-level structure of the byte-identity invariant rewrite is sound. The won't-fix rationale for #863 is well-argued. However, careful byte-by-byte reading of Perl `:2476-2559` reveals three concrete byte-shape defects in §3.1's write order (CRIT-1, CRIT-2, CRIT-3), the stdout-vs-stderr stream is mis-identified (the "verified from earlier captured output" rationale is invalid because shell capture catches both streams), and the SPEC section number being rewritten (§9.7 = Speedup) is wrong — the byte-identity invariant lives at §8.3 / §9 / §9.4. These four Critical issues will produce a splitting report that fails the harness's strict `cmp` check, log to the wrong stream, and edit the wrong SPEC paragraph. With those corrected + the seven Important refinements (banker's-rounding fixture, four gap-V tests, file-name updates, etc.), the plan is implementable and should land cleanly.
