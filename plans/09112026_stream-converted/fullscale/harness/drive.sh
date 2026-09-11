#!/bin/sh
# Driver for the #1120 full-scale A/B. Runs STRICTLY ONE benchmark at a time:
# each arm gets the whole machine, so no run is another run's noisy neighbour.
# A lockfile makes that guarantee hard rather than conventional.
set -u

FS=${FS:-/tmp/fs}
WORK=${WORK:-$FS/run}
IMAGE=${IMAGE:-bismark-ab}
REPS=${REPS:-1}
# Reps are numbered REP_OFFSET .. REP_OFFSET+REPS-1, so a later batch can extend
# an existing series instead of overwriting rep 1's files.
REP_OFFSET=${REP_OFFSET:-1}
SHAPES=${SHAPES:-"pe_directional_p4"}
R1=${R1:-/fs/data/SRR24766921_10M_1.fastq.gz}
R2=${R2:-/fs/data/SRR24766921_10M_2.fastq.gz}
GENOME=${GENOME:-/fs/genome/GRCm39}
INTERVAL=${INTERVAL:-1}

LOCK=$FS/.bench.lock
if ! mkdir "$LOCK" 2>/dev/null; then
  echo "REFUSING TO START: $LOCK exists — another benchmark is running." >&2
  echo "Benchmarks must not overlap (noisy neighbour). Remove it only if stale." >&2
  exit 1
fi
trap 'rmdir "$LOCK" 2>/dev/null' EXIT INT TERM

mkdir -p "$WORK"
[ -f "$WORK/results.tsv" ] || printf 'shape\tarm\trep\twall_s\tpeak_temp_kb\tpeak_conv_kb\tmem_peak_bytes\tcpu_usec\tpids_peak\tout_kb\trc\n' > "$WORK/results.tsv"

shape_args() {
  case "$1" in
    pe_directional_p4)    echo "-p 4" ;;
    pe_nondirectional_p4) echo "-p 4 --non_directional" ;;
    pe_directional_p1)    echo "-p 1" ;;
    pe_directional_mc2)   echo "-p 4 --multicore 2" ;;
    *) echo "UNKNOWN" ;;
  esac
}

# The 13 GB index and the inputs are read into page cache before EVERY run, so
# no run is penalised for being the one that faulted them in. With 124 GB of RAM
# they stay resident; this is a few seconds once warm.
warm() {
  cat "$FS/genome/$(basename "$GENOME")"/Bisulfite_Genome/*/*.bt2 > /dev/null 2>&1
  cat "$FS/${R1#/fs/}" "$FS/${R2#/fs/}" > /dev/null 2>&1
}

one() { # shape arm rep
  warm
  # Fresh container per run: the cgroup counters start at zero and describe
  # this run's tree alone.
  docker run --rm -t \
    -v "$FS":/fs -v bismark-target-189:/t \
    -e R1="$R1" -e R2="$R2" -e GENOME="$GENOME" -e WORK=/fs/run -e INTERVAL="$INTERVAL" \
    -w /fs "$IMAGE" \
    /fs/bin/run_arm.sh "$1" "$2" "$3" $(shape_args "$1")
}

for shape in $SHAPES; do
  [ "$(shape_args "$shape")" = UNKNOWN ] && { echo "unknown shape: $shape" >&2; exit 2; }
  echo "### $shape — $REPS rep(s), serial, arm order alternating per rep"
  r=$REP_OFFSET
  last=$(( REP_OFFSET + REPS - 1 ))
  while [ "$r" -le "$last" ]; do
    # Alternate which arm goes first. Fixed order would hand the page-cache
    # warm-up penalty to the same arm on every rep.
    if [ $((r % 2)) -eq 1 ]; then order="stream files"; else order="files stream"; fi
    for arm in $order; do one "$shape" "$arm" "$r"; done
    r=$((r + 1))
  done
done

echo
echo "### results.tsv written to $WORK/results.tsv"
