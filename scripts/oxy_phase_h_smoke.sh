#!/usr/bin/env bash
#
# oxy_phase_h_smoke.sh — partial Phase H byte-identity smoke against real WGBS data.
#
# Runs both the Perl `bismark_methylation_extractor` and the Rust
# `bismark-methylation-extractor-rs` on the same BAM input, then compares
# every output file byte-for-byte. Also measures wall-clock for both
# runs to validate SPEC §9.7's ≥ 4× speedup target at N=4.
#
# **Scope (Phase F + flavour A):** validates the file set the Rust binary
# currently produces — 12 strand×context split files (or fewer per
# --comprehensive / --merge_non_CpG), M-bias.txt, _splitting_report.txt.
# Does NOT validate bedGraph / cytosine_report output (those subprocess
# chains arrive in Phase G; the full Phase H gate runs after G).
#
# Usage:
#   ./scripts/oxy_phase_h_smoke.sh <BAM> [--parallel N] [--mode MODE] [--out DIR]
#
# Defaults: --parallel 4, --mode default (no extra flags), --out ./oxy_phase_h_out
#
# MODE values:
#   default                 — no extra flags (12 split files)
#   comprehensive           — --comprehensive (3 files)
#   merge_non_CpG           — --merge_non_CpG (8 files)
#   comprehensive_merge     — --comprehensive --merge_non_CpG (2 files)
#   gzip                    — --gzip (12 .gz files)
#
# Auto-detects --paired-end from the @PG header (matches Perl behaviour).
#
# Environment overrides:
#   PERL_BIN                — path to bismark_methylation_extractor (default: ./bismark_methylation_extractor)
#   RUST_BIN                — path to bismark-methylation-extractor-rs (default: cargo bin in workspace)
#
# Output:
#   $OUT/perl/              — Perl output
#   $OUT/rust/              — Rust output
#   $OUT/diff_summary.txt   — per-file diff results + speedup metric
#
# Exit codes:
#   0  — all output files byte-identical
#   1  — at least one file differs OR a binary crashed
#   2  — usage error

set -euo pipefail

# ─── Args ─────────────────────────────────────────────────────────────

BAM=""
PARALLEL=4
MODE=default
OUT_DIR="./oxy_phase_h_out"

while [[ $# -gt 0 ]]; do
  case $1 in
    --parallel)
      PARALLEL="$2"; shift 2 ;;
    --mode)
      MODE="$2"; shift 2 ;;
    --out)
      OUT_DIR="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,/^$/p' "$0"; exit 0 ;;
    *)
      if [[ -z "$BAM" ]]; then BAM="$1"; shift; else
        echo "error: unexpected arg: $1" >&2; exit 2
      fi
      ;;
  esac
done

if [[ -z "$BAM" ]]; then
  echo "usage: $0 <BAM> [--parallel N] [--mode MODE] [--out DIR]" >&2
  exit 2
fi

if [[ ! -f "$BAM" ]]; then
  echo "error: BAM not found: $BAM" >&2
  exit 2
fi

# ─── Repo root + binary paths ─────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"
# Build the Rust binary in release mode for fair speed comparison.
if [[ -z "${RUST_BIN:-}" ]]; then
  echo "==> building bismark-methylation-extractor-rs (release)..." >&2
  (cd "$REPO_ROOT/rust" && cargo build --release -p bismark-extractor) >&2
  RUST_BIN="$REPO_ROOT/rust/target/release/bismark-methylation-extractor-rs"
fi

if [[ ! -x "$PERL_BIN" ]]; then
  echo "error: Perl binary not executable: $PERL_BIN" >&2; exit 2
fi
if [[ ! -x "$RUST_BIN" ]]; then
  echo "error: Rust binary not executable: $RUST_BIN" >&2; exit 2
fi

# ─── Mode → extra flags ───────────────────────────────────────────────

EXTRA_FLAGS=()
case "$MODE" in
  default)                  ;;
  comprehensive)            EXTRA_FLAGS+=(--comprehensive) ;;
  merge_non_CpG)            EXTRA_FLAGS+=(--merge_non_CpG) ;;
  comprehensive_merge)      EXTRA_FLAGS+=(--comprehensive --merge_non_CpG) ;;
  gzip)                     EXTRA_FLAGS+=(--gzip) ;;
  *)
    echo "error: unknown mode: $MODE" >&2; exit 2 ;;
