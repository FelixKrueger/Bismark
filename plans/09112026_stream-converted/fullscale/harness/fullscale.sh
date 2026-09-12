#!/bin/bash
# FULL-SCALE A/B for #1120 — streamed (default) vs --no_stream_converted.
# Follows plans/09112026_stream-converted/FULLSCALE.md sections 4-6.
#
# Env:
#   GENOME  bisulfite-prepared genome dir      R1/R2  input FASTQs
#   SHAPES  space-separated shape labels       REPS   reps per arm (interleaved)
#   WORK    scratch root
set -uo pipefail

BIN=${BIN:-/t/release}
GENOME=${GENOME:-/fs/genome/GRCm39}
R1=${R1:-/fs/data/SRR24766921_10M_1.fastq.gz}
R2=${R2:-/fs/data/SRR24766921_10M_2.fastq.gz}
WORK=${WORK:-/fs/run}
REPS=${REPS:-1}
SHAPES=${SHAPES:-"pe_directional_p4"}

mkdir -p "$WORK"
TSV=$WORK/results.tsv
[ -f "$TSV" ] || printf 'shape\tarm\trep\twall_s\tpeak_temp_kb\tmax_rss_kb\tcpu_pct\trc\n' > "$TSV"

# Shape label -> extra bismark args (everything bar --genome/-1/-2/-o/--temp_dir)
shape_args() {
  case "$1" in
    pe_directional_p4)     echo "-p 4" ;;
    pe_nondirectional_p4)  echo "-p 4 --non_directional" ;;
    pe_directional_p1)     echo "-p 1" ;;
    pe_directional_mc2)    echo "-p 4 --multicore 2" ;;
    *) echo "unknown shape: $1" >&2; return 1 ;;
  esac
}

# Peak temp-dir bytes, SAMPLED live: both arms clean up, so a post-hoc du is
# always zero and proves nothing (FULLSCALE.md section 5).
watch_temp() {
  local pid=$1 dir=$2 peak=0 cur
  while kill -0 "$pid" 2>/dev/null; do
    cur=$(du -sk "$dir" 2>/dev/null | cut -f1); cur=${cur:-0}
    [ "$cur" -gt "$peak" ] && peak=$cur
    sleep 0.2
  done
  echo "$peak"
}

run_one() { # $1=shape $2=arm $3=rep
  local shape=$1 arm=$2 rep=$3
  local out=$WORK/${shape}_${arm} tmp=$WORK/tmp_${shape}_${arm}
  rm -rf "$out" "$tmp"; mkdir -p "$out" "$tmp"
  local extra; extra=$(shape_args "$shape") || return 1
  [ "$arm" = files ] && extra="$extra --no_stream_converted"

  local start end rc peak pid
  start=$(date +%s%N)
  # shellcheck disable=SC2086
  /usr/bin/time -v -o "$out/time.txt" \
    $BIN/bismark --genome "$GENOME" -1 "$R1" -2 "$R2" $extra \
      -o "$out" --temp_dir "$tmp" > "$out/stdout.log" 2> "$out/stderr.log" &
  pid=$!
  peak=$(watch_temp "$pid" "$tmp")
  wait "$pid"; rc=$?
  end=$(date +%s%N)

  local rss cpu wall
  rss=$(awk -F': ' '/Maximum resident set size/ {print $2}' "$out/time.txt" 2>/dev/null)
  cpu=$(awk -F': ' '/Percent of CPU/ {print $2}' "$out/time.txt" 2>/dev/null | tr -d '%')
  wall=$(awk -v a="$start" -v b="$end" 'BEGIN { printf "%.2f", (b-a)/1e9 }')

  [ $rc -ne 0 ] && { echo "  *** $shape/$arm rep$rep exited $rc ***"; tail -20 "$out/stderr.log"; }

  # C2: nothing converted may survive in the streamed arm's temp dir.
  local leftover
  leftover=$(find "$tmp" \( -name '*_C_to_T*' -o -name '*_G_to_A*' -o -name '*.fifo.*' \) 2>/dev/null | wc -l)
  echo "$leftover" > "$out/leftover.count"

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$shape" "$arm" "$rep" "$wall" "$peak" "${rss:-NA}" "${cpu:-NA}" "$rc" >> "$TSV"
  printf '  %-22s %-7s rep%-3s wall %8ss  peak_temp %10s KB  rss %10s KB  leftover %s\n' \
    "$shape" "$arm" "$rep" "$wall" "$peak" "${rss:-NA}" "$leftover"
}

# C1: records must match exactly. Compare `samtools view` bodies, not raw BAM —
# the header's @PG CL: line is the verbatim argv and differs by design.
compare() {
  local shape=$1 a=$WORK/${shape}_stream b=$WORK/${shape}_files ok=0
  samtools view "$a"/*.bam > "$a/records.sam" 2>/dev/null
  samtools view "$b"/*.bam > "$b/records.sam" 2>/dev/null
  for d in "$a" "$b"; do
    grep -vE '^(Bismark completed|Bismark version)' "$d"/*_report.txt 2>/dev/null \
      | grep -vE "$WORK|/fs/" > "$d/report.filtered"
  done
  if cmp -s "$a/records.sam" "$b/records.sam"; then bam="BAM identical"; else bam="*** BAM DIFFERS ***"; ok=1; fi
  if cmp -s "$a/report.filtered" "$b/report.filtered"; then rep="report identical"; else rep="*** report DIFFERS ***"; ok=1; fi
  printf '  %-22s %-22s %-22s (%s records)\n' "$shape" "$bam" "$rep" "$(wc -l < "$a/records.sam")"
  [ "$(cat "$a/leftover.count")" -ne 0 ] && { echo "  *** $shape streamed arm left converted/fifo files ***"; ok=1; }
  return $ok
}

fail=0
for shape in $SHAPES; do
  echo "### $shape — $REPS rep(s), arms interleaved"
  for r in $(seq 1 "$REPS"); do
    for arm in stream files; do run_one "$shape" "$arm" "$r"; done
  done
  compare "$shape" || fail=1
done

echo
echo "### medians"
for shape in $SHAPES; do
  for arm in stream files; do
    n=$(awk -F'\t' -v s="$shape" -v a="$arm" '$1==s && $2==a' "$TSV" | wc -l)
    [ "$n" -eq 0 ] && continue
    med=$(awk -F'\t' -v s="$shape" -v a="$arm" '$1==s && $2==a {print $4}' "$TSV" | sort -n | sed -n "$(( (n + 1) / 2 ))p")
    pk=$(awk -F'\t' -v s="$shape" -v a="$arm" '$1==s && $2==a {print $5}' "$TSV" | sort -n | tail -1)
    printf '  %-22s %-7s median %8s s over %s rep(s), peak temp %10s KB\n' "$shape" "$arm" "$med" "$n" "$pk"
  done
done
exit $fail
