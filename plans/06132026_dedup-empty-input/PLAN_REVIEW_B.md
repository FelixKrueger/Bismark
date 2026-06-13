# PLAN_REVIEW_B — `deduplicate_bismark_rs` graceful zero-alignment handling

**Reviewer:** B (independent, fresh context)
**Plan reviewed:** `plans/06132026_dedup-empty-input/PLAN.md` (rev 0, 2026-06-13)
**Verdict:** APPROVE-WITH-CHANGES — Critical: 0, Important: 4, Optional: 5
**Source verified against:** `rust/bismark-dedup/{src/pipeline.rs, src/main.rs, src/report.rs, src/error.rs, src/dedup.rs}`, `tests/{integration_dedup.rs, methylseq_conformance.rs, byte_identity_real_data.rs, sanity.rs}`, `rust/bismark-io/src/{read.rs, write.rs}`. Branch confirmed `rust/dedup-empty-input` @ `f1bcf42`.

---

## Summary

The plan is well-researched, empirically grounded, and the core mechanism is sound. Every line reference I checked is accurate (a rarity). The headline insight — that the `count==0` report rendering already exists and is unit-tested but dead-in-practice, and that the existing `write_bam(path, &[])` test helper already round-trips a header-only BAM through the exact `BamWriter::new → finish()` path the fix relies on — is correct and de-risks the change substantially. The intentional divergence from Perl is justified and correctly scoped to the zero-records case. I found **no Critical blockers** and **no risk of leaking into the non-empty byte-identity guarantees**. The changes I recommend are about completeness of the surface fix, tightening V7, and two small correctness/consistency items.

---

## 1. Logic review

### 1.1 Mechanism is verified sound (no Critical issues)
- **Writer round-trip for zero records is proven.** `BamWriter::new` writes the header at construction (`write.rs:62`); `write_record` is never invoked when the iterator is empty; `finish()` writes the BGZF EOF via `try_finish()` (`write.rs:99`) unconditionally. The test harness `write_bam(&input, &[])` (`integration_dedup.rs:90-97`) **already** uses this identical path to produce the header-only fixtures the current tests read back — so the "valid, downstream-readable header-only BAM" claim (Assumption 3) is not speculative; it is already exercised in-tree. SAM (`write.rs:240`, flush-only) and CRAM (`write.rs:308`, EOF-container) finalise correctly with zero records too. ✓
- **`stream_pe` over empty is a clean no-op.** Verified `pipeline.rs:258-263`: `iter.next()` returns `None` on the first call → `break` before any `UnpairedFinalRecord` can be raised. Plan §Edge-cases and Self-Review are correct. ✓
- **Coordinate-sort check is record-count-independent.** `check_not_coordinate_sorted` runs at reader construction on the `@HD SO:` header field (`read.rs` doc + lines 249/338/413/471), so a header-only BAM does not trip it spuriously and a header-only-but-coordinate-sorted-PE BAM would *still* fail-loud at open (correct — that is a header property, not a records property). The plan doesn't mention this; it's fine, but see Optional O-5. ✓
- **FLAG 0x4 filter claim verified.** `read.rs:7-10` module doc explicitly: "Silently filters unmapped reads (SAM FLAG & 0x4)." All-unmapped → zero records → same guard. ✓
- **`main.rs` needs no behavioral change.** Confirmed: `process_one`/`process_multiple` write the report and return `Ok` → exit 0 on the `Ok` path (`main.rs:183-185`, `267-269`, `37-43`). Only the comment at `main.rs:295` is stale. ✓

