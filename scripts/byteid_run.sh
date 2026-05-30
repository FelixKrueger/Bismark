#!/usr/bin/env bash
#
# byteid_run.sh — full-scale byte-identity (PARITY-with-Perl) gate for one dataset.
# Part of plans/05302026_extractor-fulldata-validation/PLAN.md (Phase 1).
#
# Two checks per dataset:
#   (1) Rust-vs-Perl PARITY — runs scripts/phase_h_smoke.sh at --parallel 1 (Perl
#       --multicore 1, deterministic; this timed Perl run also serves as the serial
#       perf anchor). phase_h_smoke.sh now excludes the expected Perl-only M-bias
#       *.png delta and dumps a triage diff on any strict-cmp FAIL.
#   (2) Rust-vs-Rust worker-invariance — runs the Rust binary across --parallel
#       {sweep} and asserts every per-context output is sorted-identical to N=1
#       (output is worker-count-invariant via the batch_seq reorder).
#
# This proves PARITY with Perl v0.25.1, NOT absolute correctness.
#
# Usage:
#   byteid_run.sh <BAM> --dataset NAME --out DIR [--modes "gzip plain"] [--sweep "1 2 4 8 16"]
# Env: RUST_BIN, PERL_BIN (passed through to phase_h_smoke.sh).
# Exit: 0 = all PASS; 1 = any genuine mismatch (after PNG-exclusion + rounding triage).

set -euo pipefail

BAM="" DATASET="" OUT_DIR="" MODES="gzip" SWEEP="1 2 4 8 16"
while [[ $# -gt 0 ]]; do
  case $1 in
    --dataset) DATASET="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    --modes) MODES="$2"; shift 2 ;;
    --sweep) SWEEP="$2"; shift 2 ;;
    *) if [[ -z "$BAM" ]]; then BAM="$1"; shift; else echo "unexpected arg: $1" >&2; exit 2; fi ;;
  esac
done
[[ -z "$BAM" || -z "$DATASET" || -z "$OUT_DIR" ]] && {
  echo "usage: byteid_run.sh <BAM> --dataset NAME --out DIR [--modes \"gzip plain\"] [--sweep \"1 2 4 8 16\"]" >&2; exit 2; }
if [[ -L "$BAM" ]]; then echo "ERROR: $BAM is a symlink — stage it locally first" >&2; exit 2; fi
[[ -f "$BAM" ]] || { echo "ERROR: BAM not found: $BAM" >&2; exit 2; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_BIN="${RUST_BIN:-$HOME/Github/Bismark/rust/target/release/bismark-methylation-extractor-rs}"
STATUS="$OUT_DIR/byteid_${DATASET}.status"
mkdir -p "$OUT_DIR"
: > "$STATUS"
FAIL=0

PE_FLAG=""
if command -v samtools >/dev/null 2>&1 && samtools view -H "$BAM" | grep -q '@PG.*ID:Bismark.*-1 '; then
  PE_FLAG="--paired-end"
fi

# ── (1) Rust-vs-Perl parity, per mode, at --parallel 1 ──────────────────
for mode in $MODES; do
  echo "==> [$DATASET] Rust-vs-Perl parity: mode=$mode (--parallel 1)" | tee -a "$STATUS"
  smoke_out="$OUT_DIR/parity_${DATASET}_${mode}"
  if RUST_BIN="$RUST_BIN" "$SCRIPT_DIR/phase_h_smoke.sh" "$BAM" \
        --parallel 1 --mode "$mode" --out "$smoke_out" >>"$STATUS" 2>&1; then
    echo "  PARITY PASS: $DATASET $mode" | tee -a "$STATUS"
  else
    echo "  PARITY FAIL: $DATASET $mode (see $smoke_out/diff_summary.txt)" | tee -a "$STATUS"
    FAIL=1
  fi
done

# ── (2) Rust-vs-Rust worker-invariance sweep (gzip mode) ────────────────
echo "==> [$DATASET] Rust-vs-Rust worker-invariance: --parallel { $SWEEP }" | tee -a "$STATUS"
sweep_base="$OUT_DIR/sweep_${DATASET}"
declare -A REF_MD5=()  # per-file md5 from N=1
first_n=""
for n in $SWEEP; do
  d="$sweep_base/p${n}"; rm -rf "$d"; mkdir -p "$d"
  "$RUST_BIN" --output_dir "$d" --parallel "$n" ${PE_FLAG:+$PE_FLAG} --gzip "$BAM" \
      >"$d/.stdout" 2>"$d/.stderr" || { echo "  RUST RUN FAIL at --parallel $n" | tee -a "$STATUS"; FAIL=1; continue; }
  # md5 each per-context output (sorted content; .gz via zcat). Skip reports/M-bias.
  while IFS= read -r f; do
    base=$(basename "$f")
    case "$base" in *_splitting_report.txt|*.M-bias.txt|*.png) continue ;; esac
    if [[ "$base" == *.gz ]]; then md=$(zcat "$f" | LC_ALL=C sort | md5sum | cut -d' ' -f1)
    else md=$(LC_ALL=C sort "$f" | md5sum | cut -d' ' -f1); fi
    if [[ -z "$first_n" ]]; then REF_MD5["$base"]="$md"
    elif [[ "${REF_MD5[$base]:-MISSING}" != "$md" ]]; then
      echo "  INVARIANCE FAIL: $base differs at --parallel $n (n1=${REF_MD5[$base]:-MISSING} n${n}=$md)" | tee -a "$STATUS"; FAIL=1
    fi
  done < <(find "$d" -maxdepth 1 -type f \( -name '*.txt.gz' -o -name '*.txt' \) )
  [[ -z "$first_n" ]] && first_n="$n"
  echo "  --parallel $n: $( [[ "$n" == "$first_n" ]] && echo 'reference captured' || echo 'compared to N=1' )" | tee -a "$STATUS"
  # keep only N=1 outputs for inspection
  [[ "$n" != "$first_n" ]] && rm -rf "$d"
done

echo "" | tee -a "$STATUS"
if [[ "$FAIL" -eq 0 ]]; then
  echo "BYTEID PASS (parity-with-Perl + worker-invariance): $DATASET" | tee -a "$STATUS"
else
  echo "BYTEID FAIL: $DATASET — HARD GATE (triage diff_summary; check PNG/rounding before declaring a regression)" | tee -a "$STATUS"
fi
exit $FAIL
