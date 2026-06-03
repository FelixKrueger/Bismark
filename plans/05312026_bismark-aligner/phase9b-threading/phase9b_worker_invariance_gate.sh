#!/bin/bash
# Phase-9b oxy WORKER-INVARIANCE gate (--multicore/--parallel).
#
# Gate (PLAN §1): for any N, the Rust output is byte-identical across worker counts
# AND equals Perl SINGLE-CORE:
#     bismark_rs --parallel 4  ==  bismark_rs --parallel 1  ==  Perl bismark (single-core)
#
# 🔴 A-Imp6: the Perl oracle is run WITHOUT --multicore (single-core). Perl's OWN
# --multicore N stripes reads (fork+modulo) → a reordered merge that is NOT identical
# to its single-core output; passing --parallel to Perl would fail the diff for the
# wrong reason. So: Rust gets --parallel {1,4}; Perl gets the SAME argv MINUS --parallel.
#
# Comparisons per cell:
#   • Rust --parallel 1 vs Perl single-core: DECOMPRESSED SAM (@PG filtered — see below)
#     + report (wall-clock filtered) + aux DECOMPRESSED content (Perl uses external
#     gzip → only decompressed content can match across the Perl/Rust boundary).
#   • Rust --parallel 4 vs Rust --parallel 1: the worker-invariance half — decompressed
#     SAM + report + aux DECOMPRESSED content. (Gz framing is an impl detail like BGZF for
#     the BAM: the N==1 inline-incremental encoder vs the N>1 bulk-merge encoder give
#     equivalent gz with different block boundaries at scale — decompressed content is the
#     invariant, NOT raw gz bytes.)
#
# Use a read count NOT divisible by the worker count so a chunk boundary is straddled
# (e.g. 1000003). Usage: phase9b_worker_invariance_gate.sh <N> [PAR]   (PAR default 4)
set -uo pipefail

N="${1:-1000003}"        # coprime-ish to {2,4,8} → boundary straddled at every N
PAR="${2:-4}"
ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
GENOME=$HOME/bismark_benchmarks/genome
RUST=/var/tmp/aligner_p9b/target/release/bismark_rs
SE_FQ=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
PE1_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R1_val_1.fq.gz
PE2_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R2_val_2.fq.gz
BASE=/var/tmp/aligner_p9b_gate
rm -rf "$BASE"; mkdir -p "$BASE"

# First N FastQ reads (4 lines each), kept as FastQ.
subset () { zcat "$1" | head -n $((4 * N)); }
subset "$SE_FQ"  > "$BASE/se.fq"
subset "$PE1_FQ" > "$BASE/pe_1.fq"
subset "$PE2_FQ" > "$BASE/pe_2.fq"
echo "subset: se.fq=$(($(wc -l <"$BASE/se.fq")/4)) reads, pe=$(($(wc -l <"$BASE/pe_1.fq")/4)) pairs; PAR=$PAR"

# Drop the WHOLE @PG block (env/argv-specific metadata): the samtools line embeds an
# abs path, and the Bismark `@PG CL:"bismark <argv>"` faithfully records the per-run
# argv — incl. the `--parallel N` value being VARIED and the harness's per-run -o/
# --temp_dir. A worker-invariance gate compares the ALIGNMENT (records + @HD/@SQ), not
# the @PG metadata (which legitimately differs by --parallel). @HD/@SQ + records remain.
filter_sam ()    { samtools view -h "$1" | grep -v '^@PG'; }
filter_report () { grep -v '^Bismark completed in ' "$1"; }

