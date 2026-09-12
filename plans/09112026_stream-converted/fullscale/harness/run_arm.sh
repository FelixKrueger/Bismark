#!/bin/bash
# One benchmark arm, run alone inside its own container so the cgroup counters
# (memory.current / cpu.stat / pids.current) describe exactly this run's process
# tree and nothing else. Emits a time series, not just a peak.
#
# The sample loop is kept to ~3 forks per tick (one find, one awk, one sleep) and
# uses bash builtins for the clock and the cgroup reads. An earlier version spent
# over a second per tick, which put each sample's timestamp a long way in front of
# the measurement it labelled — fatal for the disk curve, where the whole point is
# WHEN the converted bytes appear.
#
# Args: $1=shape $2=arm $3=rep $4.. extra bismark args
set -uo pipefail

shape=$1; arm=$2; rep=$3; shift 3
extra="$*"

BIN=${BIN:-/t/release}
GENOME=${GENOME:-/fs/genome/GRCm39}
R1=${R1:?}; R2=${R2:?}
WORK=${WORK:-/fs/run}
INTERVAL=${INTERVAL:-1}
CG=/sys/fs/cgroup

tag=${shape}_${arm}_r${rep}
out=$WORK/out/$tag; tmp=$WORK/tmp/$tag
rm -rf "$out" "$tmp"; mkdir -p "$out" "$tmp" "$WORK/series"
series=$WORK/series/$tag.tsv

[ "$arm" = files ] && extra="$extra --no_stream_converted"

t0=$EPOCHREALTIME
# EPOCHREALTIME is "sec.usec" with a fixed 6-digit fraction, so this is exact
# integer microseconds and needs no subprocess.
us() { local v=$1; echo $(( ${v%.*} * 1000000 + 10#${v#*.} )); }
t0_us=$(us "$t0")
# shellcheck disable=SC2086
$BIN/bismark --genome "$GENOME" -1 "$R1" -2 "$R2" $extra \
  -o "$out" --temp_dir "$tmp" > "$out/stdout.log" 2> "$out/stderr.log" &
pid=$!

printf 't_s\ttemp_kb\tout_kb\tconverted_kb\tmem_bytes\tanon_bytes\tcpu_pct\tpids\tfifos\n' > "$series"
read -r _ prev_cpu < "$CG/cpu.stat"
prev_us=$t0_us

while kill -0 "$pid" 2>/dev/null; do
  # Clock and cgroup counters are builtins — no forks, so these are taken
  # essentially at the instant the sample is stamped.
  now=$EPOCHREALTIME
  now_us=$(( ${now%.*} * 1000000 + 10#${now#*.} ))
  read -r mem      < "$CG/memory.current"
  # memory.current counts page cache too, so the file arm looks like it uses
  # gigabytes more memory when what it actually has is its own converted files
  # sitting in cache. anon is the allocation that matters for the comparison.
  read -r _ anon   < "$CG/memory.stat"
  read -r npid     < "$CG/pids.current"
  read -r _ cpu    < "$CG/cpu.stat"

  # One walk covers both directories: total, converted payload and FIFO count.
  read -r temp_kb conv_kb fifos out_kb <<< "$(
    find "$tmp" "$out" -mindepth 1 \( -type f -o -type p \) \
         -printf '%y\t%s\t%p\n' 2>/dev/null |
    awk -F'\t' -v tmp="$tmp/" -v out="$out/" '
      { if (index($3, tmp) == 1) {
          if ($1 == "p") { f++ } else { t += $2
            if ($3 ~ /_C_to_T|_G_to_A/) c += $2 }
        } else if (index($3, out) == 1) { o += $2 } }
      END { printf "%d %d %d %d", t/1024, c/1024, f+0, o/1024 }')"

  # cpu.stat is cumulative microseconds of CPU and the clock is microseconds too,
  # so percent-of-one-core is a plain ratio. Integer math, one decimal place.
  dt=$(( now_us - prev_us ))
  if [ "$dt" -gt 0 ]; then dc=$(( (cpu - prev_cpu) * 1000 / dt )); else dc=0; fi
  prev_cpu=$cpu; prev_us=$now_us

  el=$(( now_us - t0_us ))
  printf '%d.%02d\t%s\t%s\t%s\t%s\t%s\t%d.%d\t%s\t%s\n' \
    $(( el / 1000000 )) $(( el % 1000000 / 10000 )) \
    "$temp_kb" "$out_kb" "$conv_kb" "$mem" "$anon" \
    $(( dc / 10 )) $(( dc % 10 )) "$npid" "$fifos" >> "$series"
  sleep "$INTERVAL"
done
wait "$pid"; rc=$?
end=$EPOCHREALTIME

end_us=$(us "$end"); wall_us=$(( end_us - t0_us ))
wall=$(printf '%d.%02d' $(( wall_us / 1000000 )) $(( wall_us % 1000000 / 10000 )))
read -r mem_peak  < "$CG/memory.peak"
read -r pids_peak < "$CG/pids.peak"
read -r _ cpu_tot < "$CG/cpu.stat"
peak_temp=$(awk -F'\t' 'NR>1 && $2>m {m=$2} END {print m+0}' "$series")
peak_conv=$(awk -F'\t' 'NR>1 && $4>m {m=$4} END {print m+0}' "$series")
# printf %d, not print: awk turns a 24-billion-byte value into 2.44536e+10 and
# throws away the low digits.
peak_anon=$(awk -F'\t' 'NR>1 && $6>m {m=$6} END {printf "%.0f", m}' "$series")
final_out=$(du -sk "$out" 2>/dev/null | cut -f1)

# C2: nothing converted may survive in the streamed arm's temp dir.
leftover=$(find "$tmp" \( -name '*_C_to_T*' -o -name '*_G_to_A*' -o -name '*.fifo.*' \) 2>/dev/null | wc -l)

[ -s "$WORK/results.tsv" ] || printf 'shape\tarm\trep\twall_s\tpeak_temp_kb\tpeak_conv_kb\tmem_peak_bytes\tanon_peak_bytes\tcpu_usec\tpids_peak\tout_kb\trc\n' > "$WORK/results.tsv"
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$shape" "$arm" "$rep" "$wall" "$peak_temp" "$peak_conv" "$mem_peak" "$peak_anon" \
  "$cpu_tot" "$pids_peak" "${final_out:-0}" "$rc" >> "$WORK/results.tsv"

printf '  %-24s %-6s r%-2s wall %8ss  peak_conv %9s KB  peak_temp %9s KB  memPeak %7s MB  pids %3s  samples %s  rc=%s\n' \
  "$shape" "$arm" "$rep" "$wall" "$peak_conv" "$peak_temp" \
  "$((mem_peak/1048576))" "$pids_peak" "$(( $(wc -l < "$series") - 1 ))" "$rc"
[ "$leftover" -ne 0 ] && echo "  *** $tag: streamed arm left $leftover converted/fifo file(s) ***"
[ "$rc" -ne 0 ] && tail -20 "$out/stderr.log"
exit "$rc"
