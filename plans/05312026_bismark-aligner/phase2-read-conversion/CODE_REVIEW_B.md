# CODE REVIEW B — Phase 2: Read conversion (C→T, FastQ SE directional)

**Reviewer:** B (independent, fresh context)
**Date:** 2026-06-01
**Scope:** `convert.rs` (new) + wiring in `lib.rs`, `config.rs` (`ReadProcessing`), `cli.rs` (`--icpc` doc),
`Cargo.toml` (flate2), `tests/cli.rs`. Audit-only — **no code modified** (parallel dual-review, no file races).
**Gate:** byte-identical converted temp FastQ vs Perl `bismark` v0.25.1 `biTransformFastQFiles` (5489–5651) + `fix_IDs` (6235–6246).

## Summary

The conversion logic is **faithful to the Perl source**. I verified the per-record transform line-by-line
against Perl 5577–5634: `count++` before skip/upto (5588), `chomp`(\n only)→`fix_IDs`→re-append `\n`
(5584–86), `uc` then `tr/C/T/` (5597/5625), `id2`/`qual` written verbatim (5626), `last unless (all 4)`
truncated-tail drop (5582), the count==1-only FastQ sanity sitting *after* skip (the documented Perl quirk,
5612–16), and falsy-`0` skip/upto (5590/5593). `fix_id` matches both branches of `fix_IDs` (collapse-run vs
truncate). Temp naming (`${prefix.}?${basename}_C_to_T.fastq[.gz]`, extensions kept, raw `${temp_dir}${name}`
concat) and multi-member gzip input are correct. Tests are well-targeted: **34 unit + 15 integration pass;
`clippy --all-targets -D warnings` clean.**

**One real correctness/UX bug:** the Phase-1 `deferred_flags()` notice still advertises `--skip`, `--upto`,
`--gzip`, and `--prefix` as *"recognised but not yet active … wired in a later phase"* — but Phase 2 now
**actively applies all four** to the converted temp file for the v1 SE-directional spine. A user passing
`--skip 2` will be told it is inert while it silently takes effect. The rest are Low/informational (a couple
of redundant computations, Drop-based gzip finalization, per-call temp-dir normalization).

No byte-identity divergence found. The Perl-golden deferral to the Phase-10 oxy gate is reasonable given
`biTransformFastQFiles` is not callable standalone, but it is the single most important caveat (see Medium).

---

## Issues by area

### 1. Byte-identity faithfulness (convert.rs vs Perl) — PASS

Verified line-by-line; all faithful:

| Concern | Perl | Rust | Verdict |
|---|---|---|---|
| `count++` before skip/upto | 5588 | `convert.rs:188` (before 196/202) | ✅ |
| ID: chomp `\n` only (CR kept) → `fix_id` → re-append `\n` | 5584–86 | `chomp_newline` (108–114) + `191–192` | ✅ CR preserved (test 256/276) |
| `fix_IDs` default `[ \t]+`→`_` | 6242 | `fix_id` else-branch (77–92) collapses runs | ✅ |
| `fix_IDs` `--icpc` truncate at first `[ \t]` | 6239 | `fix_id` if-branch (72–76) | ✅ |
| `uc` then `tr/C/T/` (a→A,c→T,g→G,t→T,n→N) | 5597,5625 | `seq_uc` (210) + `convert_seq_c_to_t` (98–105) | ✅ |
| `id2`/`qual` verbatim (incl. line endings) | 5626 | `233–234` (raw bytes) | ✅ |
| max-length guard on uc seq incl. `\n` (mm2-only) | 5598–5604 | `213–217` (`seq_uc.len()` incl. newline) | ✅ inert on Bowtie 2 |
| tab-in-id detect after fix_id, before sanity | 5607–09 | `221` | ✅ (computed, see L-1) |
| FastQ sanity only when count==1, after skip | 5612–16 | `224–228` | ✅ skip-bypass quirk preserved (test 382) |
| `last unless (id and seq and id2 and qual)` | 5582 | `185–187` (any `read_until`==0 → break) | ✅ truncated tail dropped (test 421) |
| falsy `0` skip/upto | 5590,5593 (`if($skip)`/`if($upto)`) | `196–200`,`202–206` (`s>0`/`u>0` guards) | ✅ (test 376) |
| temp name: extensions kept + suffix; prefix `p.<base>` | 5514–5546 | `140–151`,`143–146` | ✅ |
| raw `${temp_dir}${name}` concat (not Path::join) | 5548/5554 | `152` (`format!`) | ✅ |
| `.gz` input → gunzip | 5500–05 | `157–161` `MultiGzDecoder` (multi-member, test 348) | ✅ |

