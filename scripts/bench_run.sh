#!/usr/bin/env bash
#
# bench_run.sh — one timed extractor config (one tool, one mode, one --parallel),
# repeated R times, emitting one CSV row per rep. Part of the full-dataset
# benchmark campaign (plans/05302026_extractor-fulldata-validation/PLAN.md).
#
# Captures, per rep:
#   wall_s       — GNU /usr/bin/time -v "Elapsed (wall clock)"
#   cpu_cores    — (User + System) / wall   [from time -v rusage; per-mode — gzip
#                  write-back deflates this, so gzip-mode cores are a FLOOR]
#   max_rss_kb   — time -v "Maximum resident set size" (kernel hiwater; authoritative)
#   peak_threads — max of /proc/<pid>/task count, sampled at 0.2s by PID (NOT pgrep-by-name)
#   peak_fds     — max of /proc/<pid>/fd count (empirically confirms the ~12 open .gz files)
#   exit         — child exit code; non-zero (incl. Rust gzip ENOSPC panic) is recorded as FAILURE
#
# Usage:
#   bench_run.sh <BAM> --tool {rust|perl} --mode {gzip|plain|mbias_only} \
#       --parallel N --reps R --dataset NAME --out DIR [--csv FILE] [--min-free-gb G]
#
# Env: RUST_BIN, PERL_BIN (resolved paths to the two binaries).
#
# Exit: 0 if all reps exited 0; 1 if any rep failed (a row is still written with exit!=0).

set -euo pipefail

BAM="" TOOL="" MODE="" PARALLEL="" REPS=3 DATASET="" OUT_DIR="" CSV="" MIN_FREE_GB=20
while [[ $# -gt 0 ]]; do
  case $1 in
    --tool) TOOL="$2"; shift 2 ;;
    --mode) MODE="$2"; shift 2 ;;
    --parallel) PARALLEL="$2"; shift 2 ;;
    --reps) REPS="$2"; shift 2 ;;
    --dataset) DATASET="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    --csv) CSV="$2"; shift 2 ;;
    --min-free-gb) MIN_FREE_GB="$2"; shift 2 ;;
    *) if [[ -z "$BAM" ]]; then BAM="$1"; shift; else echo "unexpected arg: $1" >&2; exit 2; fi ;;
  esac
done
[[ -z "$BAM" || -z "$TOOL" || -z "$MODE" || -z "$PARALLEL" || -z "$DATASET" || -z "$OUT_DIR" ]] && {
  echo "usage: bench_run.sh <BAM> --tool {rust|perl} --mode {gzip|plain|mbias_only} --parallel N --reps R --dataset NAME --out DIR" >&2; exit 2; }
CSV="${CSV:-$OUT_DIR/results.csv}"

RUST_BIN="${RUST_BIN:-$HOME/Github/Bismark/rust/target/release/bismark-methylation-extractor-rs}"
PERL_BIN="${PERL_BIN:-$HOME/micromamba/envs/bismark-test/bin/bismark_methylation_extractor}"
command -v /usr/bin/time >/dev/null || { echo "GNU /usr/bin/time required" >&2; exit 2; }

# Input MUST be a real local file (S3 symlinks contaminate timing) — hard guard.
if [[ -L "$BAM" ]]; then echo "ERROR: $BAM is a symlink — stage it to local disk first" >&2; exit 2; fi
[[ -f "$BAM" ]] || { echo "ERROR: BAM not found: $BAM" >&2; exit 2; }

# Free-space precheck (gzip mode writes ~12 large .gz; Rust panics on ENOSPC).
AVAIL_GB=$(( $(df -Pk "$OUT_DIR" 2>/dev/null | awk 'NR==2{print $4}') / 1024 / 1024 ))
if [[ "$MODE" == "gzip" && "$AVAIL_GB" -lt "$MIN_FREE_GB" ]]; then
  echo "ERROR: only ${AVAIL_GB}GB free in $OUT_DIR; need >= ${MIN_FREE_GB}GB for gzip mode" >&2; exit 2
fi

# PE auto-detect (matches phase_h_smoke.sh / Perl behaviour).
PE_FLAG=""
if command -v samtools >/dev/null 2>&1 && samtools view -H "$BAM" | grep -q '@PG.*ID:Bismark.*-1 '; then
  PE_FLAG="--paired-end"
fi

case "$MODE" in
  gzip) MODE_FLAGS=(--gzip) ;;
  plain) MODE_FLAGS=() ;;
  mbias_only) MODE_FLAGS=(--mbias_only) ;;
  *) echo "unknown mode: $MODE" >&2; exit 2 ;;
