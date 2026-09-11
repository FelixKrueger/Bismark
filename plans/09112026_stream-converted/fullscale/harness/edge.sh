#!/bin/bash
# FULLSCALE.md section 7 — the cases only a big run can reach. These are
# pass/fail probes, not benchmarks, so they use a smaller subset than the A/B
# matrix; what each one needs is the SHAPE of the run, not its length.
set -uo pipefail

BIN=${BIN:-/t/release}
GENOME=${GENOME:-/fs/genome/GRCm39}
R1=${R1:?}; R2=${R2:?}
WORK=${WORK:-/fs/edge}
mkdir -p "$WORK"
fail=0
say() { printf '\n=== %s ===\n' "$*"; }

# ---------------------------------------------------------------- 7.3 limits
say "7.3 — thread and FD counts under the widest fan-out"
# Non-directional PE is 4 converted streams and 8 pipes; --multicore doubles the
# concurrent chunk count on top.
out=$WORK/fd_out; tmp=$WORK/fd_tmp; rm -rf "$out" "$tmp"; mkdir -p "$out" "$tmp"
$BIN/bismark --genome "$GENOME" -1 "$R1" -2 "$R2" --non_directional -p 2 --multicore 2 \
  -o "$out" --temp_dir "$tmp" > "$out/stdout.log" 2> "$out/stderr.log" &
pid=$!
max_fd=0; max_thr=0; max_pids=0
while kill -0 $pid 2>/dev/null; do
  # Whole tree: the parent plus every bowtie2 child.
  tot_fd=0; tot_thr=0; n=0
  for p in $pid $(pgrep -P $pid 2>/dev/null); do
    f=$(ls /proc/$p/fd 2>/dev/null | wc -l); tot_fd=$((tot_fd + f))
    t=$(ls /proc/$p/task 2>/dev/null | wc -l); tot_thr=$((tot_thr + t))
    n=$((n + 1))
  done
  [ $tot_fd  -gt $max_fd  ] && max_fd=$tot_fd
  [ $tot_thr -gt $max_thr ] && max_thr=$tot_thr
  [ $n       -gt $max_pids ] && max_pids=$n
  sleep 0.5
done
wait $pid; rc=$?
soft=$(ulimit -Sn); hard=$(ulimit -Hn)
echo "  peak open FDs (tree): $max_fd     ulimit -n soft=$soft hard=$hard"
echo "  peak threads (tree):  $max_thr    peak processes: $max_pids"
echo "  exit: $rc"
[ "$rc" -ne 0 ] && { echo "  *** FAIL: run exited $rc ***"; tail -15 "$out/stderr.log"; fail=1; }
awk -v f="$max_fd" -v s="$soft" 'BEGIN {
  printf "  headroom: %.1f%% of the soft FD limit used\n", f * 100 / s
  if (f > s * 0.5) print "  *** WARNING: over half the FD limit ***"
}'

# ---------------------------------------------------- 7.1 long names + multicore
say "7.1 — 200-character sample names under --multicore 4"
# FIFO names cap the stem at NAME_STEM_CAP (64 bytes) and uniqueness comes from a
# process-wide counter, not the name. A collision would show as interleaved or
# truncated output rather than an error, so this is checked by byte-identity
# against a short-named run, not just by exit code.
long=$(printf 'a%.0s' $(seq 1 190))
mkdir -p "$WORK/longname"
ln -sf "$R1" "$WORK/longname/${long}_1.fastq.gz"
ln -sf "$R2" "$WORK/longname/${long}_2.fastq.gz"
echo "  basename length: ${#long}_1.fastq.gz -> $(( ${#long} + 13 )) chars"

for variant in short long; do
  out=$WORK/mc_${variant}_out; tmp=$WORK/mc_${variant}_tmp
  rm -rf "$out" "$tmp"; mkdir -p "$out" "$tmp"
  if [ "$variant" = short ]; then a=$R1; b=$R2
  else a=$WORK/longname/${long}_1.fastq.gz; b=$WORK/longname/${long}_2.fastq.gz; fi
  $BIN/bismark --genome "$GENOME" -1 "$a" -2 "$b" -p 2 --multicore 4 \
    -o "$out" --temp_dir "$tmp" > "$out/stdout.log" 2> "$out/stderr.log"
  rc=$?
  echo "  $variant: exit $rc"
  [ $rc -ne 0 ] && { echo "  *** FAIL ***"; tail -15 "$out/stderr.log"; fail=1; continue; }
  samtools view "$out"/*.bam 2>/dev/null | sort > "$out/records.sorted"
  echo "    records: $(wc -l < "$out/records.sorted")"
done
if [ -s "$WORK/mc_short_out/records.sorted" ] && [ -s "$WORK/mc_long_out/records.sorted" ]; then
  # Read names differ (they carry the sample name), so compare the alignment
  # columns that do not: FLAG, RNAME, POS, MAPQ, CIGAR, SEQ.
  for v in short long; do
    cut -f2-6,10 "$WORK/mc_${v}_out/records.sorted" | sort > "$WORK/mc_${v}.cols"
  done
  if cmp -s "$WORK/mc_short.cols" "$WORK/mc_long.cols"; then
    echo "  long-name run is alignment-identical to the short-name run"
  else
    echo "  *** FAIL: long-name run DIFFERS from short-name run ***"; fail=1
  fi
fi

# --------------------------------------------- 7.2 genuinely constrained temp_dir
say "7.2 — a --temp_dir smaller than the uncompressed input"
# The premise of the whole feature: the streamed arm should complete where the
# file arm runs out of room. /tiny is a small tmpfs supplied by the caller.
TINY=${TINY:-/tiny}
if [ ! -d "$TINY" ]; then
  echo "  SKIP: $TINY not mounted"
else
  avail=$(df -k "$TINY" | awk 'NR==2 {print $2}')
  need=$(( $(zcat "$R1" | wc -c) / 1024 ))
  echo "  temp_dir capacity: $(( avail / 1024 )) MiB"
  echo "  one converted copy of R1 alone: $(( need / 1024 )) MiB (the file arm needs this x2)"
  for arm in files stream; do
    out=$WORK/tiny_${arm}_out; rm -rf "$out"; mkdir -p "$out"
    rm -rf "${TINY:?}"/*; 
    extra=""; [ "$arm" = files ] && extra="--no_stream_converted"
    # shellcheck disable=SC2086
    $BIN/bismark --genome "$GENOME" -1 "$R1" -2 "$R2" -p 4 $extra \
      -o "$out" --temp_dir "$TINY" > "$out/stdout.log" 2> "$out/stderr.log"
    rc=$?
    n=$(samtools view "$out"/*.bam 2>/dev/null | wc -l)
    printf '  %-7s exit %-3s records %s\n' "$arm" "$rc" "$n"
    [ "$arm" = files ]  && [ $rc -eq 0 ] && echo "    (note: the file arm FIT — temp_dir is not tight enough to prove the point)"
    [ "$arm" = files ]  && [ $rc -ne 0 ] && { echo "    file arm failed as expected:"; grep -iE "space|ENOSPC|No space" "$out/stderr.log" | tail -3; }
    [ "$arm" = stream ] && [ $rc -ne 0 ] && { echo "  *** FAIL: the streamed arm must survive here ***"; tail -15 "$out/stderr.log"; fail=1; }
  done
  rm -rf "${TINY:?}"/*
fi

echo
[ $fail -eq 0 ] && echo "### SECTION 7: ALL PASS" || echo "### SECTION 7: FAILURES ABOVE"
exit $fail
