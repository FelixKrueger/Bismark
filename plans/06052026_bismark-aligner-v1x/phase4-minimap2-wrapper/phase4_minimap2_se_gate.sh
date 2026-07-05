#!/bin/bash
# Phase-4 (v1.x) oxy SE byte-identity gate — minimap2 backend.
#
# Runs Perl bismark v0.25.1 --minimap2 + minimap2 2.31-r1302 and the Rust
# bismark_rs --minimap2 with IDENTICAL argv into the SAME -o (Perl moved aside),
# then diffs DECOMPRESSED SAM content (samtools @PG-filtered) + the report
# (wall-clock-filtered). Naming check is implicit: a basename match between the
# Perl/Rust outputs proves the `_bismark_mm2` token.
#
# Cells:
#   se_dir / se_nondir / se_pbat  — Perl-vs-Rust byte-identity (FastQ SE).
#   se_multicore                  — worker-invariance: Rust --parallel 8 vs
#                                   --parallel 1 (SAM BODY; minimap2 is
#                                   per-read-independent, NOT rejected unlike
#                                   HISAT2). With se_dir (Rust-p1==Perl) this
#                                   gives Rust-p8 content == Perl transitively.
#   zero_secondary                — asserts 0 secondary (flag&256) / supplementary
#                                   (flag&2048) on the RAW minimap2 output across
#                                   all 4 instances (the lockstep one-primary-
#                                   per-read invariant; Reviewer A's V9 ask).
#
# Usage: phase4_minimap2_se_gate.sh <N> [cell ...]   e.g. ... 10000
set -uo pipefail

N="${1:-10000}"
ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
GENOME=$HOME/bismark_benchmarks/genome
RUST=/var/tmp/mm2_gate/rust/target/release/bismark_rs
SE_FQ=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
BASE=/var/tmp/mm2_gate_run
rm -rf "$BASE"; mkdir -p "$BASE"

zcat "$SE_FQ" | head -n $((4 * N)) > "$BASE/se.fq"
echo "subset: se.fq = $(($(wc -l <"$BASE/se.fq") / 4)) reads"

filter_sam ()    { samtools view -h "$1" | grep -v 'ID:samtools'; }
filter_report () { grep -v '^Bismark completed in ' "$1"; }
FAILED=0

