#!/bin/bash
# ==========================================================================
# Phase-5 (v1.x) combined real-data gate — 10M single-core strict.
#
# For each cell: Perl Bismark v0.25.1 + the pinned aligner (single-core) vs
# bismark_rs (single-core), IDENTICAL argv into the SAME -o (Perl moved aside).
# Compare DECOMPRESSED SAM (keep the Bismark @PG — identical argv → it matches —
# drop only the samtools @PG line) + the report (wall-clock filtered) via the
# Phase-10 STREAMING `cmp` comparator (NOT a buffering `diff`), with a non-empty
# + count-equality backstop so a vacuous/truncated stream can't "pass".
#
# Backends: Bowtie 2 (anchor) / HISAT2 (single-core only — NOT worker-invariant) /
# minimap2 SE (worker-invariant → a bonus --parallel-P==--parallel-1 leg).
# Genomes: human GRCh38 + mouse GRCm39 (RRBS). minimap2 is SE-only.
# `ht2_pe_pbat` uses the R1<->R2 swap (R2 as -1, R1 as -2, --pbat) for a REAL
# pbat signal (else directional data lands ~0 reads on the complementary strand).
#
# Usage: phase5_combined_gate.sh [P] [N] [CELLS]
#   P     worker count for the minimap2 worker leg / Bowtie2-mm2 --parallel (default 8)
#   N     subset reads per input (default 10000000)
#   CELLS space-separated subset (default: all)
# ==========================================================================
set -uo pipefail
export LC_ALL=C

P="${1:-8}"
N="${2:-10000000}"
CELLS="${3:-bt2_se_dir bt2_pe_dir mm2_se_dir mm2_se_nondir mm2_se_pbat ht2_se_dir ht2_se_nondir ht2_se_pbat ht2_pe_dir ht2_pe_nondir ht2_pe_pbat rrbs_bt2_pe_dir rrbs_ht2_pe_dir}"

ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
# Prefer the /home recycle-insurance copy; fall back to the /var/tmp build.
RUST=$HOME/bismark_rs_p5
[ -x "$RUST" ] || RUST=/var/tmp/mm2_gate/rust/target/release/bismark_rs

HG=$HOME/bismark_benchmarks/genome              # human GRCh38
MM=$HOME/bismark_benchmarks/RRBS_PE/genome      # mouse GRCm39
SE_FQ=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
PE1_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R1_val_1.fq.gz
PE2_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R2_val_2.fq.gz
RR1_FQ=$HOME/bismark_benchmarks/RRBS_PE/SRR24766921_GSM7433369_Colon_3_Months_Rep1_Mus_musculus_RRBS_R1.fastq.gz
RR2_FQ=$HOME/bismark_benchmarks/RRBS_PE/SRR24766921_GSM7433369_Colon_3_Months_Rep1_Mus_musculus_RRBS_R2.fastq.gz
BASE=/var/tmp/mm2_p5_gate
mkdir -p "$BASE/in" "$BASE/sorttmp"

echo "================= Phase-5 v1.x COMBINED GATE ================="
echo "P=$P  N=$N  CELLS=$CELLS"
echo "RUST=$RUST"; "$RUST" --version 2>&1 | grep -i version | head -1 || true
echo "date: $(date -u)"; echo

# --- stage subsets off the S3 mount into $BASE/in (idempotent: skip if present) ---
stage () { [ -s "$2" ] || zcat "$1" | head -n $((4 * N)) > "$2"; }
stage "$SE_FQ"  "$BASE/in/se.fq"
stage "$PE1_FQ" "$BASE/in/pe_1.fq"
stage "$PE2_FQ" "$BASE/in/pe_2.fq"
stage "$RR1_FQ" "$BASE/in/rr_1.fq"
stage "$RR2_FQ" "$BASE/in/rr_2.fq"
echo "staged: se=$(($(wc -l <"$BASE/in/se.fq")/4)) pe=$(($(wc -l <"$BASE/in/pe_1.fq")/4)) rrbs=$(($(wc -l <"$BASE/in/rr_1.fq")/4))"
echo

FAILED=0

# --- comparators (LC_ALL=C; streaming cmp + bounded sed-window on mismatch) ---
cmp_files () {  # <a> <b> <label>
  local a="$1" b="$2" label="$3"
  if cmp -s "$a" "$b"; then echo "      ok   $label ($(wc -l <"$a") lines)"; return 0; fi
  echo "      !!!  DIFF $label"; FAILED=1
  local off ln; off=$(cmp "$a" "$b" 2>/dev/null | sed -n 's/.*char \([0-9]*\).*/\1/p')
  ln=1; [ -n "$off" ] && ln=$(head -c "$off" "$a" | wc -l); ln=$((ln<1?1:ln))
  echo "      first diff ~line $ln (byte ${off:-?}):"
  diff <(sed -n "$((ln>3?ln-3:1)),$((ln+3))p" "$a") <(sed -n "$((ln>3?ln-3:1)),$((ln+3))p" "$b") 2>/dev/null | head -14 | sed 's/^/        /'
  return 1
}
n_sam_strict () { samtools view -h "$1" | grep -v 'ID:samtools' > "$2"; }   # keep Bismark @PG, drop samtools @PG
n_report ()     { grep -v '^Bismark completed in ' "$1" > "$2"; }

