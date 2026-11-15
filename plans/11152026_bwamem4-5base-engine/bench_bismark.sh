#!/bin/bash
# Whole Bismark run: dev --bwamem3 (bwa-mem3 0.16.0) vs this branch --bwamem4 (bwa-mem4 4.5.0),
# interleaved. Usage: bench_bismark.sh <run dir> <index prefix> <reps> <threads>; prints TSV.
set -u
S=${S:?set S to the folder holding mm3env/, bwa4-tag/, bismark-dev and bismark-new}
cd "$1" || exit 1
idx=$2; reps=$3; t=$4
echo -e "engine\trep\twall_s\tuser_s\tsys_s\tmax_rss_bytes\tload1\tbam_records_md5"
for i in $(seq 1 "$reps"); do
  for e in 3 4; do
    out="bis_r${i}_e${e}_$(date +%s)"
    mkdir "$out" || exit 1
    if [ "$e" = 3 ]; then
      eng=(--bwamem3 --path_to_bwamem3 "$S/mm3env/bin"); bin="$S/bismark-dev"
    else
      eng=(--bwamem4 --path_to_bwamem4 "${B4DIR:-$S/bwa4-tag/release}"); bin="$S/bismark-new"
    fi
    load=$(sysctl -n vm.loadavg | awk '{print $2}')
    /usr/bin/time -l "$bin" "${eng[@]}" --illumina_5base --five_base_index "$idx" \
      --genome genome -1 R1.fq.gz -2 R2.fq.gz -p "$t" -o "$out" >/dev/null 2>"$out.time"
    read -r wall user sys < <(awk '/^ +[0-9.]+ real +[0-9.]+ user +[0-9.]+ sys/{print $1, $3, $5}' "$out.time")
    rss=$(awk '/maximum resident set size/{print $1}' "$out.time")
    md5=$(samtools view "$out"/*.bam | md5)
    echo -e "bwa-mem$e\t$i\t$wall\t$user\t$sys\t$rss\t$load\t$md5"
  done
done