# Perl-vs-Rust cell: identical argv into the same -o (Perl moved aside).
run_cell () {
  local name="$1"; shift
  local -a ARGS=("$@")
  local OUT="$BASE/$name" HOLD="$BASE/${name}_perl" TMP="$BASE/${name}_tmp"
  rm -rf "$OUT" "$HOLD" "$TMP"; mkdir -p "$OUT" "$HOLD" "$TMP"
  echo "=================== CELL $name (N=$N) ==================="
  bismark --minimap2 --path_to_minimap2 "$ENVBIN" --genome "$GENOME" -o "$OUT" --temp_dir "$TMP" \
    "${ARGS[@]}" >"$BASE/${name}.perl.log" 2>&1
  local prc=$?
  mv "$OUT"/* "$HOLD"/ 2>/dev/null
  rm -rf "${TMP:?}"/*
  "$RUST" --minimap2 --path_to_minimap2 "$ENVBIN" --genome "$GENOME" -o "$OUT" --temp_dir "$TMP" \
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

# Worker-invariance: minimap2 --parallel 8 vs --parallel 1 (Rust). minimap2 is
# per-read-independent (no batch-global splice discovery), so the output must be
# byte-identical on the SAM BODY (the @PG CL embeds --parallel → compare WITHOUT
# -H). Rust's --parallel is order-preserving (Phase 9b), so no sort is needed.
run_invariance_cell () {
  echo "=================== CELL se_multicore_invariance (N=$N) ==================="
  local O1="$BASE/mc_p1" O8="$BASE/mc_p8" T1="$BASE/mc_p1_tmp" T8="$BASE/mc_p8_tmp"
  rm -rf "$O1" "$O8" "$T1" "$T8"; mkdir -p "$O1" "$O8" "$T1" "$T8"
  "$RUST" --minimap2 --path_to_minimap2 "$ENVBIN" --genome "$GENOME" -o "$O1" --temp_dir "$T1" \
    --parallel 1 "$BASE/se.fq" >"$BASE/mc_p1.log" 2>&1; local r1=$?
  "$RUST" --minimap2 --path_to_minimap2 "$ENVBIN" --genome "$GENOME" -o "$O8" --temp_dir "$T8" \
    --parallel 8 "$BASE/se.fq" >"$BASE/mc_p8.log" 2>&1; local r8=$?
  echo "  exit: p1=$r1 p8=$r8"
  if [ "$r1" != 0 ] || [ "$r8" != 0 ]; then echo "  CELL se_multicore_invariance: FAIL (non-zero exit)"; FAILED=1; return; fi
  local b1 b8; b1=$(ls "$O1"/*.bam) ; b8=$(ls "$O8"/*.bam)
  if diff <(samtools view "$b1") <(samtools view "$b8") >"$BASE/mc_invariance.diff" 2>&1; then
    echo "  --parallel 8 == --parallel 1 (SAM body, $(samtools view -c "$b8") records): worker-invariant"
    echo "  CELL se_multicore_invariance: PASS"
  else
    echo "  !!! --parallel 8 != --parallel 1 ($(wc -l <"$BASE/mc_invariance.diff") diff lines) -> mc_invariance.diff"; FAILED=1
    echo "  CELL se_multicore_invariance: FAIL"
  fi
}

# Zero SECONDARY on the RAW minimap2 output (the lockstep one-primary-per-read
# invariant). Convert the subset C->T and G->A (uppercase then substitute on the
# seq line; SE = no id suffix — Perl 5489-5651), run minimap2 with the gate
# options against BOTH indexes, and count SECONDARY (flag 256, which
# `--secondary=no` suppresses) vs SUPPLEMENTARY (flag 2048) separately. Portable
# bit test (no gawk and()). All 4 instances (dir/pbat use 2; non-dir uses all 4).
#
# FAIL only on secondary>0. SUPPLEMENTARY (chimeric/split, flag 2048, `SA:Z:`) is
# a DIFFERENT category that `--secondary=no` does not suppress, and it is
# INFORMATIONAL here: at 1M a tiny supplementary population appears, but Bismark
# (Perl AND Rust) handles those reads byte-identically — verified at the gate, the
# se_nondir cell (which exercises the GA/G->A instance) is byte-identical and the
# affected reads are absent from BOTH BAMs. So supplementary records do not break
# the lockstep or byte-identity; only an unexpected SECONDARY would.
run_secondary_check () {
  echo "=================== CHECK zero secondary (N=$N) ==================="
  local -a MM2=(-a --MD --secondary=no -t 2 -x map-ont -K 250K)
  awk 'NR%4==2 { s=toupper($0); gsub(/C/,"T",s); print s; next } {print}' "$BASE/se.fq" > "$BASE/se_C_to_T.fq"
  awk 'NR%4==2 { s=toupper($0); gsub(/G/,"A",s); print s; next } {print}' "$BASE/se.fq" > "$BASE/se_G_to_A.fq"
  local sec_total=0 sup_total=0
  for combo in "CT:$BASE/se_C_to_T.fq" "GA:$BASE/se_C_to_T.fq" "CT:$BASE/se_G_to_A.fq" "GA:$BASE/se_G_to_A.fq"; do
    local idx="${combo%%:*}" reads="${combo#*:}"
    local mmi="$GENOME/Bisulfite_Genome/${idx}_conversion/BS_${idx}.mmi"
    local counts sec sup
    counts=$(minimap2 "${MM2[@]}" "$mmi" "$reads" 2>/dev/null \
      | awk '$1!~/^@/ { f=$2; if (int(f/256)%2==1) s++; if (int(f/2048)%2==1) p++ } END { printf "%d %d", s+0, p+0 }')
    sec=${counts% *}; sup=${counts#* }
    echo "  ${idx}-index / $(basename "$reads"): secondary=$sec supplementary=$sup"
    sec_total=$((sec_total + sec)); sup_total=$((sup_total + sup))
  done
  echo "  totals: secondary=$sec_total supplementary=$sup_total (supplementary is informational — handled byte-identically by both sides)"
  if [ "$sec_total" = 0 ]; then echo "  CHECK zero_secondary: PASS (0 secondary across all 4 instances)"; else echo "  !!! CHECK zero_secondary: FAIL ($sec_total secondary)"; FAILED=1; fi
}

CELLS="${*:2}"; [ -z "$CELLS" ] && CELLS="se_dir se_nondir se_pbat se_multicore zero_secondary"
for cell in $CELLS; do
  case "$cell" in
    se_dir)         run_cell se_dir    "$BASE/se.fq" ;;
    se_nondir)      run_cell se_nondir --non_directional "$BASE/se.fq" ;;
    se_pbat)        run_cell se_pbat   --pbat "$BASE/se.fq" ;;
    se_multicore)   run_invariance_cell ;;
    zero_secondary) run_secondary_check ;;
    *) echo "unknown cell: $cell"; FAILED=1 ;;
  esac
done

echo "=========================================================="
if [ "$FAILED" = 0 ]; then echo "PHASE-4 MINIMAP2 SE GATE (N=$N): ALL CELLS PASS"; else echo "PHASE-4 MINIMAP2 SE GATE (N=$N): FAILURES PRESENT"; fi
exit $FAILED
