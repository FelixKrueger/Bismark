# Phase E plan review — Reviewer A

- **Plan reviewed:** `plans/05262026_bismark-extractor/PHASE_E_PLAN.md` (rev 0, 2026-05-27).
- **Reviewer:** A (independent of B).
- **Verdict:** **Ready to implement with minor revisions.** Plan is structurally sound, the SPEC alignment is good, the Phase B/C/D ripple is understood, and risks are flagged. The action items below are mostly tightening and three small correctness items the plan currently leaves ambiguous.

---

## 1. Logic review

### 1.1 What's right

- The **mode-aware `OutputKey` enum** centralises shape variation cleanly. With the discriminant carrying the per-mode key payload, `mode_keys()` becomes the single source of truth and `write_call`'s lookup stays `HashMap<OutputKey, _>` — no `Option<Strand>` flag-soup struct. Cross-checked: the alternative (flat struct with `Option`s) would break `Eq`/`Hash` semantics across modes (two different modes could hash-collide on a "partial key"). Enum is the right call.
- **Comprehensive `_context_` infix** is verified against Perl `:5333` (`s/^/CpG_context_/`) — the plan's claim is correct. Same for `_context_` in `--comprehensive --merge_non_CpG` (Perl `:5085` `s/^/CpG_context_/` and `:5109` `s/^/Non_CpG_context_/`).
- **`--merge_non_CpG` 8-file mode without `_context_` infix** (just `CpG_OT_`, `Non_CpG_OT_`, etc.) is verified against Perl `:5139` `s/^/CpG_OT_/`. Plan's §4.1 matches.
- **Yacht orientation** `+` for OT/CTOB, `-` for OB/CTOT, is verified against Perl `:1602-1613` (the `$strand = '+' | '-'` assignment from the OT/CTOT/CTOB/OB classification chain) — yacht's 8th column is that literal `$strand` value, not the four-letter class name. The plan's mapping is correct.
- **`mbias_only_silence` is correctly scoped** to the `InvalidXmByte` Err arm only. Verified against Perl `:2972` and `:3054`: both `die "..." unless ($mbias_only)` sites are inside the catch-all `else { ... }` branch (after `eq '.'` and `lc eq 'u'` are matched as silent-skips). No other error variants in `classify_xm_byte` should be silenced — `SkipUnknownContext` and `SkipNonCytosine` are already `Ok(..)` so the `Err if mbias_only_silence` arm in the plan's snippet is unreachable for them. The proposed implementation is correct.
- **Splitting-report counters stay unconditional**, including under `--mbias_only`. Already locked correctly in Phase B's `route.rs` (counter increment is before the `mbias_only` short-circuit, line 44-65 of the current `route.rs`).
- **Eager-open skip under `--mbias_only`** is correctly tied to `config.mbias_only` (which now derives from `output_mode == MbiasOnly`). The empty-map case for `flush_all` / `cleanup_all` works automatically because both iterate the HashMap.
- **Gzip footer-on-drop** behaviour (§4.3) is correctly described — `GzEncoder::drop` flushes the footer. The risk note (R2 in §12) — cleanup-on-error truncates the gzip, but the file is deleted anyway — is correct.

### 1.2 Logic gaps + concerns

**G1 (Important). The `record_start as usize` → `reference_end` → `as u32` cast path in §5.3 has two related issues.**

The snippet:
```rust
let record_start = record.alignment_start().unwrap_or(0) as u32;
let record_end = record.cigar().reference_end(record_start as usize) as u32;
```

Three problems:

1. **`alignment_start().unwrap_or(0)` silently degrades unmapped records** to `start=0`, `end=0`. The plan §4.6 says "Yacht record with `record.alignment_start() == None` → Defensive `InternalError` (same as Phase C cross-chr defensive check). Filtered upstream by bismark-io's unmapped-filter; shouldn't fire in practice." That stated behaviour is **not what the snippet implements** — the snippet would produce yacht rows with `start=0\tend=0\torientation=+/-` rather than erroring. Decide which one is correct and align the snippet with the stated behaviour (probably: emit `InternalError` if `alignment_start().is_none()` AND mode is yacht).

