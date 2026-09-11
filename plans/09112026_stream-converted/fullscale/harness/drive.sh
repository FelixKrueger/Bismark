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

# The container sees $FS as /fs, so WORK has to be translated, NOT hardcoded.
# It was hardcoded to /fs/run once, which sent a 2M batch's output into the 10M
# run's directories and destroyed one of them. Only the per-phase snapshot into
# git made that recoverable.
CWORK=/fs/$(basename "$WORK")

LOCK=$FS/.bench.lock
# The lock enforces "one benchmark at a time". It also has to survive its holder
# being killed: a stale lock once made every later phase refuse instantly and the
# schedule ran to completion in seconds having measured nothing. So the holder
# records its pid, and a lock whose holder is gone is reclaimed rather than obeyed.
if ! mkdir "$LOCK" 2>/dev/null; then
  holder=$(cat "$LOCK/pid" 2>/dev/null)
  if [ -n "$holder" ] && kill -0 "$holder" 2>/dev/null; then
    echo "REFUSING TO START: benchmark pid $holder still running (lock $LOCK)." >&2
    echo "Benchmarks must not overlap (noisy neighbour)." >&2
    exit 1
  fi
  echo "NOTE: reclaiming stale lock $LOCK (holder '${holder:-unknown}' is gone)" >&2
  rm -rf "$LOCK"
  mkdir "$LOCK" 2>/dev/null || { echo "could not take lock" >&2; exit 1; }
fi
echo $$ > "$LOCK/pid"
trap 'rm -rf "$LOCK" 2>/dev/null' EXIT INT TERM

mkdir -p "$WORK"
# run_arm.sh owns the results.tsv header, so the schema is defined in exactly one
# place. Writing it here too is how the file ended up with an 11-column header
# above 12-column rows.

shape_args() {
  case "$1" in
    pe_directional_p4)    echo "-p 4" ;;
    pe_nondirectional_p4) echo "-p 4 --non_directional" ;;
    # FULLSCALE.md asks for -p 1 here. Bismark rejects it: "Please select a value
    # for -p of 2 or more!" (aligner/options.rs:165, matching Perl 7993-8007).
    # -p 2 is the least-slack configuration that actually runs.
    pe_directional_p2)    echo "-p 2" ;;
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
    -e R1="$R1" -e R2="$R2" -e GENOME="$GENOME" -e WORK="$CWORK" -e INTERVAL="$INTERVAL" \
    -w /fs "$IMAGE" \
    /fs/bin/run_arm.sh "$1" "$2" "$3" $(shape_args "$1")
}

for shape in $SHAPES; do
  [ "$(shape_args "$shape")" = UNKNOWN ] && { echo "unknown shape: $shape" >&2; exit 2; }
  echo "### $shape — $REPS rep(s) from #$REP_OFFSET, serial, arm order alternating per rep"
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
