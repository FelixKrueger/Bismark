#!/bin/bash
# Phase-2b oxy PE byte-identity gate (HISAT2 backend, single-core).
# Runs Perl bismark v0.25.1 --hisat2 + HISAT2 2.2.2 and the Rust bismark_rs
# --hisat2 with IDENTICAL argv into the SAME -o (Perl moved aside), then diffs
# DECOMPRESSED SAM (samtools @PG filtered) + the _PE_report.txt (wall-clock
# filtered) + the gzipped --unmapped/--ambiguous aux (decompressed). Cells:
# PE {directional, non-dir, pbat}, FastA PE {dir, non-dir}, and a single-core
# --ambig_bam PE cell (directional) — the ONLY PE path that re-emits the raw
# aligner record (output.rs build_raw_record), never byte-gated for HISAT2.
#
#   PE naming: <r1base>_bismark_hisat2_pe.bam / _bismark_hisat2_PE_report.txt
#   (a basename match between Perl/Rust outputs IS the naming-token check.)
#   No --multicore PE cell — --multicore+--hisat2 is rejected (2a).
#
# Usage: phase2b_hisat2_pe_gate.sh <N> [cells...]   e.g.  phase2b_hisat2_pe_gate.sh 10000
set -uo pipefail

N="${1:-10000}"
ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
GENOME=$HOME/bismark_benchmarks/genome
RUST=/var/tmp/aligner_2a/rust/target/release/bismark_rs
PE1_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R1_val_1.fq.gz
PE2_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R2_val_2.fq.gz
BASE=/var/tmp/aligner_2b_gate
rm -rf "$BASE"; mkdir -p "$BASE"

# Subset: first N pairs (uncompressed .fq) + FastA twins (@id->>id, drop +/qual).
zcat "$PE1_FQ" | head -n $((4 * N)) > "$BASE/pe_1.fq"
zcat "$PE2_FQ" | head -n $((4 * N)) > "$BASE/pe_2.fq"
awk 'NR%4==1{print ">" substr($0,2)} NR%4==2{print}' "$BASE/pe_1.fq" > "$BASE/pe_1.fa"
awk 'NR%4==1{print ">" substr($0,2)} NR%4==2{print}' "$BASE/pe_2.fq" > "$BASE/pe_2.fa"
echo "subset: $(($(wc -l <"$BASE/pe_1.fq")/4)) pairs"

filter_sam ()    { samtools view -h "$1" | grep -v 'ID:samtools'; }
filter_report () { grep -v '^Bismark completed in ' "$1"; }

FAILED=0
# Perl-vs-Rust cell, identical argv into the same -o (Perl moved aside).
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
  # gzipped aux (--unmapped / --ambiguous): compare DECOMPRESSED (flate2 != gzip bytes).
  for paux in "$HOLD"/*_reads*.fq.gz; do
    [ -e "$paux" ] || continue
    local b; b=$(basename "$paux"); local raux="$OUT/$b"
    if [ -f "$raux" ] && diff <(zcat "$paux") <(zcat "$raux") >"$BASE/${name}.${b}.diff" 2>&1; then
      echo "  AUX $b: identical (decompressed)"
    else
      echo "  AUX $b: !!! DIFF/MISSING -> ${name}.${b}.diff"; ok=0
    fi
  done
  if [ "$ok" = 1 ]; then echo "  CELL $name: PASS"; else echo "  CELL $name: FAIL"; FAILED=1; fi
}

CELLS="${*:2}"
[ -z "$CELLS" ] && CELLS="pe_dir pe_nondir pe_pbat pe_fasta_dir pe_fasta_nondir pe_ambig_dir"
for cell in $CELLS; do
  case "$cell" in
    pe_dir)          run_cell pe_dir          -1 "$BASE/pe_1.fq" -2 "$BASE/pe_2.fq" ;;
    pe_nondir)       run_cell pe_nondir       --non_directional -1 "$BASE/pe_1.fq" -2 "$BASE/pe_2.fq" ;;
    pe_pbat)         run_cell pe_pbat         --pbat -1 "$BASE/pe_1.fq" -2 "$BASE/pe_2.fq" ;;
    pe_fasta_dir)    run_cell pe_fasta_dir    -f -1 "$BASE/pe_1.fa" -2 "$BASE/pe_2.fa" ;;
    pe_fasta_nondir) run_cell pe_fasta_nondir -f --non_directional -1 "$BASE/pe_1.fa" -2 "$BASE/pe_2.fa" ;;
    pe_ambig_dir)    run_cell pe_ambig_dir    --ambig_bam --unmapped --ambiguous -1 "$BASE/pe_1.fq" -2 "$BASE/pe_2.fq" ;;
    *) echo "unknown cell: $cell"; FAILED=1 ;;
  esac
done

echo "=========================================================="
if [ "$FAILED" = 0 ]; then echo "PHASE-2b HISAT2 PE GATE (N=$N): ALL CELLS PASS"; else echo "PHASE-2b HISAT2 PE GATE (N=$N): FAILURES PRESENT"; fi
exit $FAILED
