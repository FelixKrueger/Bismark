#!/bin/bash
# ==========================================================================
# Phase-10 Gate B — full-scale content-identity + perf (PLAN rev 1 §3.1 Gate B).
#
# Per cell, on the FULL dataset, Perl --parallel P (timed) vs Rust --parallel P
# (timed) with identical argv (orchestration flags aside):
#   B0   same-genome guard      (both sides read the identical --genome path)
#   B1   report identity        (LC_ALL=C cmp, wall-clock line filtered) [necessary, NOT sufficient]
#   B1.5 count reconciliation    (samtools view -c Perl == Rust == report-implied; wc -l equality) — BEFORE any hash
#   B2   content multiset        (whole-body LC_ALL=C sort -> md5; on mismatch, per-RNAME md5 vector for locality)
#   B2h  header identity         (samtools view -H, @PG block filtered, cmp; enumerate surviving lines)
#   B2.5 distinct-RNAME set       (cut -f3 | LC_ALL=C sort -u) — directly gates scaffold/chromosome diversity
#   B3   aux identity            (--unmapped/--ambiguous FastQ RECORD-ized then sort->md5; --ambig_bam via samtools view, suffix-matched)
#   B4   perf                    (/usr/bin/time -v wall + max RSS, both sides; framed honestly in GATE_OXY.md)
# V13 (WGBS SE+PE only): fresh-Perl --parallel P vs PRE-EXISTING Perl --parallel 4 BAM content
#   (Perl/Perl, two worker counts) -> corroborates the A1 multicore-layout-invariance at FULL scale
#   AND retires the unknown-Bowtie2-version of the old BAM. NOT an independent-correctness signal.
#
# Order-normalized by design: Perl --parallel P reorders vs single-core, so content
# is compared as a MULTISET. Strict ORDER is Gate A's job (10M) + 9b (1M). Canonical:
# LC_ALL=C on every sort/cmp/md5sum/grep; FastQ aux record-ized (paste - - - -) before sort.
#
# --ambig_bam: Perl --multicore names it <base>.fq_..ambig.bam (vs single-core <base>_..ambig.bam);
# Rust reproduces the single-core name. Content is identical (verified at 10M, Gate A A-assumption),
# so we suffix-match *.ambig.bam and compare content.
#
# Usage: phase10_fullscale_content_gate.sh [P] [SUBSET_N] [CELLS]
#   P        worker count for BOTH Perl --parallel and Rust --parallel (default 16)
#   SUBSET_N if set, head-subset each FULL input to N reads (for SMOKE-testing this harness);
#            empty = the real full datasets (84M SE / 84M PE pairs / 46.7M RRBS pairs)
#   CELLS    subset of {se_dir pe_dir rrbs_pe_dir}; default all
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

FULL=$HOME/bismark_benchmarks/full_size
RRBS=$HOME/bismark_benchmarks/RRBS_PE
SE_SRC=$FULL/SRR24827373_Homo_sapiens_Bisulfite-Seq_SE_trimmed_full_size.fq.gz
PE1_SRC=$FULL/SRR24827373_GSM7445361_32F_NB3_p28_p2n2p_p10_rep1_Homo_sapiens_Bisulfite-Seq_R1_val_1.fq.gz
PE2_SRC=$FULL/SRR24827373_GSM7445361_32F_NB3_p28_p2n2p_p10_rep1_Homo_sapiens_Bisulfite-Seq_R2_val_2.fq.gz
RR1_SRC=$RRBS/SRR24766921_GSM7433369_Colon_3_Months_Rep1_Mus_musculus_RRBS_R1.fastq.gz
RR2_SRC=$RRBS/SRR24766921_GSM7433369_Colon_3_Months_Rep1_Mus_musculus_RRBS_R2.fastq.gz
# Pre-existing Perl --parallel 4 BAMs (V13 layout-invariance cross-check; WGBS only)
OLD_SE_BAM=$FULL/SRR24827373_Homo_sapiens_Bisulfite-Seq_SE_trimmed_full_size_bismark_bt2.bam
OLD_PE_BAM=$FULL/SRR24827373_GSM7445361_32F_NB3_p28_p2n2p_p10_rep1_Homo_sapiens_Bisulfite-Seq_R1_val_1_bismark_bt2_pe.bam

BASE=/var/tmp/aligner_p10_gateB
mkdir -p "$BASE/in" "$BASE/sorttmp"
# Absolute -S (NOT a %): the pod advertises the NODE's RAM (~991G) via /proc/meminfo,
# but the cgroup caps at ~256G, so `sort -S 25%` would target ~248G and risk an
# OOM-kill on the 40G PE sort. 16G is ample (external merge-sort spills to -T disk)
# and safely under the limit. Sorts run sequentially + AFTER the alignments free their RAM.
SORTOPT="-S 16G --parallel=$P -T $BASE/sorttmp"

