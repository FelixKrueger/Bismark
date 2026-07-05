#!/bin/bash
# ==========================================================================
# Phase-1 spike (v1.x) — HISAT2 determinism.
#
# QUESTION: Is Perl Bismark v0.25.1 + HISAT2 2.2.2 byte-deterministic run-to-run
# on real bisulfite reads (→ a byte-identity Rust HISAT2 wrapper is reachable)?
# Capture the exact HISAT2 `aligner_options` Bismark assembles + any reorder/seed
# flags + the SAM tag set (ZS vs XS) + whether spliced (N-CIGAR) records appear.
#
# SUCCESS: two independent `bismark --hisat2` runs on the same 10k SE reads →
# byte-identical decompressed SAM (@PG filtered).  SCOPE: SE directional, 10k,
# human GRCh38, HISAT2 only (throwaway; minimap2 = Phase 3; the Rust port = Phase 2).
#
# Usage: spike_hisat2_determinism.sh [N]   (N reads, default 10000)
# ==========================================================================
set -uo pipefail
export LC_ALL=C
N="${1:-10000}"
ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
GENOME=$HOME/bismark_benchmarks/genome
SE=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
BASE=/var/tmp/v1x_hisat2_spike
rm -rf "$BASE"; mkdir -p "$BASE/in" "$BASE/r1" "$BASE/r2" "$BASE/t1" "$BASE/t2"
zcat "$SE" | head -n $((4*N)) > "$BASE/in/se.fq"
echo "=== HISAT2 determinism spike: N=$N SE reads ==="
echo "hisat2: $($ENVBIN/hisat2 --version 2>&1 | head -1)"
echo "bismark: $(bismark --version 2>&1 | grep -i version | head -1)"
echo "date: $(date -u)"; echo

run () {  # run <outdir> <tmpdir>
  bismark --hisat2 --path_to_hisat2 "$ENVBIN" --genome "$GENOME" \
    -o "$1" --temp_dir "$2" "$BASE/in/se.fq" > "$1/run.log" 2>&1
}
echo "--- run 1 ---"; run "$BASE/r1" "$BASE/t1"; e1=$?
echo "--- run 2 ---"; run "$BASE/r2" "$BASE/t2"; e2=$?
echo "exit: run1=$e1 run2=$e2"
if [ "$e1" != 0 ] || [ "$e2" != 0 ]; then echo "!!! non-zero exit — see run.log"; tail -20 "$BASE/r1/run.log"; exit 1; fi

B1=$(ls "$BASE/r1"/*_bismark_hisat2.bam 2>/dev/null | head -1)
B2=$(ls "$BASE/r2"/*_bismark_hisat2.bam 2>/dev/null | head -1)
echo; echo "bam1=$B1"; echo "bam2=$B2"
echo "records: run1=$(samtools view -c "$B1" 2>/dev/null)  run2=$(samtools view -c "$B2" 2>/dev/null)"

echo; echo "=== Q1 DETERMINISM: run1 vs run2 decompressed SAM (@PG filtered) ==="
if diff <(samtools view -h "$B1" | grep -v '^@PG') <(samtools view -h "$B2" | grep -v '^@PG') > "$BASE/det.diff" 2>&1; then
  echo "  ✅ DETERMINISTIC: run1 == run2 byte-identical ($(samtools view -c "$B1") records)"
else
  echo "  ❌ NON-DETERMINISTIC: $(wc -l < "$BASE/det.diff") diff lines; head:"; head -20 "$BASE/det.diff" | sed 's/^/    /'
fi

echo; echo "=== Q2 HISAT2 aligner_options (from report) ==="
grep -h 'run with HISAT2' "$BASE/r1"/*_SE_report.txt 2>/dev/null | sed 's/^/  /'
echo "  --- option/seed mentions in run.log ---"
grep -h -i 'hisat2.*-x \|specified options\|--seed\|--no-1mm\|--no-spliced\|--norc\|--nofw\|reorder' "$BASE/r1/run.log" 2>/dev/null | head | sed 's/^/  /'

echo; echo "=== Q3 SAM tag set (first aligned record) ==="
samtools view "$B1" 2>/dev/null | head -1 | tr '\t' '\n' | grep -E '^[A-Z][A-Z0-9]:' | sed 's/^/  /'
echo "  ZS:i present? $(samtools view "$B1" 2>/dev/null | head -2000 | grep -c 'ZS:i:') of first 2000 ; XS:i present? $(samtools view "$B1" 2>/dev/null | head -2000 | grep -c 'XS:i:')"

echo; echo "=== Q4 spliced (N-CIGAR) records? ==="
echo "  N-CIGAR count: $(samtools view "$B1" 2>/dev/null | awk -F'\t' '$6 ~ /N/' | wc -l)"

echo; echo "=== Q5 output filenames + report counts ==="
ls "$BASE/r1" | grep -v -E 'run.log|\.diff' | sed 's/^/  /'
echo "  --- report final-alignment block ---"
grep -hE 'Sequences analysed|unique best hit|Mapping efficiency|no alignments|did not map uniquely|genomic sequence could not' "$BASE/r1"/*_SE_report.txt 2>/dev/null | sed 's/^/  /'
echo; echo "=== spike done ==="
