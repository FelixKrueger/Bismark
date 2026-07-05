# Code Review — bismark-extractor Phase B (Reviewer B)

**Reviewer:** B (independent of Reviewer A)
**Date:** 2026-05-26
**Branch:** `rust/iron-chancellor`
**Files reviewed:** `rust/bismark-extractor/src/{call,mbias,output,state,route,header,pipeline,error,main,lib}.rs`, `Cargo.toml`, `tests/sanity.rs`, `tests/se_phase_b.rs`, `tests/se_phase_b_smoke.rs`
**Plan reviewed against:** `plans/05262026_bismark-extractor/PHASE_B_PLAN.md` rev 1
**Validation status reported:** 50 tests passing, clippy clean, fmt clean

---

## Summary

Phase B is a clean, well-bounded implementation. The eager-open file map, the rev-1 routing-order fix, the orientation-respecting kernel, and the phase-gate dispatch are all faithful to both the plan and the Perl reference (spot-checked at lines 32, 2911-2961, 5151-5205). Byte-identity-critical surfaces (header literal, suffix-strip, `+`/`-` strand char) match Perl exactly. The smoke test covers a respectable bug surface without requiring a Perl baseline.

Findings concentrate on **Phase-F readiness** (per-record allocation pressure), **one minor efficiency miss** (header clone), **two test-coverage gaps** vs the plan, and **error-wrapping ambiguity** that will compound at Phases C/E. No correctness issues block merge.

---

## Issues by area

### Logic / Correctness

**L1. `route_call` `+`/`-` strand char encoding (false alarm — verified correct).** [Resolved]
Initial concern was that the Rust `let strand_char = if call.methylated { '+' } else { '-' };` was encoding methylation status, not physical strand orientation. Direct verification against Perl (lines 2911, 2921, 2931, 2941, 2951, 2961) confirms the `+`/`-` field is indeed `+` for uppercase-XM (methylated) and `-` for lowercase-XM (unmethylated). Rust matches. No defect.

**L2. `BismarkIoError::Io` vs `IoWrite` jacket asymmetry. [Low]**
`error.rs` has both `BismarkIo(#[from] BismarkIoError)` and `IoWrite(#[from] std::io::Error)`. A `std::io::Error` raised by extractor code (e.g. `OutputFileMap::write_call`) wraps as `IoWrite`. A `std::io::Error` originating *inside* a bismark-io reader gets wrapped twice: `io::Error -> BismarkIoError::Io -> BismarkExtractorError::BismarkIo`. So the same underlying error type wears two different jackets depending on origin. Display strings differ (`"output write failed: ..."` vs `"<transparent BismarkIoError display>"`). Not a bug today, but at Phase C (PE) and Phase F (rayon) when error paths multiply, debugging which `io::Error` came from where will be harder. **Recommend:** flatten `BismarkIo` to `#[error("input read failed: {0}")] InputRead(#[from] BismarkIoError)` — distinct error display lets ops triage by reading the message.

**L3. `cleanup_partial_outputs_continues_past_one_failure` test missing.** [Low]
Plan §7.1 promises this test ("**Rev 1 (A Optional)**: one remove failure doesn't prevent the other 11"). Not present in either test file. The `cleanup_all` implementation does loop over all 12 entries and `eprintln!`s on individual failures — behaviour is correct, but the regression guard is absent. **Recommend:** add a test that pre-locks/pre-deletes one file path so `remove_file` returns `Err`, then asserts the other 11 are removed.

**L4. `flush_all` ordering before `write_splitting_report` is correct, doc-comment overstates the rationale.** [Nit]
`state.rs:80` calls `fhs.flush_all()` before `write_splitting_report`. The output module's doc comment (`output.rs:136-138`) says "so the report can name accurate file sizes if it ever needs to" — Phase B's report doesn't include file sizes. The doc comment forward-promises a Phase-D/H behaviour without an issue link. Either drop the speculative justification or add a TODO referencing Phase H. (No behavior bug.)

### Efficiency / Phase-F readiness

**E1. Per-record QNAME clone in `extract_calls` is unconditional. [Medium — Phase F concern, not Phase B blocker]**
`call.rs:143`: `let read_id = render_qname(record);` runs once per record **even on the happy path** where no `InvalidXmByte` ever fires. `render_qname` does `String::from_utf8_lossy(...).into_owned()` — for 55M records at ~30-byte QNAMEs that's ~1.6 GiB of allocation churn, all wasted. Under rayon (Phase F) this churn multiplies across threads and hits the allocator's mutex.
**Recommend (Phase F):** pass a closure or `Cow<'_, str>`: `extract_calls` accepts `read_id_lazy: impl FnOnce() -> String` (or `&dyn Fn() -> String`) and `classify_xm_byte` only calls it on the error path. Alternative: pass `&[u8]` QNAME and only allocate the String in `InvalidXmByte` construction.
**Phase B verdict:** non-blocker — flag for Phase F.

