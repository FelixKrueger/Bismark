# Plan Coverage Report — Phase B

**Mode:** A+B (design→impl + code→impl), scoped to Phase B
**Plan(s):** `SPEC.md` (rev 1) — Phase B = §11 row B (+ §5/§6/§8/§9/§13 Phase-B items); `IMPL_phase-B.md` (15-row checklist + Tasks T1–T9)
**Code:** worktree `~/Github/Bismark-nome`, crate `rust/bismark-nome-filtering`
**Date:** 2026-05-31
**Verdict:** COMPLETE

## Summary

- Total ledger items: 15 (checklist) + 9 (tasks) + 9 (§12 test-matrix items) = audited together below.
- DONE: 15/15 checklist, 9/9 tasks, 9/9 §12 matrix items.
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented + matched, treated as DONE): 4 (VS-N warn-skip dropped-as-impossible; VS-pad folded into `main`; `EmptyInput`-after-header D4; Phase-A `valid_invocation` test replaced).

**Build / test / clippy (worktree, sandbox-disabled):**
- `cargo build --workspace` → Finished, clean. All siblings still pinned at `bismark-io = 1.0.0-beta.8` (the SPEC P7 no-version-bump promotion holds; workspace resolves).
- `cargo test -p bismark-nome-filtering` → **63 tests pass, 0 fail** (49 unit incl. lib+cli+filename+substr+nome, 6 cli_phase_a, 7 golden_phase_b, 1 doctest).
- `cargo clippy -p bismark-nome-filtering -p bismark-io --all-targets -- -D warnings` → clean.

## Coverage ledger — IMPL_phase-B 15-row checklist (Mode B)