### 1.2 IMPORTANT — Open-question Q-1 (the stderr line) interacts with the `--multiple` path in a way the plan under-specifies
The plan proposes (Open Q-1, §A.3) emitting the informational line "once in `main::process_one`/`process_multiple` after `run_*` returns a `count==0` report." This is the cleaner option, but note: a `count==0` report is **also** produced by a legitimately-empty `--multiple` run where *every* file was header-only, AND by the all-unmapped case. Keying the message purely on `report.count() == 0` is fine semantically (all those cases ARE "no alignments"), but the wording in §Behavior step 5 ("Input contains no alignments…") should be chosen so it reads correctly for the multi-file case too (e.g. it shouldn't say "the file"). Minor, but pin the wording at implement time against all three triggers. Not a correctness issue.

### 1.3 IMPORTANT — `--multiple` mixed-format / `@SQ`-mismatch ordering is now observable on an all-empty run
Currently, in `run_multiple` the format-consistency check (`pipeline.rs:362-367`) and `validate_chr_consistency` (`379-381`) run **before** the file1 peek-stash (`408-414`). After the fix removes the peek-stash, this ordering is preserved (those checks stay above the streaming loop), so a `--multiple` run of two header-only BAMs with *mismatched* `@SQ` sets will still fail-loud with `MultipleSqMismatch` rather than silently producing an empty output. **This is the correct behavior** (validation errors must still fire even on empty input), but the plan never states it. Add an explicit edge-case bullet and ideally a test: `--multiple` two header-only files with divergent `@SQ` → still errors (not graceful). Without this stated, an implementer "simplifying the multiple path" could accidentally move validation below the loop and regress it. (cite `pipeline.rs:361-386`)

### 1.4 IMPORTANT — the `run_multiple` "stream all readers" rewrite must preserve the `len()==1 → run_single` short-circuit AND per-file `refid_tables` indexing
The plan §A.3 says "iterate **all** readers (not `readers_iter` after popping one)." Verified the current code pops file1 then iterates `readers_iter.enumerate()` with `i = i_zero_based + 1` to index `refid_tables[i]` (`pipeline.rs:432-440`). When the rewrite iterates **all** readers from index 0, the enumerate offset must drop to `i = i_zero_based` (no +1) so `refid_tables[i]` stays aligned. This is a trivial off-by-one but it is exactly the kind of silent-wrong-result bug (wrong chr_id translation in `--multiple` with reordered `@SQ`) that the byte-identity tests for `--multiple` would need to catch. Confirm a `--multiple` reordered-`@SQ` non-empty test exists and stays green (it does: `multiple_mode_*` family + the cross-file dedup test at `integration_dedup.rs:~540`). Flag this off-by-one explicitly in the implementation outline. (cite `pipeline.rs:404-440`, `558-591`, `890-936`, `1030-1075`)

### 1.5 Parallel paths (`ThreadedBamWriter`) — verified safe for zero writes
`ThreadedBamWriter::finish()` calls `bgzf.finish()` via `get_mut()` (`write.rs:183-184`), which writes the EOF marker + flushes pending blocks regardless of how many `write_record` calls happened (zero is fine). No worker ever receives a record; the pool is torn down at `finish()`. The plan's claim that the parallel paths "just skip the loop body" is correct. ✓ One note: `run_multiple_parallel` uses `readers.drain(..)` (`pipeline.rs:558`) vs `run_multiple`'s `readers.into_iter()` (`404`) — the rewrite must keep that distinction (the parallel readers Vec is `mut` and drained). Cosmetic, but don't blindly copy-paste between the two.

### 1.6 UMI paths — verified, with one subtlety
`run_single_umi` / `_parallel_umi` build `records_with_umi(extractor)` then peek (`pipeline.rs:822-825`, `962-965`). Removing the peek and feeding `records_with_umi(...)` straight into `stream_*_umi` is correct for zero records: `require_umi`/`compute_se_umi_key` are never called (loop body skipped), so a header-only UMI run will **not** raise `UmiExtractionFailed`. Good — that's the intended graceful outcome. The `cleanup_partial_output_on_err` wrapper (`622-630`) becomes a no-op on the success path (it only unlinks on `Err`), so the header-only output survives. ✓ Note the multi-file UMI variants wrap streaming in an inner closure with `?` (`908-938`, `1047-1077`); the rewrite there must keep that closure structure or restructure the error-join carefully — slightly fiddlier than the non-UMI multiple path.

---

## 2. Assumptions

| # | Assumption | Verdict |
|---|---|---|
| 1 | Graceful supersedes byte-identity on zero-alignment input only | **Valid.** No non-empty path touched; byte-identity tests (`byte_identity_real_data.rs`, all non-empty) unaffected. |
| 2 | Real trigger is header-only; Bismark never emits FLAG-4 | **Valid** (Felix-confirmed); all-unmapped kept as defensive test only. Sound. |
| 3 | Header verbatim → valid downstream-readable file | **Valid + proven in-tree** (see §1.1). |
| 4 | `count=0` → `0 (N/A%)` acceptable methylseq-side (MultiQC tolerant) | **UNVERIFIED — the one real residual risk (matches plan's own Open Q-2).** See §2.1. |
| 5 | `--multiple` mixed empty/non-empty → process non-empty, counts reflect analysed | **Valid**, mechanism confirmed; but see the off-by-one (§1.4) and validation-ordering (§1.3) caveats. |
| 6 | CRAM header-only works like BAM | **Valid mechanism** (`write.rs:308`), but genuinely untested for zero records; defensive-only is the right call. |

### 2.1 IMPORTANT — Open Q-2 (`N/A%` vs `0.00%`) is the single highest-leverage unknown and the plan defers it on a guess
The entire *point* of this change is to stop methylseq crashing. If methylseq's MultiQC Bismark-dedup module chokes on `N/A%` while parsing the zero-count report, the pipeline re-breaks one module later (MultiQC), and the fix fails its actual goal — just less visibly. The plan's mitigation is "assume MultiQC is tolerant, confirm in V7." That is acceptable **only if V7 is a hard gate** (see §4). Two observations that should inform the decision:
- The Rust `0 (N/A%)` rendering is itself an **intentional divergence-of-convenience already**, not a Perl-byte-identity requirement on this path (Perl never produces a zero-count report — it dies, emitting at most a 0-byte file per the oracle table). So switching the *empty case only* to `0.00%` would **not** violate any byte-identity guarantee, because there is no Perl oracle for a zero-count dedup report to be identical to. This removes the usual objection to changing the rendering.
- However, the `count==0 → N/A` branch (`report.rs:107-115`) is shared by any future zero-count path and is unit-pinned (`format_uses_na_when_count_is_zero`, `report.rs:201-208`). Changing it to `0.00%` would mean editing that test too. Recommend: **keep `N/A%` but make V7 prove MultiQC parses it**; if V7 fails, the `0.00%` fallback is cheap and byte-identity-safe. The plan should state this fallback decision tree explicitly rather than leaving it as "consider."

### 2.2 IMPORTANT — the cascade claim is weaker than the plan's confidence implies
§Context "Cascade check" tested only the **plain** `bismark_methylation_extractor_rs -s` on a header-only BAM. methylseq's actual extractor invocation is `--bedGraph --CX --cytosine_report --genome_folder …`, which exercises entirely different code (bedGraph aggregation, coverage2cytosine streaming, genome reads). The plan honestly flags this (the parenthetical caveat + V7), but the prose "fixing dedup should *unblock* the methylseq chain" overstates what was verified. The MEMORY notes are actually reassuring here — the extractor inline-streaming epic's RRBS/empty handling and the c2c port both handle degenerate inputs — but none of that is a *tested* header-only-through-the-full-command path. Treat the cascade as "plausible, not proven" until V7. This is not a blocker for the dedup fix itself (dedup graceful is correct regardless of what's downstream), but it bears directly on whether the *stated goal* (methylseq completes) is achieved.

---

## 3. Efficiency

No concerns. The change strictly *removes* work (the `peek()` / `first_record` stash). Complexity unchanged at `O(records)` time, `O(distinct positions)` memory. The empty path is `O(1)` after header clone + writer open/finish. The plan's self-assessment here is accurate. (Optional O-4 below is a maintainability, not efficiency, point.)

---

## 4. Validation sufficiency

V1–V6, V8, V9 are appropriate and cover the high-risk modes (SE/PE header-only, `--multiple` empty-file1, `--parallel`, UMI, all-unmapped, full-suite regression, lint/fmt). Gaps:

### 4.1 IMPORTANT — V7 must be a HARD gate, not "ideally"
V7 is the only validation that tests the **actual reason this work exists**. As written ("ideally a real methylseq run") it's soft. Given §2.1 and §2.2, the done-criteria should require **at minimum**: the full extractor command (`--bedGraph --CX --cytosine_report --genome_folder …`) run on a header-only BAM exits 0 and produces parseable outputs. The real end-to-end methylseq run on `:2.0.0-beta.6` (with a no-alignment sample) is the gold standard and should be done before declaring the *goal* met (it can be in the release step F, but it must not be skipped). Recommend splitting V7 into V7a (local full-extractor-command on header-only BAM — hard gate before merge) and V7b (real methylseq run — hard gate before announcing the fix works, can be post-merge in step F). Also: V7 should explicitly assert the MultiQC step parses the `N/A%` report (closes Open Q-2 empirically).

### 4.2 Missing test the plan should add (ties to §1.3)
No validation row covers `--multiple` with **all-empty files but mismatched `@SQ`** (should still error) or **empty-file1 + non-empty-file2 with reordered `@SQ`** (chr_id translation correctness on the now-rewritten path — guards the §1.4 off-by-one). Add at least the reordered-`@SQ` empty-file1 case; it's the only way to catch a silent-wrong-result in the multiple rewrite.

### 4.3 Silent-wrong-result surface — assessed low
The main silent-wrong-result risk is the `--multiple` refid_table off-by-one (§1.4), covered by §4.2's proposed test. The report `count`/`leftover`/`n_positions` on the empty path are structurally 0 (from `DedupState::new()`, verified `dedup.rs:424-429` `empty_state_zero_counters`), so no arithmetic can go wrong. Low risk overall.

---

## 5. Alternatives

### 5.1 IMPORTANT/OPTIONAL — single shared empty-input helper vs. 8 hand-edits
The plan does 8 near-identical edits across 8 functions. The codebase's own history (MEMORY: "Dual-driver back-port trap — independent drivers ship infrastructure bugs twice; grep sibling driver after every fix") is a direct warning against exactly this pattern. The 8 functions are already heavily duplicated (single/multiple × parallel/non-parallel × umi/non-umi), and the §1.4 off-by-one is a concrete example of a bug that could be introduced in some-but-not-all of the 4 `--multiple` variants. **Recommendation (Optional, but strongly considered):** don't attempt a big refactor in this change (risk), but DO factor the `--multiple` "stream all readers in order" body into one shared helper used by all 4 multiple variants, so the loop/enumerate/refid-indexing logic exists once. The single-file variants are genuinely one-line deletions and don't need sharing. If full sharing is judged too invasive for a hotfix, the minimum mitigation is: after editing, `grep` all 8 sites and diff the 4 multiple-path loops against each other for structural identity (the MEMORY lesson, applied).

### 5.2 OPTIONAL — `0.00%` vs `N/A%`
Covered in §2.1: keep `N/A%`, make V7 prove it, keep `0.00%` as a documented byte-safe fallback. Don't pre-emptively change it.

### 5.3 OPTIONAL — proactively fix downstream tools now vs. defer
The plan defers (correctly). The extractor is reportedly already graceful; bedGraph/c2c are inline-streamed by the extractor now (MEMORY). Deferring is right — fixing dedup is the targeted unblock, and V7 will reveal if any downstream tool needs the same treatment. Don't expand scope.

### 5.4 OPTIONAL — collapse `EmptyInput` into `NoInputFiles` (Open Q-3)
The plan keeps `EmptyInput` for the defensive `inputs.is_empty()` guards and updates its doc. This is fine and minimal-churn. Mild observation: after this change, `EmptyInput` is **only** reachable via the four `inputs.is_empty()` guards (`pipeline.rs:355/524/857/998`), which are themselves documented as already-blocked-upstream by `NoInputFiles` in `Cli::validate()`. So `EmptyInput` becomes fully dead in practice (two layers of unreachability). Keeping it is harmless; collapsing to `NoInputFiles` would remove a now-misleadingly-named variant ("input file is empty" will, after this change, *never* mean "zero records" — exactly the confusion the doc-comment update is trying to paper over). Slight preference for collapsing, but not worth blocking on. Either way, the updated doc comment is essential (and the plan has it).

---

## Action items (prioritized)

### Critical
*(none)*

### Important
1. **Make V7 a hard gate, split into V7a (local full-extractor command on header-only BAM — before merge) and V7b (real methylseq `:2.0.0-beta.6` run — before declaring the goal met).** Explicitly assert MultiQC parses the `N/A%` zero-count report. This closes the only real residual risk (Open Q-2) and validates the actual purpose of the work. (`PLAN.md` V7; `report.rs:107-115`)
2. **State the `N/A%` decision tree:** keep `N/A%` (byte-identity-safe on this path since Perl has no zero-count-report oracle); fall back to `0.00%` only if V7 shows MultiQC chokes; that fallback also requires editing `format_uses_na_when_count_is_zero`. (`report.rs:201-208`)
3. **Add explicit handling + a test for `--multiple` validation-still-fires-on-empty:** mixed-format / `@SQ`-mismatch must still error even when all inputs are header-only; and add a reordered-`@SQ` empty-file1 + non-empty-file2 test to guard the refid-table indexing. (`pipeline.rs:361-386`, §1.3 + §4.2)
4. **Flag the `--multiple` refid-table off-by-one in the implementation outline:** when iterating all readers from index 0, drop the `i = i_zero_based + 1` to `i = i_zero_based` so `refid_tables[i]` stays aligned. Silent-wrong-result risk in `--multiple` reordered-`@SQ`. (`pipeline.rs:432-440`, `583-591`, `928-936`, `1067-1075`)

### Optional
5. **Factor the 4 `--multiple` "stream all readers in order" bodies into one shared helper** (or, minimum, grep+diff all 8 sites post-edit for structural parity — the dual-driver back-port lesson). Reduces the chance of fixing 6-of-8 paths. (`pipeline.rs` all 4 multiple variants)
6. **Pin the Open Q-1 stderr wording so it reads correctly for the multi-file and all-unmapped triggers too** (avoid "the file"); emit once in `main` keyed on `report.count()==0`. (`main.rs:183`, `267`)
7. **Consider collapsing `EmptyInput` → `NoInputFiles`** (Open Q-3): after the fix, "input file is empty" never means "zero records," and the variant is doubly-unreachable. Minimal upside; the doc-comment fix is mandatory either way. (`error.rs:20-24`)
8. **Update the stale comment at `main.rs:295`** (`// Empty input — let downstream EmptyInput fire.` → reflects graceful handling) — already in the plan §A.5, just confirming it's load-bearing for reader sanity in `check_bclconvert_format_conflict`.
9. **Add a one-line note that the coordinate-sort check still fires on header-only PE input** (it's a header property) so the graceful path isn't mistaken for "accept anything." (`read.rs` sort-check; §1.1)

---

## Verdict

**APPROVE-WITH-CHANGES.** The plan is accurate (every source line reference I checked holds), the mechanism is verified-sound (the writer round-trip and the `count==0` rendering already exist and are exercised in-tree), and the intentional divergence is correctly bounded to zero-records with zero risk to the non-empty byte-identity guarantees. No Critical blockers. The four Important items are about (a) making V7 a real gate that proves the *actual goal* (methylseq + MultiQC complete), and (b) guarding the `--multiple` rewrite against a silent off-by-one and against dropping validation on empty input. Address those and this is a clean, low-risk, well-targeted fix.
