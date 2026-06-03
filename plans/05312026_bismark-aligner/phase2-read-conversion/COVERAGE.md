# Plan Coverage Report

**Mode:** B (code vs. plan — design plan treated as the spec)
**Plan(s):** `phase2-read-conversion/PLAN.md`
**Date:** 2026-06-01
**Verdict:** COMPLETE

## Summary

- Total items: 35
- DONE: 33
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 2 (both documented in §13 and verified real)

Tests run: `cargo test -p bismark-aligner` → **34 unit + 15 integration + 0 doc — all green, 0 failed** (matches §13's claimed counts exactly).

## Coverage ledger

### §3 Behavior steps

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Temp-file name: basename keeps extensions, `<prefix.>?<basename>` + `_C_to_T.fastq[.gz]` | §3.1 | DONE | `convert.rs:140-151` — `file_name()`, `format!("{p}.{basename}")`, suffix branch on `gzip`. Tests `golden_plain_fastq`, `prefix_prepended_to_name`, `gzip_output_decompresses_to_plain`. |
| 2 | Full path = raw concat `<temp_dir><name>` (NOT `Path::join`) | §3.1 | DONE | `convert.rs:152` `format!("{}{name}", temp_dir_prefix(...))` — raw string concat. |
| 3 | temp_dir normalization (absolute + trailing `/` when set; empty → CWD) | §3.1, §5.0 | DEVIATED (documented) | Lives in `convert::temp_dir_prefix` (`convert.rs:120-131`), not `resolve()`. §13 documents this + rationale (filesystem side-effect `create_dir_all`+`canonicalize` doesn't belong in pure config). Behaviourally identical: empty→`""`, else canonicalize + trailing `MAIN_SEPARATOR`. Verified real. |
| 4 | Open input: `.gz` → MultiGzDecoder, else raw; buffered | §3.2 | DONE | `convert.rs:156-161` — `ends_with(".gz")` branch, `BufReader`. |
| 5 | Per-record 4-line loop; stop when any line missing (clean EOF / truncated tail dropped) | §3.3 | DONE | `convert.rs:174-187` — 4× `read_until(b'\n')`; `if n1==0\|\|n2==0\|\|n3==0\|\|n4==0 { break }`. Test `truncated_tail_record_dropped`, `empty_input_yields_empty_output`. |
| 6 | Loop step 1: `count += 1` before skip/upto | §3.3.1 | DONE | `convert.rs:188` `count += 1;` precedes the skip/upto block (196-207). |
| 7 | Loop step 2: ID chomp `\n` only (CR kept) → fix_id → re-append `\n` | §3.3.2 | DONE | `convert.rs:191-192` `fix_id(chomp_newline(&id), ...)` then `push(b'\n')`. `chomp_newline` (108-114) strips only `\n`. Tests `chomp_strips_only_newline`, `fix_id_default_…` (CR kept). |
| 8 | Loop step 3: skip (`continue` while `count<=skip`) / upto (`break` once `count>upto`), falsy-0 disables | §3.3.3 | DONE | `convert.rs:196-207` — `if let Some(s)=skip && s>0 && count<=s { continue }`, `if let Some(u)=upto && u>0 && count>u { break }`. Tests `skip_and_upto_select_records`, `falsy_zero_disables_skip_and_upto`. |
| 9 | Loop step 4: ASCII-uppercase whole seq line (newline preserved) | §3.3.4 | DONE | `convert.rs:210` `seq.to_ascii_uppercase()`; convert helper preserves `\n`/`\r`. |
| 10 | Loop step 5: max-length guard (`maximum_length_cutoff`), mm2-only/inert on v1 | §3.3.5 | DONE | `convert.rs:213-217` guarded `if let Some(cutoff)=max && len>cutoff { continue }`; never set on Bowtie 2 spine. Guard present (faithfulness hook). |
| 11 | Loop step 6: tab-in-id detection flag (non-byte-affecting) | §3.3.6 | DONE | `convert.rs:221` `let _id_has_tab = fixed_id.contains(&b'\t');` — flag computed, unused (matches Perl warn-only). |
| 12 | Loop step 7: FastQ sanity only when `count==1` (id starts `@`, id2 starts `+`), else die | §3.3.7 | DONE | `convert.rs:224-228` `if count==1 && (!starts_with(@) \|\| !id2.starts_with(+)) { Err(...) }`. Tests `record1_malformed_errors_but_record_n_passes`, `skip_bypasses_record1_sanity`. |
| 13 | Loop step 8: `C→T` on uppercased seq; write `id + seq + id2 + qual` (id2/qual verbatim) | §3.3.8 | DONE | `convert.rs:231-234` writes `fixed_id`, `convert_seq_c_to_t(&seq_uc)`, `id2`, `qual` (last two verbatim). |
| 14 | Close + return temp path (name + full path) | §3.4 | DONE | `convert.rs:237-243` — flush, drop, return `ConvertedReads { name, path, count }`. |
| 15 | `fix_IDs`: default `[ \t]+`→single `_`; `--icpc` truncate at first space/tab; leading `@` kept | §3 fix_IDs | DONE | `convert.rs:71-93` byte-level run-collapse / position-truncate. Tests `fix_id_default_underscores_whitespace_runs`, `fix_id_icpc_truncates_at_first_whitespace`. |

### §3 Edge cases

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 16 | gzip input → identical decompressed conversion | §3 edge | DONE | Test `gzip_input_matches_plain`. |
| 17 | Empty input → 0 records, empty temp, no error | §3 edge | DONE | Test `empty_input_yields_empty_output` (count 0, output empty). |
| 18 | Truncated final record dropped | §3 edge | DONE | Test `truncated_tail_record_dropped` (count==1, one record out). |
| 19 | CRLF preserved (chomp removes `\n` only; uc/C→T keep `\r`; verbatim id2/qual) | §3 edge | DONE | `chomp_strips_only_newline` (`id\r\n`→`id\r`), `convert_seq_uc_then_c_to_t` (`ccCC\r\n`→`TTTT\r\n`), `fix_id` keeps CR. |
| 20 | lowercase bases uppercased before C→T (`c`→`T`) | §3 edge | DONE | `convert_seq_c_to_t` uc-then-map; tests cover `acgt`→`ATGT`. |
| 21 | `--gzip` temp: decompressed content gated, raw bytes not | §3 edge | DONE | Test `gzip_output_decompresses_to_plain` gunzips + compares to `GOLDEN_OUT`. |
| 22 | skip ≥ total → empty; upto=0 / skip=0 falsy disables | §3 edge | DONE | `falsy_zero_disables_skip_and_upto`; skip+upto selection in `skip_and_upto_select_records`. |

### §5 Implementation outline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 23 | Prereq: add `flate2` (pinned) to Cargo.toml | §5.0 | DONE | `Cargo.toml:28` `flate2 = "=1.1.9"` (matches genome-prep pin). |
| 24 | Prereq: correct Phase-1 `--icpc` doc comment | §5.0 | DONE | `cli.rs:216-217` now reads "Truncate read IDs at the first space/tab … (Bismark issue #236; affects `fix_IDs`)" — no longer "HISAT2/deferred". |
| 25 | Prereq: additive `ReadProcessing` sub-struct on `RunConfig`, populated in `resolve()` | §5.0 | DONE | `config.rs:110-120` struct (skip/upto/icpc/maximum_length_cutoff only); `config.rs:124-147` field on `RunConfig`; `config.rs:164-169` populated in `resolve()`. |
| 26 | Prereq: temp_dir normalization (absolute + trailing `/`) | §5.0 | DEVIATED (documented) | See item 3 — placed in `convert::temp_dir_prefix`, not `resolve()`. Documented in §13. |
| 27 | `ConvertOptions` (+ `From`/builder) and `ConvertedReads` | §5.1 | DONE | `convert.rs:27-66` both structs; `ConvertOptions::from_config` (`45-54`) reads gzip/prefix from `output`, rest from `read_processing` (single source of truth per §8). |
| 28 | `fix_id(id, icpc)` helper, unit-tested | §5.2 | DONE | `convert.rs:71-93` + 2 unit tests. |
| 29 | `convert_seq_line` (uc then C→T, newline preserved), unit-tested | §5.3 | DONE | `convert_seq_c_to_t` (`convert.rs:98-105`) + unit test. (Renamed `convert_seq_line`→`convert_seq_c_to_t`; cosmetic, both helpers present.) |
| 30 | `bisulfite_convert_fastq_se` (open gz/plain → 4-line loop → skip/upto → sanity → write) | §5.4 | DONE | `convert.rs:134-244`. Signature matches §4 proposal. |
| 31 | Wire into `lib::run` SE-directional path; print temp path; keep Phase-1 summary | §5.5 | DONE | `lib.rs:61` `convert_reads(&config)`; `lib.rs:68-94` matches `(SingleEnd, Directional, FastQ)`, prints "Created C->T converted …"; other modes print later-phase note; `config.summary()` still emitted (`lib.rs:60`). |
| 32 | Tests: unit (fix_id, convert_seq, CRLF, lowercase) + integration golden | §5.6 | DONE | All present (see §9 mapping below). |

### §9 Validation table

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| V1 | `fix_id` default | §9 #1 | DONE | `fix_id_default_underscores_whitespace_runs`. |
| V2 | `fix_id --icpc` | §9 #2 | DONE | `fix_id_icpc_truncates_at_first_whitespace`. |
| V3 | `convert_seq_line` `ACGTacgtN\n`→`ATGTATGTN\n` | §9 #3 | DONE | `convert_seq_uc_then_c_to_t`. |
| V4 | CRLF preserved | §9 #4 | DONE | `chomp_strips_only_newline` + `convert_seq_uc_then_c_to_t` (`\r\n`). |
| V5 | Golden temp file (committed Perl-v0.25.1-derived golden) | §9 #5 | DEVIATED (documented) | `golden_plain_fastq` vs `GOLDEN_IN`/`GOLDEN_OUT` constants (`convert.rs:289-292`). §13 documents these are **spec-derived** (hand-computed from the verified Perl transform), with the authoritative Perl-generated end-to-end gate **deferred to Phase 10 oxy** (since `biTransformFastQFiles` isn't standalone-callable). Exercises space-ID, tab-ID, lowercase, non-bare `+`-line — matches the plan's required cases. Verified real. |
| V6 | gzip input single + multi-member | §9 #6 | DONE | `gzip_input_matches_plain` + `multi_member_gzip_input` (two concatenated members). |
| V7 | `--gzip` temp output decompresses to plain | §9 #7 | DONE | `gzip_output_decompresses_to_plain`. |
| V8 | skip/upto + count | §9 #8 | DONE | `skip_and_upto_select_records` (skip 2, upto 4 → r3,r4). |
| V9 | falsy `0` semantics | §9 #9 | DONE | `falsy_zero_disables_skip_and_upto`. |
| V10 | `--skip` bypasses sanity | §9 #10 | DONE | `skip_bypasses_record1_sanity` (malformed record 1, `--skip 1`, no error). |
| V11 | `--icpc` end-to-end | §9 #11 | DONE | `icpc_truncates_ids_end_to_end` (`@r1 comment`→`@r1`). |
| V12 | malformed record 1 vs N>1 | §9 #12 | DONE | `record1_malformed_errors_but_record_n_passes` (rec1 dies; rec5/GARBAGE passes verbatim). |
| V13 | malformed FastQ record 1 → die | §9 #13 | DONE | Same test (`is_err()` on non-`@` first line). |
| V14 | empty / truncated-tail | §9 #14 | DONE | `empty_input_yields_empty_output` + `truncated_tail_record_dropped`. |

### §13 documented deviations — reality check

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 33 | Deviation A: temp_dir normalization in `convert::temp_dir_prefix`, not `resolve()` | §13 | DONE (verified) | Confirmed at `convert.rs:120-131`; `resolve()` (`config.rs:164-169`) does NOT normalize temp_dir — stores raw `output.temp_dir`. Deviation is real and as described; behaviourally identical. |
| 34 | Deviation B: V5 golden is spec-derived `GOLDEN_IN`/`GOLDEN_OUT`, Perl-golden deferred to Phase 10 | §13 | DONE (verified) | Confirmed `convert.rs:289-292` constants are hand-derived (doc comment says so); no Perl-generated fixture committed; Phase-10 oxy gate is the authoritative end-to-end check per the memory + epic. As described. |
| 35 | No UNdocumented deviation found | audit | DONE | The only naming nuance (`convert_seq_line`→`convert_seq_c_to_t`) is cosmetic and the helper exists with the specified behavior; not a behavioral deviation. The Phase-1 `deferred_flags()` still lists `--skip/--upto/--gzip/--prefix` even though Phase 2 now uses them — see note below; this is a pre-existing Phase-1 cosmetic STDERR notice, not gated and not in scope of any Phase-2 task. |

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| fix_id_default_underscores_whitespace_runs | src/convert.rs | PASS |
| fix_id_icpc_truncates_at_first_whitespace | src/convert.rs | PASS |
| convert_seq_uc_then_c_to_t | src/convert.rs | PASS |
| chomp_strips_only_newline | src/convert.rs | PASS |
| golden_plain_fastq | src/convert.rs | PASS |
| gzip_input_matches_plain | src/convert.rs | PASS |
| multi_member_gzip_input | src/convert.rs | PASS |
| gzip_output_decompresses_to_plain | src/convert.rs | PASS |
| skip_and_upto_select_records | src/convert.rs | PASS |
| falsy_zero_disables_skip_and_upto | src/convert.rs | PASS |
| skip_bypasses_record1_sanity | src/convert.rs | PASS |
| record1_malformed_errors_but_record_n_passes | src/convert.rs | PASS |
| icpc_truncates_ids_end_to_end | src/convert.rs | PASS |
| truncated_tail_record_dropped | src/convert.rs | PASS |
| empty_input_yields_empty_output | src/convert.rs | PASS |
| prefix_prepended_to_name | src/convert.rs | PASS |
| happy_path_resolves_and_prints_config (asserts "Created C->T converted" + temp file present) | tests/cli.rs | PASS |
| deferred_flag_emits_notice / pbat_genome_as_positional_resolves / sam_output_is_deferred (+ 12 more) | tests/cli.rs | PASS |

Full suite: 34 unit + 15 integration + 0 doc, all green (0 failed).

## Verdict

**COMPLETE.** Every §3 loop step (1–8), §3 edge case, §5 outline step (0–6), and §9 validation (#1–#14) maps to real code with a corresponding passing test. The two deviations are real, documented in §13, and behaviourally faithful:

1. **temp_dir normalization** lives in `convert::temp_dir_prefix` (not `resolve()`) — justified by the filesystem side-effect; output is identical.
2. **Golden V5** uses spec-derived `GOLDEN_IN`/`GOLDEN_OUT` constants with the authoritative Perl-generated end-to-end byte-identity gate deferred to Phase 10 oxy (`biTransformFastQFiles` is not standalone-callable).

Prereq checks all confirmed: `flate2 = "=1.1.9"` in `Cargo.toml`; `--icpc` doc corrected in `cli.rs` (issue #236 / `fix_IDs`, no longer "HISAT2/deferred"); `ReadProcessing` is on `RunConfig` and populated in `resolve()`.

**One minor, out-of-scope observation (not a gap):** `config::deferred_flags()` (`config.rs:264-287`) still lists `--skip`, `--upto`, `--gzip`, and `--prefix` in its "recognised but not yet active" STDERR notice, even though Phase 2 now actively consumes them. This is a stale Phase-1 cosmetic notice (STDERR only, not byte-gated, no test asserts their *absence*); no Phase-2 task in the PLAN requires updating it. Flagging for awareness only — it does not affect the COMPLETE verdict.
