# SPEC_REVIEW_B — `bismark-extractor` SPEC

Reviewer: B (independent, fresh context)
Target: `/Users/fkrueger/Github/Bismark/rust/bismark-extractor/SPEC.md` (rev 0, 683 lines)
Source verified against: `/Users/fkrueger/Github/Bismark/bismark_methylation_extractor` (6,050 LOC, v0.25.1)

## Verdict: **NEEDS-REVISIONS**

The SPEC is structurally sound and the design choices in §6 close the known prior-art bugs. However, a small number of concrete correctness errors in the §3 flag inventory, two **silent-divergence risks** in §5/§7.1, **duplicated §8 and §9 sections** (the SPEC physically contains the same headings twice), and **gaps in the byte-identity test contract** prevent approval as-is. The fixes are mechanical, not architectural.

---

## Critical findings (block merge)

### C1 — Duplicate §8 and §9 sections (structural)

The SPEC has §8 (lines 473-547) AND a second §8 (lines 589-614), AND §9 (lines 548-587) AND a second §9 (lines 616-628). The numbering then continues §10 (line 630) etc. This is a hard editorial bug: a reader cannot tell which §8.3 / §9.4 is authoritative. Phase H references "§8.3 real-data byte-identity gate" — which one?

**Fix:** Delete one copy of §8 and one copy of §9. The first occurrence (lines 473-587) is more detailed; keep that, delete the second (lines 589-628).

### C2 — `--fasta` claim is factually wrong (SPEC §3 row 4)

SPEC: *"Legacy; accepted but unused — variable never read in 6050 LOC. Document as accepted-no-op; emit a one-line stderr deprecation warning."*

Verified: `$genomic_fasta` IS read at Perl line 5040, and when set it writes the line `"Genomic equivalent sequences will be printed out in FastA format"` into the `_splitting_report.txt` (line 5041). This means treating `--fasta` as a true no-op in Rust will **break byte-identity** of the splitting report whenever the user passes `--fasta`.

**Fix:** Either (a) reject `--fasta` outright (preferred — it really is dead from a functional standpoint), or (b) accept and inject the same splitting-report line for byte-identity. Pick one and update §3, §4.3, §11.

### C3 — Unknown XM byte: Perl `die`s, Rust silently skips (SPEC §5 + §7.1)

Perl source lines 2972, 3054 (and parallel sites): the catch-all `else` branch **dies** with `"The methylation call string contained the following unrecognised character: ..."` unless `--mbias_only` is set.

SPEC §5 says only `U`/`u`/`.` are silently skipped, and §7.1's `classify_xm_byte` does `_ => None` — i.e., **anything unknown is silently skipped in Rust**. This is a behaviour divergence: a corrupted/anomalous BAM that crashes Perl would silently produce output (with the bad bytes dropped) in Rust. Byte-identity is preserved on clean inputs but the error-handling contract differs.

**Fix:** Make `classify_xm_byte` (or its caller) return an explicit `Err(UnrecognisedXmByte)` for non-`./U/u/Z/z/X/x/H/h` bytes, mirroring Perl's `die`. Suppress the error when `--mbias_only` is set, matching Perl's `unless($mbias_only)` guard.

### C4 — `--ignore_3prime` / `--ignore_3prime_r2` cite "(epic §)" instead of Perl line

SPEC §3 rows 7 and 8 list the Perl line as `(epic §)`. Verified the actual lines are **989** (`'ignore_3prime=i'`) and **990** (`'ignore_3prime_r2=i'`).

**Fix:** Replace `(epic §)` with `989` and `990` respectively.

### C5 — Flag count mismatch and table-row inconsistency

- SPEC §2 says "All 34 Perl CLI flags".
- SPEC §3 prose says "All 34 Perl flags catalogued" and "Perl `GetOptions` lists 26 entries; the additional 8 in this table come from auxiliary flags".
- The §3 table actually has **35 rows** (1-35).
- Verified `GetOptions` lines 959-993 list **35 distinct entries** (with `CX|CX_context` as a single flag with two names).

**Fix:** Update §2 + §3 prose to "All 35 Perl GetOptions entries" and drop the spurious "26 + 8 = 34" reconciliation paragraph — every flag in the table IS in the GetOptions block. Nothing comes from "auxiliary".

### C6 — "Sorted-md5" assertion hides ordering bugs (SPEC §8.3)

The byte-identity gate uses `sort | md5` for split files. Sorted-md5 only proves *set* equality, not *byte* equality. The whole §9 parallelism story is "produce output byte-identical to `--multicore 1`" — but the test cannot catch an ordering regression because it sorts before hashing.

