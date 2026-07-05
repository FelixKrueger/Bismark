#!/usr/bin/env bash
# oxy B-faithful gate — Rust `bismark_rs --hisat2 --multicore N` must be byte-identical
# (decompressed BAM body) to Perl `bismark --hisat2 -p N` for the matching N. The remap
# routes `--multicore N` → ONE HISAT2 instance with `-p N --reorder`, so the oracle is
# Perl's `-p N` mode (NOT single-core, NOT Perl `--multicore N`).
#
# Compare `samtools view` (NO header → no @PG, no version line) → md5. `--reorder` + single
# instance ⇒ in-order body is deterministic, so a plain md5 compare is exact.
set -uo pipefail
ENV=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENV:$PATH
RUST=/var/tmp/hisat2mc_build/rust/target/release/bismark_rs
GENOME=$HOME/bismark_benchmarks/genome
SE_FULL=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
PE_R1_FULL=$HOME/bismark_benchmarks/10M_PE/directional_10M_R1_val_1.fq.gz
PE_R2_FULL=$HOME/bismark_benchmarks/10M_PE/directional_10M_R2_val_2.fq.gz
WORK=/var/tmp/hisat2mc_gate
N=1000000

mkdir -p "$WORK"; cd "$WORK"
echo "### gate start $(date)"
[ -x "$RUST" ] || { echo "!! rust binary missing at $RUST"; exit 1; }
echo "### rust: $($RUST --version 2>&1 | head -1)"

# ---- subsample ----
SE=$WORK/se.fq.gz
[ -s "$SE" ] || { echo "### subsampling SE..."; zcat "$SE_FULL" | head -n $((N*4)) | gzip > "$SE"; }
PE1=$WORK/pe_R1.fq.gz; PE2=$WORK/pe_R2.fq.gz
if [ ! -s "$PE1" ]; then
  if [ -s "$PE_R1_FULL" ] && [ -s "$PE_R2_FULL" ]; then
    echo "### subsampling PE..."
    zcat "$PE_R1_FULL" | head -n $((N*4)) | gzip > "$PE1"
    zcat "$PE_R2_FULL" | head -n $((N*4)) | gzip > "$PE2"
  else
    echo "### PE reads not found ($PE_R1_FULL / $PE_R2_FULL) — PE cell will be SKIPPED"
  fi
fi

PASS=0; FAIL=0; SKIP=0
compare_bam () {  # $1 label  $2 rust_outdir  $3 perl_outdir
  local label=$1 ro=$2 po=$3
  local rb pb
  rb=$(ls "$ro"/*_bismark_hisat2*.bam 2>/dev/null | grep -v ambig | head -1)
  pb=$(ls "$po"/*_bismark_hisat2*.bam 2>/dev/null | grep -v ambig | head -1)
  if [ -z "$rb" ] || [ -z "$pb" ]; then
    echo "[$label] MISSING bam (rust='$rb' perl='$pb') — see $ro/log / $po/log"; FAIL=$((FAIL+1)); return
  fi
  local rm pm rn pn spl
  rm=$(samtools view "$rb" | md5sum | cut -d' ' -f1)
  pm=$(samtools view "$pb" | md5sum | cut -d' ' -f1)
  rn=$(samtools view -c "$rb"); pn=$(samtools view -c "$pb")
  spl=$(samtools view "$rb" | awk '$6 ~ /N/' | wc -l)
  if [ "$rm" = "$pm" ]; then
    echo "[$label] PASS — rust==perl body (rust $rn rec / perl $pn / $spl spliced / md5 $rm)"; PASS=$((PASS+1))
  else
    echo "[$label] FAIL — rust $rm ($rn rec) != perl $pm ($pn rec)"; FAIL=$((FAIL+1))
  fi
}

se_cell () {  # $1 label  $2 N  $3.. extra args
  local label=$1 n=$2; shift 2
  local ro=$WORK/r_$label po=$WORK/p_$label
  rm -rf "$ro" "$po"; mkdir -p "$ro" "$po"
  echo "### [$(date +%H:%M:%S)] se_cell $label (N=$n $*)"
  "$RUST" --hisat2 --multicore "$n" --path_to_hisat2 "$ENV" --samtools_path "$ENV" \
      -o "$ro" --temp_dir "$ro" "$@" "$GENOME" "$SE" > "$ro/log" 2>&1
  bismark --hisat2 -p "$n" --path_to_hisat2 "$ENV" --samtools_path "$ENV" \
      -o "$po" --temp_dir "$po" "$@" "$GENOME" "$SE" > "$po/log" 2>&1
  compare_bam "$label" "$ro" "$po"
}

pe_cell () {  # $1 label  $2 N  $3.. extra args
  local label=$1 n=$2; shift 2
  [ -s "$PE1" ] || { echo "[$label] SKIP — no PE reads"; SKIP=$((SKIP+1)); return; }
  local ro=$WORK/r_$label po=$WORK/p_$label
  rm -rf "$ro" "$po"; mkdir -p "$ro" "$po"
  echo "### [$(date +%H:%M:%S)] pe_cell $label (N=$n $*)"
  "$RUST" --hisat2 --multicore "$n" --path_to_hisat2 "$ENV" --samtools_path "$ENV" \
      -o "$ro" --temp_dir "$ro" "$@" "$GENOME" -1 "$PE1" -2 "$PE2" > "$ro/log" 2>&1
  bismark --hisat2 -p "$n" --path_to_hisat2 "$ENV" --samtools_path "$ENV" \
      -o "$po" --temp_dir "$po" "$@" "$GENOME" -1 "$PE1" -2 "$PE2" > "$po/log" 2>&1
  # PE bam name has _pe.
  local rb pb
  rb=$(ls "$ro"/*_bismark_hisat2_pe.bam 2>/dev/null | head -1)
  pb=$(ls "$po"/*_bismark_hisat2_pe.bam 2>/dev/null | head -1)
  if [ -z "$rb" ] || [ -z "$pb" ]; then echo "[$label] MISSING pe bam"; FAIL=$((FAIL+1)); return; fi
  local rmd pmd
  rmd=$(samtools view "$rb" | md5sum | cut -d' ' -f1)
  pmd=$(samtools view "$pb" | md5sum | cut -d' ' -f1)
  if [ "$rmd" = "$pmd" ]; then
    echo "[$label] PASS — rust==perl pe body ($(samtools view -c "$rb") rec / md5 $rmd)"; PASS=$((PASS+1))
  else
    echo "[$label] FAIL — rust $rmd ($(samtools view -c "$rb")) != perl $pmd ($(samtools view -c "$pb"))"; FAIL=$((FAIL+1))
  fi
}

# ---- matrix ----
# SE directional, per-N (prove the route maps N→-p N byte-identically for each N)
se_cell se_dir_p2 2
se_cell se_dir_p4 4
se_cell se_dir_p8 8
# strand machinery under the route (directional dataset → few complementary-strand reads,
# but byte-identity is the point, like the Phase-8 caveat)
se_cell se_nondir_p4 4 --non_directional
se_cell se_pbat_p4   4 --pbat
# --ambig_bam under B (single instance — confirms the ambig path is reached, not the
# Bowtie-2-only multicore ambig temp machinery)
se_cell se_dir_ambig_p4 4 --ambig_bam
# paired-end directional
pe_cell pe_dir_p4 4

echo
echo "=== GATE SUMMARY: PASS=$PASS FAIL=$FAIL SKIP=$SKIP ==="
echo "### gate done $(date)"
