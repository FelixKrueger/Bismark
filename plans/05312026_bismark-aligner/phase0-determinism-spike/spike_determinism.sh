#!/usr/bin/env bash
# Phase 0 determinism spike — Rust bismark-aligner port.
#
# Question: is Perl Bismark v0.25.1 -> Bowtie2 2.5.5 deterministic enough that a
#           faithful Rust reimplementation can be byte-identical? (SE directional.)
# Criteria: C1 two full Bismark runs -> identical records + header
#           C2 standalone Bowtie2 run twice -> identical records
#           C3 @PG line captured + reconstructable; no -p/--reorder/--seed reordering
# Out of scope: no Rust; SE directional only; not testing methylation correctness or scale.
#
# Run on oxy:  dcli ssh oxy 'bash -s' < spike_determinism.sh
set -u

ENVBIN="$HOME/micromamba/envs/bismark-test/bin"
export PATH="$ENVBIN:$PATH"
GENOME="$HOME/bismark_benchmarks/genome"
READS="$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz"
WORK="/var/tmp/aligner_spike_det"
N=10000

pass=0; fail=0
note(){ echo "[$1] $2"; if [ "$1" = PASS ]; then pass=$((pass+1)); elif [ "$1" = FAIL ]; then fail=$((fail+1)); fi; }

echo "################ VERSIONS ################"
bismark  --version 2>/dev/null | grep -i version | head -1
bowtie2  --version 2>/dev/null | head -1
samtools --version 2>/dev/null | head -1

rm -rf "$WORK"; mkdir -p "$WORK/run1" "$WORK/run2" "$WORK/tmp1" "$WORK/tmp2" "$WORK/bt2"
cd "$WORK" || exit 1

echo "################ SUBSAMPLE $N reads ################"
zcat "$READS" 2>/dev/null | head -n $((N*4)) | gzip > "$WORK/subset.fq.gz"
echo "subset reads: $(zcat "$WORK/subset.fq.gz" | wc -l | awk '{print $1/4}')"

run_bismark(){ # $1=outdir $2=tmpdir $3=logfile
  bismark --genome "$GENOME" "$WORK/subset.fq.gz" -o "$1" --temp_dir "$2" > "$3" 2>&1
}
echo "################ RUN 1 ################"; run_bismark "$WORK/run1" "$WORK/tmp1" "$WORK/run1.log"; echo "bismark exit: $?"
echo "################ RUN 2 ################"; run_bismark "$WORK/run2" "$WORK/tmp2" "$WORK/run2.log"; echo "bismark exit: $?"

BAM1=$(ls "$WORK"/run1/*_bismark_bt2.bam 2>/dev/null | head -1)
BAM2=$(ls "$WORK"/run2/*_bismark_bt2.bam 2>/dev/null | head -1)
echo "BAM1=$BAM1"; echo "BAM2=$BAM2"
if [ -z "$BAM1" ] || [ -z "$BAM2" ]; then
  echo "!! bismark did not produce a BAM — log tails:"; tail -25 "$WORK/run1.log"; note FAIL "C1 bismark run did not complete"; echo "pass=$pass fail=$fail"; exit 1
fi

echo "--- mapping efficiency (run1 report) ---"
grep -iE 'Mapping efficiency|Sequences analysed|Number of .*unique' "$WORK"/run1/*_SE_report.txt 2>/dev/null | head

echo "################ C1: FULL-PIPELINE DETERMINISM ################"
samtools view -H "$BAM1" > "$WORK/h1.sam"; samtools view -H "$BAM2" > "$WORK/h2.sam"
samtools view    "$BAM1" > "$WORK/r1.sam"; samtools view    "$BAM2" > "$WORK/r2.sam"
echo "--- @PG line (run1) ---"; grep '^@PG' "$WORK/h1.sam"
echo "--- @HD/@SQ count: $(grep -cE '^@(HD|SQ)' "$WORK/h1.sam") header lines ---"
if cmp -s "$WORK/h1.sam" "$WORK/h2.sam"; then note PASS "C1a header identical run-to-run"; else echo "HEADER DIFF:"; diff "$WORK/h1.sam" "$WORK/h2.sam" | head; note FAIL "C1a header differs"; fi
if cmp -s "$WORK/r1.sam" "$WORK/r2.sam"; then note PASS "C1b records identical run-to-run ($(wc -l < "$WORK/r1.sam") records)"; else echo "RECORDS DIFF:"; diff "$WORK/r1.sam" "$WORK/r2.sam" | head; note FAIL "C1b records differ"; fi
echo "--- raw BAM md5 (informational: BGZF framing) ---"; md5sum "$BAM1" "$BAM2"

echo "################ C3: @PG + INVOCATION FLAGS ################"
grep -iE 'specified options|Now starting the Bowtie 2 aligner|Using Bowtie 2 index|Option' "$WORK/run1.log" | head -12
echo "--- reordering/threading flags in the bowtie2 invocation? ---"
if grep -iqE '(^| )-p |--threads|--reorder|--non-deterministic|--seed ' "$WORK/run1.log"; then
  echo "FOUND (inspect):"; grep -iE '(^| )-p |--threads|--reorder|--non-deterministic|--seed ' "$WORK/run1.log" | head
  note WARN "C3 found threading/seed flags — verify they don't reorder"
else
  note PASS "C3 no -p/--threads/--reorder/--non-deterministic/--seed in default invocation"
fi

echo "################ C2: STANDALONE BOWTIE2 DETERMINISM ################"
# CT instance: C->T transliterate the read sequence, align with --norc, single-threaded
zcat "$WORK/subset.fq.gz" | awk 'NR%4==2{gsub(/C/,"T")}1' > "$WORK/bt2/ct.fq"
CTIDX="$GENOME/Bisulfite_Genome/CT_conversion/BS_CT"
bowtie2 --norc -x "$CTIDX" -U "$WORK/bt2/ct.fq" -S "$WORK/bt2/a.sam" 2>"$WORK/bt2/a.log"
bowtie2 --norc -x "$CTIDX" -U "$WORK/bt2/ct.fq" -S "$WORK/bt2/b.sam" 2>"$WORK/bt2/b.log"
grep -v '^@' "$WORK/bt2/a.sam" > "$WORK/bt2/a.body"; grep -v '^@' "$WORK/bt2/b.sam" > "$WORK/bt2/b.body"
if cmp -s "$WORK/bt2/a.body" "$WORK/bt2/b.body"; then note PASS "C2 standalone bowtie2 identical ($(wc -l < "$WORK/bt2/a.body") records)"; else echo "BT2 DIFF:"; diff "$WORK/bt2/a.body" "$WORK/bt2/b.body" | head; note FAIL "C2 standalone bowtie2 differs"; fi

echo "################ SUMMARY ################"
echo "PASS=$pass  FAIL=$fail"
[ "$fail" -eq 0 ] && echo "SPIKE VERDICT: byte-identity premise HOLDS for SE directional" || echo "SPIKE VERDICT: premise AT RISK — see failures above"