**Fix:** Add an **unsorted** byte-identity check (e.g., `md5 < <(gzcat file)`) for at least the `--multicore 1` path. The sorted-md5 check then remains as the cross-parallelism gate for `--multicore N` where Rust intentionally preserves Perl's exact line order (so unsorted equality should ALSO hold). At minimum: assert unsorted equality at N=1; sorted equality at N=4/8 only if you can't get unsorted to hold there (you should be able to — §9.4 promises strict input order via `BTreeMap<u64, WorkerOutput>`).

### C7 — No unit test for collector reorder under simulated worker skew (SPEC §9.4)

The `BTreeMap<u64, WorkerOutput>` collector is the load-bearing invariant for byte-identity at `--multicore N`. §8.1 lists no unit test exercising it (e.g., "collector buffers and re-emits in input_idx order when worker 2 finishes before worker 1").

**Fix:** Add a `parallel_collector_reorders_skewed_worker_output` test to §8.1. Without it, a regression that loses ordering (e.g., switching to a `HashMap`, or merging via crossbeam without the index sort) would only be caught at the real-data gate.

---

## Important findings (must address before implementation)

### I1 — `mbias_accumulate_routes_to_chg_table_for_X_byte` test missing (SPEC §8.1)

§8.1 has `mbias_accumulate_increments_meth_for_uppercase` and `..._unmeth_for_lowercase` (CpG implicit) but no test asserting that an `X` byte hits the `chg` Vec, an `H` byte hits the `chh` Vec. Alan Hoyle's port shipped with empty CHG/CHH M-bias tables — this exact regression has happened before. **A test that catches it explicitly is mandatory**, not optional.

**Fix:** Add three tests: `mbias_accumulate_routes_X_to_chg`, `mbias_accumulate_routes_H_to_chh`, `mbias_accumulate_routes_Z_to_cpg`.

### I2 — Directional-vs-non-directional fixture (SPEC §8.2)

The synthetic fixture is described as "PE reads … across all four strands". For Bismark, a **directional** library produces only OT/OB reads; CTOT/CTOB files are **always empty** (and the Perl extractor still emits them with just a version-header line). If the §8.2 fixture is non-directional (all 4 strands), it won't catch Alan's bug-class where Rust *spuriously emits CTOT/CTOB content for a directional library*.

**Fix:** Commit **two** fixtures — `directional_dataset.bam` (OT+OB only, CTOT/CTOB files must be header-only) and `non_directional_dataset.bam` (all 4 strands populated). Run byte-identity gates against both.

### I3 — Missing edge case in §8.4: PE with mates on different chromosomes; mixed-strand pairs

The SPEC §8.4 lists 9 edge cases. Two scientifically-impossible-but-defensively-important fixtures are missing:
- **R1 and R2 on different chromosomes** — Bismark output never produces this, but `bismark-io::BismarkPair::from_mates` may not reject it explicitly. The extractor should reject loudly (not silently process).
- **Mixed-strand pair** (e.g. R1 conversion CT, R2 conversion CT but `XG:Z:GA` etc.) — Perl's strand-assignment at lines 1917-1931 explicitly dies on "Unexpected combination". Spec the same Rust behaviour.

**Fix:** Add both to §8.4.

### I4 — Edge case: empty XM tag (all `.` or 0-length)

Implicit in `extract_calls_empty_xm_yields_empty_vec` (§8.1) — but only for "no methylation bytes". The truly degenerate case (XM length 0 from a malformed BAM) is not tested. `extract_calls` divides by 8 for the `Vec::with_capacity` hint — fine — but the loop's read_pos invariant (`read_pos == seq_len` after CIGAR walk) needs to hold for seq_len = 0.

**Fix:** Add `extract_calls_zero_length_xm_yields_empty_vec_without_panic` to §8.1.

### I5 — `--CX_context` scope contradiction

§2 lists `--CX_context` as "Out of scope for v1.0". §3 row 24 lists `--CX|--CX_context` as a v1.2 flag tied to `--cytosine_report`. §11 doesn't surface this contradiction.

**Fix:** Either drop `--CX_context` from §2's out-of-scope list (it's in scope, just gated to subprocess), or clarify that §3 row 24 means "accepted and passed through to the `coverage2cytosine` subprocess" not "implemented inline." Open question §11 should call out which.

### I6 — `--samtools_path` no-op behaviour underspecified

SPEC §3 row 27 says "Accepted-no-op in Rust port". Should it emit a stderr warning (matching dedup precedent for ignored flags)? §11 doesn't mention. v1.1 dedup explicitly warns on ignored `--samtools_path` — Rust extractor should match for UX consistency.

**Fix:** Add to §11 open questions and pick a default: "Emit one-line stderr warning '`--samtools_path` ignored (Rust port uses noodles)' to match dedup pattern."

### I7 — `--genome_folder` error path underspecified