| # | Plan item (SPEC §) | Task | Status | Notes |
|---|--------------------|------|--------|-------|
| 1 | `revcomp` via `tr/ACTG/TGAC/` — A↔T, C↔G, identity on N/other (P3) | T1 | DONE | `complement_base`/`revcomp` in `nome.rs:42-57`; matches Perl `:276,281`. Tests `complement_maps_actg_passes_other`, `revcomp_reverses_then_complements`. |
| 2 | Context classify `^CG`→CG / `^C.G$`→CHG / `^C..$`→CHH / else→None; CNG→CHG, CNN→CHH | T2 | DONE | `classify` `nome.rs:72-86` (CG→CHG→CHH→None order); matches Perl `:291-303`. Test `classify_contexts` incl. `CNG`/`CNN`/`NCG`/`GCA`/len≠3. |
| 3 | `cytosine_lookup`: byte-scan (not regex), `pos=i+1`, fwd-C `tri=ext[pos+1..]/up=ext[pos..]`, rev-G `tri=ext[pos-1..]+revcomp/up=ext[pos..]+revcomp`, `len<3` skip, `g=pos+offset-1` | T3 | DONE | `nome.rs:99-192`. Byte scan over `seq`; `pos=i+1`; fwd/rev tri+upstream offsets verified against Perl `:262-285`; `tri.len()<3` skip (`:287`); `g=pos+offset-1` coverage check (`:305`). Tests `cg_acg_*`, `reverse_g_strand_cpg_tcg_*`, `position_not_covered_*`. |
| 4 | NOMe filter + tally keyed on col-2 `state` (P5): CG⇒{z,Z}+up∈{ACG,TCG}; CHG⇒{x,X}+up^GC; CHH⇒{h,H}+up^GC | T3 | DONE | `nome.rs:158-181`. Tally keys strictly on `state` (`+`/`-`), not call case — `tally_keys_on_state` covered by `cg_acg_unmethylated_lowercase_call_counts_unmeth_cg` (call `z`, state `-`→unmeth). GpC via `is_gpc` (`^GC`). Matches Perl `:312-376`. A-I3 structure preserved (no spurious early-out). |
| 5 | Yacht parse: 8 TAB fields, `^Bismark` skip, gz-aware input (`MultiGzDecoder`) | T4/T6 | DONE | Parse + `^Bismark` skip `nome.rs:262-278`; gz-aware open in `write_report` `nome.rs:320-334` (`MultiGzDecoder` on `.gz` suffix). Tests `skips_bismark_header_*`, `golden_gz_input_matches_plain`. |
| 6 | Consecutive-ReadID grouping; first line sets start/end/chr; flush-on-change + EOF flush; shared flush routine FLUSHES ONLY, seed in loop body (P17) | T4 | DONE | `per_read_filtering` `nome.rs:253-305`. Flush via `process_read` (flush-only), re-seed in the `_ =>` arm; EOF flush calls same routine, no reseed. Matches Perl `:105-168` + `:177-219`. Tests `skips_bismark_header_and_groups_consecutive_in_order`, `non_consecutive_same_id_is_two_reads`. |
| 7 | Same-position-within-read = last wins (unconditional insert, not `or_insert`) (P13) | T4 | DONE | `read.insert(pos, ...)` unconditional `nome.rs:283,292`. Test `same_position_within_read_last_wins` (hard assert: `+Z` then `-z` @ same pos → unmeth). |
| 8 | Length calc; suitability guard uses `last_start` for BOTH strands (P2); unknown-chr ⇒ chr_len=0 ⇒ skip | T5 | DONE | `process_read` `nome.rs:208-225`. Guard `(start-2>1) && (chr_len >= start-2+length+4)` with `start` for both strands; unknown chr → `unwrap_or(&[])` → len 0. Matches Perl `:117-132`. Tests `unknown_chromosome_emits_nothing`, `guard_ge_boundary_suitable_and_one_less_not`. |
| 9 | seq/ext via `perl_substr` fwd(`start-1`/`start-3`) & rev(`end-1`/`end-3`); rev `end∈{1,2}`⇒all-zero; fwd `start≤3`⇒NO line (P1); guard `>=` boundary | T5 | DONE | `nome.rs:227-244`. fwd/rev extraction matches Perl `:133-156`. `perl_substr` (`substr.rs`) handles negative offset (rev `end-3`<0) and `start==L` without panic. Tests `forward_read_start_le_3_emits_no_line`, `reverse_read_end_1_emits_all_zero_line`, `guard_ge_boundary_suitable_and_one_less_not`. |
| 10 | Output: `GzEncoder<BufWriter<File>>`+`Compression::default()`; header FIRST; data line `id\tchr\toffset\tend\t<4 counts>`; offset/end ASCENDING (P9); `finish()` | T6 | DONE | `write_report` `nome.rs:311-338` (`GzEncoder` + `Compression::default()`, `finish()`); line via `cytosine_lookup` writeln `nome.rs:185-191`; ascending offset/end set by caller. Matches Perl `:74-77,389`. (Note: writer wraps `File` directly, not `BufWriter<File>` — see Deviations §D-min; behaviour-identical, decompressed bytes match.) |
| 11 | D4/P11: header before read loop; empty/all-`^Bismark` ⇒ `EmptyInput` AFTER `finish()` ⇒ header-only `.gz` + non-zero exit | T6 | DONE | `write_report` writes `HEADER` before `per_read_filtering`, `finish()`es, then propagates result; `EmptyInput` raised in `per_read_filtering` `nome.rs:302`. Matches Perl `:74-78,173-175`. Tests `write_report_empty_input_writes_header_then_errors` (unit), `vs_empty_leaves_header_only_gz_and_exits_nonzero` (golden, exit code 1 + header-only gz). |
| 12 | `run()` restructure (validate+dir+infile-exists+genome, then writer+header+filter+finish); `pub mod nome` | T6 | DONE | `lib.rs:52-80` (`run`), `lib.rs:22` (`pub mod nome`). Phase-A reserving comment replaced by `write_report` call. |
| 13 | `generate_goldens.sh` (repo Perl v0.25.1) + tiny synthetic genome + `.txt`/`.txt.gz` yacht fixtures | T7 | DONE | `tests/data/phase_b/generate_goldens.sh` (runs repo Perl `NOMe_filtering`), `genome/chr1.fa`+`chr2.fa`, `main/edge/ncontext/empty.yacht.txt` + `main.yacht.txt.gz`. |
| 14 | Golden matrix (decompress-then-`assert_eq!`, emission order, un-sorted): ACG/TCG accept, GCG reject, GpC CHG/CHH, `^Bismark` skip, gz round-trip | T8 | DONE | `golden_main_multi_context` exercises ACG-accept, TCG-accept, GCG-reject, GpC-CHG, GpC-CHH, VS-pad in ONE read; `^Bismark` banner is line 1 of `main.yacht.txt` (skipped); `golden_gz_input_matches_plain`. Decompress-then-compare, un-sorted. main.golden math independently verified: meth_CG=1, unmeth_CG=2, meth_GC=1, unmeth_GC=1. |
| 15 | Edge integration: VS-edge, VS-empty/D4, VS-N, VS-guard, VS-pad, VS-crlf, unknown-chr skip, non-consecutive same-ReadID | T9 | DONE | See §12 mapping table below — every item maps to a real, passing test. |

