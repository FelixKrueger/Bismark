#!/usr/bin/env bash
#
# phase_h_smoke.sh — Phase H per-cell byte-identity smoke against real WGBS data.
#
# Runs both the Perl `bismark_methylation_extractor` and the Rust
# `bismark-methylation-extractor-rs` on the same BAM input, then compares
# every output file. Also measures wall-clock for both runs to validate
# SPEC §9.7's ≥ 4× speedup target at N=4.
#
# **Post-colossal-migration (2026-05-28):** machine-agnostic via BAM-path
# argv. Renamed from `oxy_phase_h_smoke.sh` per Phase H sub-issue #871 +
# the memory `reference_colossal_access.md`.
#
# **Phase H matrix driver usage:** invoke via `scripts/phase_h_se_matrix.sh`
# (for #871) or `scripts/phase_h_pe_matrix.sh` (for #872, future). Standalone
# invocation also supported.
#
# **Scope:** validates the extractor's own output streams — 12 strand×context
# split files (or fewer per --comprehensive / --merge_non_CpG; 6 for
# directional SE post-Phase C.2 empty-sweep), M-bias.txt, _splitting_report.txt.
# Does NOT validate bedGraph / cytosine_report output (subprocess-to-Perl
# in Phase G; Phase H sub-gate 2 covers those, blocked on epic #797).
#
# Usage:
#   ./scripts/phase_h_smoke.sh <BAM> [--parallel N] [--mode MODE] [--out DIR] \
#       [--extra-rust "<flags>"] [--extra-perl "<flags>"]
#
# Defaults: --parallel 4, --mode default, --out ./phase_h_out
#
# MODE values:
#   default                 — no extra flags (12 split files for PE; 6 for SE post-sweep)
#   comprehensive           — --comprehensive (3 files)
#   merge_non_CpG           — --merge_non_CpG (8 files)
#   comprehensive_merge     — --comprehensive --merge_non_CpG (2 files)
#   gzip                    — --gzip (12 .gz files for PE; 6 for SE)
#
# --extra-rust / --extra-perl: arbitrary additional flags appended to the
# respective binary's argv. Parsed as bash arrays (read -r -a); pass-through
# is verbatim. The matrix drivers use these to inject per-cell --ignore /
# --ignore_3prime / --ignore_r2 / --include_overlap etc.
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
#   $OUT/diff_summary.txt   — per-file diff results + wall-clock metrics
#                             (Perl: <int>s / Rust: <int>s lines parseable by
#                             the matrix drivers)
#
# Exit codes:
#   0  — all output files byte-identical (or sorted-equivalent per SPEC §8.3 rev 3)
#   1  — at least one file differs OR a binary crashed
#   2  — usage error

set -euo pipefail

# ─── Args ─────────────────────────────────────────────────────────────

BAM=""
PARALLEL=4
MODE=default
OUT_DIR="./phase_h_out"
EXTRA_RUST_STR=""
EXTRA_PERL_STR=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --parallel)
      PARALLEL="$2"; shift 2 ;;
    --mode)
      MODE="$2"; shift 2 ;;
    --out)
      OUT_DIR="$2"; shift 2 ;;
    --extra-rust)
      EXTRA_RUST_STR="$2"; shift 2 ;;
    --extra-perl)
      EXTRA_PERL_STR="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,/^$/p' "$0"; exit 0 ;;
    *)
      if [[ -z "$BAM" ]]; then BAM="$1"; shift; else
        echo "error: unexpected arg: $1" >&2; exit 2
      fi
      ;;
  esac
done

# Phase H rev 1 I6: parse --extra-* as bash arrays via `read -r -a`. This
# preserves space-separated tokens correctly regardless of the parent shell's
# IFS setting (see memory feedback_bash_ifs_word_splitting). Pass-through is
# verbatim via "${EXTRA_RUST[@]}" / "${EXTRA_PERL[@]}" at the invocation site.
EXTRA_RUST=()
EXTRA_PERL=()
if [[ -n "$EXTRA_RUST_STR" ]]; then
  read -r -a EXTRA_RUST <<< "$EXTRA_RUST_STR"
fi
if [[ -n "$EXTRA_PERL_STR" ]]; then
  read -r -a EXTRA_PERL <<< "$EXTRA_PERL_STR"
fi

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

echo "==> running Perl bismark_methylation_extractor (multicore=$PARALLEL, mode=$MODE${EXTRA_PERL_STR:+, extra-perl=\"$EXTRA_PERL_STR\"})..." >&2
PERL_START=$(date +%s)
"$PERL_BIN" \
  --output "$PERL_OUT" \
  --multicore "$PARALLEL" \
  ${PE_FLAG:+$PE_FLAG} \
  ${EXTRA_FLAGS[@]+"${EXTRA_FLAGS[@]}"} \
  ${EXTRA_PERL[@]+"${EXTRA_PERL[@]}"} \
  "$BAM" 2>&1 | tail -3 || { echo "Perl run failed" >&2; exit 1; }
PERL_END=$(date +%s)
PERL_ELAPSED=$((PERL_END - PERL_START))

# ─── Run Rust ─────────────────────────────────────────────────────────

