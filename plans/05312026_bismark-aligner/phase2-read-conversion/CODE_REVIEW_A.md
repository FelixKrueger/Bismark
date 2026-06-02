# CODE REVIEW A — Phase 2: Read conversion (`convert.rs` + wiring)

> Reviewer A (independent, fresh context). **Audit-only — no code modified.**
> Target branch/worktree: `rust/aligner` @ `~/Github/Bismark-aligner`.
> Gate: byte-identical converted temp FastQ vs Perl Bismark v0.25.1 `biTransformFastQFiles` (5489–5651) + `fix_IDs` (6235–6246).

## Summary

**Verdict: APPROVE with minor follow-ups.** The core conversion (`convert.rs`) is a faithful, line-by-line reproduction of the Perl transform for the v1 spine (FastQ / single-end / directional). I verified every step of `biTransformFastQFiles` against the Rust loop and found **no byte-identity divergence** in the converted-record path: `count++`-before-skip/upto, `chomp`(`\n`-only)→`fix_id`→re-append `\n`, falsy-`0` skip/upto, `uc`-then-`tr/C/T/`, verbatim `id2`/`qual`, record-1-only sanity (bypassed when skipping), `last unless (all 4)` truncated-tail drop, and multi-member gzip are all correct. Tests are thorough and assert real behavior; the spec-derived-golden caveat is honestly documented and the Phase-10 Perl-oracle deferral is reasonable.

All **34 unit + 15 integration tests pass**; `clippy --all-targets -- -D warnings` is **clean**.

The findings below are **not** byte-identity breaks on the wired path. Two (M1, M2) are latent faithfulness gaps that bite only with specific flags/inputs that the v1 spine *can* reach; the rest are quality / carry-forward notes.

---

## Issues by area

### 1. Byte-identity faithfulness

**M1 — `--prefix` does not strip trailing dots (Perl 8238 `$prefix =~ s/\.+$//`).** [Medium]
`convert.rs:143-146` builds the name as `format!("{p}.{basename}")` using `cfg.output.prefix` verbatim. Perl trims trailing dots from `$prefix` in `process_command_line` (line 8238) *before* `biTransformFastQFiles` concatenates `"$prefix.$C_to_T_infile"` (5518). So Perl `--prefix foo.` yields `foo.reads.fq_C_to_T.fastq`; the Rust port yields `foo..reads.fq_C_to_T.fastq`. This is a **temp-file *name*** divergence, not a content divergence — the converted bytes are identical, and the name is internal/transient. But the prefix *also* governs the final output BAM/report names in later phases, where it **will** be byte-visible. Fix belongs in `resolve_output` (config.rs:404) — `cli.prefix.clone().map(|p| p.trim_end_matches('.').to_string())` — so both the temp name (Phase 2) and the eventual output name (later phases) inherit the trim from the single source of truth. Recommend fixing now while the seam is fresh; at minimum add a tracking note. Cite: bismark 8234-8242; config.rs:404; convert.rs:143-146.

**M2 — `--mm2_maximum_length` without an mm2 aligner is silently accepted (Perl 8333-8335 dies).** [Medium]
Perl makes `--mm2_maximum_length` a **fatal error** unless `--minimap2`/`--mm2` is set (`unless ($mm2){ ... if ($maximum_length_cutoff){ die ... } }`, 8329-8335). In the Rust port, `--minimap2` is rejected up front (config.rs:208-212), so `maximum_length_cutoff` can only ever arrive on the Bowtie 2 spine — exactly the case Perl rejects. Today nothing validates this: `ReadProcessing.maximum_length_cutoff` is populated unconditionally (config.rs:168), the `ConvertOptions` guard at convert.rs:213-217 *can* therefore fire on a v1 Bowtie 2 run, and a record could be silently dropped. The plan repeatedly calls this guard "inert on the v1 spine" — that is only true *because Perl forbids reaching it*, an invariant the port does not yet enforce. Net effect: a user passing `bismark_rs --mm2_maximum_length 50 ...` (no `--minimap2`) gets silent record-dropping where Perl would die. Recommend: in `resolve()` (or `resolve_aligner`/a dedicated guard), reject `maximum_length_cutoff.is_some()` when the aligner is not minimap2, mirroring Perl 8333. The convert-side guard can then stay as the documented mm2-phase hook. Cite: bismark 8329-8335; config.rs:160-168; convert.rs:213-217.

