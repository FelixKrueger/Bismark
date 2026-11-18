#!/bin/bash
# bwa-mem3 vs bwa-mem4 alone, Bismark's exact --meth command line, interleaved repetitions.
# Usage: bench_engine.sh <index prefix> <R1> <R2> <reps> <threads...>
# Prints TSV on stdout; also keeps one SAM per engine in $OUT_SAM_DIR (if set) for an identity check.
set -u
S=${S:?set S to the folder holding mm3env/, bwa4-tag/, bismark-dev and bismark-new}
B3=$S/mm3env/bin/bwa-mem3
B4=${B4:-$S/bwa4-tag/release/bwa-mem4}
idx=$1; r1=$2; r2=$3; reps=$4; shift 4
opts=(mem --meth=5base --meth-scoring neutral --meth-context all --meth-seed-prune=spec30 --meth-tags none -K 100000000)
tmp=$(mktemp -d "$S/bench.XXXX")
echo -e "engine\tthreads\trep\twall_s\tuser_s\tsys_s\tmax_rss_bytes\tpeak_footprint_bytes\tload1\tbody_md5"
for t in "$@"; do
  for rep in $(seq 1 "$reps"); do
    for e in 3 4; do
      bin=$B3; [ "$e" = 4 ] && bin=$B4
      load=$(sysctl -n vm.loadavg | awk '{print $2}')
      /usr/bin/time -l "$bin" "${opts[@]}" -t "$t" "$idx" "$r1" "$r2" >"$tmp/sam" 2>"$tmp/err"
      # The /usr/bin/time line is the only one that starts with spaces then "<n> real".
      read -r wall user sys < <(awk '/^ +[0-9.]+ real +[0-9.]+ user +[0-9.]+ sys/{print $1, $3, $5}' "$tmp/err")
      rss=$(awk '/maximum resident set size/{print $1}' "$tmp/err")
      fp=$(awk '/peak memory footprint/{print $1}' "$tmp/err")
      md5=$(grep -v '^@' "$tmp/sam" | md5)
      [ -n "${OUT_SAM_DIR:-}" ] && [ "$rep" = 1 ] && cp "$tmp/sam" "$OUT_SAM_DIR/bwa-mem$e.t$t.sam"
      echo -e "bwa-mem$e\t$t\t$rep\t$wall\t$user\t$sys\t$rss\t$fp\t$load\t$md5"
    done
  done
done
