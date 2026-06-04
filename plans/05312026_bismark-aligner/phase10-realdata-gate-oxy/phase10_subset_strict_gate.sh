#!/bin/bash
# ==========================================================================
# Phase-10 Gate A — subset strict byte-identity + worker-invariance + the
# direct A1-assumption check (PLAN rev 1 §3.1 Gate A).
#
# Per cell, three legs — all on DECOMPRESSED content, @PG block filtered,
# report wall-clock line filtered:
#   A-strict:     Rust --parallel 1   == Perl single-core    (IN ORDER  -> cmp)
#   A-worker:     Rust --parallel P   == Rust --parallel 1   (IN ORDER  -> cmp)
#   A-assumption: Perl --multicore P  == Perl single-core    (REORDERED -> sort|md5)
#
# A-assumption directly MEASURES the load-bearing claim that Perl's fork+modulo
# --multicore emits the same record MULTISET as single-core — the premise Gate B
# (full-scale content compare vs Perl --parallel P) rests on. It REUSES the Perl
# single-core output produced for A-strict, and A-worker reuses Rust --parallel 1,
# so the only SLOW run per cell is the single Perl single-core alignment.
#
# Canonicalization (PLAN §3.4): every sort/cmp/md5sum/grep runs under LC_ALL=C so
# the byte-wise total order is deterministic ("two independent sorts are equal iff
# the multisets are equal"). FastQ aux is record-ized (paste - - - -) before any
# CONTENT sort so the comparison unit is the 4-line RECORD, not the line.
#
# Usage: phase10_subset_strict_gate.sh [P] [SUBSET_N] [CELLS]
#   P        worker count for A-worker / A-assumption (default 16; match Gate B)
#   SUBSET_N if set, head-subset each input to N reads (4N lines); empty = use
#            inputs verbatim (10M datasets are already 10M; RRBS subset = 10000000)
#   CELLS    space-separated subset of {se_dir pe_dir rrbs_pe_dir}; default all
# ==========================================================================
set -uo pipefail
export LC_ALL=C

P="${1:-16}"
SUBSET_N="${2:-}"
CELLS="${3:-se_dir pe_dir rrbs_pe_dir}"

ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
RUST=/var/tmp/aligner_p10/rust/target/release/bismark_rs

HG=$HOME/bismark_benchmarks/genome                 # human GRCh38
MM=$HOME/bismark_benchmarks/RRBS_PE/genome         # mouse GRCm39

SE_FQ=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
PE1_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R1_val_1.fq.gz
PE2_FQ=$HOME/bismark_benchmarks/10M_PE/directional_10M_R2_val_2.fq.gz
RR1_FQ=$HOME/bismark_benchmarks/RRBS_PE/SRR24766921_GSM7433369_Colon_3_Months_Rep1_Mus_musculus_RRBS_R1.fastq.gz
RR2_FQ=$HOME/bismark_benchmarks/RRBS_PE/SRR24766921_GSM7433369_Colon_3_Months_Rep1_Mus_musculus_RRBS_R2.fastq.gz

BASE=/var/tmp/aligner_p10_gateA
rm -rf "$BASE"; mkdir -p "$BASE/in" "$BASE/sorttmp"

echo "================= Phase-10 GATE A ================="
echo "P=$P  SUBSET_N=${SUBSET_N:-<none>}  CELLS=$CELLS"
echo "RUST=$RUST"; "$RUST" --version 2>&1 | grep -i version | head -1 || true
echo "date: $(date -u)"; echo

# --- stage inputs (optionally head-subset) off the S3 mount into $BASE/in ---
stage () {  # stage <src.gz> <dst.fq>
  if [ -n "$SUBSET_N" ]; then zcat "$1" | head -n $((4*SUBSET_N)) > "$2"
  else zcat "$1" > "$2"; fi
}
stage "$SE_FQ"  "$BASE/in/se.fq"
stage "$PE1_FQ" "$BASE/in/pe_1.fq"
stage "$PE2_FQ" "$BASE/in/pe_2.fq"
stage "$RR1_FQ" "$BASE/in/rr_1.fq"
stage "$RR2_FQ" "$BASE/in/rr_2.fq"
echo "staged: se=$(($(wc -l <"$BASE/in/se.fq")/4)) reads, pe=$(($(wc -l <"$BASE/in/pe_1.fq")/4)) pairs, rrbs=$(($(wc -l <"$BASE/in/rr_1.fq")/4)) pairs"
echo