## §12 test-matrix mapping (the explicitly-required check)

| §12 item | Mapped test(s) | Status | Notes |
|----------|----------------|--------|-------|
| VS-edge (fwd `start≤3`→no line; rev `end∈{1,2}`→all-zero) | `golden_edge_asymmetry` (`edge.golden`) + unit `forward_read_start_le_3_emits_no_line`, `reverse_read_end_1_emits_all_zero_line` | DONE | `edge.yacht.txt` has BOTH a fwd `start=2` read (no line) and a rev `start=10,end=1` read; golden = header + one all-zero line `rev_start chr1 1 10 0 0 0 0`. |
| VS-empty / D4 | `vs_empty_leaves_header_only_gz_and_exits_nonzero` (golden, exit 1 + header-only gz) + unit `write_report_empty_input_writes_header_then_errors`, `empty_or_all_bismark_input_errors_empty` | DONE | `empty.golden` decompresses to exactly the header line; exit code asserted as 1. |
| VS-N (CNG/CNN classification) | `golden_ncontext` (`ncontext.golden`) | DONE | chr2 read over `C-N-G` (pos8→CHG) + `C-N-N` (pos14→CHH); only the TCG CpG (pos4, state `-`) counts → `0 1 0 0`. The revcomp-`tri`-not-`C` warn-skip sub-branch is the **documented unreachable** case (tri[0] always `C`); N-context coverage via `ncontext` golden as planned. (`classify`→None still pinned by `classify_contexts` `NCG`/`GCA` unit cases.) |
| VS-guard (`chr_len == start-2+length+4` suitable; one less not) | `guard_ge_boundary_suitable_and_one_less_not` | DONE | Same yacht read against 11 bp (suitable, emits) and 10 bp (not) genomes; `>=` boundary pinned. |
| VS-crlf | `vs_crlf_input_matches_lf_golden` | DONE | CRLF copy of `main` → same decompressed bytes as `main.golden`. Confirms cols 2-7 unmangled; only unused col-8 carries `\r` and Rust `lines()` strips `\r\n` anyway. |
| gz-input | `golden_gz_input_matches_plain` | DONE | `main.yacht.txt.gz` → same output as plain. Fixture present on disk (138 B). |
| unknown-chr | `unknown_chromosome_read_yields_header_only_no_data_line` (golden) + unit `unknown_chromosome_emits_nothing` | DONE | Read on `chrZ` → guard fails (chr_len 0) → header-only report, exit 0 (distinct from `EmptyInput`). |
| non-consecutive same-ReadID | `non_consecutive_same_id_is_two_reads` | DONE | id r1, r2, r1 → THREE lines (r1 flushed twice). Matches consecutive-only grouping (P10). |
| emission-order (un-sorted) | `golden_main_multi_context`, `golden_ncontext`, `skips_bismark_header_and_groups_consecutive_in_order` | DONE | All compare raw bytes in input read order; no sort step exists anywhere (A-I9). |
| VS-dir (§4 path contract) | `cli::tests::dir_path_contract_resolves_input_and_output_under_dir` + every golden `run_case` (copies fixture into tempdir used as `--dir`) | DONE | Phase-A carry-forward (IMPL line 11): input = `dir.join(infile)`, output = `dir.join(derived)`, no real chdir. Golden tests exercise it end-to-end. |
| VS-pad (CpG as literal last base of fwd read) | folded into `golden_main_multi_context` | DONE (documented deviation, matched) | Verified: `main` read ends at pos32; chr1 pos32=C in `ACG` context (CpG) is counted as unmeth_CG (the 2nd unmeth). Pad bytes (chr1 pos33+) come from genome beyond the read end. No separate fixture, as documented. |