esac

if [[ "$TOOL" == "rust" ]]; then
  BIN="$RUST_BIN"; OUT_FLAG=(--output_dir); PAR_FLAG=(--parallel "$PARALLEL")
elif [[ "$TOOL" == "perl" ]]; then
  BIN="$PERL_BIN"; OUT_FLAG=(--output); PAR_FLAG=(--multicore "$PARALLEL")
else echo "unknown tool: $TOOL" >&2; exit 2; fi
[[ -x "$BIN" ]] || { echo "ERROR: binary not executable: $BIN" >&2; exit 2; }

# CSV header (once).
if [[ ! -f "$CSV" ]]; then
  echo "tool,dataset,mode,parallel,rep,wall_s,cpu_cores,max_rss_kb,peak_threads,peak_fds,exit" > "$CSV"
fi

CLK=$(getconf CLK_TCK 2>/dev/null || echo 100)  # reserved; time -v gives User/System directly
ANY_FAIL=0
for rep in $(seq 1 "$REPS"); do
  run_out="$OUT_DIR/${TOOL}_${MODE}_p${PARALLEL}_rep${rep}"
  rm -rf "$run_out"; mkdir -p "$run_out"
  tf="$run_out/.time.txt"

  # Run under GNU time -v; capture the real child PID via parent lookup for /proc sampling.
  /usr/bin/time -v -o "$tf" "$BIN" "${OUT_FLAG[@]}" "$run_out" "${PAR_FLAG[@]}" \
      ${PE_FLAG:+$PE_FLAG} ${MODE_FLAGS[@]+"${MODE_FLAGS[@]}"} "$BAM" >"$run_out/.stdout" 2>"$run_out/.stderr" &
  tpid=$!

  # Resolve the binary's PID (time's child) — parent-PID lookup, never name-based pgrep.
  cpid=""
  for _ in $(seq 1 50); do cpid=$(pgrep -P "$tpid" 2>/dev/null | head -1 || true); [[ -n "$cpid" ]] && break; sleep 0.05; done

  peak_threads=0; peak_fds=0
  if [[ -n "$cpid" ]]; then
    while kill -0 "$tpid" 2>/dev/null; do
      t=$(ls "/proc/$cpid/task" 2>/dev/null | wc -l || echo 0)
      d=$(ls "/proc/$cpid/fd"   2>/dev/null | wc -l || echo 0)
      [[ "$t" -gt "$peak_threads" ]] && peak_threads=$t
      [[ "$d" -gt "$peak_fds" ]] && peak_fds=$d
      sleep 0.2
    done
  fi
  wait "$tpid"; ec=$?

  # Parse GNU time -v.
  wall=$(awk -F': ' '/Elapsed \(wall clock\)/{print $2}' "$tf" | tr -d ' ')
  # h:mm:ss or m:ss → seconds
  wall_s=$(awk -F: '{ if (NF==3) print $1*3600+$2*60+$3; else if (NF==2) print $1*60+$2; else print $1 }' <<<"$wall")
  usr=$(awk -F': ' '/User time/{print $2}' "$tf" | tr -d ' ')
  sys=$(awk -F': ' '/System time/{print $2}' "$tf" | tr -d ' ')
  rss=$(awk -F': ' '/Maximum resident set size/{print $2}' "$tf" | tr -d ' ')
  cores=$(awk -v u="$usr" -v s="$sys" -v w="$wall_s" 'BEGIN{ if (w>0) printf "%.2f",(u+s)/w; else print "NA" }')

  if [[ "$ec" -ne 0 ]]; then
    ANY_FAIL=1
    echo "  ✗ FAILURE: $TOOL $MODE p$PARALLEL rep$rep exit=$ec (see $run_out/.stderr)" >&2
    tail -3 "$run_out/.stderr" >&2 || true
  fi
  echo "$TOOL,$DATASET,$MODE,$PARALLEL,$rep,${wall_s:-NA},${cores:-NA},${rss:-NA},$peak_threads,$peak_fds,$ec" >> "$CSV"
  echo "  $TOOL $MODE p$PARALLEL rep$rep: wall=${wall_s}s cores=${cores} rss=${rss}KB threads=${peak_threads} fds=${peak_fds} exit=$ec" >&2

  # Keep outputs only for rep1 (for inspection); purge the rest to save disk.
  [[ "$rep" -ne 1 ]] && rm -rf "$run_out"
done
exit $ANY_FAIL
