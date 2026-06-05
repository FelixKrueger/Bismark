#!/bin/bash
# Phase-2a oxy SE byte-identity gate (HISAT2 backend).
# Runs Perl bismark v0.25.1 --hisat2 + HISAT2 2.2.2 and the Rust bismark_rs
# --hisat2 with IDENTICAL argv into the SAME -o (Perl moved aside), then diffs
# DECOMPRESSED SAM content (samtools @PG filtered) + the report (wall-clock
# filtered). Cells: SE {directional, non-dir, pbat}, FastA SE (directional), and
# a --parallel multicore cell (Rust --parallel 8 vs --parallel 1: worker-
# invariance + proves the parallel.rs `_bismark_hisat2` naming token).
#
#   bismark naming: <base>_bismark_hisat2.bam / _bismark_hisat2_SE_report.txt
#   (a basename match between Perl/Rust outputs IS the naming-token check.)
#
# Usage: phase2a_hisat2_se_gate.sh <N>     e.g.  phase2a_hisat2_se_gate.sh 10000
set -uo pipefail

N="${1:-10000}"
ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
GENOME=$HOME/bismark_benchmarks/genome
RUST=/var/tmp/aligner_2a/rust/target/release/bismark_rs
SE_FQ=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
BASE=/var/tmp/aligner_2a_gate
rm -rf "$BASE"; mkdir -p "$BASE"

# Subset: first N reads (uncompressed .fq) + a FastA twin (@id->>id, drop +/qual).
zcat "$SE_FQ" | head -n $((4 * N)) > "$BASE/se.fq"
awk 'NR%4==1{print ">" substr($0,2)} NR%4==2{print}' "$BASE/se.fq" > "$BASE/se.fa"
echo "subset: se.fq=$(($(wc -l <"$BASE/se.fq")/4)) reads, se.fa=$(($(wc -l <"$BASE/se.fa")/2)) reads"

filter_sam ()    { samtools view -h "$1" | grep -v 'ID:samtools'; }
filter_report () { grep -v '^Bismark completed in ' "$1"; }

FAILED=0
# Perl-vs-Rust cell with identical argv into the same -o (Perl moved aside).
run_cell () {
  local name="$1"; shift
  local -a ARGS=("$@")
  local OUT="$BASE/$name" HOLD="$BASE/${name}_perl" TMP="$BASE/${name}_tmp"
  rm -rf "$OUT" "$HOLD" "$TMP"; mkdir -p "$OUT" "$HOLD" "$TMP"
  echo "=================== CELL $name (N=$N) ==================="
  bismark --hisat2 --path_to_hisat2 "$ENVBIN" --genome "$GENOME" -o "$OUT" --temp_dir "$TMP" \
    "${ARGS[@]}" >"$BASE/${name}.perl.log" 2>&1
  local prc=$?
  mv "$OUT"/* "$HOLD"/ 2>/dev/null
  rm -rf "$TMP"/*
  "$RUST" --hisat2 --path_to_hisat2 "$ENVBIN" --genome "$GENOME" -o "$OUT" --temp_dir "$TMP" \
    "${ARGS[@]}" >"$BASE/${name}.rust.log" 2>&1
  local rrc=$?
  echo "  exit: perl=$prc rust=$rrc"
  if [ "$prc" != 0 ] || [ "$rrc" != 0 ]; then
    echo "  CELL $name: FAIL (non-zero exit; see ${name}.{perl,rust}.log)"; FAILED=1; return
  fi
  local ok=1
  for pbam in "$HOLD"/*.bam; do
    [ -e "$pbam" ] || continue
    local b; b=$(basename "$pbam"); local rbam="$OUT/$b"
    if [ ! -f "$rbam" ]; then echo "  BAM $b: MISSING on rust side (naming-token mismatch?)"; ok=0; continue; fi
    if diff <(filter_sam "$pbam") <(filter_sam "$rbam") >"$BASE/${name}.${b}.diff" 2>&1; then
      echo "  BAM $b: BYTE-IDENTICAL ($(samtools view -c "$rbam") records)"
    else
      echo "  BAM $b: !!! DIFF ($(wc -l <"$BASE/${name}.${b}.diff") diff lines) -> ${name}.${b}.diff"; ok=0
    fi
  done
  for prep in "$HOLD"/*_report.txt; do
    [ -e "$prep" ] || continue
    local b; b=$(basename "$prep"); local rrep="$OUT/$b"
    if [ -f "$rrep" ] && diff <(filter_report "$prep") <(filter_report "$rrep") >"$BASE/${name}.${b}.diff" 2>&1; then
      echo "  REPORT $b: identical (modulo wall-clock)"
    else
      echo "  REPORT $b: !!! DIFF/MISSING -> ${name}.${b}.diff"; ok=0
    fi
  done
  if [ "$ok" = 1 ]; then echo "  CELL $name: PASS"; else echo "  CELL $name: FAIL"; FAILED=1; fi
}

# Multicore cell: --multicore/--parallel + --hisat2 must be HARD-REJECTED.
# HISAT2's splice-site discovery is input-batch-global, so chunking the reads
# changes the alignments — the chunked output is NOT byte-identical to Perl (Perl
# itself is not worker-invariant here; verified on the 1M oxy subset:
# single-core 1310 spliced vs --multicore 8 1219). The faithful HISAT2 path is
# single-core, so bismark_rs fails loudly. This cell asserts that reject.
run_multicore_cell () {
  echo "=================== CELL se_multicore_reject (N=$N) ==================="
  local O8="$BASE/mc_p8" T8="$BASE/mc_p8_tmp"
  rm -rf "$O8" "$T8"; mkdir -p "$O8" "$T8"
  "$RUST" --hisat2 --path_to_hisat2 "$ENVBIN" --genome "$GENOME" -o "$O8" --temp_dir "$T8" \
    --parallel 8 "$BASE/se.fq" >"$BASE/mc_p8.log" 2>&1
  local rc=$?
  if [ "$rc" != 0 ] && grep -q "not supported with --hisat2" "$BASE/mc_p8.log"; then
    echo "  --parallel 8 + --hisat2: correctly REJECTED (exit $rc, splice-discovery message)"
    echo "  CELL se_multicore_reject: PASS"
  else
    echo "  !!! --parallel 8 + --hisat2 was NOT rejected (exit $rc) — see mc_p8.log"; FAILED=1
    echo "  CELL se_multicore_reject: FAIL"
  fi
}

# Cell selection: run only the cells named as args after N (default: all).
CELLS="${*:2}"; [ -z "$CELLS" ] && CELLS="se_dir se_nondir se_pbat se_fasta se_multicore"
for cell in $CELLS; do
  case "$cell" in
    se_dir)       run_cell se_dir    "$BASE/se.fq" ;;
    se_nondir)    run_cell se_nondir --non_directional "$BASE/se.fq" ;;
    se_pbat)      run_cell se_pbat   --pbat "$BASE/se.fq" ;;
    se_fasta)     run_cell se_fasta  -f "$BASE/se.fa" ;;
    se_multicore) run_multicore_cell ;;
    *) echo "unknown cell: $cell"; FAILED=1 ;;
  esac
done

echo "=========================================================="
if [ "$FAILED" = 0 ]; then echo "PHASE-2a HISAT2 SE GATE (N=$N): ALL CELLS PASS"; else echo "PHASE-2a HISAT2 SE GATE (N=$N): FAILURES PRESENT"; fi
exit $FAILED