## Tasks T1–T9 (Mode B task ledger)

| Task | Goal | Status | Evidence |
|------|------|--------|----------|
| T1 | `revcomp`+`complement_base` (P3) | DONE | `nome.rs:42-57`; 2 unit tests pass. |
| T2 | Context classification | DONE | `nome.rs:59-86`; `classify_contexts` passes. |
| T3 | `cytosine_lookup` scan+extract+classify+NOMe+tally+write | DONE | `nome.rs:88-192`; 8 unit tests (incl. reverse-strand, GCG reject, GpC CHG/CHH, state-keyed tally, not-covered, call/context mismatch). Byte-faithful vs Perl `:242-391` (verified line-by-line). |
| T4 | `per_read_filtering` parse+group+flush | DONE | `nome.rs:253-305`; 5 unit tests. P13/P17 honoured. |
| T5 | `process_read` length+guard+extract+edge asymmetry | DONE | `nome.rs:199-247`; 4 unit tests. P1/P2 honoured. |
| T6 | Output wiring + `run()` restructure + `pub mod nome` | DONE | `nome.rs:307-338` (`write_report`, HEADER), `lib.rs:22,52-80`. Header-bytes + empty-input unit tests pass. |
| T7 | Fixtures + `generate_goldens.sh` | DONE | `tests/data/phase_b/**` present; script runs repo Perl. (Genome is 2 chrs `chr1.fa`/`chr2.fa`; the N-run lives in chr2 — satisfies the "short scaffold + N run" intent; goldens regenerable.) |
| T8 | Golden matrix tests | DONE | `golden_phase_b.rs:47-83`; 4 golden tests pass. |
| T9 | Edge integration tests | DONE | `golden_phase_b.rs:85-152`; VS-empty, VS-crlf, unknown-chr + (VS-edge via T8 `golden_edge_asymmetry`). All pass. |

## Mode A (design SPEC → IMPL plan) spot-check

Every Phase-B SPEC requirement maps to ≥1 IMPL task:
- §5 (yacht parse, `^Bismark` skip, gz, consecutive grouping, same-position last-wins, flush-vs-seed, empty-input) → checklist 5,6,7,11 / T4,T6. DONE.
- §6 (always-gz, header-before-loop, data-line format, ascending offset/end, un-sorted, decompressed compare) → checklist 10,11,14 / T6,T8. DONE.
- §8 (suitability guard, ±2 extraction, fwd/rev tri+upstream, revcomp, CG/CHG/CHH, NOMe ACG/TCG + GpC, tally, shared flush) → checklist 1,2,3,4,8,9 / T1–T5. DONE.
- §9 (`perl_substr` consumption — negative offset, `start==L`) → consumed by T3/T5 (`substr.rs` itself is Phase A; its 8 unit tests pass). DONE.
- §13 pitfalls P1–P3,P5,P9,P10,P11,P13,P17 (the Phase-B-relevant ones) → each has a guarding test (see ledger). DONE. (P6,P7,P14,P15,P16 are Phase-A genome/filename concerns; P4/P8 covered by tri-offset + decompress-compare.)

