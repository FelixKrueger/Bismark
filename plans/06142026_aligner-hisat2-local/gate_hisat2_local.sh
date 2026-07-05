#!/usr/bin/env bash
# oxy byte-identity gate — Rust `bismark_rs --hisat2 --local` must be byte-identical
# (decompressed BAM body) to Perl `bismark --hisat2 --local` for the matching cell.
# HISAT2-local = drop `--no-softclip` (soft-clip allowed) + L-form `--score-min L,0,-0.2`
# + local ln() MAPQ, no `--local` flag. Real WGBS reads DO soft-clip under HISAT2-local
# (~3.4% in the 200k probe) → the non-vacuity assert holds without synthetic tailing.
set -uo pipefail
ENV=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENV:$PATH
RUST=/var/tmp/hisat2local_build/rust/target/release/bismark_rs
GENOME=$HOME/bismark_benchmarks/genome
SE_FULL=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
PE_R1_FULL=$HOME/bismark_benchmarks/10M_PE/directional_10M_R1_val_1.fq.gz
PE_R2_FULL=$HOME/bismark_benchmarks/10M_PE/directional_10M_R2_val_2.fq.gz
WORK=/var/tmp/hisat2local_gate
N=1000000

mkdir -p "$WORK"; cd "$WORK"
echo "### gate start $(date); rust=$($RUST --version 2>&1 | head -1)"
[ -x "$RUST" ] || { echo "!! rust binary missing"; exit 1; }

SE=$WORK/se.fq.gz
[ -s "$SE" ] || zcat "$SE_FULL" | head -n $((N*4)) | gzip > "$SE"
PE1=$WORK/pe_R1.fq.gz; PE2=$WORK/pe_R2.fq.gz
[ -s "$PE1" ] || zcat "$PE_R1_FULL" | head -n $((N*4)) | gzip > "$PE1"
[ -s "$PE2" ] || zcat "$PE_R2_FULL" | head -n $((N*4)) | gzip > "$PE2"

PASS=0; FAIL=0
bodymd5 () { samtools view "$1" 2>/dev/null | md5sum | cut -d' ' -f1; }
scount  () { samtools view "$1" 2>/dev/null | awk '$6 ~ /S/' | wc -l; }
nrec    () { samtools view -c "$1" 2>/dev/null; }