FAILED=0
# run_cell <name> <args...> — <args> are the library/read flags shared by all 3 runs
# (e.g. "--non_directional -1 a.fq -2 b.fq"). Perl gets them verbatim (no --parallel);
# Rust gets them + --parallel {1,PAR}.
run_cell () {
  local name="$1"; shift
  local -a ARGS=("$@")
  local PERL="$BASE/${name}_perl" R1="$BASE/${name}_rust_p1" RP="$BASE/${name}_rust_p${PAR}"
  local TMP="$BASE/${name}_tmp"
  rm -rf "$PERL" "$R1" "$RP" "$TMP"; mkdir -p "$PERL" "$R1" "$RP" "$TMP"
  echo "=================== CELL $name (N=$N, PAR=$PAR) ==================="

  bismark --genome "$GENOME" -o "$PERL" --temp_dir "$TMP/p" --path_to_bowtie2 "$ENVBIN" \
    "${ARGS[@]}" >"$BASE/${name}.perl.log" 2>&1; local prc=$?
  "$RUST" --genome "$GENOME" -o "$R1" --temp_dir "$TMP/r1" --path_to_bowtie2 "$ENVBIN" \
    --parallel 1 "${ARGS[@]}" >"$BASE/${name}.rust_p1.log" 2>&1; local r1rc=$?
  "$RUST" --genome "$GENOME" -o "$RP" --temp_dir "$TMP/rp" --path_to_bowtie2 "$ENVBIN" \
    --parallel "$PAR" "${ARGS[@]}" >"$BASE/${name}.rust_p${PAR}.log" 2>&1; local rprc=$?
  echo "  exit: perl=$prc rust_p1=$r1rc rust_p${PAR}=$rprc"
  if [ "$prc" != 0 ] || [ "$r1rc" != 0 ] || [ "$rprc" != 0 ]; then
    echo "  CELL $name: FAIL (non-zero exit)"; FAILED=1; return
  fi

  local ok=1
  # BAMs: decompressed SAM identical across Perl / Rust-p1 / Rust-pPAR.
  for pbam in "$PERL"/*.bam; do
    [ -e "$pbam" ] || continue
    local b; b=$(basename "$pbam")
    if ! diff <(filter_sam "$pbam") <(filter_sam "$R1/$b") >"$BASE/${name}.${b}.p1.diff" 2>&1; then
      echo "  BAM $b: !!! rust_p1 != Perl -> ${name}.${b}.p1.diff"; ok=0
    fi
    if ! diff <(filter_sam "$R1/$b") <(filter_sam "$RP/$b") >"$BASE/${name}.${b}.par.diff" 2>&1; then
      echo "  BAM $b: !!! rust_p${PAR} != rust_p1 (WORKER-VARIANT!) -> ${name}.${b}.par.diff"; ok=0
    fi
    [ "$ok" = 1 ] && echo "  BAM $b: byte-identical p1==pPAR==Perl ($(samtools view -c "$R1/$b") records)"
  done
  # Reports: identical (modulo wall-clock).
  for prep in "$PERL"/*_report.txt; do
    [ -e "$prep" ] || continue
    local b; b=$(basename "$prep")
    diff <(filter_report "$prep") <(filter_report "$R1/$b") >"$BASE/${name}.${b}.p1.diff" 2>&1 || { echo "  REPORT $b: !!! rust_p1 != Perl"; ok=0; }
    diff <(filter_report "$R1/$b") <(filter_report "$RP/$b") >"$BASE/${name}.${b}.par.diff" 2>&1 || { echo "  REPORT $b: !!! rust_p${PAR} != rust_p1"; ok=0; }
  done
  # Aux (.fq.gz): DECOMPRESSED content == Perl / p1 / pPAR. (Gz framing is an impl detail,
  # like BGZF for the BAM — the N==1 inline-incremental encoder and the N>1 bulk-merge
  # encoder produce equivalent gz with different block boundaries at scale; what must be
  # invariant is the decompressed reads + their order.)
  for raux in "$R1"/*.fq.gz; do
    [ -e "$raux" ] || continue
    local b; b=$(basename "$raux")
    if [ -f "$PERL/$b" ] && ! diff <(zcat "$PERL/$b") <(zcat "$R1/$b") >/dev/null 2>&1; then
      echo "  AUX $b: !!! rust_p1 decompressed != Perl"; ok=0
    fi
    if ! diff <(zcat "$R1/$b") <(zcat "$RP/$b") >/dev/null 2>&1; then
      echo "  AUX $b: !!! rust_p${PAR} decompressed != rust_p1 (WORKER-VARIANT!)"; ok=0
    fi
  done

  if [ "$ok" = 1 ]; then echo "  CELL $name: PASS"; else echo "  CELL $name: FAIL"; FAILED=1; fi
}

run_cell se_dir      --unmapped --ambiguous --ambig_bam "$BASE/se.fq"
run_cell se_nondir   --non_directional --unmapped --ambiguous --ambig_bam "$BASE/se.fq"
run_cell se_pbat     --pbat --unmapped --ambiguous --ambig_bam "$BASE/se.fq"
run_cell pe_dir      --unmapped --ambiguous --ambig_bam -1 "$BASE/pe_1.fq" -2 "$BASE/pe_2.fq"
run_cell pe_nondir   --non_directional --unmapped --ambiguous --ambig_bam -1 "$BASE/pe_1.fq" -2 "$BASE/pe_2.fq"
run_cell pe_pbat     --pbat --unmapped --ambiguous --ambig_bam -1 "$BASE/pe_1.fq" -2 "$BASE/pe_2.fq"

echo "=========================================================="
if [ "$FAILED" = 0 ]; then
  echo "PHASE-9b WORKER-INVARIANCE GATE (N=$N, PAR=$PAR): ALL CELLS PASS"
else
  echo "PHASE-9b WORKER-INVARIANCE GATE (N=$N, PAR=$PAR): FAILURES PRESENT"
fi
exit $FAILED