2. **The `as u32` casts are unchecked**. `usize` → `u32` truncates silently on 64-bit. For human/mouse chromosomes (max ~250M), positions fit comfortably; but if anyone ever runs Bismark on a contig > 2^32 the yacht output would silently truncate. Not a Phase E blocker — but worth either (a) a `try_from` with a defensive `InternalError`, or (b) widening yacht's `record_start`/`record_end` fields to `u64` to match Phase C's other position handling. Phase B's `MethCall::ref_pos` is `u32`; yacht should be consistent with whatever `MethCall::ref_pos` does.

3. **Re-computing `record.cigar().reference_end(start)` once per call in yacht mode** is the R3 risk noted in §12. The plan should either (a) compute it once per record at the top of `route_call` and pass to `write_call`, or (b) accept the per-call recomputation explicitly with a TODO for Phase F. The current snippet does (a) at `route_call`, which is fine — but then `record_start` and `record_end` are passed to `write_call` even for non-yacht modes, where they're unused. Either branch on `config.output_mode == Yacht` inside `route_call` (compute conditionally) or accept the unused 8 bytes of arg passing. The plan should pick a side.

**G2 (Important). The `OutputFileMap::new` signature ripple is broader than the plan acknowledges.**

§7.3 says "Phase D's review-hygiene precedent (Reviewer B I2), Phase E modifies these existing test files in-place because their owner PRs are still in review."

That's accepted policy. But two operational concerns:

1. The new arg order `(output_dir, basename, no_header, mode, gzip)` puts mode-flags *after* `no_header`. Phase B's existing call sites all pass `(output_dir, basename, no_header)`. Inserting `mode` + `gzip` between or after means a search-and-replace can't avoid touching every call site. **Suggest:** put new params at the end (mode, gzip), and also add the call sites' file/line numbers to §7.3 so the implementer doesn't miss one. (`grep -rn "OutputFileMap::new" tests/` would catch ~10 sites — the plan says "~10" already, which matches my grep, but listing them explicitly would prevent miss.)

2. Phase D's `mbias_writer_phase_d.rs` and `mbias_writer_phase_d_smoke.rs` will need the new signature too — the plan mentions this in §7.3 prose but not in §3.2's modified-files list. Add them.

**G3 (Important). The `extract_calls` signature change to add `mbias_only_silence` ripples to ~10 test sites in `tests/se_phase_b.rs`** — same comment as G2: list them, and put the new param at the *end* of the arg list. The plan's signature in §5.4 already does this (good), but §5.5's call-site snippet has the param last only by coincidence. Locking the position explicitly in §5.4 is the right call.

**G4 (Optional). The yacht header line (§2 scope decision) says "Same Perl version header as other modes when `!--no_header`."** Verified against Perl `:5077` — yes, that's correct. However, Perl `:5077` is *inside* the `unless($mbias_only)` block, so when `--yacht` somehow co-occurred with `--mbias_only` Perl would not print the header. Phase A rejects this combination, so the gap is moot, but worth a one-line note in §4.6 confirming the rejection still applies under the new plan.

**G5 (Important). Plan §16 fixes only one SPEC §4.1 row, but two are wrong.**

SPEC §4.1 line 92 (Comprehensive): `CpG_{input}.txt[.gz]` — **wrong**, should be `CpG_context_{input}.txt[.gz]`. Plan fixes this.

SPEC §4.1 line 94 (`--comprehensive --merge_non_CpG`): `Non_CpG_{input}.txt[.gz]` — **also wrong**. Verified against Perl `:5085` and `:5109`: this mode emits `CpG_context_*` and `Non_CpG_context_*` (with the `_context_` infix). The plan's §4.1 *table* correctly says `{CpG|Non_CpG}_context_{basename}.txt[.gz]`, but the SPEC fix in §16 doesn't update SPEC line 94. **Add the second SPEC fix to §16.**

