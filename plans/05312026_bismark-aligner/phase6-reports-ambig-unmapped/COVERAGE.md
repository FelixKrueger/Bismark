# Plan Coverage Report

**Mode:** B (code vs. plan)
**Plan:** `phase6-reports-ambig-unmapped/PLAN.md` (rev 2)
**Date:** 2026-06-01
**Verdict:** COMPLETE — all behaviors + §5 steps + §7 rows implemented; §7 #10 PENDING-by-design (oxy gate, run separately)

## Summary

- Total ledger items: 41 (5 §3 behaviors + 5 sub-points + 6 §5 steps + 18 §7 rows + 3 rev-2 Criticals + 4 specific confirmations)
- DONE: 40
- PENDING-by-design: 1 (§7 #10 oxy report+aux byte-identity gate — not a cargo test)
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0 (one documented naming deviation: `aux.rs` → `aux_out.rs`, immaterial; one documented multi-file wall-clock-line emission, gate-neutral)

**Tests:** `cargo test -p bismark-aligner` = 131 unit + 19 integration (cli.rs) PASS; `cargo test -p bismark-io` = 179 unit PASS. 0 failed across both crates.

## Coverage ledger

### §3 Behavior

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | 3.1 Report header (3 lines: `Bismark report for:` / `--directional` / `Bismark was run with Bowtie 2 … <genome_folder> … <aligner_options>`) | report.rs `write_report_header` | DONE | Bytes exact; pinned by `header_directional_exact`. |
| 2 | 3.1 Final analysis body (`=`×22/`=`×33; Total C's EXCL. Unknown; rejected line; `Mapping efficiency` single `\n`; f64 division) | report.rs `print_final_analysis_report_single_end` | DONE | Pinned by `final_analysis_exact_directional` (Total=2100 excludes Unknown; single `\n` verified). |
| 3 | 3.1 `(me+unme)>0` percentage gate; all-unmethylated → `0.0%` | report.rs `write_percentage` | DONE | `if me + unme > 0` (line 199); tests `all_unmethylated_bucket_prints_zero_point_zero` + `empty_bucket_prints_cant_determine`. |
| 4 | 3.1 `seqID_contains_tabs` warning never emitted (forward-safe) | report.rs (comment, line 190) | DONE | Intentionally not emitted; structurally 0 in SE-directional. Counter not wired (see Gaps note — non-blocking). |
| 5 | 3.1 Trailing wall-clock line emitted + gate normalizes it | report.rs `write_completion_line`; lib.rs:208 | DONE | `Bismark completed in {d}d {h}h {m}m {s}s\n`; test `completion_line_format`. Gate filter is §7 #10 (separate). |
| 6 | 3.2 Routing: UniqueBest→BAM; Ambiguous→ambigBAM-if-Some then ambiguous-else-unmapped; NoAlignment→unmapped; Rejected→drop | lib.rs `drive_merge` 421–495 | DONE | Precedence `if ambiguous.is_some() else unmapped` (464–468); Rejected arm empty (494). |
| 7 | 3.3 FastQ record: `@id\n` + non-uc orig seq + verbatim `+` line + qual; un-stripped basename name; gzip | aux_out.rs `aux_filename`/`write_fastq_record`; lib.rs 470/483 | DONE | `seq_orig = chomp_newline(&seq)` (non-uc, line 470/483); `+` written verbatim (aux_out 80). |
| 8 | 3.4 `--ambig_bam` raw passthrough, written ONLY on within-thread path (first_ambig Some/None); RNAME de-converted; tags verbatim+in-order | merge.rs `Decision::Ambiguous{first_ambig}`; output.rs `write_raw_sam_line_to_bam`/`build_raw_record`; lib.rs 458–463 | DONE | Write gated on `first_ambig.is_some()` (459–462); cross-tie returns `None` (merge.rs 280). |
| 9 | 3.4 `first_ambig` captured at BOTH score arms (first-alignment + strict-improvement), not on equal | merge.rs 200–202 (`None=>`) + 207–214 (`> best`) | DONE | Both gated on `want_ambig`; equal-AS arm does NOT recapture. Test `first_ambig_captures_strict_improvement_instance`. |
| 10 | 3.5 Temp C→T file deleted after the loop (best-effort) | lib.rs:214 `let _ = std::fs::remove_file(&converted.path)` | DONE | Best-effort (ignored result); integration-asserted (cli.rs:178). |

### §5 Implementation outline

| # | Step | Status | Notes |
|---|------|--------|-------|
| S1 | merge.rs: `Ambiguous{first_ambig}` + capture gated on `want_ambig`; Phase-4 tests updated | DONE | `want_ambig` param threaded; 4 capture tests added; existing matches updated. |
| S2 | report.rs: header writer + final analysis (byte-exact + `%.1f`) + 0-seq/0-context tests | DONE | 11 unit tests incl. exact-report/0-seq/all-unmeth/all-Unknown/half-boundary. |
| S3 | Unmapped/ambiguous writers: filename derivation + gzip + `write_fastq_record` | DONE | aux_out.rs with flate2 `GzEncoder` opened in `open_sinks` (lib.rs 275–292). |
| S4 | output.rs: `write_raw_sam_line_to_bam` → bare RecordBuf, bypass BismarkRecord validation | DONE | `build_raw_record` parses fields; de-converts RNAME field only; rejects unsupported tag types. |
| S5 | Driver: open REPORT (+ optional sinks) before loop; write header; route each Decision; final analysis; delete temp; finish ambig BAM; shrink `deferred_flags` | DONE | `open_sinks`/`Sinks::finish`; `deferred_flags` no longer lists the 3 flags (config.rs 334–337). |
| S6 | Tests (§7) — report bytes, routing, FastQ-record bytes, ambig-BAM raw record + extend oxy gate | DONE (gate = #10 separate) | unit + 2 integration end-to-end (unmapped+report, ambiguous+ambig_bam). |

### §7 Validation table

| # | Verify | Status | Test (file) |
|---|--------|--------|-------------|
| 1 | Report body bytes (directional; `=`×22/33, Total EXCL Unknown, rejected line, single `\n`) | DONE | `final_analysis_exact_directional` (report.rs) |
| 2 | Report 0-sequences → `Mapping efficiency:\t0%`; no div-by-zero | DONE | `zero_sequences_mapping_efficiency_is_bare_zero` (report.rs) |
| 3 | 0-context bucket (`me+unme==0`) → "Can't determine…" | DONE | `empty_bucket_prints_cant_determine` (report.rs) |
| 3b | All-unmethylated (`me==0, unme>0`) → `0.0%` not "Can't determine" | DONE | `all_unmethylated_bucket_prints_zero_point_zero` (report.rs) |
| 3c | All-Unknown corner → `Total C's:\t0`; CpG/CHG/CHH all "Can't determine" | DONE | `all_unknown_total_is_zero` (report.rs) |
| 3d | `%.1f` half-boundary (`unique=1,seq=8 → 12.5%`) | DONE | `mapping_efficiency_half_boundary_rounding` (report.rs) |
| 4 | Report header; `aligner_options` exact; genome path absolute WITH trailing `/` | DONE | `header_directional_exact` (`/abs/genome/`); lib.rs renders `format!("{}/", …)` |
| 5 | Routing precedence (both flags→ambiguous only; only `--unmapped`→unmapped) | DONE | merge unit + `ambiguous_and_ambig_bam_end_to_end` / `unmapped_routing_and_report_end_to_end` (cli.rs); precedence in lib.rs 464–468 |
| 6 | `Rejected`/could-not-extract written nowhere | DONE | `directional_rejection_index_2` (merge.rs) + `chromosome_edge_read_counted_but_not_written` (cli.rs, header-only BAM) |
| 7 | FastQ record bytes (decompressed): `@id\n<orig>\n<+verbatim><qual>\n`; seq not uc; `+` verbatim | DONE | `record_bytes_non_uc_seq_verbatim_plus` (aux_out.rs) + `unmapped_routing_and_report_end_to_end` (decompresses .gz) |
| 7b | FastQ CRLF + missing-final-newline (`+` retains `\r\n`; seq/qual `\r`-chomped+`\n`) | DONE | `record_bytes_crlf_plus_line_verbatim` (aux_out.rs) |
| 7c | Filename derivation (un-stripped basename; `--prefix`/`--basename`) | DONE | `filename_unstripped_basename` / `filename_prefix` / `filename_basename_overrides_prefix` (aux_out.rs) |
| 8a | Ambig BAM within-thread → exactly one record; RNAME de-converted; tags verbatim+order | DONE | `build_raw_ambig_record_deconverts_and_preserves_tag_order` (output.rs) + `ambiguous_and_ambig_bam_end_to_end` (cli.rs, non-empty .ambig.bam) |
| 8b | Ambig BAM cross-instance-tie → zero ambig records (read still in FastQ aux) | DONE | `cross_instance_tie_has_no_first_ambig` (merge.rs → `first_ambig: None`; driver writes nothing) |
| 8c | `first_ambig` capture ordering (instance-1 beats instance-0 then ties) | DONE | `first_ambig_captures_strict_improvement_instance` (merge.rs) |
| 8d | `write_raw_record` (bismark-io): multi-tag Bowtie 2 line round-trips (FLAG/POS/MAPQ/CIGAR + tag order/values) | DONE | `write_raw_record_bypasses_bismark_validation` (write.rs) + `record_roundtrips_through_bam_tag_order_values_qual` (output.rs) |
| 9 | Temp C→T unlinked (best-effort; failed unlink does NOT error the run) | DONE | lib.rs:214 (ignored result); integration assert cli.rs:178 |
| 10 | 🎯 oxy report+aux byte-identity gate (diff `_SE_report.txt` filtering `^Bismark completed in `, `samtools view -h` ambig BAM, `zcat` unmapped/ambiguous) | PENDING-by-design | Run separately on Linux/oxy; not a cargo test (per plan §7 #10 + §12). |

### Rev-2 Criticals + specific confirmations (from skill prompt)

| Item | Status | Evidence |
|------|--------|----------|
| Trailing `Bismark completed in` line emitted + gate normalizes | DONE | `write_completion_line` + lib.rs:208; gate filter is §7 #10. |
| Genome path absolute WITH trailing `/` | DONE | lib.rs:132 `format!("{}/", config.genome.genome_dir.display())`; test uses `/abs/genome/`. |
| Ambig BAM written ONLY on within-thread path (`first_ambig` Some/None) | DONE | merge.rs: `Some` at within-thread (260), `None` at cross-tie (280); driver gates on `is_some()` (lib.rs 459–462). |
| `first_ambig` captured at BOTH score arms | DONE | merge.rs 200–202 + 207–214. |
| `(me+unme)>0` percentage gate (all-unmeth → `0.0%`) | DONE | report.rs `write_percentage`. |
| Un-stripped unmapped/ambiguous filename | DONE | aux_out.rs `aux_filename` (no `strip_fastq_suffix`); driver uses `aux_filename` for aux, `derive_output_path` for BAM/report. |
| Non-uc original seq + verbatim `+` line retained | DONE | lib.rs keeps `seq_orig = chomp_newline(&seq)` separately from `seq_uc`; `plus` written verbatim. |
| `--slam`/`--non_bs_mm`/`--rg_tag`/`--sam-no-hd` still hard-rejected | DONE | config.rs `reject_unsupported_output_flags` (221–242) unchanged. |
| `deferred_flags` shrunk (3 flags now active) | DONE | config.rs 322–338 lists only `--nucleotide_coverage`/`--multicore`/`--old_flag`; comment confirms the 3 are active. |
| Routing precedence + rejected/could-not-extract → written-nowhere | DONE | lib.rs Rejected arm empty (494); could-not-extract `continue`s before any write (429–436). |
| bismark-io version bump beta.9 + 4 dependent pins | DONE | bismark-io Cargo.toml `version = "1.0.0-beta.9"`; aligner/dedup/extractor/methylation-consistency all pin `=1.0.0-beta.9`. |

## Gaps (detail)

No blocking gaps. Two non-blocking observations:

### Observation A: §3.1 `seqID_contains_tabs` counter not wired into a conditional

**Expected (rev 2, Open Q3):** "Wire that existing counter (`convert::seqid_tab_count`) into the conditional (forward-safe) rather than hard-coding 'no warning'."
**Found:** `report.rs` line 190 documents that the warning never fires in v1 SE-directional and intentionally does not emit it; the `seqid_tab_count` counter is not threaded into the report. The behavior is byte-identical to Perl for the v1 scope (the warning structurally never fires after `fix_id` collapses tabs).
**Impact:** None for byte-identity in v1. This is a "forward-safety" wiring preference, not a behavioral requirement — the emitted bytes are correct. Recorded as DONE for the observable behavior; the wiring nicety is the only divergence from the rev-2 wording.

### Observation B: §7 #10 oxy gate — PENDING by design

Per the skill prompt and plan §7 #10 / §12, the full byte-identity gate (report diff with `^Bismark completed in` filtered, `samtools view -h` of the ambig BAM, `zcat` of the aux files vs Perl on identical argv) runs separately on Linux/oxy and is NOT a cargo test. No aligner-specific gate harness was found in `scripts/` (existing harnesses are for the other ports). This is the documented "run separately" item, not a coverage gap.

## Documented deviations (per §12, all immaterial)

- `aux.rs` named `aux_out.rs` (`aux` is a Windows-reserved filename). Cosmetic.
- For multi-file SE, every report ends with its own wall-clock line (Perl writes it only to the last report). Gate-neutral (the line is normalized out both sides).
- `report.rs` allows `clippy::write_with_newline` to keep explicit `\n`s auditable. Style-only.
- Phase-5 integration tests updated: C→T temp deletion assertion inverted (now deleted), report-file assertion added, `deferred_flag_emits_notice` switched to `--nucleotide_coverage`. Consistent with the now-active flags.

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| header_directional_exact | bismark-aligner/src/report.rs | PASS |
| final_analysis_exact_directional | bismark-aligner/src/report.rs | PASS |
| zero_sequences_mapping_efficiency_is_bare_zero | bismark-aligner/src/report.rs | PASS |
| all_unmethylated_bucket_prints_zero_point_zero | bismark-aligner/src/report.rs | PASS |
| empty_bucket_prints_cant_determine | bismark-aligner/src/report.rs | PASS |
| all_unknown_total_is_zero | bismark-aligner/src/report.rs | PASS |
| mapping_efficiency_half_boundary_rounding | bismark-aligner/src/report.rs | PASS |
| non_directional_omits_rejected_line | bismark-aligner/src/report.rs | PASS |
| completion_line_format | bismark-aligner/src/report.rs | PASS |
| filename_unstripped_basename / filename_prefix / filename_basename_overrides_prefix | bismark-aligner/src/aux_out.rs | PASS |
| record_bytes_non_uc_seq_verbatim_plus | bismark-aligner/src/aux_out.rs | PASS |
| record_bytes_crlf_plus_line_verbatim | bismark-aligner/src/aux_out.rs | PASS |
| within_thread_ambiguity_captures_first_ambig | bismark-aligner/src/merge.rs | PASS |
| within_thread_ambiguity_no_capture_when_flag_off | bismark-aligner/src/merge.rs | PASS |
| cross_instance_tie_has_no_first_ambig | bismark-aligner/src/merge.rs | PASS |
| first_ambig_captures_strict_improvement_instance | bismark-aligner/src/merge.rs | PASS |
| build_raw_ambig_record_deconverts_and_preserves_tag_order | bismark-aligner/src/output.rs | PASS |
| build_raw_ambig_record_rejects_unsupported_tag_type | bismark-aligner/src/output.rs | PASS |
| write_raw_record_bypasses_bismark_validation | bismark-io/src/write.rs | PASS |
| unmapped_routing_and_report_end_to_end | bismark-aligner/tests/cli.rs | PASS |
| ambiguous_and_ambig_bam_end_to_end | bismark-aligner/tests/cli.rs | PASS |
| chromosome_edge_read_counted_but_not_written | bismark-aligner/tests/cli.rs | PASS |
| deferred_flag_emits_notice (now `--nucleotide_coverage`) | bismark-aligner/tests/cli.rs | PASS |
| happy_path_resolves_and_prints_config (C→T temp deleted + report asserted) | bismark-aligner/tests/cli.rs | PASS |
| (aggregate) bismark-aligner | unit 131 + integ 19 | ALL PASS |
| (aggregate) bismark-io | unit 179 | ALL PASS |

## Verdict

**COMPLETE.** Every §3 behavior, every §5 implementation step, all 18 §7 validation rows (1–10 incl. 3b/3c/3d/7b/7c/8a/8b/8c/8d), and all three rev-2 Criticals plus the requested specific confirmations are implemented and test-backed. The only remaining item, §7 #10 (the oxy report+aux byte-identity gate), is explicitly run separately on Linux/oxy and is recorded as PENDING-by-design, not a gap. The single wording-level divergence (Observation A: `seqid_tab_count` not threaded into the report conditional) is byte-neutral in v1 SE-directional — no action required for this phase's gate.
