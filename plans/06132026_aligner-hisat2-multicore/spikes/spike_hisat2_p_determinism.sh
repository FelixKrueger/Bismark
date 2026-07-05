#!/usr/bin/env bash
# Spike (Phase 0, plans/06132026_aligner-hisat2-multicore) — HISAT2 `-p N` determinism for Approach B.
#
# QUESTION: Does Bismark `--hisat2 -p N --reorder` (one instance, whole read set, N>=2) produce
# decompressed-BAM content IDENTICAL to the bare-no-`-p` single-core run, deterministically
# run-to-run, for N in {2,4,8}?  (-> B-strong)  Is it at least deterministic per-N?  (-> B-faithful)
# And does `--multicore N` (fork+chunk) DIFFER (the worker-variance B avoids)?
#
# Perl-only: the Perl oracle drives HISAT2 exactly as the Rust port would, so this answers the
# gate question without a Rust build. SE directional (splice discovery is library-independent;
# the documented 1310-vs-1219 worker-variance was measured SE).
#
# NOTE: `-p 1` does NOT exist (Bismark dies, >=2 required) -> "single-core" = the bare no-`-p` run.
set -uo pipefail

ENV=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENV:$PATH
GENOME=$HOME/bismark_benchmarks/genome
READS_FULL=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
WORK=/var/tmp/hisat2_mc_spike
N_READS=1000000          # 1M reads = the documented scale where the fork-variance shows (1310 vs 1219)

mkdir -p "$WORK"; cd "$WORK"
echo "### spike start: $(date)  host nproc=$(nproc)"
echo "### bismark: $(command -v bismark) | hisat2: $(hisat2 --version 2>&1 | head -1)"

# ---- 1. subsample reads (deterministic head) --------------------------------
READS=$WORK/reads_${N_READS}.fq.gz
if [ ! -s "$READS" ]; then
  echo "### subsampling ${N_READS} reads..."
  zcat "$READS_FULL" | head -n $((N_READS*4)) | gzip > "$READS"
fi
LINES=$(zcat "$READS" | wc -l)
echo "### reads file: $READS  ($LINES lines = $((LINES/4)) reads)"
[ "$LINES" -eq $((N_READS*4)) ] || { echo "!! read count wrong ($LINES) — aborting"; exit 1; }

# ---- 2. run helper ----------------------------------------------------------
run_bismark () {            # $1 label ; $2.. extra bismark args
  local label=$1; shift
  local odir=$WORK/out_$label
  rm -rf "$odir"; mkdir -p "$odir"
  echo "### [$(date +%H:%M:%S)] run $label : bismark --hisat2 $* ..."
  /usr/bin/time -v bismark --hisat2 \
      --path_to_hisat2 "$ENV" --samtools_path "$ENV" \
      -o "$odir" --temp_dir "$odir" \
      "$@" \
      "$GENOME" "$READS" > "$odir/run.log" 2>&1
  echo "###   $label exit=$? : $(ls "$odir"/*_bismark_hisat2.bam 2>/dev/null | head -1)"
}

# baseline single-core (NO -p) x2  (establish the baseline + its own determinism)
run_bismark sc_a
run_bismark sc_b
# -p N x2 each  (N-sweep + per-N run-to-run determinism)
for N in 2 4 8; do
  run_bismark "p${N}_a" -p "$N"
  run_bismark "p${N}_b" -p "$N"
done
# fork+chunk cross-check x1  (confirm --multicore DIFFERS from single-core)
run_bismark mc4 --multicore 4

# ---- 3. compare -------------------------------------------------------------
bam ()        { ls "$WORK/out_$1"/*_bismark_hisat2.bam 2>/dev/null | head -1; }
nrec ()       { samtools view -c "$(bam "$1")" 2>/dev/null; }
body_md5 ()   { samtools view "$(bam "$1")" 2>/dev/null | md5sum | cut -d' ' -f1; }            # in-order (order+content)
sorted_md5 () { samtools view "$(bam "$1")" 2>/dev/null | LC_ALL=C sort | md5sum | cut -d' ' -f1; } # content only
splice_ct ()  { samtools view "$(bam "$1")" 2>/dev/null | awk '$6 ~ /N/' | wc -l; }
splice_md5 () { samtools view "$(bam "$1")" 2>/dev/null | awk '$6 ~ /N/' | LC_ALL=C sort | md5sum | cut -d' ' -f1; }

echo
echo "=== RESULTS (label / nrec / in-order-body-md5 / sorted-body-md5 / spliced / spliced-md5) ==="
for L in sc_a sc_b p2_a p2_b p4_a p4_b p8_a p8_b mc4; do
  printf "%-7s %-9s io:%-34s so:%-34s spl:%-8s splmd5:%-34s\n" \
    "$L" "$(nrec "$L")" "$(body_md5 "$L")" "$(sorted_md5 "$L")" "$(splice_ct "$L")" "$(splice_md5 "$L")"
done

echo
echo "=== VERDICT CHECKS ==="
SC=$(body_md5 sc_a)
echo "baseline determinism (sc_a==sc_b in-order): $([ "$(body_md5 sc_a)" = "$(body_md5 sc_b)" ] && echo YES || echo NO)"
for N in 2 4 8; do
  A=$(body_md5 "p${N}_a"); B=$(body_md5 "p${N}_b")
  echo "-p $N determinism (a==b in-order):      $([ "$A" = "$B" ] && echo YES || echo NO)"
  echo "-p $N == single-core (in-order):         $([ "$A" = "$SC" ] && echo YES || echo NO)"
  echo "-p $N == single-core (sorted/content):   $([ "$(sorted_md5 "p${N}_a")" = "$(sorted_md5 sc_a)" ] && echo YES || echo NO)"
  echo "-p $N spliced subset == single-core:     $([ "$(splice_md5 "p${N}_a")" = "$(splice_md5 sc_a)" ] && echo YES || echo NO)"
done
echo "--multicore 4 == single-core (sorted):     $([ "$(sorted_md5 mc4)" = "$(sorted_md5 sc_a)" ] && echo YES || echo NO)  (expect NO = worker-variance)"

echo
echo "=== timings (wall) ==="
for L in sc_a p2_a p4_a p8_a mc4; do
  echo -n "$L: "; grep -i "Elapsed (wall" "$WORK/out_$L/run.log" | head -1
done
echo "### spike done: $(date)"
