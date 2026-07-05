#!/usr/bin/env bash
#
# Real-data byte-identity gate for filter_non_conversion_rs.
# Runs the REAL Perl filter_non_conversion v0.25.1 AND the Rust binary on a
# real Bismark BAM across 4 decision-mode cells, and asserts the decompressed
# kept/removed bodies (samtools view) + the report (run-time-line normalized,
# SPEC D2) are byte-identical.
#
# Usage:
#   fnc_real_data_gate.sh <perl_filter_non_conversion> <rust_binary> <bam> <-s|-p>
# Env:
#   SAMTOOLS  samtools binary (default: samtools on PATH)
#
# Exit 0 = all cells byte-identical; nonzero = at least one cell differs.
set -uo pipefail
export LC_ALL=C

PERL_FNC="$1"; RUST_BIN="$2"; BAM="$3"; MODE="$4"
SAMTOOLS="${SAMTOOLS:-samtools}"
BASE="$(basename "$BAM")"; STEM="${BASE%.bam}"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
overall=0

strip_timing() { sed '/^filter_non_conversion completed in /,$d' "$1"; }

run_cell() {
  local label="$1"; shift
  local pdir="$WORK/p_$label" rdir="$WORK/r_$label"
  mkdir -p "$pdir" "$rdir"
  # Symlink (not copy) the input so Perl/Rust read the real BAM via the link
  # while writing outputs into the per-cell dir — avoids copying a multi-GB BAM.
  ln -sf "$BAM" "$pdir/$BASE"; ln -sf "$BAM" "$rdir/$BASE"
  ( cd "$pdir" && perl "$PERL_FNC" "$MODE" "$@" "$BASE" >/dev/null 2>&1 )
  ( cd "$rdir" && "$RUST_BIN" "$MODE" "$@" "$BASE" >/dev/null 2>&1 )

  local ok=1 msg=""
  for suf in nonCG_filtered.bam nonCG_removed_seqs.bam; do
    "$SAMTOOLS" view "$pdir/$STEM.$suf" > "$WORK/p.sam" 2>/dev/null
    "$SAMTOOLS" view "$rdir/$STEM.$suf" > "$WORK/r.sam" 2>/dev/null
    if ! cmp -s "$WORK/p.sam" "$WORK/r.sam"; then
      ok=0; msg+=" body:$suf DIFFERS;"
    fi
  done
  strip_timing "$pdir/$STEM.non-conversion_filtering.txt" > "$WORK/p.rep" 2>/dev/null
  strip_timing "$rdir/$STEM.non-conversion_filtering.txt" > "$WORK/r.rep" 2>/dev/null
  if ! cmp -s "$WORK/p.rep" "$WORK/r.rep"; then
    ok=0; msg+=" report DIFFERS;"
  fi

  if [ "$ok" = 1 ]; then
    local nf nr
    nf=$("$SAMTOOLS" view -c "$rdir/$STEM.nonCG_filtered.bam" 2>/dev/null)
    nr=$("$SAMTOOLS" view -c "$rdir/$STEM.nonCG_removed_seqs.bam" 2>/dev/null)
    echo "  [$MODE/$label] PASS (kept=$nf removed=$nr, byte-identical)"
  else
    echo "  [$MODE/$label] FAIL -$msg"
    overall=1
  fi
}

echo "=== gate: $BASE ($MODE) ==="
echo "  records: $("$SAMTOOLS" view -c "$BAM" 2>/dev/null)"
run_cell default
run_cell threshold5 --threshold 5
run_cell consecutive --consecutive
run_cell percentage20 --percentage_cutoff 20

if [ "$overall" = 0 ]; then echo "=== $MODE: ALL CELLS BYTE-IDENTICAL ==="; else echo "=== $MODE: DIFFERENCES FOUND ==="; fi
exit "$overall"