**G6 (Optional). Perl emits warnings (not errors) when `--mbias_only` is combined with `--comprehensive` / `--merge_non_CpG` (Perl `:1043-1048`).** The Phase E plan derives `OutputMode::MbiasOnly` from the CLI parse, which absorbs all combinations silently. This is *functionally* equivalent (the mode wins; downstream behaviour is the same), but Perl's stderr output won't match. Probably fine for Phase H byte-identity (stderr is rarely captured) but worth a one-line note in §4.6's "Comprehensive + `--mbias_only`" row to acknowledge the missing warn.

### 1.3 Cross-Perl behavioural checks

I cross-checked these claims and they match the Perl source:

- `:5333` Comprehensive uses `CpG_context_` infix. **Verified.**
- `:5077` Yacht header line is the same `version $version\n`. **Verified.**
- `:5148-5151` `unless($mbias_only)` skip-open guard. **Verified** (it's actually present on every individual `open` line for all modes, e.g. `:5341`, `:5379`, `:5392`).
- `:2972/3054` `die "..." unless ($mbias_only)` on the invalid-XM-byte branch only. **Verified.** Sites are at `:2972` (CHH unmethylated branch) and `:3054` (CHH reverse-strand). I also spot-checked: the other XM branches use `elsif` chains and fall through to one shared catch-all `else`, so the `die` only fires for unrecognised bytes — never for `U`/`u`/`.`/`H`/`h`/etc.
- Yacht 8-col row format `(id, +/-, chr, pos, xm_byte, start, end, strand)`. **Verified** at `:4472`, `:4485`, etc. — the 8-tuple `join("\t", ...)` is consistent across all six XM-branches.
- Yacht `strand` value is `+`/`-` (not OT/CTOT/CTOB/OB). **Verified** — Perl `$strand = '+'` for OT/CTOB (`:1604`, `:1610`), `$strand = '-'` for CTOT/OB (`:1607`, `:1613`).

---

## 2. Assumptions review

### 2.1 Locked assumptions — all verified

§9.1's locked list is sound. I verified the four most load-bearing ones above (Comprehensive infix, yacht orientation, mbias_only silent-skip, eager-open skip).

### 2.2 Hidden / under-stated assumptions

**A1.** §4.3 says "`Box<dyn Write + Send>`" but Phase E is `--parallel 1`. The `Send` bound is forward-looking for Phase F. Worth noting it's a free constraint at Phase E (the bound is satisfied automatically by `File` and `GzEncoder<File>`) but locks in a small flexibility cost: any future writer that's `!Send` (e.g. a thread-local lock guard) can't be slotted in. Not a Phase E concern; just an explicit assumption to record.

**A2.** §4.5's snippet assumes `classify_xm_byte` is the *only* error source in `extract_calls`. From reading `call.rs:71-92`, that's currently true — but if Phase F or later adds a second `?` operator inside `extract_calls`, the `mbias_only_silence` gate would silently swallow that too. Recommend the implementation use a narrower match on the specific `BismarkExtractorError::InvalidXmByte` variant rather than the catch-all `Err(e) if mbias_only_silence => {}` shown in §4.5. The narrow form keeps the silencing scope minimal:

```rust
match classify_xm_byte(...) {
    Ok(...) => { ... }
    Err(BismarkExtractorError::InvalidXmByte { .. }) if mbias_only_silence => {}
    Err(e) => return Err(e),
}
```

**A3.** §2's table claim "`flate2 = "=1.0.34"` matches the version transitively pulled by `noodles_bgzf`" — I didn't verify the version pin against the current Cargo.lock. The implementer should confirm before adding. If `noodles_bgzf` has bumped its `flate2` since the plan was written, lock to the same version to avoid duplication. (`cargo tree | grep flate2` will confirm.)

**A4.** §4.3 says `BufWriter::with_capacity(8 * 1024, inner)` is "fine for both" gzip and plain. That's likely true but for gzip the dominant cost shifts from syscalls to compression CPU, so the 8 KiB buffer is conservative — a larger buffer (e.g. 64 KiB) might reduce vtable-call frequency. Phase F profiling concern, not a Phase E blocker.

**A5 (Important).** §4.4 step 2 says "when `config.mbias_only`, skip eager-open entirely. The map is empty; `write_call` never gets called (route_call short-circuits earlier)". That's true *for paths that go through `route_call`*. But the M-bias.txt writer in Phase D is in `state.finalize`, which calls `fhs.flush_all()` before writing the M-bias.txt (state.rs:106). Confirm `flush_all` on an empty map is a no-op (it is, per `output.rs:164-169`, the for-loop has zero iterations). Test `output_file_map_skips_eager_open_for_mbias_only` should also assert that `flush_all` succeeds on the empty map — currently it only asserts the empty-dir invariant.

---

## 3. Efficiency

§8's analysis is reasonable. Confirming and extending:

- **Vtable cost per write**: `Box<dyn Write>` adds one indirect call per `BufWriter::write_all`. With 8 KiB buffer and ~50-byte per-call rows, the vtable is hit roughly every 160 calls, then once-per-flush. Negligible at Phase E.
- **`format_yacht_row` allocates a `Vec<u8>` per call**: §8 already flags this as a Phase F concern. Inline `write_all` into the writer would avoid the allocation. **Minor optimisation worth doing at Phase E** — it doesn't complicate the code and saves one alloc per yacht call. The signature `format_yacht_row(...) -> Vec<u8>` becomes `write_yacht_row(writer, ...) -> Result<()>`. (Plan §5.1's helper is a public-API choice; the impl side can stay zero-alloc.)
- **`reference_end` per call**: G1.3 above — compute once per record, not per call. The plan acknowledges this (R3) but the §5.3 snippet does compute per-record (good). Just make sure that's actually what gets implemented.
- **`Box<dyn Write + Send>` static-dispatch deferral**: §9.2 #2 punts this to Phase F. Defensible — Phase E's only path with this is `--gzip`, where compression CPU dominates by orders of magnitude. The static-dispatch enum `Either<File, GzEncoder<File>>` is a Phase F optimisation that depends on whether multicore amplifies the cost. Reasonable defer.

