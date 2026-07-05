#!/bin/bash
# Phase-3 spike — minimap2 determinism + both-strand selection (oxy). Throwaway.
#
# Q1 (gating): is Perl bismark --minimap2 + minimap2 2.31 byte-deterministic
#   run-to-run on 10k SE reads (→ a byte-identity Rust wrapper is reachable)?
# Q2: what `aligner_options` does Bismark assemble for minimap2 (the -ax sr etc.)?
# Q3: minimap2 aligns BOTH strands (Perl comments out --norc/--nofw). How does
#   that change the unique-vs-ambiguous arithmetic vs the strand-restricted
#   Bowtie2/HISAT2 model? (report counts + the per-instance FLAG distribution:
#   --norc would give flag 0 only; both-strand gives flag 0 AND 16.)
# Q4: which tags does minimap2 emit (the 2nd-best score the merge needs)?
# Q5: input ORDER preserved in the output (Bismark's lockstep parse needs it)?
#
# Usage: bash spike_minimap2_determinism_selection.sh 10000
set -uo pipefail
N="${1:-10000}"
ENVBIN=$HOME/micromamba/envs/bismark-test/bin
export PATH=$ENVBIN:$PATH
GENOME=$HOME/bismark_benchmarks/genome
CT_MMI=$GENOME/Bisulfite_Genome/CT_conversion/BS_CT.mmi
SE_FQ=$HOME/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
BASE=/var/tmp/mm2_spike
rm -rf "$BASE"; mkdir -p "$BASE"
zcat "$SE_FQ" | head -n $((4 * N)) > "$BASE/se.fq"
echo "minimap2: $(minimap2 --version)   subset: $(($(wc -l <"$BASE/se.fq")/4)) reads"

filter_sam () { samtools view -h "$1" | grep -v 'ID:samtools'; }

# ---- Q1 determinism: two independent bismark --minimap2 runs -----------------
for r in 1 2; do
  rm -rf "$BASE/run$r" "$BASE/tmp$r"; mkdir -p "$BASE/run$r" "$BASE/tmp$r"
  bismark --minimap2 --path_to_minimap2 "$ENVBIN" --genome "$GENOME" \
    -o "$BASE/run$r" --temp_dir "$BASE/tmp$r" "$BASE/se.fq" > "$BASE/run$r.log" 2>&1
  echo "run$r exit=$?"
done
B1=$(ls "$BASE/run1"/*.bam 2>/dev/null | head -1)
B2=$(ls "$BASE/run2"/*.bam 2>/dev/null | head -1)
echo "=== Q1 DETERMINISM ==="
if [ -z "$B1" ] || [ -z "$B2" ]; then
  echo "!!! no BAM produced — see run1.log:"; tail -20 "$BASE/run1.log"
else
  echo "run1 bam: $(basename "$B1") = $(samtools view -c "$B1") records"
  if diff <(filter_sam "$B1") <(filter_sam "$B2") > "$BASE/det.diff" 2>&1; then
    echo "RESULT: run1 == run2 BYTE-IDENTICAL (decompressed SAM, @PG filtered)"
  else
    echo "RESULT: run1 != run2 — DIFFERS ($(wc -l <"$BASE/det.diff") diff lines). Sample:"
    head -8 "$BASE/det.diff"
  fi
fi

# ---- Q2 aligner_options ------------------------------------------------------
echo "=== Q2 aligner_options (from the report) ==="
grep -h "run with minimap2" "$BASE/run1"/*_report.txt 2>&1 | head -1

# ---- Q3 selection: report counts vs the strand-restricted model --------------
echo "=== Q3 report counts (Bismark --minimap2) ==="
grep -hE "analysed in total|unique best|no alignments under|did not map uniquely|could not be extracted" \
  "$BASE/run1"/*_report.txt 2>&1

# ---- Q3b/Q4/Q5 raw minimap2 on BS_CT.mmi (the CTreadCTgenome instance, NO --norc) ----
echo "=== Q3b/Q4/Q5 raw minimap2 -ax sr on BS_CT.mmi ==="
minimap2 -ax sr "$CT_MMI" "$BASE/se.fq" 2> "$BASE/mm2_raw.log" > "$BASE/raw_ct.sam"
echo "minimap2 raw exit=$?  alignment lines (excl @): $(grep -vc '^@' "$BASE/raw_ct.sam")"
echo "FLAG distribution (col2; with --norc this would be flag 0 only — both-strand shows 0 AND 16):"
grep -v '^@' "$BASE/raw_ct.sam" | cut -f2 | sort -n | uniq -c | sort -rn | head -12
echo "distinct read IDs vs alignment lines (>1 line/read ⇒ secondary/supplementary):"
echo "  distinct qnames: $(grep -v '^@' "$BASE/raw_ct.sam" | cut -f1 | sort -u | wc -l)"
echo "tag types on the raw stream (first 5000 lines):"
grep -v '^@' "$BASE/raw_ct.sam" | head -5000 | grep -oE '	[A-Za-z][A-Za-z0-9]:[AiZfB]:' | sort | uniq -c | sort -rn | head -20
echo "Q5 ORDER: first 5 raw qnames vs first 5 input qnames (must match for lockstep):"
paste <(grep -v '^@' "$BASE/raw_ct.sam" | cut -f1 | head -5) \
      <(awk 'NR%4==1{print substr($0,2)}' "$BASE/se.fq" | head -5)
echo "=== run1 minimap2 command line (from log) ==="
grep -h "Minimap2 command line" -A1 "$BASE/run1.log" 2>/dev/null | head -4