No design requirement is diluted or unmapped.

## Documented deviations (verified, treated as DONE/acceptable)

| Deviation | Verified |
|-----------|----------|
| VS-N warn-skip branch UNREACHABLE (tri[0] always `C`); dedicated test dropped as impossible; N-context via `ncontext` golden | Confirmed against Perl `:260-303` (loop only entered on a `C`/`G` match → tri starts with the complemented base = `C`). `classify→None` still pinned by `classify_contexts` unit cases. Correct simplification. |
| VS-pad folded into `main` golden (no separate fixture) | Confirmed: `main` read end=32, chr1 pos32=C/`ACG`/CpG counted as the 2nd unmeth_CG; pad from genome beyond read end. |
| `EmptyInput` RAISED after header written (D4) | Confirmed: `write_report` writes HEADER → `per_read_filtering`(→`EmptyInput`) → `finish()` → propagate. Header-only `.gz` lands; exit 1. |
| Phase-A `valid_invocation_loads_genome_and_exits_zero` replaced by `valid_invocation_produces_gzipped_report` | Confirmed in `cli_phase_a.rs:43-70` (now asserts the `.manOwar.txt.gz` lands). Phase-A no-output behavior superseded as documented. |

## Minor observations (NOT gaps — no action required)

- **D-min (writer wrapping):** IMPL Task 6 sketch used `GzEncoder<BufWriter<File>>`; the code uses `GzEncoder<File>` directly (`nome.rs:316-317`). `GzEncoder` already buffers its compressed output and the decompressed goldens are byte-identical, so this is behaviour-equivalent. Not a coverage gap.
- **Crate not yet committed:** the entire `bismark-nome-filtering` crate shows as untracked (`git status ?? rust/bismark-nome-filtering/`) and `main.yacht.txt.gz` is present on disk but not yet `git add`ed. The plan's commit step is explicitly "commit only when Felix asks," so this is expected. Flagging only so the gz fixture isn't lost on commit (it is required by `golden_gz_input_matches_plain`).
- **`malformed_short_line_is_skipped`** (A4 defensive-skip) is implemented + tested, matching the IMPL decision (skip, not strict-error). Documented in code.

## Test verification table

| Test | File | Status |
|------|------|--------|
| 49 `--lib` unit tests (lib/cli/filename/substr/nome) | `src/*.rs` | PASS |
| 6 CLI integration | `tests/cli_phase_a.rs` | PASS |
| 7 golden + edge integration | `tests/golden_phase_b.rs` | PASS |
| 1 doctest (`derive_manowar_name`) | `src/filename.rs` | PASS |
| **Total** | | **63 PASS / 0 FAIL** |
| clippy `-D warnings` (nome-filtering + bismark-io, all-targets) | — | CLEAN |
| `cargo build --workspace` (siblings pinned `beta.8`) | — | CLEAN |

## Verdict

**COMPLETE.** Every IMPL_phase-B checklist row (1–15), every Task (T1–T9), and every §12 test-matrix item (VS-edge, VS-empty/D4, VS-N, VS-guard, VS-crlf, gz-input, unknown-chr, non-consecutive, emission-order, VS-dir, VS-pad) maps to real, passing tests. The four documented deviations are all matched and correct. The core algorithm was audited line-by-line against Perl `NOMe_filtering` v0.25.1 (`:74-219`, `:242-391`) with no correctness divergence found, and the `main.golden`/`ncontext.golden` arithmetic was independently re-derived from the committed genome and confirmed. Build, full test suite, and clippy are all green. No gaps; nothing to address before the Phase-C real-data gate.