---

## 4. Validation sufficiency

§7's test list is thorough. Gaps:

**V1 (Important). No test for `--gzip --mbias_only`.** This combination produces an empty output dir + M-bias.txt + splitting-report (no `.gz` files). The plan's §4.6 row "—gzip + empty BAM" assumes the per-mode files exist; the mbias_only combo skips them entirely. Add `smoke_mbias_only_gzip_emits_no_split_files_and_no_gz_files` to assert the absence of any `.gz` artifacts.

**V2 (Important). No test for `--yacht --gzip` smoke.** §7.2 has `smoke_yacht_emits_1_file_with_8_col_rows` (plain) and `smoke_gzip_default_emits_12_gz_files` (gzip default). Add `smoke_yacht_gzip_emits_1_gz_file_with_8_col_rows` — yacht's 8-col format under gzip is a specific combination Phase H byte-identity will probe.

**V3 (Important). No PE test for `mbias_only_silence`.** §7.1 has `extract_calls_mbias_only_silence_skips_invalid_xm_byte` but the plan doesn't say whether the synthetic fixture is SE or PE. Under PE, the silenced byte could be on R1 or R2 — both arms of `extract_calls` in `pipeline.rs:312-313` pass the same `mbias_only_silence`. Add a PE fixture where the invalid byte is on R2 to confirm the param is honoured for both reads. Phase D's `pe_phase_c.rs` has PE infrastructure that can be reused.

**V4 (Optional). No test for empty-BAM `--yacht`.** Empty yacht run should produce a single `any_C_context_*.txt` with version header only (or empty if `--no_header`). Smoke test would assert.