**L1 — Perl progress/warning lines not reproduced.** [Low — out of current gate]
Perl emits `Skipping the first $skip reads` (5508), `Processing reads up to ...` (5511), `Writing a C -> T converted version ...` (5548), `Removing this sequence with length ...` (5601), and the final `Created C -> T converted version ... ($count sequences)` (5638). The Rust port emits only its own `Created C->T converted version of {read} -> {path} ({count} sequences)` (lib.rs:79-83). These are STDERR diagnostics, **not** part of the byte-identity gate (gate = converted file content + later decompressed SAM/report). No action needed for Phase 2; flagging so it's a conscious omission, not an oversight, if STDERR parity is ever desired.

**L2 — `seqID_contains_tabs` flag computed then discarded — must reach the REPORT later.** [Low — carry-forward]
convert.rs:221 computes `_id_has_tab` and drops it. The Perl flag (5608) is not merely a STDERR warning: at 2140-2142 it also `print REPORT`s a tab-warning line into the **Bismark report**, which *is* byte-gated in a later phase. The Rust check is faithful (operates on the post-`fix_id` id, matching Perl 5607), and in practice the flag never fires in default mode (tabs → `_`) or `--icpc` mode (truncated at tab), so the report line never appears — but that reasoning should be confirmed (not assumed) when the report phase lands. Recommend: when the report writer is built, plumb this flag (e.g. return it on `ConvertedReads`) rather than re-deriving it. Cite: bismark 2140-2142, 5607-5609; convert.rs:221.

**Verified correct (no action):**
- `chomp` strips `\n` only, `\r` kept (convert.rs:108-114, fix_id leaves `\r`) ✓ (Perl 5584, `$/="\n"`).
- `count += 1` before skip/upto, after the `last unless` 4-line guard ✓ (Perl 5582-5594; convert.rs:185-207).
- Falsy `0` skip/upto via `s > 0` / `u > 0` ✓ (Perl `if($skip)`/`if($upto)`; convert.rs:196-207).
- `uc` then `tr/C/T/` on the uppercased line incl. newline; `id2`/`qual` verbatim ✓ (Perl 5597/5624-5626; convert.rs:210/231-234).
- Record-1-only sanity, **after** skip → bypassed when skipping ✓ (Perl 5612-5616; convert.rs:224-228).
- `last unless ($id and $seq and $id2 and $qual)` truncated-tail drop via `n==0` on any of 4 reads ✓ (convert.rs:185-187). Last record without trailing `\n` is written verbatim (read_until returns >0) ✓.
- Multi-member gzip via `MultiGzDecoder` ✓ (Perl `gunzip -c`); decompressed bytes identical.
- Extensions kept, `_C_to_T.fastq[.gz]` suffix, raw `${temp_dir}${name}` concat (not `Path::join`) ✓ (Perl 5514/5542-5545/5548; convert.rs:143-152).
- temp_dir normalization (abs + trailing sep, empty→CWD) faithful to Perl 8207-8232 (deviation: placed in `convert::temp_dir_prefix` not `resolve()` — documented, behaviorally identical) ✓.

### 2. The `convert_reads` gate (lib.rs:68-94)

**No silent half-support.** The `match` wires conversion **only** for `(SingleEnd, Directional, FastQ)` and prints an explicit "wired in a later phase" note for every other mode (lib.rs:86-91). This is correct and honest — no mode is silently skipped or partially processed. SE non-directional / pbat / PE / FastA all fall to the catch-all and emit the note. ✓

**L3 — the deferred path returns `Ok(())` but the Phase-1 summary already printed "no alignment performed".** [Low — informational]
For non-wired modes, `run` prints the resolved-config summary then the "later phase" note and exits 0 with no temp file. That's the intended Phase-2 behavior (no alignment anywhere yet), but a user on, say, a PE run gets a success exit with neither conversion nor alignment. Acceptable for a phased build; just confirm the EPIC tracks that PE/non-dir/pbat/FastA conversion + the missing STDERR/exit semantics are owned by later phases.