# SE cell: $1 label, $2 = Rust extra args, $3 = Perl extra args (matched), $4.. shared
se_cell () {
  local label=$1 rust_x=$2 perl_x=$3; shift 3
  local ro=$WORK/r_$label po=$WORK/p_$label
  rm -rf "$ro" "$po"; mkdir -p "$ro" "$po"
  echo "### [$(date +%H:%M:%S)] se $label : rust($rust_x) vs perl($perl_x) $*"
  "$RUST" --hisat2 --local $rust_x "$@" --path_to_hisat2 "$ENV" --samtools_path "$ENV" \
      -o "$ro" --temp_dir "$ro" "$GENOME" "$SE" > "$ro/log" 2>&1
  bismark --hisat2 --local $perl_x "$@" --path_to_hisat2 "$ENV" --samtools_path "$ENV" \
      -o "$po" --temp_dir "$po" "$GENOME" "$SE" > "$po/log" 2>&1
  local rb pb; rb=$(ls "$ro"/*_bismark_hisat2.bam 2>/dev/null|grep -v ambig|head -1); pb=$(ls "$po"/*_bismark_hisat2.bam 2>/dev/null|grep -v ambig|head -1)
  if [ -z "$rb" ] || [ -z "$pb" ]; then echo "[$label] MISSING bam"; FAIL=$((FAIL+1)); return; fi
  local rm pm sc; rm=$(bodymd5 "$rb"); pm=$(bodymd5 "$pb"); sc=$(scount "$rb")
  if [ "$rm" = "$pm" ]; then echo "[$label] PASS rust==perl ($(nrec "$rb") rec / $sc soft-clipped / md5 $rm)"; PASS=$((PASS+1));
  else echo "[$label] FAIL rust $rm != perl $pm"; FAIL=$((FAIL+1)); fi
}

# ---- matrix ----
se_cell se_dir          ""                  ""
se_cell se_nondir       "--non_directional" "--non_directional"
se_cell se_pbat         "--pbat"            "--pbat"
# multicore cell: Rust --local --multicore 4 (→ single instance -p4) == Perl --local -p 4
se_cell se_dir_mc       "--multicore 4"     "-p 4"

# PE directional
ro=$WORK/r_pe_dir po=$WORK/p_pe_dir; rm -rf "$ro" "$po"; mkdir -p "$ro" "$po"
echo "### [$(date +%H:%M:%S)] pe pe_dir"
"$RUST" --hisat2 --local --path_to_hisat2 "$ENV" --samtools_path "$ENV" -o "$ro" --temp_dir "$ro" "$GENOME" -1 "$PE1" -2 "$PE2" > "$ro/log" 2>&1
bismark --hisat2 --local --path_to_hisat2 "$ENV" --samtools_path "$ENV" -o "$po" --temp_dir "$po" "$GENOME" -1 "$PE1" -2 "$PE2" > "$po/log" 2>&1
rb=$(ls "$ro"/*_bismark_hisat2_pe.bam 2>/dev/null|head -1); pb=$(ls "$po"/*_bismark_hisat2_pe.bam 2>/dev/null|head -1)
if [ -n "$rb" ] && [ -n "$pb" ] && [ "$(bodymd5 "$rb")" = "$(bodymd5 "$pb")" ]; then
  echo "[pe_dir] PASS rust==perl ($(nrec "$rb") rec / $(scount "$rb") soft-clipped / md5 $(bodymd5 "$rb"))"; PASS=$((PASS+1));
else echo "[pe_dir] FAIL (rust $(bodymd5 "$rb") vs perl $(bodymd5 "$pb"))"; FAIL=$((FAIL+1)); fi

# ---- non-vacuity (B2+B3): local soft-clips AND differs from end-to-end ----
echo "### non-vacuity: --hisat2 (end-to-end) vs --hisat2 --local on the same SE reads"
eo=$WORK/p_se_ete; rm -rf "$eo"; mkdir -p "$eo"
bismark --hisat2 --path_to_hisat2 "$ENV" --samtools_path "$ENV" -o "$eo" --temp_dir "$eo" "$GENOME" "$SE" > "$eo/log" 2>&1
eb=$(ls "$eo"/*_bismark_hisat2.bam 2>/dev/null|head -1)
local_sc=$(scount "$(ls "$WORK"/p_se_dir/*_bismark_hisat2.bam|grep -v ambig|head -1)")
ete_sc=$(scount "$eb")
echo "### soft-clips: local=$local_sc  end-to-end=$ete_sc"
if [ "$local_sc" -gt 0 ] && [ "$local_sc" -gt "$ete_sc" ]; then
  echo "[non-vacuity] PASS (local soft-clips=$local_sc > end-to-end=$ete_sc — the --no-softclip drop is exercised)"; PASS=$((PASS+1));
else echo "[non-vacuity] FAIL (local=$local_sc ete=$ete_sc — gate is VACUOUS)"; FAIL=$((FAIL+1)); fi

# ---- Q4: Perl --hisat2 --local determinism (run twice, same body md5) ----
q4o=$WORK/p_se_dir2; rm -rf "$q4o"; mkdir -p "$q4o"
bismark --hisat2 --local --path_to_hisat2 "$ENV" --samtools_path "$ENV" -o "$q4o" --temp_dir "$q4o" "$GENOME" "$SE" > "$q4o/log" 2>&1
q4b=$(ls "$q4o"/*_bismark_hisat2.bam 2>/dev/null|grep -v ambig|head -1)
p1=$(bodymd5 "$(ls "$WORK"/p_se_dir/*_bismark_hisat2.bam|grep -v ambig|head -1)"); p2=$(bodymd5 "$q4b")
if [ "$p1" = "$p2" ]; then echo "[Q4] PASS Perl --hisat2 --local is run-to-run deterministic ($p1)"; PASS=$((PASS+1));
else echo "[Q4] FAIL Perl --local non-deterministic ($p1 != $p2)"; FAIL=$((FAIL+1)); fi

echo; echo "=== GATE SUMMARY: PASS=$PASS FAIL=$FAIL ==="
echo "### gate done $(date)"