**V5 (Important). No test that `cleanup_all` works after a failed gzip write.** §4.3's R2 risk note assumes the cleanup path deletes `.gz` files. The unit/smoke tests don't exercise the error path. Add a test that injects an I/O error (e.g. write to `/dev/full` or close the file mid-write) under `--gzip` and asserts the `.gz` file is removed. This is the test that proves R2's risk is contained.

**V6 (Optional). The §7.1 test `output_file_map_gzip_writes_valid_gz_content` asserts round-trip decode matches plain content "byte-for-byte"** — that's only true if `GzEncoder::default()`'s compression level is deterministic and the plain mode test uses the same input. Recommend asserting the *decompressed* gzip content equals the plain-mode bytes, not that the .gz file equals anything specific.

**V7 (Optional). The `main_accepts_*_no_longer_rejected` tests in §7.1 say "passes phase-gate; fails downstream because tempfile isn't a real BAM, but the rejection text is absent."** Be careful that the assertion doesn't just check for the absence of `PhaseNotYetImplemented`; the test should specifically grep stderr (or the `Result` error variant) for `"Phase"` and confirm it's not present, while accepting the downstream BAM-parse failure. A naive `assert!(result.is_err())` won't catch a regression where the phase-gate accidentally re-rejects.

---

## 5. Alternatives

**Alt-1. Flat `OutputKey` struct with `Option<BismarkStrand>`** — rejected (correctly, in my view). Two reasons not stated in the plan: (a) `Hash`/`Eq` on `Option<Strand>` would collide across modes if not paired with mode, requiring mode-in-key anyway; (b) "missing strand" is semantically different across modes (Comprehensive: strand-agnostic; Yacht: SE-only so always-single-strand) — encoding as `None` loses information. The enum is correct.