**E2. `reader.header().clone()` in `extract_se` is avoidable. [Low]**
`pipeline.rs:56`: `let header = reader.header().clone();` then `build_chr_name_table(&header)`. The `clone()` is needed only because `reader.records(&mut self)` mutably borrows `reader` later — but `chr_table` is fully constructed *before* `records()` is called and the `header` binding is then unused. Could be: `let chr_table = build_chr_name_table(reader.header())?;` and drop the `let header = ...` line. Saves a header clone (cheap for tiny genomes, ~50-100 KiB for human, more for multi-megabase contig genomes like assemblies-in-progress). **Recommend:** remove the clone.

**E3. `OutputFileMap::write_call`: 8 separate `write_all` calls per row. [Low]**
`output.rs:122-132` does 8 `BufWriter::write_all` calls per call line. Each goes through a virtual call + capacity check. For 55M PE × ~20 calls = 1.1B `write_all` calls = ~9B vtable hops. The 8-KiB `BufWriter` capacity is small (each writer's per-flush amortization is ~400 lines). **Recommend (Phase F):** use a thread-local `Vec<u8>` scratch buffer (`format_meth_line` writes into it, single `write_all` flushes it), and bump `BufWriter` capacity to 64 KiB to reduce syscall pressure. Non-blocker for Phase B.

### Errors / Defensive coding

**Err1. `OutputFileMap::write_call`'s `.expect("OutputFileMap missing key ...")`. [Verified safe]**
The key is `OutputKey { context: call.context, strand }`. `CytosineContext` has exactly 3 variants. `BismarkStrand` has exactly 4 variants (verified at `bismark-io/src/strand.rs:36-45`). `DEFAULT_KEYS` enumerates all 12. No `BismarkStrand` value can escape the map. `.expect` is safe. **Optional polish:** convert to a `BismarkExtractorError::InternalError` with `let Some(...) = map.get_mut(&key) else { return Err(InternalError {...}); };` so test fixtures that construct corrupted state surface a typed error instead of a panic.

**Err2. `extract_se`'s refid lookup panics if `reference_sequence_id` is `None`. [Low]**
`pipeline.rs:86-89`: `.expect("mapped record must have reference_sequence_id")`. bismark-io filters unmapped at the iterator, but the invariant is only documented in a comment in `record.rs`. If that filter ever regresses (e.g. a future bismark-io variant exposes unfiltered records), this panics rather than returning an `InternalError`. **Recommend:** convert to `ok_or_else(InternalError {...})`. Same pattern as the `chr_table.get(refid)` branch immediately below.

**Err3. `state.cleanup_partial_outputs()` is called from every pre-finalize error site, but `flags_bits & 0x1` check happens AFTER `record_result` decode. [Verified safe]**
If the BAM has a corrupt first record, `record_result` returns `Err`, the cleanup runs, and we exit early. If the first record is well-formed but PAIRED-flagged, we cleanup and exit. Order is correct.

### Structure / Style

**S1. `state.rs::ExtractState::new` adds an `input_path` parameter not in the plan signature.** [Nit]
Plan §5.4 had `new(config, input_basename)`. Implementation expanded to `new(config, input_path, input_basename)` because the splitting report needs the input file path. Reasonable evolution; document briefly in the doc-comment that the signature deviates from the plan. (Plan-coverage check will catch this.)

**S2. `write_splitting_report` takes 4 positional args.** [Low]
`output.rs:212`: `(path, input_path, config, report)`. Approaching the SPEC §6.1 rule-of-thumb threshold (3-4 args is fine; 5+ is plan-violating). Currently OK; Phase D (M-bias writer) and Phase E will add more report fields. Keep the signature flat or introduce `ReportContext { input_path, config, report }`.

**S3. `state.rs`'s `#[allow(dead_code)] pub mode: OutputMode` is acceptable pre-wiring.** [Verified]
Phase B main dispatch only allows `OutputMode::Default`, so the field is stored but never read. `#[allow(dead_code)]` is the right tool — `cfg(feature = "phase-e")` would over-engineer for a 3-month timeline. No change needed.

**S4. Doc comment on `OutputFileMap::flush_all` forward-promises Phase D behaviour without issue link.** [Nit]
See L4 above.

### Tests

**T1. `extract_calls_walks_cigar_with_soft_clips` is NOT a false-positive test.** [Verified]
Spot-checked against `bismark-io/src/cigar.rs:131-138` and `record.rs:284`. CIGAR `2S8M` produces two AlignedPosition items with `read_pos == 0, 1` and `ref_offset == None`, then 8 items with `read_pos == 2..10` and `ref_offset == Some(0..8)`. The `filter_map(|ap| ap.ref_offset?)` in `iter_aligned` discards the soft-clip items but leaves `read_pos` at its post-increment value. The test's `assert_eq!(calls[0].read_pos, 2)` is correct and meaningful — it locks the soft-clip-includes-in-count invariant against future regressions in `aligned_positions`.

**T2. Smoke test's OB record uses synthetic XR=CT, XG=GA. [Verified realistic enough for Phase B]**
`from_xr_xg("CT", "GA")` produces `BismarkStrand::OB` (verified in `strand.rs:60`). The XM `Z....` on OB strand will be reversed by `iter_aligned`, so the Z byte (BAM position 0) emerges at `read_pos_5p == 4`. It still classifies as CpG-meth and routes to `CpG_OB_*.txt`. **Caveat:** real Bismark OB records have a reverse-complemented sequence too — the smoke test's `seq=b"ACGTC"` doesn't reflect that. Not a functional issue for Phase B (extractor doesn't use the sequence; only XR/XG/XM/CIGAR matter), but document the fixture's structural-vs-biological scope in a comment so a future reader doesn't assume it's a faithful Bismark output.

**T3. Plan promises `cleanup_partial_outputs_continues_past_one_failure`; not present.** [Low — see L3]

**T4. Plan promises `extract_se_two_records_route_to_different_files`; implicitly covered by smoke.** [Nit]
Plan §7.1 lists this as a unit test in `tests/se_phase_b.rs`, but the smoke test's 3+2 OT/OB fixture covers the same behaviour end-to-end. Acceptable substitution; plan-coverage check will note it.

**T5. `route_call_increments_counter_before_mbias_only_short_circuit` test forces `state.mbias_only = true` even though main rejects.** [Verified safe pre-wiring]
The test is the only place the `mbias_only` short-circuit branch is exercised in Phase B. It locks the rev-1 ordering invariant (counter increment BEFORE the short-circuit). This is correct pre-wiring for Phase E: when E lands `--mbias_only` enablement at main dispatch, this test will fail if E accidentally inverts the ordering. **No trap.** If Phase E changes the semantic (unlikely — Perl is the truth), the test rightly breaks and forces a deliberate update.

### Phase-C readiness

**PC1. `ExtractState::new(config, input_path, input_basename)` carries over cleanly to PE.** [Verified]
PE pipeline will need the same chr_table + ExtractState + cleanup_on_error pattern. `mbias[1]` is already pre-wired (R2 slot) and exercised by `route_call_r2_goes_to_mbias_index_1` test. `ReadIdentity::R2` routing path is already live.

**PC2. The `flags_bits & 0x1` check needs to be lifted to a header-level dispatch in Phase C.** [Note]
Phase B rejects PE per-record (correct for the current architecture). Phase C will need either (a) `detect_paired_from_header` from `bismark-dedup` promoted to bismark-io, OR (b) keep per-record check + branch to PE handling in the loop. Plan §9.2 #3 already calls this out — no surprise.

**PC3. `derive_basename` is PE-compatible.** [Verified]
PE doesn't change the input-file model (still one BAM per invocation; PE means flag 0x1 set in records, not "two input files"). No refactoring needed.

**PC4. `extract_se` and the future `extract_pe` will share ~80% of body code.** [Recommend]
Reading-loop scaffolding (open_reader, build_chr_name_table, derive_basename, ExtractState::new, finalize) is identical. Plan a private `extract_records<F: FnMut(BismarkRecord, &mut ExtractState) -> Result<...>>` helper at Phase C entry to keep PE and SE bodies thin. Not a Phase B fix.

---

## Fixes applied

None — Reviewer B is read-only per skill spec.

---

## Prioritized recommendations

| Priority | Item | Action |
|---|---|---|
| Medium | **E1.** QNAME allocation on hot path (1.6 GiB churn at 55M records). | Phase F: make `read_id` lazy (closure or `Cow`). |
| Low | **L3 / T3.** Missing `cleanup_partial_outputs_continues_past_one_failure` test. | Add the test. |
| Low | **L2.** `BismarkIo` vs `IoWrite` jacket symmetry. | Phase C: rename `BismarkIo` to `InputRead` with distinct display string. |
| Low | **E2.** Avoidable `reader.header().clone()`. | Drop the clone. |
| Low | **E3.** 8 `write_all` per call line. | Phase F: scratch buffer + larger BufWriter. |
| Low | **Err2.** `reference_sequence_id().expect(...)` panics on regression. | Convert to typed `InternalError`. |
| Nit | **S1.** `ExtractState::new` signature drift from plan. | Document the extra param. |
| Nit | **L4 / S4.** Speculative doc comment on `flush_all`. | Drop or link future-phase issue. |
| Nit | **T2.** Smoke OB fixture doesn't reverse-complement seq. | Add a comment clarifying structural scope. |

---

## Verdict: **APPROVE-WITH-NITS**

Phase B is correct, well-tested, byte-identity-locked at every gateable boundary (header literal, suffix-strip, `+`/`-` field, ordering invariants), and ready to merge. The one missing test (L3/T3) is a small follow-up. All other items are Phase-F or Phase-C concerns that don't block merge.