esac

# ─── Output dirs ──────────────────────────────────────────────────────

PERL_OUT="$OUT_DIR/perl"
RUST_OUT="$OUT_DIR/rust"
mkdir -p "$PERL_OUT" "$RUST_OUT"

# ─── PE auto-detect via samtools view -H ──────────────────────────────

PE_FLAG=""
# Look for `@PG` line with `ID:Bismark` and `-1` arg → PE alignment.
# samtools is on most oxy environments; fall back to `head -c` parse if not.
if command -v samtools >/dev/null 2>&1; then
  if samtools view -H "$BAM" | grep -q '@PG.*ID:Bismark.*-1 '; then
    PE_FLAG="--paired-end"
  fi
fi

# ─── Run Perl ─────────────────────────────────────────────────────────

echo "==> running Perl bismark_methylation_extractor (multicore=$PARALLEL, mode=$MODE)..." >&2
PERL_START=$(date +%s)
"$PERL_BIN" \
  --output "$PERL_OUT" \
  --multicore "$PARALLEL" \
  ${PE_FLAG:+$PE_FLAG} \
  "${EXTRA_FLAGS[@]}" \
  "$BAM" 2>&1 | tail -3 || { echo "Perl run failed" >&2; exit 1; }
PERL_END=$(date +%s)
PERL_ELAPSED=$((PERL_END - PERL_START))

# ─── Run Rust ─────────────────────────────────────────────────────────

echo "==> running bismark-methylation-extractor-rs (parallel=$PARALLEL, mode=$MODE)..." >&2
RUST_START=$(date +%s)
"$RUST_BIN" \
  --output_dir "$RUST_OUT" \
  --parallel "$PARALLEL" \
  ${PE_FLAG:+$PE_FLAG} \
  "${EXTRA_FLAGS[@]}" \
  "$BAM" 2>&1 | tail -3 || { echo "Rust run failed" >&2; exit 1; }
RUST_END=$(date +%s)
RUST_ELAPSED=$((RUST_END - RUST_START))

# ─── Compare ──────────────────────────────────────────────────────────

SUMMARY="$OUT_DIR/diff_summary.txt"
{
  echo "Phase H byte-identity smoke — partial (Phase F + flavour A)"
  echo "Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "BAM: $BAM"
  echo "Mode: $MODE"
  echo "Parallel: $PARALLEL"
  echo "PE flag: ${PE_FLAG:-(none — SE auto-detected)}"
  echo ""
  echo "── Wall-clock ──"
  echo "Perl: ${PERL_ELAPSED}s"
  echo "Rust: ${RUST_ELAPSED}s"
  if [[ "$RUST_ELAPSED" -gt 0 ]]; then
    # bash arithmetic only does integer; compute ×10 for one decimal
    SPEEDUP10=$(( PERL_ELAPSED * 10 / RUST_ELAPSED ))
    echo "Speedup: ${SPEEDUP10:0:-1}.${SPEEDUP10: -1}× (Perl/Rust)"
    echo "Target: ≥ 4.0× at N=4 (SPEC §9.7)"
  fi
  echo ""
  echo "── Byte-identity (file-by-file) ──"
} > "$SUMMARY"

DIFFS=0
TOTAL=0
PERL_FILES=$(cd "$PERL_OUT" && ls -1 2>/dev/null | sort)
RUST_FILES=$(cd "$RUST_OUT" && ls -1 2>/dev/null | sort)

# File-name set diff
NAME_DIFF=$(diff <(echo "$PERL_FILES") <(echo "$RUST_FILES") || true)
if [[ -n "$NAME_DIFF" ]]; then
  echo "FILE-NAME-SET MISMATCH:" >> "$SUMMARY"
  echo "$NAME_DIFF" >> "$SUMMARY"
  echo "" >> "$SUMMARY"
fi

