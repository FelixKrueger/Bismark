# Plan Review — Phase B (`bismark-extractor`) — Reviewer B

**Plan:** `plans/05262026_bismark-extractor/PHASE_B_PLAN.md` (rev 0).
**SPEC:** `rust/bismark-extractor/SPEC.md` (rev 2).
**Reviewer:** B (independent; A running in parallel; no shared state).
**Verdict (TL;DR):** **NEEDS-REVISIONS** — one *Critical* item (eager- vs lazy-open file creation contradicts Perl semantics and the SPEC's "structural prevention of Alan's bug" framing); several *Important* items around boundary semantics, header ordering, `--no_header` vs `--mbias_only` guards, and the `mbias_only_silence` dead-code branch; a handful of *Optional* nits.

---

## 1. Logic review

### Critical

**C1. Lazy file creation contradicts Perl's eager file creation — direct byte-identity hazard.**

Plan §2 / §4.2 / §4.5 / §9.1 lock in "open on first write per `(context, strand)` key" and claim "all output files absent" for empty input and that this "Closes Alan's 'spurious CTOT/CTOB files' bug structurally" (also echoed in SPEC §8.4: "CTOT/CTOB drained empty" / "absent depending on FH-creation strategy").

But Perl actually **opens every configured strand file eagerly at the start of the run** and writes the version header line immediately. See `bismark_methylation_extractor`:
- Lines 5148-5151 open `CpG_OT`; line 5159 writes the header.
- Lines 5171-5174 open `CpG_CTOT`; line 5182 writes the header.
- Same for CTOB (5194/5205) and OB (5217/5228), then all four for `Non_CpG_*` (5243-5323).
- All of these are guarded only by `unless($mbias_only)` and `unless($no_header)` — *not* by "any call was actually routed here".

So for a directional SE BAM with `--no_header` off, the Perl baseline contains 12 files (or 6 for SE comprehensive depending on mode), with `CpG_CTOT_*`, `CpG_CTOB_*`, `CHG_CTOT_*`, `CHG_CTOB_*`, `CHH_CTOT_*`, `CHH_CTOB_*` each containing **just the version header line** (no records).

Consequences:
1. The plan's `output_file_map_lazy_creates_only_keys_seen` validation test (§7.1 + §10) codifies the *wrong* invariant: the Rust port will leave CTOT/CTOB files absent from disk, while the Perl baseline will have them present with one-line content. Phase H byte-identity will fail.
2. The "empty input" edge case (§4.5 first row) similarly diverges: Perl still writes 12 one-line header files for an empty BAM; the Rust plan writes none.
3. Alan's "spurious CTOT/CTOB" bug, as framed in SPEC §8.4 / §12, must have been about *spurious CONTENT* (mis-routed records) in those files, not about their existence. If we treat "no file on disk" as the structural fix, we introduce a new byte-identity regression while claiming to close an old one.

**Recommended resolution:** switch `OutputFileMap` to eager-open at construction time (one file per configured `(context, strand)` for the active `OutputMode`), write the header line immediately at open time (gated by `!no_header && !mbias_only`), and keep the lazy `HashMap` only as an internal cache if you want — but DO touch every key the mode requires. The unit test `output_file_map_lazy_creates_only_keys_seen` should be rewritten as `output_file_map_eagerly_creates_all_strand_files_for_default_mode_with_header`. Update SPEC §8.4 row "Directional library" similarly: assert CTOT/CTOB files exist on disk and contain *exactly the header line* (not "0-byte / absent").

If you genuinely want to keep lazy creation for performance, you have to do so in BOTH the open-time path AND a "touch all configured keys before finalize()" pass — but that's strictly more work than eager-open. Just open eagerly.

### Important

**I1. `iter_aligned` `read_pos_5p` is NOT "post-soft-clip read coords."**

Plan §4.5 row "soft-clipped boundary" claims the boundary check operates on the "5'-oriented post-soft-clip read position." This is misleading. `bismark-io::CigarExt::aligned_positions()` (cigar.rs:131-138) increments `read_pos` THROUGH soft-clip operations (`SoftClip` increments `self.read_pos += 1` per base, same as `Match`). `BismarkRecord::iter_aligned` then filters `filter_map(|ap| ap.ref_offset?)` which drops the soft-clip positions but does NOT renumber the remaining `read_pos`. So for `+` strand `5S95M`, the first emitted call has `read_pos_5p == 5`, not `0`. The XM `xm[read_pos]` indexing still works because the XM length equals the read sequence length (soft-clip bytes in XM are `.`).

The plan's edge-case row for `2S8M` ("first emitted call has `read_pos == 0`") is *wrong*. Plan §7.1 test `extract_calls_walks_cigar_with_soft_clips` will catch this if assertion is written, but the prose-level claim "soft-clip not counted in `iter_aligned`'s yielded positions" is incorrect.

Net behavior happens to match Perl byte-identically (I verified `--ignore` semantics against Perl lines 1627, 1651 and the soft-clip handling at 4245-4247) — Perl's `substr(meth_call, ignore)` operates on the same BAM-stored XM length including soft-clip, so the math lines up — but the plan should update its prose to say "`read_pos_5p` counts from the 5' end of the sequenced read INCLUDING soft-clipped bases (matches Perl's pre-CIGAR-adjustment indexing)." Fix the test assertion accordingly.

**I2. Header line emission and `--no_header` guard logic.**

Plan §4.2 says: "Perl writes `Bismark methylation extractor version v0.25.1\n` ... unless `--no_header`." Correct so far. But the plan's `OutputFileMap::write_call` emits the header lazily *on first write*. Combined with C1, this means:

- (a) For files that never receive a call, no header is emitted — wrong vs Perl.
- (b) For files that do receive a call, the order of header emission across files depends on the order records appear in the BAM (e.g. if the first record routes to `CpG_OT`, that file's header is written first). Perl's header order is determined by the static file-open order (CpG_OT → CpG_CTOT → CpG_CTOB → CpG_OB → Non_CpG_OT → ...). For byte-identity *within* one file the header content is identical, so this is mostly fine, but it confirms that lazy emission is structurally different from Perl and gives Phase H more work to track down drift sources.

Additionally, the plan misses that Perl's header guard is `unless($no_header) ... unless($mbias_only)` (double-guarded). The plan's spec phrasing "suppressed by `config.no_header == true`" is correct in the directional path; just verify that the `mbias_only` short-circuit (which Phase B doesn't expose but still has a kernel param) skips header emission as well when wired in Phase E.

**I3. `mbias_only_silence` kernel param is dead code in Phase B.**

`extract_calls`'s `mbias_only_silence: bool` (plan §5.1, called with `false` from `extract_se`) only matters under `--mbias_only`, but Phase B's main dispatch rejects `--mbias_only` with `PhaseNotYetImplemented`. So the `true` branch is shipped + unit-tested (`extract_calls_under_mbias_only_silence_skips_invalid` in §7.1) but unreachable from `main`.

This isn't strictly wrong — pre-wiring kernel params for Phase E is fine — but the plan should either:
1. Document explicitly that `mbias_only_silence` is "pre-wired for Phase E; Phase B's CLI dispatch keeps it at `false`" (one line in §13 / risks), OR
2. Defer the parameter entirely to Phase E and pass nothing in Phase B (smaller surface, less dead code).

Option 1 is fine but flag the dead branch explicitly. Currently the plan smuggles it into the kernel signature without naming the deviation from the "no dead code in Phase B" implicit contract.

**I4. Splitting-report counter increment order under `--mbias_only`.**

Plan §6 step 6 says: M-bias accumulation step + splitting-report counter increment both happen inside `route_call`. SPEC §7.5 pseudocode shows: step 1 = M-bias accumulate; step 2 = `if state.mbias_only: return` (early-out); step 3 = splitting-report counters.

This means under `--mbias_only`, the splitting-report counters DO NOT fire. But Perl's `_splitting_report.txt` *does* contain per-context counts even in `--mbias_only` mode (the counts are accumulated regardless of whether file writes happen). The SPEC's pseudocode short-circuits before the counter increment — which would break `_splitting_report.txt` content under `--mbias_only`.

Phase B rejects `--mbias_only` so this doesn't bite in Phase B, but the SPEC pseudocode is wrong-as-written and the plan inherits the bug for Phase E. Recommend explicit ordering in the plan §6 step 6:

```
1. accumulate M-bias (unless mbias_off)
2. increment splitting-report counters  ← happens regardless of mbias_only
3. if mbias_only: return
4. format + write split-file line
```

Document this deviation from SPEC §7.5 in the plan's §13. Phase E will need it.

**I5. Integration-test fixture is "defer if infeasible" — no minimal end-to-end smoke test.**

Plan §7.2 says the integration test "is `#[ignore]`'d behind `RUN_FIXTURE_INTEGRATION=1`" and "if creating the Perl baseline is infeasible during the implementation pass ... defer the integration test to Phase H." That leaves Phase B with zero whole-pipeline tests that actually run the binary end-to-end on a real BAM.

This is a *validation sufficiency* gap (skill review area #4). A Phase B bug in the SE main loop (e.g. wrong record-iteration ordering, BufWriter not flushed in `finalize`, lazy-open with the wrong path joining) could pass every unit test and only surface at Phase H byte-identity.

**Recommended minimal smoke:** build the synthetic ~50-read BAM in Phase B (NO Perl baseline needed for the smoke), run the Rust binary on it via `assert_cmd`, and assert:
- The expected file set exists on disk after the run.
- Each split file is non-empty (or contains at least the header line per C1).
- The splitting report exists and parses (line-count > 0).
- Exit code is 0.

This costs maybe ~60 LOC and catches a wide class of "the binary panicked / produced nothing / wrote to the wrong dir" bugs *without* depending on the Perl toolchain. The full byte-identity comparison can still defer to Phase H, but a "binary ran end-to-end and produced something" smoke gate belongs in Phase B.

Also: the plan should commit the fixture-generation script `tests/data/regenerate.sh` even if the *baseline* generation is deferred — that way Phase H is one-command to regenerate.

### Optional / nit

**O1. `String::from_utf8_lossy` on chr names — silent UTF-8 substitution.**

Plan §8 + §9.1 row "Refid → chr name" says "Bismark chr names are ASCII in practice; lossy fallback can't hurt byte-identity for ASCII inputs." Defensible default, but the substitution `\u{fffd}` (3 bytes) for any non-ASCII byte (1 byte) would silently corrupt output if a user supplies a custom assembly with non-ASCII chr names. Two cleaner options:

- Cheap: assert in `build_chr_name_table` that every chr name is ASCII; error out with a clear message if not. One `is_ascii()` check per chr.
- Cheaper: document as a Phase H concern (the byte-identity test would fail loudly, and Bismark's own genome-prep doesn't accept non-ASCII names anyway).

Pick one. Don't leave it as a silent lossy-substitution that the user won't notice.

**O2. `derive_basename` corner cases unspecified.**

Plan §9.2 #4 says "only strip the single trailing extension." Doesn't specify:
- `foo.bam.tmp.bam` — strips to `foo.bam.tmp` (one extension).
- `foo` (no extension) — stays `foo`.
- `FOO.BAM` (uppercase) — Perl is case-sensitive on suffix match (`s/bam$/txt/` is case-sensitive in Perl); Rust port should match this.
- `foo.sam.gz` — plan §6 step 8 lists `.bam.gz` and `.sam.gz` in the helper; does it also strip `.cram.gz`? (CRAM doesn't compress with gzip, so probably moot, but specify.)

One short example block in the plan would resolve all of these.

**O3. SE non-directional test coverage.**

Plan §7.1 tests `output_file_map_lazy_creates_only_keys_seen` covers a directional input that touches only `CpG_OT`. No test covers a non-directional SE input that touches CTOT and CTOB. SPEC §8.4 has a "Non-directional library" fixture row but the plan's §7.1 doesn't enumerate a unit-level test for CTOT/CTOB strand routing.

Given C1 (eager-open fix), this becomes less critical (all 4 strand files will exist regardless). But a unit test that drives a synthetic CTOT-strand record through `route_call` and asserts the call ends up in `CpG_CTOT_*` would close the audit gap explicitly.

**O4. `paths` mirror in `OutputFileMap` is fragile.**

Plan §5.3: `OutputFileMap` carries both `fhs: HashMap<OutputKey, BufWriter<File>>` and `paths: HashMap<OutputKey, PathBuf>` "for cleanup_partial_outputs." Two parallel maps risk drifting (a key in `fhs` without a matching `paths` entry, or vice versa, due to a bug). Combine into a single `HashMap<OutputKey, (PathBuf, BufWriter<File>)>` — same cost, no invariant to maintain.

**O5. `extract_calls`'s `xm.len() / 8` heuristic.**

`Vec::with_capacity(xm.len() / 8)` (plan §6 step 2) is the "CpG density heuristic." For mostly-`.`-XM reads (most of the read is non-cytosine), this over-allocates; for very-CpG-dense (e.g. CGI overlap), it under-allocates and Vec grows. A simpler `Vec::with_capacity(xm.len() / 16)` or even `Vec::new()` with reliance on amortized growth gives identical perf in profiling. Minor cleanup.

**O6. Header content independence from emission order.**

Plan §4.2 says the header is a fixed Perl-version string — no per-file metadata. So C1's note about lazy emission ordering doesn't affect header *content* byte-identity; only *file existence* matters. Good — but explicitly note this in the plan so the user knows the order question is moot.

---

## 2. Assumptions

### Confirmed against source

- **`iter_aligned` orientation correction**: verified in `bismark-io/src/record.rs:263-311`. For `+` strand: forward iteration, `read_pos_5p == BAM read_pos`. For `-` strand: remapped to `seq_len - 1 - read_pos` and reversed. Plan §3.1 + §6.5 inheritance is correct.
- **Insertion semantics divergence vs Perl**: `record.rs:251-260` documents that `iter_aligned` SKIPS insertion positions (Perl emits them with `xm_byte=='.'`). Plan delegates to `iter_aligned` without re-flagging this — fine for Phase B's M-bias path because Perl's `.` would be filtered by `classify_xm_byte` anyway, but the plan should explicitly reference this divergence as "inherited from bismark-io; no Phase B work" for future readers.
- **`ReadIdentity::from_flags(u16)`** signature verified in `bismark-io/src/record.rs:64`. Plan §5.6 call site is correct.
- **`detect_paired_from_header`** already exists in `bismark-dedup/src/pipeline.rs:137` — plan §9.2 open question #3 ("re-implement inline if bismark-io doesn't expose one") can be resolved: it lives in `bismark-dedup`, not `bismark-io`. For Phase B, "AutoDetect → treat as SE, error on first PE record" is fine; the dedup helper should move to `bismark-io` when Phase C wires PE.

### Surfaced (implicit)

- The plan assumes `record.inner().flags().bits() & 0x1 != 0` is the right PAIRED-flag check. Noodles `Flags::is_segmented()` is the idiomatic equivalent. Either works; just pick one for the codebase consistency with dedup (which uses `Flags` methods, e.g. `flags().is_first_segment()`).
- The plan assumes `BufWriter<File>` with 8 KiB default buffer is enough. `BufWriter::with_capacity(8 * 1024, ...)` is the explicit form. Phase B's efficiency-target paragraph says 8 KiB but the code snippet just says `BufWriter::new(...)` which yields a default 8 KiB — happens to match. Worth being explicit in the implementation.
- The plan doesn't say what happens if `output_dir` doesn't exist on disk. Perl creates it (lines 970 area, `make_path`). Phase A might already do this in `ResolvedConfig`; if not, Phase B's `OutputFileMap::new` should `std::fs::create_dir_all(output_dir)` defensively.

---

## 3. Efficiency

Acceptable for Phase B. The dominant cost is `iter_aligned`'s materialization (one ~1.1 KiB Vec per record) — already paid in `bismark-io`. `OutputFileMap` HashMap lookup is `O(1)`; for 12 keys an enum-indexed `[Option<BufWriter<File>>; 12]` array would be faster but the difference is well below the noise floor of file I/O at parallel=1.

Minor:
- The HashMap can be `FxHashMap` (already a dep of `bismark-dedup`); cheap perf win, but really negligible at this scale.
- Per O5, the pre-allocation heuristic is overkill.

No scalability concerns at parallel=1.

---

## 4. Validation sufficiency

Coverage map vs the listed validation table (plan §10):

| Validation row | Sufficient? |
|----------------|------------|
| Orientation invariant for `-` strand | ✓ Strong unit test. Catches the orientation regression class. |
| Missing CHG/CHH M-bias routes | ✓ Four explicit tests. Closes Alan's bug at unit level. |
| Lazy file creation | ✗ **Codifies the wrong invariant** (per C1). Should be eager-open + header-write tests. |
| Partial-output cleanup on InvalidXmByte | ✓ Direct test exists. |
| Phase-gate dispatch | ✓ Five rejection tests. Comprehensive. |
| End-to-end SE smoke | ✗ **Defers to Phase H if infeasible** (per I5). Leaves no whole-pipeline test in Phase B. |
| Empty input | ✗ Mentioned in §4.5 but no explicit test enumerated in §7.1. Add `extract_se_empty_input_writes_no_split_files` (renamed per C1 to "...writes_only_header_lines"). |
| Clippy + rustfmt | ✓ Standard hygiene gate. |

Other gaps:
- No test for `extract_se` on a multi-record BAM where successive records produce calls across different `(context, strand)` keys (i.e. the loop's accumulator + file-handle map correctness). Add at least one synthetic 2-record test.
- No test for the splitting-report's *parameter summary* section. Plan §4.3 says "parameter summary block — one line per relevant CLI flag — matches Perl's tone." Without at least one assertion on what parameters appear, this block can silently drift from Perl. Suggest a unit test that snapshots one minimal config's parameter section against a golden string (defer exact byte-identity to Phase H but lock the structure now).
- No assertion that `flush_all` actually flushes before `write_splitting_report` runs (a tricky ordering bug: if the splitting report is written first, the split files may be missing trailing data on `Drop`). Add `extract_se_flushes_split_files_before_writing_report` or just structure `finalize()` to call `flush_all()` first and let `Drop` handle final close.

---

## 5. Alternatives considered

**A1. Eager-open vs lazy-open (C1 above).** Pick eager-open. Cheap, matches Perl, simplifies header-emission ordering, removes one entire class of byte-identity drift. No real reason to keep lazy.

**A2. Enum-indexed file array vs HashMap.** A typed enum (e.g. `OutputKey { Cpg(StrandIdx), Chg(StrandIdx), Chh(StrandIdx) }`) maps to a `[Option<BufWriter<File>>; 12]` lookup table. Faster, less heap. But more boilerplate. Plan §5.3's HashMap is the right Phase B choice — defer the array form to Phase F when multicore actually puts lookup on the hot path.

**A3. `ExtractParams` struct usage.** Plan §6 step 12 / §9.2 #5 defer this. Fine for Phase B. SPEC §6.3 explicitly motivates `ExtractParams` as a structural anti-bug device for the 14-arg `extract_calls` Alan's port had. Phase B's 6-arg signature is below that threshold and the kernel function itself is simple enough that the struct adds no value. Defensible.

**A4. `read_pos_5p` semantic clarity.** Three options:
- (a) Rename `read_pos_5p` in `bismark-io` to `read_pos_5p_including_softclip` — verbose but unambiguous.
- (b) Keep the name, add a one-line docstring clarification.
- (c) Renumber in `iter_aligned` so the first emitted call always has `read_pos_5p == 0` (would require additional bookkeeping for soft-clips).

The plan implicitly assumes (b). Confirm in the plan §3.1 reference list that `bismark-io`'s `iter_aligned` doc actually states this — if not, file a bismark-io docs follow-up.

---

## 6. Action items

### Critical (block implementation)

1. **C1.** Switch `OutputFileMap` to eager-open for all configured `(context, strand)` keys in `OutputMode::Default`. Write the header line at open time (gated by `!no_header && !mbias_only`). Rewrite the `output_file_map_lazy_creates_only_keys_seen` test as `..._eagerly_creates_all_strand_files_for_default_mode_with_header`. Update §4.5 row "Empty input" + §8.4 SPEC row "Directional library" + §10 validation row accordingly. Reconcile with the SPEC's "structural prevention of Alan's bug" claim — likely re-frame as "PE pair-strand routes the whole pair (not per-record strand)" which is the actual structural fix, not "files don't exist on disk."

### Important (request before APPROVE)

2. **I1.** Fix plan §4.5 + §7.1 prose: `iter_aligned`'s `read_pos_5p` includes soft-clipped positions in the count. Update the `extract_calls_walks_cigar_with_soft_clips` test assertion to match (first emitted call has `read_pos_5p == soft_clip_len`, not `0`, for `+` strand). Net behavior vs Perl is still correct — only the prose/assertion is wrong.
3. **I2.** Document header emission ordering and content invariance explicitly. With C1 fixed, header order is the static file-open order (matches Perl). Add one test `output_file_map_writes_header_to_all_files_in_static_order_when_no_header_off`.
4. **I3.** Decide on `mbias_only_silence` kernel param: either keep + document the dead branch in §13 risks, or defer to Phase E entirely.
5. **I4.** Plan §6 step 6 ordering: increment splitting-report counters BEFORE the `mbias_only` short-circuit, not after. Document the deviation from SPEC §7.5 in §13 and queue the SPEC fix.
6. **I5.** Add a minimal end-to-end smoke test in Phase B that doesn't require the Perl toolchain (synthetic BAM → run binary → assert file set + non-empty). Commit `tests/data/regenerate.sh` even if Perl baseline gen is deferred.

### Optional

7. **O1.** Either ASCII-assert chr names in `build_chr_name_table` or document non-ASCII as a Phase H concern.
8. **O2.** Specify `derive_basename` behavior for edge inputs (uppercase suffix, no suffix, double suffix).
9. **O3.** Add a unit test for non-directional SE (CTOT/CTOB strand routing) at the `route_call` level.
10. **O4.** Combine `fhs` + `paths` into a single `HashMap<OutputKey, (PathBuf, BufWriter<File>)>` to remove the dual-map invariant.
11. **O5.** Drop or simplify `xm.len() / 8` preallocation; default `Vec::new()` is fine.
12. **O6.** Plan should add one explicit `extract_se_empty_input_*` test and one multi-record `extract_se_two_records_route_to_different_files` test.
13. **O7.** Use `Flags::is_segmented()` instead of `flags().bits() & 0x1` for codebase consistency with dedup.

---

## 7. Verdict

**NEEDS-REVISIONS.**

Justification: C1 is a direct byte-identity hazard built into the data-structure design and the plan's "structural fix" framing. Fixing it isn't large (eager-open at `OutputFileMap::new`, write headers immediately) but it changes the test surface and the SPEC §8.4 invariant materially. I1/I2/I4 are correctness clarifications that don't change LOC much but need to be locked before implementation starts. I5 is a Phase-B-internal validation gap — it's possible to ship Phase B without an end-to-end smoke and rely on Phase H, but doing so concentrates risk at the byte-identity gate where the user explicitly wants problems caught earlier.

Once C1 + I1-I5 are resolved (mostly edits to plan + test list, no major code-shape change), this is a solid APPROVE — the algorithm shape, SE-only scoping, kernel delegation to `iter_aligned`, partial-output cleanup, and phase-gate rejections are all well-thought-out. The plan is otherwise unusually thorough for a Phase B port.

Reviewed against SPEC.md rev 2; bismark-io v1.0.0-beta.6 source (record.rs, cigar.rs); Perl `bismark_methylation_extractor` v0.25.1 (lines 1619-1660 ignore semantics, 4245-4267 soft-clip handling, 5076-5430 file-open + header emission); `bismark-dedup` pipeline.rs (precedent for chr-table + auto-detect helper).