FAILED=0

# ---- low-level comparators (all under LC_ALL=C) ----
# cmp two files; dump a bounded diagnostic window on mismatch (never re-diff the
# full stream — that re-introduces the buffering hazard).
cmp_files () {  # <a> <b> <label>
  local a="$1" b="$2" label="$3"
  if cmp -s "$a" "$b"; then echo "      ok   $label"; return 0; fi
  echo "      !!!  DIFF $label"; FAILED=1
  local off ln; off=$(cmp "$a" "$b" 2>/dev/null | sed -n 's/.*char \([0-9]*\).*/\1/p')
  ln=1; [ -n "$off" ] && ln=$(head -c "$off" "$a" | wc -l); ln=$((ln<1?1:ln))
  echo "      first diff ~line $ln (byte ${off:-?}):"
  diff <(sed -n "$((ln>3?ln-3:1)),$((ln+3))p" "$a") <(sed -n "$((ln>3?ln-3:1)),$((ln+3))p" "$b") 2>/dev/null | head -14 | sed 's/^/        /'
  return 1
}
# md5 of two already-normalized (sorted/record-ized) streams
md5_eq () {  # <a> <b> <label>
  local a="$1" b="$2" label="$3" ma mb
  ma=$(md5sum < "$a" | cut -d' ' -f1); mb=$(md5sum < "$b" | cut -d' ' -f1)
  if [ "$ma" = "$mb" ]; then echo "      ok   $label ($ma, $(wc -l <"$a") recs)"; return 0; fi
  echo "      !!!  DIFF $label  ref=$ma other=$mb"; FAILED=1
  echo "      differing records (comm -3, head):"
  comm -3 "$a" "$b" 2>/dev/null | head -8 | sed 's/^/        /'
  return 1
}

# ---- per-artifact normalizers (write to files; LC_ALL=C inherited) ----
n_sam_ordered ()  { samtools view -h "$1" | grep -v '^@PG' > "$2"; }                 # decompressed, @PG filtered
n_sam_sorted ()   { samtools view "$1" | sort -S 16G -T "$BASE/sorttmp" > "$2"; }    # records, order-independent
n_hdr ()          { samtools view -H "$1" | grep -v '^@PG' > "$2"; }                 # header, @PG filtered
n_report ()       { grep -v '^Bismark completed in ' "$1" > "$2"; }
n_fq_ordered ()   { zcat "$1" > "$2"; }                                              # in-order FastQ (line cmp ok)
n_fq_sorted ()    { zcat "$1" | paste - - - - | sort -S 16G -T "$BASE/sorttmp" > "$2"; }  # RECORD-multiset