# run_strict <name> <genome> "<aligner+library args>" "<read args>"
run_strict () {
  local name="$1" G="$2" AARGS="$3" RARGS="$4"
  local OUT="$BASE/$name" HOLD="$BASE/${name}_perl" TMP="$BASE/${name}_tmp"
  rm -rf "$OUT" "$HOLD" "$TMP"; mkdir -p "$OUT" "$HOLD" "$TMP"
  echo "=================== CELL $name (N=$N, single-core) ==================="; date +%T
  # Perl single-core (no --multicore/--parallel).
  bismark $AARGS --genome "$G" -o "$OUT" --temp_dir "$TMP" $RARGS > "$BASE/${name}.perl.log" 2>&1
  local prc=$?
  mv "$OUT"/* "$HOLD"/ 2>/dev/null; rm -rf "${TMP:?}"/*
  # Rust single-core (default; no --parallel).
  "$RUST" $AARGS --genome "$G" -o "$OUT" --temp_dir "$TMP" $RARGS > "$BASE/${name}.rust.log" 2>&1
  local rrc=$?
  date +%T; echo "  exit: perl=$prc rust=$rrc"
  if [ "$prc" != 0 ] || [ "$rrc" != 0 ]; then echo "  CELL $name: FAIL (non-zero exit; see ${name}.{perl,rust}.log)"; FAILED=1; return; fi
  local ok=1 pbam b rbam pc rc
  for pbam in "$HOLD"/*.bam; do
    [ -e "$pbam" ] || continue; b=$(basename "$pbam"); rbam="$OUT/$b"
    if [ ! -f "$rbam" ]; then echo "      !!!  BAM $b MISSING on rust side"; ok=0; continue; fi
    # backstop: non-empty + count equality (an in-order cmp subsumes count/header
    # ONLY if it ran to completion on non-empty input).
    pc=$(samtools view -c "$pbam"); rc=$(samtools view -c "$rbam")
    if [ "$pc" = 0 ] || [ "$rc" = 0 ]; then echo "      !!!  BAM $b EMPTY (perl=$pc rust=$rc) — vacuous, fail"; ok=0; FAILED=1; continue; fi
    if [ "$pc" != "$rc" ]; then echo "      !!!  BAM $b COUNT perl=$pc rust=$rc"; ok=0; FAILED=1; continue; fi
    n_sam_strict "$pbam" "$BASE/$name.$b.perl.sam"; n_sam_strict "$rbam" "$BASE/$name.$b.rust.sam"
    cmp_files "$BASE/$name.$b.perl.sam" "$BASE/$name.$b.rust.sam" "BAM $b ($rc rec)" || ok=0
  done
  for prep in "$HOLD"/*_report.txt; do
    [ -e "$prep" ] || continue; b=$(basename "$prep")
    if [ ! -f "$OUT/$b" ]; then echo "      !!!  REPORT $b MISSING on rust side"; ok=0; continue; fi
    n_report "$prep" "$BASE/$name.$b.perl.rep"; n_report "$OUT/$b" "$BASE/$name.$b.rust.rep"
    cmp_files "$BASE/$name.$b.perl.rep" "$BASE/$name.$b.rust.rep" "REPORT $b" || ok=0
  done
  if [ "$ok" = 1 ]; then echo "  CELL $name: PASS"; else echo "  CELL $name: FAIL"; fi
}

# minimap2 worker-invariance leg (Rust --parallel P body == --parallel 1 body).
run_mm2_worker () {
  echo "=================== CELL mm2_se_worker (N=$N) ==================="; date +%T
  local O1="$BASE/mm2w_p1" O8="$BASE/mm2w_p8" T1="$BASE/mm2w_p1_tmp" T8="$BASE/mm2w_p8_tmp"
  rm -rf "$O1" "$O8" "$T1" "$T8"; mkdir -p "$O1" "$O8" "$T1" "$T8"
  "$RUST" --minimap2 --path_to_minimap2 "$ENVBIN" --genome "$HG" -o "$O1" --temp_dir "$T1" --parallel 1 "$BASE/in/se.fq" > "$BASE/mm2w_p1.log" 2>&1; local r1=$?
  "$RUST" --minimap2 --path_to_minimap2 "$ENVBIN" --genome "$HG" -o "$O8" --temp_dir "$T8" --parallel "$P" "$BASE/in/se.fq" > "$BASE/mm2w_p8.log" 2>&1; local r8=$?
  date +%T; echo "  exit: p1=$r1 p$P=$r8"
  if [ "$r1" != 0 ] || [ "$r8" != 0 ]; then echo "  CELL mm2_se_worker: FAIL (non-zero exit)"; FAILED=1; return; fi
  local b1 b8; b1=$(ls "$O1"/*.bam); b8=$(ls "$O8"/*.bam)
  samtools view "$b1" > "$BASE/mm2w_p1.body"; samtools view "$b8" > "$BASE/mm2w_p8.body"
  if cmp -s "$BASE/mm2w_p1.body" "$BASE/mm2w_p8.body"; then
    echo "  --parallel $P == --parallel 1 (body, $(wc -l <"$BASE/mm2w_p8.body") rec): worker-invariant"; echo "  CELL mm2_se_worker: PASS"
  else
    # in-order guard: distinguish reorder-only from real content divergence.
    local m1 m8; m1=$(sort "$BASE/mm2w_p1.body"|md5sum|cut -d' ' -f1); m8=$(sort "$BASE/mm2w_p8.body"|md5sum|cut -d' ' -f1)
    echo "  !!!  body cmp DIFF; sorted-multiset p1=$m1 p$P=$m8 ($([ "$m1" = "$m8" ] && echo REORDER-ONLY || echo CONTENT-DIVERGENCE))"; FAILED=1
    echo "  CELL mm2_se_worker: FAIL"
  fi
}

for cell in $CELLS; do
  case "$cell" in
    bt2_se_dir)      run_strict bt2_se_dir   "$HG" "--path_to_bowtie2 $ENVBIN" "$BASE/in/se.fq" ;;
    bt2_pe_dir)      run_strict bt2_pe_dir   "$HG" "--path_to_bowtie2 $ENVBIN" "-1 $BASE/in/pe_1.fq -2 $BASE/in/pe_2.fq" ;;
    mm2_se_dir)      run_strict mm2_se_dir   "$HG" "--minimap2 --path_to_minimap2 $ENVBIN" "$BASE/in/se.fq"; run_mm2_worker ;;
    mm2_se_nondir)   run_strict mm2_se_nondir "$HG" "--minimap2 --path_to_minimap2 $ENVBIN --non_directional" "$BASE/in/se.fq" ;;
    mm2_se_pbat)     run_strict mm2_se_pbat  "$HG" "--minimap2 --path_to_minimap2 $ENVBIN --pbat" "$BASE/in/se.fq" ;;
    ht2_se_dir)      run_strict ht2_se_dir   "$HG" "--hisat2 --path_to_hisat2 $ENVBIN" "$BASE/in/se.fq" ;;
    ht2_se_nondir)   run_strict ht2_se_nondir "$HG" "--hisat2 --path_to_hisat2 $ENVBIN --non_directional" "$BASE/in/se.fq" ;;
    ht2_se_pbat)     run_strict ht2_se_pbat  "$HG" "--hisat2 --path_to_hisat2 $ENVBIN --pbat" "$BASE/in/se.fq" ;;
    ht2_pe_dir)      run_strict ht2_pe_dir   "$HG" "--hisat2 --path_to_hisat2 $ENVBIN" "-1 $BASE/in/pe_1.fq -2 $BASE/in/pe_2.fq" ;;
    ht2_pe_nondir)   run_strict ht2_pe_nondir "$HG" "--hisat2 --path_to_hisat2 $ENVBIN --non_directional" "-1 $BASE/in/pe_1.fq -2 $BASE/in/pe_2.fq" ;;
    # R1<->R2 swap → directional data aligns as genuine pbat (CTOT/CTOB).
    ht2_pe_pbat)     run_strict ht2_pe_pbat  "$HG" "--hisat2 --path_to_hisat2 $ENVBIN --pbat" "-1 $BASE/in/pe_2.fq -2 $BASE/in/pe_1.fq" ;;
    rrbs_bt2_pe_dir) run_strict rrbs_bt2_pe_dir "$MM" "--path_to_bowtie2 $ENVBIN" "-1 $BASE/in/rr_1.fq -2 $BASE/in/rr_2.fq" ;;
    rrbs_ht2_pe_dir) run_strict rrbs_ht2_pe_dir "$MM" "--hisat2 --path_to_hisat2 $ENVBIN" "-1 $BASE/in/rr_1.fq -2 $BASE/in/rr_2.fq" ;;
    *) echo "unknown cell: $cell"; FAILED=1 ;;
  esac
done

echo "=========================================================="
if [ "$FAILED" = 0 ]; then echo "PHASE-5 COMBINED GATE (N=$N): ALL CELLS PASS"; else echo "PHASE-5 COMBINED GATE (N=$N): FAILURES PRESENT"; fi
exit $FAILED
