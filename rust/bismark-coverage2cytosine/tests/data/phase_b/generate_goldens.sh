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