echo "==> running bismark-methylation-extractor-rs (parallel=$PARALLEL, mode=$MODE${EXTRA_RUST_STR:+, extra-rust=\"$EXTRA_RUST_STR\"})..." >&2
RUST_START=$(date +%s)
"$RUST_BIN" \
  --output_dir "$RUST_OUT" \
  --parallel "$PARALLEL" \
  ${PE_FLAG:+$PE_FLAG} \
  ${EXTRA_FLAGS[@]+"${EXTRA_FLAGS[@]}"} \
  ${EXTRA_RUST[@]+"${EXTRA_RUST[@]}"} \
  "$BAM" 2>&1 | tail -3 || { echo "Rust run failed" >&2; exit 1; }
RUST_END=$(date +%s)
RUST_ELAPSED=$((RUST_END - RUST_START))

# ─── Compare ──────────────────────────────────────────────────────────

SUMMARY="$OUT_DIR/diff_summary.txt"
{
  echo "Phase H byte-identity smoke — per-cell harness"
  echo "Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "BAM: $BAM"
  echo "Mode: $MODE"
  echo "Parallel: $PARALLEL"
  # Phase H rev 1 I6: persist the verbatim extra-flag strings for the
  # matrix driver's audit log.
  echo "Extra-rust: ${EXTRA_RUST_STR:-(none)}"
  echo "Extra-perl: ${EXTRA_PERL_STR:-(none)}"
  # Phase H rev 1 §5.3: emit "Library: SE|PE" annotation so the matrix
  # driver can apply mode-specific kept-file expectations (6 for
  # directional SE, 12 for PE per Phase C.2 empty-sweep contract).
  if [[ -n "$PE_FLAG" ]]; then
    echo "Library: PE"
  else
    echo "Library: SE"
  fi
  echo "PE flag: ${PE_FLAG:-(none — SE auto-detected)}"
  echo ""
  echo "── Wall-clock ──"
  # Phase H rev 1 I2: format is anchored "^Perl: <int>s$" / "^Rust: <int>s$"
  # — matrix driver parses via `grep -E '^(Perl|Rust): ([0-9]+)s$'`. Do
  # NOT add suffix punctuation or units other than 's'.
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
# rev-2 (full-data plan, PLAN_REVIEW_B Critical): codify the expected Perl-only
# M-bias PNG delta. Perl's extractor emits *M-bias_R1.png / *_R2.png via GD::Graph
# (when the module is installed); the Rust port defers PNG generation. Their presence
# is an EXPECTED file-set delta, NOT a regression — without this an unattended
# full-scale run would hard-FAIL the moment GD::Graph is present. We exclude *.png from
# the comparison and report any Perl-only PNGs as expected; any OTHER name-set delta is
# still a hard FAIL.
PERL_FILES_ALL=$(cd "$PERL_OUT" && ls -1 2>/dev/null | LC_ALL=C sort || true)
RUST_FILES_ALL=$(cd "$RUST_OUT" && ls -1 2>/dev/null | LC_ALL=C sort || true)
PERL_FILES=$(printf '%s\n' "$PERL_FILES_ALL" | { grep -v '\.png$' || true; })
RUST_FILES=$(printf '%s\n' "$RUST_FILES_ALL" | { grep -v '\.png$' || true; })
PERL_ONLY_PNG=$(printf '%s\n' "$PERL_FILES_ALL" | { grep '\.png$' || true; })

# File-name set diff (PNG-excluded)
NAME_DIFF=$(diff <(echo "$PERL_FILES") <(echo "$RUST_FILES") || true)
if [[ -n "$PERL_ONLY_PNG" ]]; then
  echo "EXPECTED PNG DELTA (Perl-only, Rust defers PNGs — not a failure):" >> "$SUMMARY"
  echo "$PERL_ONLY_PNG" >> "$SUMMARY"
  echo "" >> "$SUMMARY"
fi
if [[ -n "$NAME_DIFF" ]]; then
  echo "FILE-NAME-SET MISMATCH (non-PNG):" >> "$SUMMARY"
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
        # rev-2 (PLAN_REVIEW_A): dump the line diff for triage — a strict FAIL here at
        # full scale may be a %.2f/%.1f half-way rounding artifact rather than a calling
        # regression. Still counted as a DIFF (the hard gate fires); the dump lets a
        # human/driver decide in seconds whether it is rounding-only and safe to resume.
        DIFFS=$((DIFFS + 1))
        SIZE_P=$(wc -c < "$PERL_OUT/$f")
        SIZE_R=$(wc -c < "$RUST_OUT/$f")
        FIRST_DIFF=$(cmp "$PERL_OUT/$f" "$RUST_OUT/$f" 2>&1 | head -1 || true)
        echo "  ✗ $f DIFFERS — perl=${SIZE_P}B rust=${SIZE_R}B ($FIRST_DIFF)" >> "$SUMMARY"
        echo "    ── triage diff (first 8 differing lines; check for rounding-only deltas) ──" >> "$SUMMARY"
        diff "$PERL_OUT/$f" "$RUST_OUT/$f" 2>&1 | head -8 | sed 's/^/      /' >> "$SUMMARY" || true
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
