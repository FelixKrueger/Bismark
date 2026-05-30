#!/usr/bin/env bash
# Regenerate Phase-B byte-identity goldens from the repo's Perl coverage2cytosine
# (v0.25.1, self-contained Perl). Run from this directory (tests/data/phase_b).
set -eo pipefail
C2C="$(cd "$(dirname "$0")/../../../../.." && pwd)/coverage2cytosine"
for mode in default cx zero thr; do
  case $mode in
    default) flags=();;
    cx)      flags=(--CX);;
    zero)    flags=(--zero_based);;
    thr)     flags=(--coverage_threshold 5);;
  esac
  perl "$C2C" -o "$mode" -g genome --dir . "${flags[@]}" in.cov >/dev/null 2>&1
done
mv default.CpG_report.txt default.report.golden
mv default.cytosine_context_summary.txt default.summary.golden
mv cx.CX_report.txt cx.report.golden
mv cx.cytosine_context_summary.txt cx.summary.golden
mv zero.CpG_report.txt zero.report.golden
mv zero.cytosine_context_summary.txt zero.summary.golden
mv thr.CpG_report.txt thr.report.golden
mv thr.cytosine_context_summary.txt thr.summary.golden

# ── Phase C goldens (whole-directory) ──
rm -rf phase_c && mkdir -p phase_c/split phase_c/split_thr
perl "$C2C" -o split -g genome --dir phase_c/split --split_by_chromosome in.cov >/dev/null 2>&1
perl "$C2C" -o split -g genome --dir phase_c/split_thr --split_by_chromosome --coverage_threshold 5 in.cov >/dev/null 2>&1

# ── Phase D goldens: --merge_CpGs / --discordance_filter (repo Perl v0.25.1) ──
# Inputs are the committed fixtures under ../phase_d; goldens are Perl's output.
# The merged/discordant cov *content* is independent of -o (no stem in cov lines).
pd=../phase_d

# Merged (1-based); --gzip shares this content (gzip compared post-decompression).
perl "$C2C" -o merge -g genome --dir "$pd" --merge_CpGs in.cov >/dev/null 2>&1
mv "$pd/merge.CpG_report.merged_CpG_evidence.cov" "$pd/merge.merged.golden"

# Merged, --zero_based (half-open end).
perl "$C2C" -o merge_zero -g genome --dir "$pd" --merge_CpGs --zero_based in.cov >/dev/null 2>&1
mv "$pd/merge_zero.CpG_report.merged_CpG_evidence.cov" "$pd/merge_zero.merged.golden"

# Discordance gross (Δ80 > 20 → diverted): merged empty, discordant has both rows.
perl "$C2C" -o disc_gross -g genome --dir "$pd" --merge_CpGs --discordance_filter 20 "$pd/disc_gross.cov" >/dev/null 2>&1
mv "$pd/disc_gross.CpG_report.merged_CpG_evidence.cov"     "$pd/disc_gross.merged.golden"
mv "$pd/disc_gross.CpG_report.discordant_CpG_evidence.cov" "$pd/disc_gross.discordant.golden"

# Discordance boundary (rounded Δ=5.0, NOT >5 → MERGED): the raw-f64 trap (V12).
perl "$C2C" -o disc_boundary -g genome --dir "$pd" --merge_CpGs --discordance_filter 5 "$pd/disc_boundary.cov" >/dev/null 2>&1
mv "$pd/disc_boundary.CpG_report.merged_CpG_evidence.cov"     "$pd/disc_boundary.merged.golden"
mv "$pd/disc_boundary.CpG_report.discordant_CpG_evidence.cov" "$pd/disc_boundary.discordant.golden"

# Both-measured gate (V6): + strand 9/0, - strand uncovered → pooled, NOT diverted.
perl "$C2C" -o gate -g genome --dir "$pd" --merge_CpGs --discordance_filter 20 "$pd/gate.cov" >/dev/null 2>&1
mv "$pd/gate.CpG_report.merged_CpG_evidence.cov"     "$pd/gate.merged.golden"
mv "$pd/gate.CpG_report.discordant_CpG_evidence.cov" "$pd/gate.discordant.golden"

# Chr-start resync, consecutive-short-scaffold SLIDE (V8b): orphans sA,sB → land on sC.
perl "$C2C" -o resync -g "$pd/resync_genome" --dir "$pd" --merge_CpGs "$pd/resync.cov" >/dev/null 2>&1
mv "$pd/resync.CpG_report.merged_CpG_evidence.cov" "$pd/resync.merged.golden"

# Chr-start resync, same-chr branch (V8a): a pos-1 orphan then a same-chr real pair.
perl "$C2C" -o samechr -g "$pd/samechr_genome" --dir "$pd" --merge_CpGs "$pd/samechr.cov" >/dev/null 2>&1
mv "$pd/samechr.CpG_report.merged_CpG_evidence.cov" "$pd/samechr.merged.golden"

# Multi-chromosome (V14): merged lines across chr1→chr2 (exercises the chr transition).
perl "$C2C" -o multi -g "$pd/multi_genome" --dir "$pd" --merge_CpGs "$pd/multi.cov" >/dev/null 2>&1
mv "$pd/multi.CpG_report.merged_CpG_evidence.cov" "$pd/multi.merged.golden"

# EOF-mid-resync (V13): trailing lone-orphan scaffolds drive the resync read-ahead to
# EOF; Perl dies (exit 255) leaving the partial merged file. Tolerate the nonzero exit
# and snapshot whatever merged lines were written before the die.
perl "$C2C" -o eof -g "$pd/eof_genome" --dir "$pd" --merge_CpGs "$pd/eof.cov" >/dev/null 2>&1 || true
mv "$pd/eof.CpG_report.merged_CpG_evidence.cov" "$pd/eof.merged.golden"

# Drop the byproduct reports/summaries — Phase D goldens are only the cov files.
rm -f "$pd"/*.CpG_report.txt "$pd"/*.cytosine_context_summary.txt