**`upto` returned-count semantics (subtle, correct):** Perl `last if ($count > $upto)` runs *after*
`++$count`, so the record that trips the limit is counted-but-not-written; the returned `$count` is
`upto+1`. Rust mirrors this (`count` incremented at 188, break at 205, returns the incremented value).
Faithful — but **not asserted** by any test (see M-2).

### 2. `convert_reads` gate in `lib.rs` — PASS (no silent half-support)

`lib.rs:70–92` matches **only** `(SingleEnd, Directional, FastQ)` and routes every other
layout/library/format combination to an explicit "wired in a later phase" STDERR note (no alignment, no
partial output). This is the correct fail-visible posture for a phased port — non-directional/pbat/PE/FastA
are not silently dropped or mis-converted. Per-file loop over `reads` is correct for comma-separated SE.

### 3. `ReadProcessing` seam — PASS

`config.rs:110–120` carries **only** the new fields (`skip`/`upto`/`icpc`/`maximum_length_cutoff`);
`gzip`/`prefix`/`temp_dir` are read from `OutputTarget` via `ConvertOptions::from_config`
(`convert.rs:45–54`) — **single source of truth, no duplication**, exactly as the plan §8 mandates.
Populated inside `resolve()` (`config.rs:164–169`), so Phase-1 tests (which build `RunConfig` only via
`resolve`) are undisturbed. Additive and correct.

### 4. Rust correctness / quality — mostly PASS (see Low items)

- Error handling: `?` throughout; `AlignerError::Validation` for the FastQ-format die and the
  un-derivable-basename case; `#[from] io::Error` covers open/create/read/write. Good.
- Buffering: `BufReader`/`BufWriter`, reused per-record buffers (`id/seq/id2/qual` cleared each iter). Good.
- `Box<dyn BufRead>` / `BufWriter<Box<dyn Write>>` dynamic-dispatch choice is idiomatic and appropriate for
  a non-hot path (not alignment).

### 5. Test quality — PASS (one caveat + one small gap)

Tests assert real behavior and cover the §9 table: golden plain, gzip-input-matches-plain, multi-member
gzip, gzip-output-decompresses-to-plain, skip+upto selection, falsy-`0`, skip-bypasses-sanity,
record-1-malformed-dies vs record-N-passes-verbatim, `--icpc` e2e, truncated tail, empty input, `--prefix`
naming. The integration test (`tests/cli.rs:142`) asserts the temp file lands in `--temp_dir`. Strong.

---

## Recommendations (prioritized)

### High

**H-1 — Stale `deferred_flags()` advertises now-live options.** `config.rs:271,272,281,282` still push
`--skip`, `--upto`, `--gzip`, `--prefix` into the "recognised but not yet active in this build (wired in a
later phase)" notice emitted by `lib.rs:53–58`. Phase 2 **actively wires all four** for the SE-directional
spine: `--skip`/`--upto` gate records (`convert.rs:196–207`), `--gzip` controls the temp extension + encoder
(`147–151`,`165–169`), `--prefix` prepends the name (`143–146`). A user passing `--skip 2` on the v1 spine
is told it is inert while it silently takes effect — directly contradicts the notice's own rationale
("rather than silently accepting and ignoring them"). **Fix:** drop `--skip`/`--upto`/`--gzip`/`--prefix`
from `deferred_flags` (keep `--basename`, which overrides the *final output* name, not the temp file, and is
genuinely still deferred). Note the existing `deferred_flag_emits_notice` test uses `--unmapped`, so it is
unaffected; consider adding an assertion that `--skip`/`--gzip` are **not** in the notice on the SE spine.
*(Caveat: confirm against the Phase-2 PR scope — if the team intends the notice to mean "active only on the
SE-directional spine, still deferred elsewhere", that nuance is not expressed and the current blanket notice
is still misleading. Recommend the simple removal for the now-wired flags.)*

### Medium