# partner <other_dir> <ref_basename> -> path of the role-matched artifact in
# other_dir, or empty. Exact-name match first; falls back for the ONE Perl quirk:
# the --ambig_bam name differs between Perl single-core (<base>_bismark_bt2.ambig.bam,
# input ext stripped) and Perl --multicore (<base>.fq_bismark_bt2.ambig.bam, ext kept).
# The Rust port faithfully reproduces the single-core name; content is identical
# (verified: same record count + sorted-body md5, header identical modulo @PG). Match
# the unique *.ambig.bam by suffix so the name divergence doesn't read as MISSING.
partner () {
  local od="$1" b="$2"
  if [ -f "$od/$b" ]; then echo "$od/$b"; return; fi
  case "$b" in
    *.ambig.bam) local arr=("$od"/*.ambig.bam); [ -e "${arr[0]}" ] && echo "${arr[0]}" ;;
  esac
}

# compare_dirs <ref_dir> <other_dir> <mode> <tag>   mode = ordered | content
# Iterates every *.bam / *_report.txt / *.fq.gz present in ref_dir and compares
# the role-matched artifact in other_dir (see partner() for the ambig-name case).
compare_dirs () {
  local ref="$1" other="$2" mode="$3" tag="$4" b op
  for rb in "$ref"/*.bam; do
    [ -e "$rb" ] || continue; b=$(basename "$rb")
    op=$(partner "$other" "$b"); [ -n "$op" ] || { echo "      !!!  MISSING partner for $b in $other"; FAILED=1; continue; }
    if [ "$mode" = ordered ]; then
      n_sam_ordered "$rb" "$ref/$b.cmp.$tag";  n_sam_ordered "$op" "$op.cmp.$tag"
      cmp_files "$ref/$b.cmp.$tag" "$op.cmp.$tag" "[$tag] BAM $b"
    else
      n_sam_sorted "$rb" "$ref/$b.srt.$tag";   n_sam_sorted "$op" "$op.srt.$tag"
      md5_eq "$ref/$b.srt.$tag" "$op.srt.$tag" "[$tag] BAM-multiset $b"
      n_hdr "$rb" "$ref/$b.hdr.$tag";          n_hdr "$op" "$op.hdr.$tag"
      cmp_files "$ref/$b.hdr.$tag" "$op.hdr.$tag" "[$tag] HEADER $b"
    fi
  done
  for rr in "$ref"/*_report.txt; do
    [ -e "$rr" ] || continue; b=$(basename "$rr")
    op=$(partner "$other" "$b"); [ -n "$op" ] || { echo "      !!!  MISSING $b in $other"; FAILED=1; continue; }
    n_report "$rr" "$ref/$b.f.$tag"; n_report "$op" "$op.f.$tag"
    cmp_files "$ref/$b.f.$tag" "$op.f.$tag" "[$tag] REPORT $b"   # counts are order-independent
  done
  for ra in "$ref"/*.fq.gz; do
    [ -e "$ra" ] || continue; b=$(basename "$ra")
    op=$(partner "$other" "$b"); [ -n "$op" ] || { echo "      !!!  MISSING $b in $other"; FAILED=1; continue; }
    if [ "$mode" = ordered ]; then
      n_fq_ordered "$ra" "$ref/$b.txt.$tag"; n_fq_ordered "$op" "$op.txt.$tag"
      cmp_files "$ref/$b.txt.$tag" "$op.txt.$tag" "[$tag] AUX $b"
    else
      n_fq_sorted "$ra" "$ref/$b.rec.$tag"; n_fq_sorted "$op" "$op.rec.$tag"
      md5_eq "$ref/$b.rec.$tag" "$op.rec.$tag" "[$tag] AUX-multiset $b"
    fi
  done
}

# run_cell <name> <genome> <args...>  (args = library flags + reads; shared by all runs)
run_cell () {
  local name="$1" genome="$2"; shift 2
  local -a ARGS=("$@")
  local d="$BASE/$name"; rm -rf "$d"; mkdir -p "$d"/{perl_sc,perl_mc,rust_p1,rust_pP,tmp}
  echo "=================== CELL $name (genome=$(basename "$genome")) ==================="
  echo "  args: ${ARGS[*]}"

  # --- the FOUR runs (Perl single-core is the long pole; reused across all legs) ---
  echo "  [run] perl single-core ..."
  /usr/bin/time -v bismark --genome "$genome" -o "$d/perl_sc" --temp_dir "$d/tmp/psc" \
    --path_to_bowtie2 "$ENVBIN" --unmapped --ambiguous --ambig_bam "${ARGS[@]}" \
    > "$d/perl_sc.log" 2>&1; local e_psc=$?
  echo "  [run] perl --multicore $P ..."
  /usr/bin/time -v bismark --genome "$genome" -o "$d/perl_mc" --temp_dir "$d/tmp/pmc" \
    --path_to_bowtie2 "$ENVBIN" --parallel "$P" --unmapped --ambiguous --ambig_bam "${ARGS[@]}" \
    > "$d/perl_mc.log" 2>&1; local e_pmc=$?
  echo "  [run] rust --parallel 1 ..."
  /usr/bin/time -v "$RUST" --genome "$genome" -o "$d/rust_p1" --temp_dir "$d/tmp/r1" \
    --path_to_bowtie2 "$ENVBIN" --parallel 1 --unmapped --ambiguous --ambig_bam "${ARGS[@]}" \
    > "$d/rust_p1.log" 2>&1; local e_r1=$?
  echo "  [run] rust --parallel $P ..."
  /usr/bin/time -v "$RUST" --genome "$genome" -o "$d/rust_pP" --temp_dir "$d/tmp/rP" \
    --path_to_bowtie2 "$ENVBIN" --parallel "$P" --unmapped --ambiguous --ambig_bam "${ARGS[@]}" \
    > "$d/rust_pP.log" 2>&1; local e_rP=$?
  echo "  exit: perl_sc=$e_psc perl_mc=$e_pmc rust_p1=$e_r1 rust_pP=$e_rP"
  if [ "$e_psc" != 0 ] || [ "$e_pmc" != 0 ] || [ "$e_r1" != 0 ] || [ "$e_rP" != 0 ]; then
    echo "  CELL $name: FAIL (non-zero exit; see *.log)"; FAILED=1; return
  fi

  # [hardening, dual code-review] record-count backstop: compare_dirs drives off the
  # REFERENCE glob and skips an empty match WITHOUT failing, so an empty-but-exit-0 main
  # BAM would pass the strict gate vacuously. Assert the main BAM is non-empty and
  # perl_sc==rust_p1 before the byte compares (mirrors Gate B's B1.5).
  local mscb mr1b nsc nr1
  mscb=$(ls "$d/perl_sc"/*_bismark_bt2*.bam 2>/dev/null | grep -v '\.ambig\.bam$' | head -1)
  mr1b=$(ls "$d/rust_p1"/*_bismark_bt2*.bam 2>/dev/null | grep -v '\.ambig\.bam$' | head -1)
  nsc=$(samtools view -c "$mscb" 2>/dev/null); nr1=$(samtools view -c "$mr1b" 2>/dev/null)
  if [ -z "$mscb" ] || [ -z "${nsc:-}" ] || [ "${nsc:-0}" -lt 1 ]; then
    echo "  !!! BACKSTOP: perl_sc main BAM empty/missing — refusing to pass vacuously"; FAILED=1; return; fi
  if [ "$nsc" = "$nr1" ]; then echo "  backstop: main BAM $nsc records (perl_sc==rust_p1)";
  else echo "  !!! BACKSTOP: main BAM count perl_sc=$nsc != rust_p1=$nr1"; FAILED=1; fi

  echo "  -- A-strict:     rust_p1 vs perl_sc (in-order byte cmp) --"
  compare_dirs "$d/perl_sc" "$d/rust_p1" ordered strict
  echo "  -- A-worker:     rust_pP vs rust_p1 (worker-invariance, in-order) --"
  compare_dirs "$d/rust_p1" "$d/rust_pP" ordered worker
  echo "  -- A-assumption: perl_mc vs perl_sc (multiset; Perl multicore==single-core) --"
  compare_dirs "$d/perl_sc" "$d/perl_mc" content assume
  echo "  CELL $name: done (cumulative FAILED=$FAILED)"
}

for c in $CELLS; do
  case "$c" in
    se_dir)      run_cell se_dir      "$HG" "$BASE/in/se.fq" ;;
    pe_dir)      run_cell pe_dir      "$HG" -1 "$BASE/in/pe_1.fq" -2 "$BASE/in/pe_2.fq" ;;
    rrbs_pe_dir) run_cell rrbs_pe_dir "$MM" -1 "$BASE/in/rr_1.fq" -2 "$BASE/in/rr_2.fq" ;;
    # pbat: feed R2 as -1 and R1 as -2 with --pbat so directional PE data aligns as
    # genuine pbat (CTOT/CTOB) — Felix's trick to give a non-empty full-scale pbat test
    # (directional data run plain --pbat lands ~0 reads). Same swapped input to both sides.
    pbat_pe)     run_cell pbat_pe     "$HG" --pbat -1 "$BASE/in/pe_2.fq" -2 "$BASE/in/pe_1.fq" ;;
    *) echo "unknown cell: $c"; FAILED=1 ;;
  esac
done

echo "=========================================================="
if [ "$FAILED" = 0 ]; then echo "PHASE-10 GATE A (P=$P, N=${SUBSET_N:-full}): ALL CELLS PASS"
else echo "PHASE-10 GATE A (P=$P, N=${SUBSET_N:-full}): FAILURES PRESENT"; fi
exit $FAILED