§3 row 22 says "Rust port rejects without explicit value" but what happens at runtime if user passes `--cytosine_report` without `--genome_folder`? CLI validation site? Error message wording? Should mention `cli_validate_rejects_cytosine_report_without_genome_folder` test.

**Fix:** Add a §8.1 unit test entry and specify the error message in §3.

### I8 — Output file buffering not specified

§7.5's `route_call` writes per-call to `state.fhs[fh_key]`. Are these `BufWriter<File>`? Per-call write to a raw `File` would be a perf disaster (12 files × millions of calls × syscall each). Not even mentioned in §7.7's `ExtractState` definition (`OutputFileMap` shape).

**Fix:** §7.7 should state `BufWriter<File>` (or equivalent) explicitly. Cite dedup's pattern.

### I9 — `--multicore` warning behaviour (lesson from dedup)

Phase D dedup added a "soft warning at --parallel > 4" (per recent commit `4213f36`). Should the extractor inherit this? Not mentioned in SPEC. If extractor scales linearly past N=4 because the bottleneck differs from dedup, the soft warning is wrong; if it doesn't, the warning is right. Either way, decide explicitly.

**Fix:** Add to §11 open questions: "Inherit dedup's `--parallel > 4` soft warning, or skip it?"

---

## Optional findings (nits)

### N1 — Profiling baseline numbers
SPEC §9.7 says "extractor takes 12.3 min single-core, 5.4 min 4-core on 10M PE WGBS" — these match CLAUDE.md. The ≥4× target at N=4 is realistic given dedup's 4.88× precedent (extraction is more CPU-heavy than dedup hash lookups, so should scale at least as well). No issue.

### N2 — `ThreadedBamReader` clarification
§6.4 says "Single producer thread decompresses BAM via `bismark-io::ThreadedBamReader`". Technically the noodles `MultithreadedReader` uses internal worker threads. The SPEC's "single decompression" claim refers to a **single decompression pipeline per process** (vs Perl's N pipelines per N forks). This is correct but could be clearer.

**Fix (optional):** Reword to "Single decompression pipeline (noodles `MultithreadedReader` handles its own BGZF worker threads internally)."

### N3 — Phase LOC sum
500 + 800 + 600 + 500 + 400 + 700 + 400 + 200 = **4100 LOC**. SPEC says "~4,000". Close enough, just round up.

### N4 — Pitfall row 7 ("Per-process splitting reports merged at end")
§12 claim that "Rayon model produces a single per-run report from the main thread — no merge step needed" is consistent with §9.5's collector-owns-state design. Verified — no contradiction.

### N5 — Test name nit: `extract_calls_walks_cigar_with_indels`
The test is listed once but tests two distinct cases (`M D M` and `M I M`). Consider splitting into `_with_deletion` and `_with_insertion` for failure-message clarity.

### N6 — CIGAR ops in §7.1 pseudocode
The CIGAR walker handles `Match | SequenceMatch | SequenceMismatch` together. Verified correct (all three consume both read and ref). `HardClip | Pad` correctly consume neither. `Insertion`/`SoftClip` consume read only; `Deletion`/`RefSkip` consume ref only. SPEC §7.1 is consistent with noodles semantics and Perl walker behaviour (lines 1620-1670 region).

---

## Action items (prioritised)

**Critical (must fix before plan-reviewer approval):**
- C1: De-duplicate §8 and §9
- C2: Fix `--fasta` behaviour (reject OR inject splitting-report line)
- C3: Make unknown XM bytes error, not silent-skip
- C4: Replace `(epic §)` with `989` / `990` in §3 rows 7-8
- C5: Fix "34 flags" → "35 GetOptions entries"; drop the bogus 26+8 reconciliation
- C6: Add unsorted byte-identity check at N=1 in §8.3
- C7: Add `parallel_collector_reorders_skewed_worker_output` unit test in §8.1

**Important (before Phase B implementation):**
- I1: Add `mbias_accumulate_routes_X_to_chg`, `..._H_to_chh`, `..._Z_to_cpg` tests
- I2: Split §8.2 fixture into directional + non-directional variants
- I3: Add cross-chromosome PE pair and mixed-strand-pair edge cases
- I4: Add zero-length-XM edge case
- I5: Resolve `--CX_context` scope contradiction
- I6: Specify `--samtools_path` warning behaviour
- I7: Specify `--genome_folder` validation error path
- I8: State `BufWriter<File>` in §7.7
- I9: Decide `--parallel > 4` warning policy

**Optional:**
- N1-N6 nits as noted above.

---

## Closing note

The structural design (§6) is correct and closes both Alan-port bug classes. The recon-derived pitfall catalog (§12) is consistent with my Perl-source reading. The byte-identity contract (§8.3) is the right target. The fixes above are tractable in one revision pass and should land before the dual-review re-run.
