#!/bin/bash
# Re-verify everything against the released bwa-mem4 v4.5.0 (tag b4197fc), baseline build.
set -u
S=${S:?set S to the folder holding mm3env/, bwa4-tag/, bismark-dev, bismark-new, devbin/ and the run folders}
export S B4=$S/bwa4-tag/release/bwa-mem4 B4DIR=$S/bwa4-tag/release
O=$S/tag; mkdir -p "$O"
echo "== uptime"; uptime

echo "== synthetic: Bismark BAM + extractor identity vs dev --bwamem3 (out3)"
cd "$S/run" || exit 1
d=tag_bis_$(date +%s); mkdir "$d"
"$S/bismark-new" --illumina_5base --bwamem4 --path_to_bwamem4 "$B4DIR" --five_base_index idx3/genome.fa \
  --genome genome -1 R1.fq.gz -2 R2.fq.gz -p 8 -o "$d" >"$d.log" 2>&1 || echo "bismark failed"
echo "bam dev=$(samtools view out3/*.bam | md5) tag=$(samtools view "$d"/*.bam | md5)"
mkdir "ex_$d"
"$S/devbin/bismark_methylation_extractor" -p --bedGraph --gzip --parallel 4 -o "ex_$d" "$d"/*.bam >"ex_$d.log" 2>&1 || echo "extractor failed"
n=0
for f in ex_out3/*; do
  g="ex_$d/$(basename "$f" | sed s/bwamem3/bwamem4/)"
  if [[ $f == *.gz ]]; then a=$(gzip -dc "$f" | sed s/bwamem3/bwamem4/g | md5); b=$(gzip -dc "$g" | md5)
  else a=$(sed s/bwamem3/bwamem4/g "$f" | md5); b=$(md5 < "$g"); fi
  [[ $a == "$b" ]] || { echo "DIFF $(basename "$g")"; n=$((n+1)); }
done
echo "extractor files compared: $(ls ex_out3 | wc -l), diffs=$n"

echo "== chr21 engine"
cd "$S/chr21run" && bash "$S/bench_engine.sh" idx3/chr21.fa R1.fq.gz R2.fq.gz 3 1 8 >"$O/bench_chr21.tsv" && python3 -I "$S/summarise.py" "$O/bench_chr21.tsv"
echo "== chr21 Bismark"
bash "$S/bench_bismark.sh" "$S/chr21run" idx3/chr21.fa 3 8 >"$O/bench_bismark_chr21.tsv" && column -t "$O/bench_bismark_chr21.tsv"
echo "== GRCh38 engine"
cd "$S/grch38run" && bash "$S/bench_engine.sh" idx3/genome.fa R1.fq.gz R2.fq.gz 3 8 16 >"$O/bench_grch38.tsv" && python3 -I "$S/summarise.py" "$O/bench_grch38.tsv"
echo "== GRCh38 Bismark"
bash "$S/bench_bismark.sh" "$S/grch38run" idx3/genome.fa 3 8 >"$O/bench_bismark_grch38.tsv" && column -t "$O/bench_bismark_grch38.tsv"
echo "== uptime"; uptime
