#!/bin/bash
# Wall-time and peak-disk A/B for #1120, streaming vs --no_stream_converted, on a
# fixture large enough for the numbers to mean something. Run with bench-ab-run.sh.
#
# The research harness (BENCHMARKS.md) measured a stand-in `awk` converter against
# real Bowtie 2; this measures the shipped implementation end to end.
set -uo pipefail
cd /repo/rust
BIN=/repo/rust/target/release
REPS=${REPS:-7}
COPIES=${COPIES:-40}

echo "### building (release)"
cargo build --locked --release -p bismark --bins 2>&1 | tail -2 || exit 1

mkdir -p /work/genome
zcat /repo/test_files/NC_010473.fa.gz > /work/genome/ecoli.fa
[ -d /work/genome/Bisulfite_Genome ] || {
  echo "### preparing the bisulfite genome"
  $BIN/bismark_genome_preparation --bowtie2 /work/genome >/dev/null 2>&1 || { echo "FAIL prep"; exit 1; }
}

# A bigger input: COPIES x the 5,000-pair fixture, with unique read IDs.
if [ ! -f /work/big_R1.fastq ]; then
  echo "### building a ${COPIES}x fixture"
  for m in 1 2; do
    : > /work/big_R${m}.fastq
    for i in $(seq 1 $COPIES); do
      zcat /repo/test_files/test_R${m}.fastq.gz | awk -v i=$i 'NR%4==1 { sub(/^@/, "@c" i "_"); } { print }' >> /work/big_R${m}.fastq
    done
  done
fi
R1=/work/big_R1.fastq; R2=/work/big_R2.fastq
echo "    input: $(du -ch $R1 $R2 | tail -1 | cut -f1) uncompressed, $(( $(wc -l < $R1) / 4 )) pairs"

# Peak converted-bytes-on-disk, sampled while the run is in flight.
watch_temp() {
  peak=0
  while kill -0 "$1" 2>/dev/null; do
    cur=$(du -sk "$2" 2>/dev/null | cut -f1); cur=${cur:-0}
    [ "$cur" -gt "$peak" ] && peak=$cur
    sleep 0.2
  done
  echo "$peak"
}

run() { # $1 = arm
  out=/work/bench_out_$1; tmp=/work/bench_tmp_$1
  rm -rf "$out" "$tmp"; mkdir -p "$out" "$tmp"
  extra=""; [ "$1" = files ] && extra="--no_stream_converted"
  start=$(date +%s%N)
  # shellcheck disable=SC2086
  $BIN/bismark --genome /work/genome -1 $R1 -2 $R2 $extra -p 4 \
    -o "$out" --temp_dir "$tmp" >/dev/null 2>"$out/err" &
  pid=$!
  peak=$(watch_temp $pid "$tmp")
  wait $pid; rc=$?
  end=$(date +%s%N)
  [ $rc -ne 0 ] && { echo "*** arm $1 exited $rc ***"; tail -5 "$out/err"; }
  awk -v a="$start" -v b="$end" -v p="$peak" 'BEGIN { printf "%.2f %s\n", (b-a)/1e9, p }'
}

# Interleaved, not arm-major: a machine that drifts (thermal, noisy neighbour)
# would otherwise hand the whole drift to one arm.
echo "### $REPS reps, -p 4, arms interleaved"
printf '%-12s %-12s %s\n' arm 'wall (s)' 'peak temp (KB)'
: > /work/bench.tsv
for r in $(seq 1 $REPS); do
  for arm in stream files; do
    read -r w p <<< "$(run $arm)"
    printf '%-12s %-12s %s\n' "$arm#$r" "$w" "$p"
    printf '%s\t%s\t%s\n' "$arm" "$w" "$p" >> /work/bench.tsv
  done
done

echo
echo "### medians"
# Plain sort + line pick: the image ships mawk, which has no multidimensional
# arrays. With an odd REPS this is the exact median; with an even one it is the
# lower of the two middle values.
for arm in stream files; do
  n=$(grep -c "^$arm	" /work/bench.tsv)
  [ "$n" -eq 0 ] && continue
  med=$(grep "^$arm	" /work/bench.tsv | cut -f2 | sort -n | sed -n "$(( (n + 1) / 2 ))p")
  peak=$(grep "^$arm	" /work/bench.tsv | cut -f3 | sort -n | tail -1)
  printf '%-12s median %s s over %s reps, peak temp %s KB\n' "$arm" "$med" "$n" "$peak"
done