**Alt-2. Static-dispatch via enum** `enum WriterKind { Plain(File), Gz(GzEncoder<File>) }` — deferred to Phase F (§9.2 #2). The plan's defer reasoning is sound: at parallel=1 the vtable cost is dwarfed by I/O and (for gzip) compression. The deferred work, when picked up, will need to re-touch every `BufWriter<Box<dyn Write + Send>>` site, but the touch is mechanical. Defensible defer.

**Alt-3. Lazy file open in non-default modes** — not considered in the plan, but worth mentioning to explicitly reject. Perl `:5341, :5379, :5392` etc. eagerly open every mode's files. Eager-open matches Perl byte-identity. Phase H test surface would catch any lazy-open regression. Locked correctly by inheritance from Phase B.

**Alt-4. Compute `record_end` lazily for non-yacht modes** — see G1.3. A small `if config.output_mode == Yacht` branch in `route_call` would skip the `reference_end` call for the 4 non-yacht modes. ~5% throughput win in non-yacht modes (the call walks the CIGAR ops). Probably worth doing inline rather than passing two unused u32s through `write_call` for non-yacht modes. Minor refactor; plan should pick a side.

**Alt-5. `gzip_level` configurability** — the plan locks `Compression::default()` (level 6). Perl uses external `gzip -c` which is also level 6 by default. Match locked. No alternative needed.

---

## 6. Risks

§12's R1/R2/R3 risk list is correct. Adding:

- **R4 (low).** `flate2 = "=1.0.34"` is pinned with `=` — workspace updates that bump `noodles_bgzf` to a new flate2 will dup. Mitigation: `cargo tree -d` post-implementation to confirm zero duplication.
- **R5 (low).** Phase B / C / D test files modified in-place during Phase E means git's merge-base for #849 / #851 / #853 will see Phase E touches the same files. If any of those phases need a rev-bump before merge, the test changes might collide. Mitigation: rebase after each upstream merge (plan §15 already accounts for this).

---

## 7. Action items

### Critical
*(None.)*

### Important

1. **G1 / Alt-4.** Resolve the `record_start`/`record_end` derivation logic: (a) decide whether `alignment_start().is_none()` for a yacht record errors or silently produces zeros (plan §4.6 says errors; §5.3 snippet implements zeros — pick one); (b) decide whether to compute `record_end` conditionally (only when yacht mode) or unconditionally and pass to `write_call`; (c) use `u32::try_from(usize_val)` or similar for the cast safety, or document explicitly that human-genome max is well below u32::MAX.

2. **G2 / G3.** List the exact call-site files+lines that need signature ripples for both `OutputFileMap::new` (Phase B/C/D test files including `mbias_writer_phase_d.rs`) and `extract_calls`. Lock arg position at the end of each function signature so search-replace is mechanical.

3. **G5.** Add the second SPEC fix to §16: SPEC §4.1 line 94 (`--comprehensive --merge_non_CpG` example) should be `Non_CpG_context_{input}.txt[.gz]`. Currently only the Comprehensive row is in §16.

4. **A5.** Test `output_file_map_skips_eager_open_for_mbias_only` should also assert `flush_all` and `cleanup_all` succeed on the empty map. (One-line addition to the existing test.)

5. **V1.** Add `--gzip --mbias_only` smoke test — asserts the output dir contains M-bias.txt + splitting-report only, with zero `.gz` files.

6. **V2.** Add `--yacht --gzip` smoke test (`any_C_context_*.txt.gz` with 8-col rows after decompression).

7. **V3.** Add PE test for `mbias_only_silence` where the invalid byte is on R2 (PE-symmetric behaviour confirmation).

8. **V5.** Add a test exercising `cleanup_all` after a `--gzip` write failure — confirms the `.gz` file gets removed and the cleanup-on-error path doesn't leak partial files. (R2 risk note in §12 claims this; needs a test.)

9. **A2.** Narrow the `mbias_only_silence` catch arm to specifically match `BismarkExtractorError::InvalidXmByte { .. }`, not the catch-all `Err(e) if mbias_only_silence`. Prevents future error variants in `extract_calls` from being silently swallowed.

### Optional

10. **G4.** §4.6 add a one-line note confirming the yacht header-line behaviour under the Phase A `--yacht --mbias_only` rejection (already rejected, but document the interaction).

11. **G6.** §4.6 note that Perl emits warnings for `--mbias_only --comprehensive` / `--mbias_only --merge_non_CpG`, and Phase E silently accepts them (deviation from Perl stderr; not a Phase H concern).

12. **Efficiency.** Inline `format_yacht_row` into `write_yacht_row(writer, ...)` to avoid the per-call `Vec<u8>` allocation. Small win, no complexity cost.

13. **V4.** Add empty-BAM `--yacht` smoke test (header-only `any_C_context_*.txt`).

14. **V6.** Reword V6-style test assertions to compare decompressed bytes to plain-mode bytes, not raw `.gz` to anything.

15. **V7.** `main_accepts_*` tests should specifically check for absence of `Phase` in the error string, not just `is_err()`.

16. **A3.** Confirm `flate2 = "=1.0.34"` matches the workspace's current `noodles_bgzf`-transitive version. (`cargo tree | grep flate2`.) If misaligned, pick the version that avoids duplication.

17. **A4.** Consider bumping `BufWriter::with_capacity` to 64 KiB for the gzip path — vtable amortisation in Phase F's profile may matter. Document as a Phase F profiling target.

---

## 8. Summary

The plan is well-grounded — the cross-Perl line citations are accurate, the mode-aware `OutputKey` enum is the right design, the Phase B/C/D ripple strategy is honest, and the SPEC fix convention is followed (modulo the missed second row, G5). The biggest concrete gaps are (G1) ambiguity in the `record_start`/`record_end` derivation snippet, (G5) one missed SPEC fix, and a small but useful set of test-coverage additions (V1, V2, V3, V5). None require a re-plan — all are tightening edits to the existing document.

**Recommend:** address the Important items inline in PHASE_E_PLAN.md (one revision pass), then proceed to implementation.

---

**Report file:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_A.md`