# Phase C.2 (#863 won't-fix): per-file byte compare with file-type dispatch:
#   *_splitting_report.txt + *.M-bias.txt → strict cmp (Perl-byte-identity)
#   *.gz                                  → zcat | sort | md5sum (data files
#                                            may differ by record ordering;
#                                            Rust BAM-order ≠ Perl multicore
#                                            fork+modulo order, both correct
#                                            but different layouts)
#   *  (plain data files)                 → sort | md5sum (same)
# Per SPEC §8.3 rev 3 "byte-identity invariant" definition (post-#863):
# splitting-report + M-bias are STRICT raw-byte; data files accept sorted-
# content equivalence (line ordering may differ but content matches).
SORTED=0    # count of files that matched sorted-content (≈ verdict)
for f in $(comm -12 <(echo "$PERL_FILES") <(echo "$RUST_FILES")); do
  TOTAL=$((TOTAL + 1))
  if cmp -s "$PERL_OUT/$f" "$RUST_OUT/$f"; then
    echo "  ✓ $f — byte-identical ($(wc -c < "$PERL_OUT/$f") bytes)" >> "$SUMMARY"
  else
    case "$f" in
      *_splitting_report.txt|*.M-bias.txt)
        # Strict raw-byte match required for these. (#864 closes report;
        # M-bias was already byte-identical post-Phase C.1.)
        DIFFS=$((DIFFS + 1))
        SIZE_P=$(wc -c < "$PERL_OUT/$f")
        SIZE_R=$(wc -c < "$RUST_OUT/$f")
        FIRST_DIFF=$(cmp "$PERL_OUT/$f" "$RUST_OUT/$f" 2>&1 | head -1 || true)
        echo "  ✗ $f DIFFERS — perl=${SIZE_P}B rust=${SIZE_R}B ($FIRST_DIFF)" >> "$SUMMARY"
        ;;
      *.gz)
        # Decompress before sort (sorting raw gzip bytes is meaningless).
        PMD=$(zcat "$PERL_OUT/$f" | LC_ALL=C sort | md5sum | cut -d' ' -f1)
        RMD=$(zcat "$RUST_OUT/$f" | LC_ALL=C sort | md5sum | cut -d' ' -f1)
        if [[ "$PMD" == "$RMD" ]]; then
          SORTED=$((SORTED + 1))
          echo "  ≈ $f — gzip-sorted-equivalent (raw differs by ordering only; md5 $PMD)" >> "$SUMMARY"
        else
          DIFFS=$((DIFFS + 1))
          echo "  ✗ $f DIFFERS — gzip-sorted mismatch (perl=$PMD rust=$RMD)" >> "$SUMMARY"
        fi
        ;;
      *)
        # Plain data file: accept sorted-content equivalence.
        PMD=$(LC_ALL=C sort "$PERL_OUT/$f" | md5sum | cut -d' ' -f1)
        RMD=$(LC_ALL=C sort "$RUST_OUT/$f" | md5sum | cut -d' ' -f1)
        if [[ "$PMD" == "$RMD" ]]; then
          SORTED=$((SORTED + 1))
          echo "  ≈ $f — sorted-equivalent (raw differs by ordering only; md5 $PMD)" >> "$SUMMARY"
        else
          DIFFS=$((DIFFS + 1))
          echo "  ✗ $f DIFFERS — sorted mismatch (perl=$PMD rust=$RMD)" >> "$SUMMARY"
        fi
        ;;
    esac
  fi
done

echo "" >> "$SUMMARY"
echo "── Result ──" >> "$SUMMARY"
RAW=$((TOTAL - DIFFS - SORTED))
if [[ "$DIFFS" -eq 0 && -z "$NAME_DIFF" ]]; then
  echo "PASS: all $TOTAL files match ($RAW raw-identical + $SORTED sorted-equivalent)" >> "$SUMMARY"
else
  echo "FAIL: $DIFFS of $TOTAL files differ${NAME_DIFF:+; file-name set mismatch}" >> "$SUMMARY"
fi

cat "$SUMMARY"

if [[ "$DIFFS" -eq 0 && -z "$NAME_DIFF" ]]; then
  exit 0
else
  exit 1
fi