**M-1 — Golden is spec-derived, not Perl-generated (documented, but it is the load-bearing caveat).**
`convert.rs:289–292` `GOLDEN_IN`/`GOLDEN_OUT` are hand-computed from the Perl transform, not emitted by Perl
v0.25.1. PLAN §13 documents this and defers the authoritative end-to-end check to the Phase-10 oxy gate
(reasonable — `biTransformFastQFiles` is not standalone-callable). **This is acceptable for Phase 2**, but
the byte-identity acceptance gate is therefore **not actually exercised yet** for this module — it rests
entirely on a human transcription of the Perl rules. Recommendation: before the Phase-10 gate, add (or at
least track) a hermetic Perl-oracle step that runs a real `bismark` with `--temp_dir` on a tiny fixture and
`cmp`s the retained `_C_to_T.fastq` — the temp file is normally deleted, so the harness must capture it
(e.g. interrupt after conversion, or a debug keep-temp). Flag explicitly to the user that **no
Perl-generated byte comparison has run for convert.rs to date.**

**M-2 — No test pins the `upto` returned-count value.** `skip_and_upto_select_records` (`convert.rs:366`)
asserts *which records* are written but never asserts `cr.count`. The subtle Perl semantics (count =
`upto+1` because the limit-tripping record is counted before the break) is faithful in code but unguarded —
a future refactor that moves `count += 1` after the upto check would silently diverge. **Fix:** add
`assert_eq!(cr.count, 5)` (for `--upto 4`) to lock the Perl `last if ($count > $upto)` ordering. Low effort,
protects a genuinely subtle behavior.

### Low

**L-1 — `_id_has_tab` computes then discards.** `convert.rs:221` runs `fixed_id.contains(&b'\t')` and binds
to `_id_has_tab`. The comment correctly notes it can never fire after `fix_id` (default collapses tabs to
`_`; `--icpc` truncates at the first tab) and that Perl's `$seqID_contains_tabs` only drives a later
warning. The kept-for-faithfulness intent is fine, but the byte-scan runs on every record for no effect.
Consider gating it behind the future warning wiring or dropping it until that phase needs it (purely a
micro-efficiency / clarity note — behavior is correct).

**L-2 — Double uppercase on the write path.** `convert.rs:210` computes `seq_uc = seq.to_ascii_uppercase()`,
then `232` calls `convert_seq_c_to_t(&seq_uc)` which **re-uppercases** internally (helper lines 100–101).
Idempotent → correct, but the seq is upcased twice per record. The redundancy exists because the unit test
(`267`) calls the helper on raw lowercase. Optional: have the helper assume pre-uppercased input (rename to
`c_to_t` and uc only at the call site), or accept the redundancy as a clarity/test-convenience tradeoff.

**L-3 — gzip stream finalized via Drop, swallowing finish errors.** `convert.rs:237–238` does
`writer.flush()` then `drop(writer)`; the `GzEncoder` is finalized only by its `Drop` impl (`try_finish`),
whose error is discarded. Tests pass (stream is valid), but a write error during the final gzip block would
be silently lost. Best practice: explicitly `into_inner()` / `finish()` the encoder and propagate the
`Result`. Low (non-hot path, transient temp file).

**L-4 — Temp-dir normalization runs per input file.** `temp_dir_prefix` (`convert.rs:120–131`) does
`create_dir_all` + `canonicalize` inside `bisulfite_convert_fastq_se`, i.e. once **per** SE read file (the
`lib.rs:73` loop), whereas Perl normalizes `$temp_dir` once in `process_command_line` (8207–8229). Idempotent
(dir already exists on calls 2..N; canonicalize is pure) so behaviorally identical, and the documented
deviation (PLAN §13: normalization in convert, not resolve, to keep `resolve` side-effect-free) is sound.
Minor: for multi-file SE this re-stats the FS N times. Acceptable; noted for completeness. Also note
`MAIN_SEPARATOR` is used rather than a literal `/` — on the Linux byte-identity platform it is `/`, matching
Perl, and the gate is on file *content* not the path string, so no divergence.

**L-5 — `from_config` is a free method, not the `From` trait the plan proposed.** PLAN §4 suggested a
`From<&RunConfig>`/builder; the impl uses an inherent `ConvertOptions::from_config` (`convert.rs:45`). This
is a fine, arguably clearer choice — noting only that it is a (trivial, documented-style) deviation from the
plan's proposed shape. No action needed.

---

## Verdict

**APPROVE with one High fix (H-1) before this rides to the next phase / PR**, plus the Medium items tracked.
The conversion engine is byte-faithful to Perl `biTransformFastQFiles`/`fix_IDs` as far as static review and
the spec-derived golden can establish; the real Perl-generated byte gate remains owed at Phase 10 (M-1). No
Critical issues. Tests + clippy green.
