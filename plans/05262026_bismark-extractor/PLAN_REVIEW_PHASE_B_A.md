# Plan review — `bismark-extractor` Phase B (Reviewer A)

**Reviewing:** `plans/05262026_bismark-extractor/PHASE_B_PLAN.md` (rev 0, 2026-05-26).
**Against:** `rust/bismark-extractor/SPEC.md` rev 2 (in-repo), Phase A source (`rust/bismark-extractor/src/*`), and `rust/bismark-io` v1.0.0-beta.6 surface.
**Reviewer:** A (fresh context window, no shared state with Reviewer B).

## Verdict at a glance

**APPROVE-WITH-NITS.** The plan is well-grounded in the SPEC, the `iter_aligned`/`record_strand` contracts hold up, and the structural prevention claims (Alan's missing-CHG/CHH and split-across-files bugs) are credibly closed by the proposed routing match + lazy file map. The kernel/SE-loop composition is correct. Several findings are worth addressing **before** the implementation trigger; none are blockers.

---

## 1. Logic review

### 1.1 SE loop composition — OK, with one ordering subtlety

The per-record pipeline (PE-flag guard → refid lookup → strand → `extract_calls` → `route_call`) composes correctly. `extract_calls` returns `Vec<MethCall>` already 5'-oriented (delegated to `iter_aligned`), so `route_call`'s M-bias increment at `call.read_pos + 1` is the correct cycle-position 1-based index — verified by reading `record.rs` lines 263-311.

**Subtle issue (Important):** the PE-flag check at §5.6 line 376 reads `record.inner().flags().bits() & 0x1`. Noodles' `Flags` type does not expose `.bits()`; the project convention (see `read.rs:585` in bismark-io, and dedup pipeline) is `u16::from(record.inner().flags()) & 0x1`. This is an API typo but worth fixing in the plan so the implementer doesn't waste a compile cycle.

### 1.2 `extract_calls` — ignore-region semantics for `-` strand: correct, but the plan's prose at §4.5 is misleading

The §4.5 row "Soft-clipped boundary" describes the ignore check as `read_pos_5p >= ignore_5p && read_pos_5p < seq_len - ignore_3p`, **using `seq_len`** rather than `xm.len()`. §6 step 2 then writes `hi = xm.len().saturating_sub(ignore_3p)` — also correct since the XM length equals the *read* length (validated at parse time in `from_noodles_record`).

Where this gets subtle: for `-` strand reads, `iter_aligned` reverses the iteration order AND remaps `read_pos_5p` to count from the sequenced 5' end. So when the loop sees `read_pos_5p = 0`, that is the **last** BAM-stored XM byte. Walking `xm[read_pos_5p as usize]` would index the WRONG byte — but the plan doesn't do that; it uses `aligned.xm_byte` carried inside `AlignedXmCall`, which is already the correctly-paired XM byte for that 5' position. **This is correct as designed**, but the plan should explicitly state in §6 step 2 / §5.1 that `extract_calls` **must not re-index into `record.xm()` by `read_pos_5p`** — the `xm_byte` carried by the iterator is the load-bearing value. Today's prose ("`b: u8 = xm[read_pos as usize]`" in SPEC §7.1 illustrative pseudocode) creates exactly this trap for implementers who skim. (Important — adds one defensive comment in `call.rs`.)

### 1.3 Refid out-of-range path — covered, panic-free

Plan handles refid → chr table miss via `BismarkExtractorError::InternalError { message }` rather than `expect`. Good — matches the "no silent wrong output" invariant and is also a strict improvement on the SPEC §7.7-style "defensive" panic. The plan's `expect(...)` at §5.6 line 385 (`record.inner().reference_sequence_id().expect(...)`) is fine because the reader filters unmapped upstream, so the expect can fire only on an upstream invariant violation. Acceptable.

### 1.4 Empty XM / `--ignore` > seq_len — covered

Saturating arithmetic + the `lo >= hi` short-circuit are both correct.

### 1.5 Splitting-report on error path — DELIBERATELY suppressed; one half-step missing

§4.5 "Invalid XM byte" row says the splitting report is NOT written on error. Good. But the plan does not explicitly say what happens to the empty splitting-report file IF it was created earlier in the run — and §10 row "Empty input" says the splitting report IS written for the empty-input success path. The two paths can't collide today because `state.finalize()` is the only writer for the report, but a future Phase D/E may stream sections early. **Optional**: add a one-line invariant note in §5.4: "`finalize()` is the only writer for `_splitting_report.txt`; partial-error path leaves it absent."

### 1.6 PE-flag rejection cleanup order — race-free

§5.6 calls `state.cleanup_partial_outputs()` before returning the `PhaseNotYetImplemented` error. Good. The cleanup runs even when `OutputFileMap` is empty (no writes have happened yet) — idempotent per §5.3 spec.

---

## 2. Assumptions review

### 2.1 Perl-version split-file header — defensible

Locking `Bismark methylation extractor version v0.25.1\n` as the header line for byte-identity at Phase H is the right call. The risk is that Perl actually emits something invocation-dependent (e.g. the program name from `$0`); the plan acknowledges this in §9.2 open question #1 and bridges in Phase H. Acceptable. (Optional: spend 5 minutes now grepping Perl line ~30 for the exact `print` statement — if it's `print OUT "Bismark methylation extractor version $bismark_version\n"` then the assumption is good; if it's `print OUT "$0 version $bismark_version\n"` it differs.)

### 2.2 Lazy file-handle creation — well-grounded

SPEC §8.4 explicitly permits this; the assumption that Perl's empty CTOT/CTOB files for directional libraries map to "absent" Rust files is reasonable. Phase H gate will catch any drift. **One nit (Optional):** the integration baseline at §7.2 will fail `cmp` if Perl emits **0-byte** CTOT/CTOB files (it likely does — open `>` in Perl creates the file even if nothing is written). The plan's integration test (§7.2) will need to either (a) skip the `cmp` for empty files, (b) `touch` an absent Rust file before comparison, or (c) generate the baseline using Bismark's `--comprehensive` to suppress the empties. Worth flagging now so the fixture generator isn't surprised.

### 2.3 AutoDetect-as-SE — defensible but under-specified for one edge case

§9.1 + §6 step 10 say AutoDetect treats first record's PAIRED flag as the decider. But §6 step 10 says "if AutoDetect AND first record has PAIRED flag set, error with PhaseNotYetImplemented." What if the input has zero mapped records (empty BAM)? The current logic never enters the per-record loop, so the AutoDetect resolution never happens — the extractor proceeds as SE with no records, emits an empty splitting report, exits 0. That's the right behavior, but the plan should state it explicitly. (Optional.)

### 2.4 Implicit assumption: `record_strand()` matches Phase H Perl strand classification

The plan's split-file routing keys on `record.record_strand()` (an SE call) and SPEC §6.1 says "for SE the **record-strand** routes the record." That's consistent. But — `bismark-io::BismarkStrand` is derived from XR+XG tag bytes (`strand.rs:48`), whereas Perl derives strand from a flag+XG combination (Perl ~lines 1500-1550). Have these been proven byte-equivalent? Dedup's v1.0 byte-identity gate validated this for the dedup output, which doesn't emit per-strand split files — so the **strand-classification → split-file-filename** mapping is exercised for the first time in extractor Phase B's integration test. Worth documenting in §9.1 as a locked assumption that Phase H will catch any drift. (Important — names a risk that's currently silent.)