### 3. The `ReadProcessing` seam (config.rs:107-120, 164-169)

**Clean and correct.** Additive sub-struct carrying **only** the new fields (`skip`/`upto`/`icpc`/`maximum_length_cutoff`); `gzip`/`prefix`/`temp_dir` are read from `OutputTarget` (convert.rs:47-48 pulls `gzip`/`prefix` from `cfg.output`), so there is **no duplicate source of truth** — exactly as the plan mandates. Populated inside `resolve()` (config.rs:164), so Phase-1 tests that build `RunConfig` only via `resolve` are undisturbed. `ConvertOptions::from_config` is the single mapping point. ✓

**L4 — `--skip`/`--upto`/`--gzip`/`--prefix` are now *live* in Phase 2 but still listed in `deferred_flags` (config.rs:271-283).** [Low]
`deferred_flags` still pushes `--skip`, `--upto`, `--gzip`, `--prefix` (config.rs:271, 272, 281, 282), so a user running an SE-directional-FastQ conversion with `--skip 2` sees the misleading "recognised but not yet active … wired in a later phase: --skip" notice *even though Phase 2 honors it*. Strictly these flags are only fully active on the wired spine, and `--gzip`/`--prefix` still affect later-phase outputs too, so the notice isn't wholly wrong — but it now over-warns for the conversion path. Recommend revisiting `deferred_flags` membership for the four flags Phase 2 actually consumes (at minimum `--skip`/`--upto`, which Phase 2 fully implements). Cite: config.rs:264-287; cli.rs:218-219 (`--icpc` correctly *not* in the list, per plan §8). ✓ on `--icpc`.

### 4. Rust correctness / quality

- **Error handling:** `?`-propagation throughout; the record-1 sanity returns a typed `AlignerError::Validation` with the Perl message text (convert.rs:224-228). `File::open`/`create`/`canonicalize` errors propagate. ✓ One nit (L5).
- **L5 — gzip-detection mismatch between read and write paths.** [Low] Input gzip is detected by **filename** (`input.to_string_lossy().ends_with(".gz")`, convert.rs:157), while output gzip is driven by the `--gzip` **flag** (convert.rs:165). That's faithful to Perl (input: `$file =~ /\.gz$/` 5500; output: `if ($gzip)` 5541). Fine — just noting the two `.gz` decisions come from different sources by design, not by accident.
- **Buffering:** `BufReader`/`BufWriter` on both ends; `read_until` reuses four cleared buffers per record (convert.rs:171, 175-182) — linear, low-allocation. `writer.flush()` + explicit `drop(writer)` before returning the path ensures the `GzEncoder` finishes its trailer (convert.rs:237-238). ✓ Correct and important for the gz case.
- **`Box<dyn BufRead/Write>`:** appropriate for the gz/plain branch; the `BufWriter<Box<dyn Write>>` wraps the (already-buffered-internally?) encoder in an outer buffer — `GzEncoder` does not buffer its *input*, so the outer `BufWriter` is genuinely useful. ✓
- **Edge cases:** empty input → 0 records, empty file (tested); truncated tail dropped (tested); CRLF preserved (tested); lowercase uppercased pre-`C→T` (tested). ✓
- **Idiom:** `to_ascii_uppercase` byte-wise, `position`/manual run-collapse in `fix_id`, let-chains (`if let Some(s) = ... && s > 0 && ...`). Clean, clippy-clean. ✓
- **Non-UTF-8 IDs:** `fix_id` is byte-level (`&[u8]`), so non-UTF-8 IDs survive (Perl is byte-oriented here too). ✓ But the **basename** derivation requires UTF-8: `input.file_name().and_then(|n| n.to_str())` (convert.rs:140) errors on a non-UTF-8 *filename*. Perl would happily handle non-UTF-8 paths. **L6 [Low]** — extremely unlikely for real read files; flagging for completeness. Could use `to_string_lossy` or operate on `OsStr`/bytes if ever needed.

