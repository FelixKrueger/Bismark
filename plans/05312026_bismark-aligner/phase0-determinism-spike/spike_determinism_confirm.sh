#!/usr/bin/env bash
# Phase 0 spike — iteration #2: raw-BAM-byte identity under IDENTICAL invocation.
# Runs Bismark twice with byte-identical command lines (relative input, -o ., --temp_dir tmp)
# so the @PG CL strings match, then compares the raw BAM bytes (BGZF-framing determinism).
set -u
export PATH="$HOME/micromamba/envs/bismark-test/bin:$PATH"
GENOME="$HOME/bismark_benchmarks/genome"
SUB="/var/tmp/aligner_spike_det/subset.fq.gz"   # reuse the 10k subset from iteration #1
W="/var/tmp/aligner_spike_det2"
rm -rf "$W"; mkdir -p "$W/d1/tmp" "$W/d2/tmp"
cp "$SUB" "$W/d1/subset.fq.gz"; cp "$SUB" "$W/d2/subset.fq.gz"
( cd "$W/d1" && bismark --genome "$GENOME" subset.fq.gz -o . --temp_dir tmp > bm.log 2>&1 ); echo "d1 exit $?"
( cd "$W/d2" && bismark --genome "$GENOME" subset.fq.gz -o . --temp_dir tmp > bm.log 2>&1 ); echo "d2 exit $?"
B1="$W/d1/subset_bismark_bt2.bam"; B2="$W/d2/subset_bismark_bt2.bam"
echo "=== @PG block (identical invocation, d1) ==="; samtools view -H "$B1" | grep '^@PG'
echo "=== raw BAM md5 ==="; md5sum "$B1" "$B2"
if cmp -s "$B1" "$B2"; then
  echo "RESULT: RAW BAM BYTES IDENTICAL under identical invocation -> full byte-identity achievable"
else
  echo "RESULT: raw bytes differ; checking decompressed records + header..."
  echo -n "records d1/d2 md5: "; samtools view "$B1" | md5sum; samtools view "$B2" | md5sum
  echo -n "header   d1/d2 md5: "; samtools view -H "$B1" | md5sum; samtools view -H "$B2" | md5sum
fi