### 2.5 `derive_basename` suffix list — incomplete

§9.2 #4 says "strip `.bam`/`.sam`/`.cram` only." Perl's basename helper (~line 5000-ish) also handles `.bam.gz`/`.sam.gz` per real-world filename conventions. The plan's `derive_basename` at §6 step 8 mentions `.bam.gz`/`.sam.gz` but §9.2 #4 contradicts this. Reconcile. (Important — affects splitting-report filename byte-identity, which Phase H gates.)

---

## 3. Efficiency review

### 3.1 Per-record allocation profile — acceptable for Phase B; flag for Phase F

- `iter_aligned()` allocates one `Vec<AlignedXmCall>` (~1.1 KiB for 95-bp reads).
- `extract_calls` allocates one `Vec<MethCall>` (~1-2 KiB capacity at `xm.len() / 8`).

For 55M PE reads this is ~110M allocations / ~200 MiB of temporary heap traffic; allocator hot-path. **At Phase B's single-threaded scope this is below the perf gate**, but it's the kind of cost that compounds under rayon (Phase F). Optional flag: a future `iter_aligned_into(&mut Vec<...>)` reusing a caller-owned buffer could halve this. Not a Phase B concern; just don't claim "no low-hanging perf wins" in the self-review.

### 3.2 HashMap-keyed `OutputFileMap` — acceptable