echo "================= Phase-10 GATE B ================="
echo "P=$P  SUBSET_N=${SUBSET_N:-FULL}  CELLS=$CELLS"
echo "RUST=$RUST"; "$RUST" --version 2>&1 | grep -i version | head -1 || true
echo "date: $(date -u)"; echo

# stage one input to /var/tmp (off the S3 mount, so timing isn't mount-bound).
# subset mode -> uncompressed head; full mode -> cp the .gz verbatim.
stage () {  # stage <src.gz> <dst_basename>  ; echoes the staged path
  local src="$1" name="$2" dst
  if [ -n "$SUBSET_N" ]; then dst="$BASE/in/$name.fq"; zcat "$src" | head -n $((4*SUBSET_N)) > "$dst"
  else dst="$BASE/in/$name.fq.gz"; [ -f "$dst" ] || cp "$src" "$dst"; fi
  echo "$dst"
}

FAILED=0
cmp_files () {  # <a> <b> <label>
  local a="$1" b="$2" label="$3"
  if cmp -s "$a" "$b"; then echo "      ok   $label"; return 0; fi
  echo "      !!!  DIFF $label"; FAILED=1
  diff "$a" "$b" 2>/dev/null | head -12 | sed 's/^/        /'; return 1
}

# ---- per-RNAME md5 vector (locality on B2 mismatch) ----
rname_md5vec () {  # <bam> <out>
  local bam="$1" out="$2" tmp; tmp=$(mktemp -d "$BASE/sorttmp/rv.XXXXXX")
  samtools view "$bam" | awk -F'\t' -v d="$tmp" '{ print >> (d"/"$3) }'
  : > "$out"
  for p in "$tmp"/*; do [ -e "$p" ] || continue
    echo "$(basename "$p") $(LC_ALL=C sort $SORTOPT "$p" | md5sum | cut -d' ' -f1)" >> "$out"; done
  LC_ALL=C sort -o "$out" "$out"; rm -rf "$tmp"
}

# run_cell <name> <genome> <old_bam|"-"> <pe:0/1> <args...>
run_cell () {
  local name="$1" genome="$2" old_bam="$3" pe="$4"; shift 4
  local -a ARGS=("$@")
  local d="$BASE/$name"; rm -rf "$d"; mkdir -p "$d"/{perl,rust,tmp}
  echo "=================== CELL $name (genome=$(basename "$genome"), PE=$pe) ==================="
  echo "  args: ${ARGS[*]}"

  # B0 — same-genome guard (both runs get the SAME $genome path)
  echo "  B0: --genome=$genome (shared by Perl + Rust)"

  echo "  [run] perl --parallel $P (timed) ..."
  /usr/bin/time -v -o "$d/perl.time" bismark --genome "$genome" -o "$d/perl" --temp_dir "$d/tmp/p" \
    --path_to_bowtie2 "$ENVBIN" --parallel "$P" --unmapped --ambiguous --ambig_bam "${ARGS[@]}" \
    > "$d/perl.log" 2>&1; local e_p=$?
  echo "  [run] rust --parallel $P (timed) ..."
  /usr/bin/time -v -o "$d/rust.time" "$RUST" --genome "$genome" -o "$d/rust" --temp_dir "$d/tmp/r" \
    --path_to_bowtie2 "$ENVBIN" --parallel "$P" --unmapped --ambiguous --ambig_bam "${ARGS[@]}" \
    > "$d/rust.log" 2>&1; local e_r=$?
  echo "  exit: perl=$e_p rust=$e_r"
  if [ "$e_p" != 0 ] || [ "$e_r" != 0 ]; then echo "  CELL $name: FAIL (non-zero exit)"; FAILED=1; return; fi

  local PB RB
  PB=$(ls "$d/perl"/*_bismark_bt2*.bam | grep -v '\.ambig\.bam$' | head -1)
  RB=$(ls "$d/rust"/*_bismark_bt2*.bam | grep -v '\.ambig\.bam$' | head -1)
  local rep; rep=$(basename "$(ls "$d/perl"/*_report.txt | head -1)")
  # [hardening, dual code-review] guard against an empty-but-exit-0 main BAM: without this,
  # the ls|grep|head glob yields "" and the count guard catches it only incidentally. Fail loud.
  if [ -z "$PB" ] || [ ! -s "$PB" ] || [ -z "$RB" ] || [ ! -s "$RB" ]; then
    echo "  !!! main BAM missing/empty (perl='$PB' rust='$RB') — cannot gate"; FAILED=1; return; fi

  # B1 — report identity (necessary, not sufficient)
  echo "  -- B1: report identity --"
  grep -v '^Bismark completed in ' "$d/perl/$rep" > "$d/perl.rep"
  grep -v '^Bismark completed in ' "$d/rust/$rep" > "$d/rust.rep"
  cmp_files "$d/perl.rep" "$d/rust.rep" "REPORT $rep"

  # B1.5 — count reconciliation BEFORE any hash
  echo "  -- B1.5: count reconciliation --"
  local cP cR; cP=$(samtools view -c "$PB"); cR=$(samtools view -c "$RB")
  local ubh disc mult implied
  ubh=$(grep -m1 'unique best hit' "$d/perl/$rep" | grep -oE '[0-9]+' | head -1)
  # Reads with a unique best hit but whose genomic sequence could not be extracted are
  # NOT written to the BAM (the edge path the SE oracle reports as 36). So BAM records =
  # (unique_best_hits - discarded) x mate-factor. Subtract the discards or the implied
  # count over-counts by exactly that many (a benign, self-validating difference).
  disc=$(grep -m1 'genomic sequence could not be extracted' "$d/perl/$rep" | grep -oE '[0-9]+' | tail -1); disc=${disc:-0}
  mult=1; [ "$pe" = 1 ] && mult=2; implied=$(( (ubh - disc) * mult ))
  echo "      perl view -c=$cP  rust view -c=$cR  report (unique_best_hit $ubh - discarded $disc) x$mult = $implied"
  if [ "$cP" = "$cR" ] && [ "$cP" = "$implied" ]; then echo "      ok   counts reconcile ($cP)";
  else echo "      !!!  COUNT MISMATCH perl=$cP rust=$cR implied=$implied (essential guard perl==rust: $([ "$cP" = "$cR" ] && echo OK || echo FAIL))"; FAILED=1; fi
  samtools view "$PB" > "$d/perl.body"; samtools view "$RB" > "$d/rust.body"
  local wP wR; wP=$(wc -l < "$d/perl.body"); wR=$(wc -l < "$d/rust.body")
  [ "$wP" = "$wR" ] && echo "      ok   wc -l equal ($wP)" || { echo "      !!!  wc -l differ perl=$wP rust=$wR"; FAILED=1; }

  # B2 — content multiset (whole-body sort -> md5)
  echo "  -- B2: content multiset (sort|md5) --"
  local mP mR
  mP=$(LC_ALL=C sort $SORTOPT "$d/perl.body" | md5sum | cut -d' ' -f1)
  mR=$(LC_ALL=C sort $SORTOPT "$d/rust.body" | md5sum | cut -d' ' -f1)
  if [ "$mP" = "$mR" ]; then echo "      ok   BAM-multiset identical ($mP, $wP recs)";
  else
    echo "      !!!  BAM-multiset DIFFERS perl=$mP rust=$mR  -> per-RNAME locality:"; FAILED=1
    rname_md5vec "$PB" "$d/perl.rvec"; rname_md5vec "$RB" "$d/rust.rvec"
    diff "$d/perl.rvec" "$d/rust.rvec" | head -20 | sed 's/^/        /'
  fi

  # B2h — header identity (@PG block filtered) + enumerate surviving lines
  echo "  -- B2h: header identity (@PG filtered) --"
  samtools view -H "$PB" | grep -v '^@PG' > "$d/perl.hdr"
  samtools view -H "$RB" | grep -v '^@PG' > "$d/rust.hdr"
  cmp_files "$d/perl.hdr" "$d/rust.hdr" "HEADER (@PG filtered)"
  echo "      header lines kept: @HD=$(grep -c '^@HD' "$d/perl.hdr") @SQ=$(grep -c '^@SQ' "$d/perl.hdr") @CO=$(grep -c '^@CO' "$d/perl.hdr") @RG=$(grep -c '^@RG' "$d/perl.hdr")"
  echo "      @HD: $(grep '^@HD' "$d/perl.hdr" | head -1)"

  # B2.5 — distinct-RNAME set equality
  echo "  -- B2.5: distinct-RNAME set --"
  cut -f3 "$d/perl.body" | LC_ALL=C sort -u > "$d/perl.rnames"
  cut -f3 "$d/rust.body" | LC_ALL=C sort -u > "$d/rust.rnames"
  cmp_files "$d/perl.rnames" "$d/rust.rnames" "RNAME-set ($(wc -l < "$d/perl.rnames") contigs)"

  # B3 — aux identity (record-ized FastQ; ambig BAM suffix-matched)
  echo "  -- B3: aux identity --"
  for pa in "$d/perl"/*_reads*.fq.gz; do
    [ -e "$pa" ] || continue; local b; b=$(basename "$pa")
    [ -f "$d/rust/$b" ] || { echo "      !!!  MISSING rust aux $b"; FAILED=1; continue; }
    local x y
    x=$(zcat "$pa" | paste - - - - | LC_ALL=C sort $SORTOPT | md5sum | cut -d' ' -f1)
    y=$(zcat "$d/rust/$b" | paste - - - - | LC_ALL=C sort $SORTOPT | md5sum | cut -d' ' -f1)
    [ "$x" = "$y" ] && echo "      ok   AUX-multiset $b ($x)" || { echo "      !!!  AUX DIFF $b perl=$x rust=$y"; FAILED=1; }
  done
  local pAmb rAmb
  pAmb=$(ls "$d/perl"/*.ambig.bam 2>/dev/null | head -1); rAmb=$(ls "$d/rust"/*.ambig.bam 2>/dev/null | head -1)
  if [ -n "$pAmb" ] && [ -n "$rAmb" ]; then
    local ax ay
    ax=$(samtools view "$pAmb" | LC_ALL=C sort $SORTOPT | md5sum | cut -d' ' -f1)
    ay=$(samtools view "$rAmb" | LC_ALL=C sort $SORTOPT | md5sum | cut -d' ' -f1)
    [ "$ax" = "$ay" ] && echo "      ok   AMBIG-BAM multiset ($ax, perl=$(basename "$pAmb") rust=$(basename "$rAmb"))" \
      || { echo "      !!!  AMBIG-BAM DIFF perl=$ax rust=$ay"; FAILED=1; }
  else echo "      (ambig bam: perl=${pAmb:-none} rust=${rAmb:-none})"; fi

  # B4 — perf (honest framing in GATE_OXY.md; Bowtie2 ~74% unchanged, Amdahl-bounded)
  echo "  -- B4: perf (wall + max RSS, P=$P) --"
  echo "      perl: $(grep -E 'Elapsed|Maximum resident' "$d/perl.time" | tr '\n' '  ')"
  echo "      rust: $(grep -E 'Elapsed|Maximum resident' "$d/rust.time" | tr '\n' '  ')"

  # V13 — fresh-Perl vs pre-existing Perl --parallel 4 BAM (WGBS SE+PE; layout-invariance + provenance)
  if [ "$old_bam" != "-" ] && [ -f "$old_bam" ] && [ -z "$SUBSET_N" ]; then
    echo "  -- V13: fresh perl --parallel $P vs pre-existing perl --parallel 4 BAM (multiset) --"
    local mOld
    mOld=$(samtools view "$old_bam" | LC_ALL=C sort $SORTOPT | md5sum | cut -d' ' -f1)
    [ "$mOld" = "$mP" ] && echo "      ok   layout-invariant: old(--p4)==fresh(--p$P) ($mOld)" \
      || echo "      NOTE old(--p4)=$mOld fresh(--p$P)=$mP  (investigate: bowtie2 version of old BAM or perl layout)"
  fi

  echo "  CELL $name: done (cumulative FAILED=$FAILED)"
}

for c in $CELLS; do
  case "$c" in
    se_dir)      run_cell se_dir      "$HG" "$OLD_SE_BAM" 0 "$(stage "$SE_SRC" se)" ;;
    pe_dir)      run_cell pe_dir      "$HG" "$OLD_PE_BAM" 1 -1 "$(stage "$PE1_SRC" pe_1)" -2 "$(stage "$PE2_SRC" pe_2)" ;;
    rrbs_pe_dir) run_cell rrbs_pe_dir "$MM" "-"          1 -1 "$(stage "$RR1_SRC" rr_1)" -2 "$(stage "$RR2_SRC" rr_2)" ;;
    # pbat: R2 as -1, R1 as -2, with --pbat -> genuine CTOT/CTOB at full scale (Felix's
    # trick). No pre-existing pbat oracle BAM, so V13 is skipped (old_bam="-"). Same swapped
    # input to Perl + Rust; content-multiset gate as for the other cells.
    pbat_pe)     run_cell pbat_pe     "$HG" "-"          1 --pbat -1 "$(stage "$PE2_SRC" pe_2)" -2 "$(stage "$PE1_SRC" pe_1)" ;;
    *) echo "unknown cell: $c"; FAILED=1 ;;
  esac
done

echo "=========================================================="
if [ "$FAILED" = 0 ]; then echo "PHASE-10 GATE B (P=$P, N=${SUBSET_N:-FULL}): ALL CELLS PASS"
else echo "PHASE-10 GATE B (P=$P, N=${SUBSET_N:-FULL}): FAILURES PRESENT"; fi
exit $FAILED