### 5. Test quality

Strong. The convert tests assert **real** behavior, not just non-panic:
- `golden_plain_fastq` asserts exact `GOLDEN_OUT` bytes + name + count. The spec-derived nature of the golden is **honestly caveated** in the constant doc-comment (convert.rs:286-288) and in PLAN §13 deviations — and the Phase-10 oxy gate (full Perl `bismark` run) is the authoritative Perl-generated oracle. This deferral is **reasonable**: `biTransformFastQFiles` is not callable standalone, so a true Perl-generated golden requires the full pipeline, which Phase 10 owns. The hand-computed transform is simple enough (ws→`_`, `uc`, `C→T`, verbatim) to trust at unit scale, and I independently re-derived `GOLDEN_OUT` from `GOLDEN_IN` against the Perl source — it matches.
- Covers the §9 table: golden, multi-member gzip, gzip-output-decompresses-to-plain, skip+upto selection, falsy-`0`, skip-bypasses-record1-sanity, record-1-vs-N>1 malformed (the verbatim-`GARBAGE` assertion is a nice over-validation guard), icpc e2e, truncated tail, empty input, prefix naming. ✓

**Test gaps (none blocking):**
- **T1 [Low]** No test for `--prefix foo.` trailing-dot behavior (the M1 gap) — add once M1 is decided.
- **T2 [Low]** No test for CRLF *end-to-end through the file loop* — `chomp_strips_only_newline` covers the helper and `convert_seq_uc_then_c_to_t` covers `\r` survival, but no `run()` integration with a full CRLF record asserting all four lines retain `\r\n` in the output. Plan §9 #4 lists CRLF; the unit coverage is adequate but a one-line file-level CRLF case would close it cleanly.
- **T3 [Low]** No test asserting a record whose `seq` line lacks a trailing `\n` (final record, no newline) is written verbatim without an injected `\n`. The `truncated_tail_record_dropped` test drops the fragment; a *complete* 4-line record with no final newline would confirm the verbatim-tail path. Minor.
- **T4 [Low]** `maximum_length_cutoff` guard (convert.rs:213-217) has **no test** — understandable since the plan calls it inert, but if M2 is fixed (validate-or-reject), add a unit test for the guard firing when set, to lock the mm2-phase hook.

---

## Recommendations (prioritized)

| Pri | Finding | Action |
|-----|---------|--------|
| **Critical** | — | none |
| **High** | — | none |
| **Medium** | **M1** prefix trailing-dot not trimmed (Perl 8238) | Trim in `resolve_output` (config.rs:404) so temp + later output names match Perl; add T1. |
| **Medium** | **M2** `--mm2_maximum_length` w/o mm2 silently accepted (Perl 8333 dies → silent record-drop on v1) | Reject in `resolve()` when cutoff set and aligner ≠ minimap2; keep the convert guard as the mm2 hook; add T4. |
| **Low** | **L4** `--skip/--upto/--gzip/--prefix` still in `deferred_flags` though Phase 2 honors them | Revisit membership (at least `--skip`/`--upto`). |
| **Low** | **L2** `seqID_contains_tabs` discarded; feeds REPORT later (Perl 2142) | Plumb the flag to the report phase rather than re-derive. |
| **Low** | **L1** Perl STDERR progress/warning lines absent | Out of gate; confirm intentional. |
| **Low** | **L5/L6/L3** gz-detection sources, non-UTF-8 basename, deferred-mode exit semantics | Note; no action needed for Phase 2. |
| **Low** | **T2/T3** file-level CRLF + verbatim-no-final-newline integration tests | Add for completeness. |

**Bottom line:** the wired conversion path is byte-faithful to Perl and well-tested; M1 and M2 are the only findings that can produce a Perl-divergent result on inputs the v1 CLI accepts, and neither corrupts the *converted record content* — M1 mis-names the (transient) temp file but bites later-phase output names, M2 lets a Perl-fatal flag silently drop records. Both are cheap to fix now.