§8 self-review acknowledges this; for 6-12 keys the overhead is negligible. A typed `[Option<BufWriter<File>>; 12]` would be faster but adds dispatch boilerplate. Defer.

### 3.3 `String::from_utf8_lossy` for chr name on every call — minor

Plan calls this "one borrow + zero alloc in practice." That's only true if the lossy path is the no-op fast path. **It is** — `Cow::Borrowed` for valid UTF-8 — so the call is genuinely free. But the chr-table build in `header.rs` at §5.7 uses `.into_owned()` which DOES allocate per chr. That's once per file, not per call. OK.

### 3.4 No I/O batching considered for splitting-report — fine

8-KiB BufWriter on the report file is overkill for ~50 lines; not worth optimizing.

---

## 4. Validation sufficiency

### 4.1 Coverage of the highest-risk failure modes

| Risk class | Phase B test | Strength |
|---|---|---|
| `-` strand orientation flip (§6.5 invariant) | `extract_calls_minus_strand_orients_5prime` | Strong if it asserts the **byte content** at position 0 matches the expected 5' XM byte. Plan's wording is ambiguous — "verify first emitted call has `read_pos==0` corresponds to the 3' BAM-stored byte." This actually means: the call.xm_byte at read_pos_5p=0 equals `record.xm()[xm.len() - 1]` for `-` strand. Recommend the implementer write the assertion in that explicit form. |
| Missing CHG/CHH context (Alan's bug) | 4 `mbias_accumulate_routes_to_{chg,chh}_for_{X,x,H,h}` tests | Strong — direct unit coverage. Plus the integration test §7.2 exercises this at the integration level. |
| Strand-routing splits a single record (Alan's other bug) | `route_call_default_mode_routes_to_strand_specific_file` | **Weak as written** — only one strand/context combination is asserted. To structurally close the bug, you want a test that processes a single record with calls in **all three contexts** and asserts that all output goes to files keyed on the SAME `record_strand`. The plan doesn't have this test. Add: `route_single_record_with_mixed_contexts_routes_to_one_strand_directory`. (Important.) |
| Lazy file creation (no spurious CTOT/CTOB) | `output_file_map_lazy_creates_only_keys_seen` | Strong. |
| Partial-output cleanup on error | `cleanup_partial_outputs_removes_open_files` | Acceptable. **Gap:** what if `cleanup_all` itself fails (e.g. one `remove_file` returns `EACCES`)? Plan says "best-effort cleanup, log via `eprintln!` on failure." Test should assert that one failing remove doesn't prevent the others. Add: `cleanup_partial_outputs_continues_past_one_failure`. (Optional.) |
| Phase-gate rejections (5 unsupported flag combos) | 5 `main_rejects_*_with_phase_error` tests | Coverage is good. **One missing:** `main_rejects_multiple_input_files` — the plan says §4.1 rejects `files.len() > 1` but no test enumerates this. Add. (Important.) |
| Empty input | `extract_se_empty_input_writes_no_split_files` | Listed in §10 but not in §7.1's test table. Either add to §7.1 or reference §10 explicitly. (Optional.) |
| PE record arriving at SE pipeline | Implicit in `main_rejects_paired_end_with_phase_error` | The CLI-level rejection is covered. **But** the §4.5 defensive per-record check (PAIRED flag set on input the user passed as SE) isn't tested. Add: `extract_se_rejects_record_with_paired_flag_set`. (Important — this is the load-bearing test that protects against tooling-error inputs the plan explicitly names.) |
| Invalid XM byte under `--mbias_only=true` parameter | `extract_calls_under_mbias_only_silence_skips_invalid` | Good — closes SPEC §8.1 rev 2 row. |

### 4.2 Could a Phase B bug slip past Phase B and only surface at Phase H?

Plausible silent-failure scenarios:

1. **Splitting-report content drift.** §10 has no per-line assertion on the report contents; `splitting_report_emits_per_context_counts` asserts counts but not the verbatim section headers. A drift in headers ("Total methylated C's" vs "Total methylated C's in CpG context") would pass Phase B and only fail Phase H byte-equal. Mitigated by the plan's §9.2 #2 explicit acceptance of this risk. **Acceptable, but optionally add a snapshot test of the literal lines** so the implementer freezes the exact format early.
2. **Header line in split files.** Only golden-asserted in `format_meth_line_exact_bytes` per the plan's table — but that test asserts the call line, not the header. No Phase B test catches drift in the header line. Recommend adding `output_file_header_matches_perl_format` asserting the literal first line of a freshly-created split file. (Important.)
3. **Percentage formatting.** `%.2f` for empty contexts → `0.00` per §4.3. No explicit test for the zero-denominator path. Add. (Optional.)
4. **Strand-classification vs Perl mapping.** Per §2.4 above — Phase B routes on `BismarkStrand` derived from XR/XG; if Perl's mapping differs in some edge case (e.g. non-directional library with specific tag combos), Phase H catches it. No Phase B mitigation possible without a Perl-baseline; flag as acceptable.

---

## 5. Alternatives worth considering

### 5.1 Reuse `bismark-dedup::detect_paired_from_header` instead of per-record PAIRED-flag check

The plan in §4.5 / §6 step 10 uses a defensive per-record PAIRED-flag check. This is correct but loses the SPEC §11 `@PG ID:Bismark` semantic. **`bismark-dedup::pipeline::detect_paired_from_header` already exists at `bismark-dedup/src/pipeline.rs:137`** and walks the header for the Bismark PG line. The plan's §9.2 #3 considers this and chooses "re-implement inline if `bismark-io` doesn't expose one." But the dedup helper is already there in the workspace, in another crate. Two paths:

- **(a)** Keep the per-record PAIRED check (Phase B's current plan). Acceptable but doesn't match SPEC §11.
- **(b)** Promote `detect_paired_from_header` from `bismark-dedup` into `bismark-io` in a tiny additive bump (`v1.0.0-beta.7`) and call it once at reader-open time. This is a 30-LOC refactor and the function is genuinely shared; this is the cleaner Phase C-ready approach.

**Recommendation:** do (a) for Phase B as the plan states, but **add a note in §9.2 #3** that (b) is the locked Phase C plan. Don't let the per-record check linger into Phase C as the only PE-detection mechanism. (Optional.)

### 5.2 `ExtractParams` deferral — defensible

§9.2 #5 documents not using `ExtractParams` in Phase B. The plan's defense ("argument struct, not 14-arg function — Phase B has 6 args max") is sound. The SPEC's intent in §6.3 was prevention of bug-class via argument-routing, and a 6-arg `extract_calls` is below the threshold. **But:** `route_call` itself has 6 args (`state, record, chr, strand, call, read_identity`), and once Phase C adds PE this becomes 7-8 args. Phase C will revisit. Acceptable, with the small risk that the deferral creates a Phase C "refactor everything to use ExtractParams" task. (Optional — flag in §13 as a known Phase C item.)

### 5.3 `OutputKey` as `(CytosineContext, BismarkStrand)` vs typed array

Plan uses `HashMap<OutputKey, BufWriter<File>>`. A typed `[Option<BufWriter<File>>; 12]` indexed by `(context as usize) * 4 + (strand as usize)` would be slightly faster, zero-alloc, and the dispatch boilerplate is one match. Not worth retrofitting for Phase B but worth a one-line note for Phase F. (Optional.)

---

## 6. SPEC drift check

Read the plan and SPEC side-by-side. Found:

- **No semantic drift** on §§7.1, 7.2, 7.5, 7.7 — the plan implements what the SPEC describes (with the documented `ExtractParams` deferral).
- **Minor drift on §6.3** (the `ExtractParams` deferral, plan §9.2 #5) — documented and defensible.
- **§4.3 row 4 (`--fasta` annotation line)** is honoured by the plan at §4.3 line 122 (good).
- **§5 invalid-XM row** maps to the plan's `BismarkExtractorError::InvalidXmByte` in §5.8 (good).
- **§8.4 directional row** maps to lazy file creation (good).
- **§7.4 (overlap detection)** correctly deferred to Phase C — the plan doesn't try to implement it.

---

## 7. Other findings

### 7.1 `chr_table.get(refid)` — type mismatch

§5.6 line 386 calls `chr_table.get(refid)` where `chr_table: Vec<String>` per §5.7. `Vec::get(&self, index: usize)` takes `usize`, but noodles' `reference_sequence_id()` returns `Option<usize>`. So the call works, but the plan's prose at §5.6 line 384 says "refid: usize" implicitly. Confirm `chr_table.get(refid)` returns `Option<&String>` not `Option<&str>` — the chr binding is `&String` and feeds into `route_call(... chr: &str ...)` via deref coercion. **Minor wording**: the plan should clarify whether `chr` is `&String`, `&str`, or `String`. (Optional.)

### 7.2 `cleanup_partial_outputs` ordering vs `?` propagation

The plan handles three error sites (`reader.records()`, PE-flag check, `extract_calls`, `route_call`) with explicit `match`+cleanup blocks. The `state.finalize(config)?` at end of `extract_se` uses bare `?` — but `finalize` itself can fail (file flush + report write). If `finalize` returns `Err`, the SE loop has already succeeded, so partial-output cleanup would be wrong. Confirm with the implementer that `finalize` errors leave outputs in place. **Action**: state this invariant in §5.4: "`finalize` failure leaves output files written so far in place; caller does NOT cleanup post-finalize." (Important — silent contract not stated.)

### 7.3 Tests for SPEC §6.2 `[MbiasTable; 2]` invariant

`mbias_R2_index_ready` covers the "index 1 exists" claim. Good — closes the structural invariant at unit-test level.

### 7.4 Sub-issue creation (§14) — not part of code review

The `gh issue create` snippet is fine and matches Phase A's precedent. The "gh CLI broken via macOS keychain TLS error" note is true for the host but out-of-scope for the plan review.

---

## 8. Action items (prioritized)

### Critical

*(none)*

### Important — address before implementation trigger

1. Fix the noodles `flags()` API call in §5.6 (use `u16::from(record.inner().flags())` not `.bits()`).
2. Add `derive_basename` policy reconciliation — §6 step 8 lists `.bam.gz`/`.sam.gz` but §9.2 #4 says single-suffix only. Pick one and align.
3. Add test `extract_se_rejects_record_with_paired_flag_set` (the load-bearing per-record PE guard at §4.5).
4. Add test `main_rejects_multiple_input_files` (the `files.len() > 1` rejection).
5. Add test `route_single_record_with_mixed_contexts_routes_to_one_strand_directory` (structural closure of Alan's split-across-files bug at unit level — current plan only asserts one strand+context combo).
6. Add test `output_file_header_matches_perl_format` asserting the literal first-line header content (so Phase H drift gets caught at Phase B unit level).
7. State the `finalize`-failure invariant in §5.4 (post-finalize errors do NOT trigger cleanup).
8. Document the assumption that `BismarkStrand`-from-XR/XG matches Perl's strand-classification across all 4 strands × all 3 contexts (currently unstated; Phase H is the only catcher).
9. Add a defensive comment to §5.1 / §6 step 2: `extract_calls` must use `aligned.xm_byte` (the iterator's carried byte), NOT `record.xm()[read_pos_5p]` — the latter would silently flip `-` strand reads.

### Optional

10. Spend 5 minutes verifying the exact Perl source line for the split-file header string (§9.1 #1) — would convert an "open question" into a "locked" one.
11. Add test `cleanup_partial_outputs_continues_past_one_failure` for the best-effort-cleanup contract.
12. Add a `percentage_formatting_handles_zero_denominator` test for the `0.00` empty-context case.
13. Add a Phase H readiness note: integration baseline needs to handle Perl's empty CTOT/CTOB files vs Rust's absent files (touch-or-skip strategy).
14. Note Phase C dependency on promoting `detect_paired_from_header` from `bismark-dedup` to `bismark-io` (and update §9.2 #3 with this Phase C plan).
15. Note Phase C/D revisit of `ExtractParams` once the argument count grows.

---

## 9. Verdict

**APPROVE-WITH-NITS.** The plan is implementation-ready in its skeleton; the Important items above are 1-2 LOC each (test additions + a couple of prose clarifications) and don't require re-design. The Phase B kernel + routing + lazy file map + splitting-report shape are sound. Structural prevention of Alan's two documented bugs is real and verifiable at unit-test level. Recommend the user fold the Important items, then proceed to implementation.
