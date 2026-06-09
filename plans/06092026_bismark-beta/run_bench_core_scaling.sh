#!/bin/bash
# ===========================================================================
# run_bench_core_scaling.sh — Bismark alignment core-scaling sweep
# (reuses the run_cell + process-tree RSS sampler from run_bench_alignment_modes.sh)
#
# Sweeps Bowtie 2 threads -p in {2,4,8,12,16} for TWO modes:
#   faithful_dir  = bismark_rs --genome -p P                      (2 instances => 2*P total cores)
#   comb1pass     = bismark_rs --combined_index --non_directional --combined_index_single_pass -p P
#                                                                  (1 instance  => P total cores)
# Per cell: wall(s) · CPU core-s (time -v User+System) · peak process-tree RSS · max concurrent align.
# X-axis of the graphs is -p (threads per Bowtie 2 instance). No concordance pass
# (correctness already gate-proven; this is pure perf).
#
# Usage:  run_bench_core_scaling.sh <READS.fq.gz> <TAG>
# ===========================================================================
set -uo pipefail

READS="${1:?usage: run_bench_core_scaling.sh <READS> <TAG>}"
TAG="${2:?usage: run_bench_core_scaling.sh <READS> <TAG>}"

ENV=/home/fkrueger/micromamba/envs/bismark-test/bin
export PATH="$ENV:$PATH"
BR=/home/fkrueger/Bismark-bench/rust/target/release/bismark_rs   # shipped 0b6bb8b
GENOME=/home/fkrueger/bismark_benchmarks/genome

OUTROOT="/var/tmp/bench_core_scaling/$TAG"
BINDIR="/home/fkrueger/v2spike_out/bench_align_modes"
DURABLE="$BINDIR/$TAG"
mkdir -p "$OUTROOT" "$DURABLE"
LOG="$DURABLE/scaling.log"
SUMMARY="$DURABLE/scaling_summary.tsv"
: > "$LOG"
printf 'mode\tp\ttotal_cores\texit\twall_s\tuser_s\tsys_s\tcpu_core_s\tpeak_rss_kb\tmax_concurrent_align\n' > "$SUMMARY"

log(){ echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }

log "READS=$READS  TAG=$TAG"
log "BR=$BR ($("$BR" --version 2>&1 | grep -i version | head -1 | tr -s ' '))"
log "bowtie2=$("$ENV"/bowtie2-align-s --version 2>/dev/null | head -1)"
log "OUTROOT=$OUTROOT  DURABLE=$DURABLE"

# process-tree peak-RSS + max-concurrent-align sampler (root-PID descendant walk)
sampler(){
  local root="$1" out="$2" stop="$3" peak=0 maxn=0 res sum cnt
  while [ ! -e "$stop" ]; do
    res=$(ps --no-headers -eo pid=,ppid=,rss=,comm= 2>/dev/null | awk -v root="$root" '
      { p[NR]=$1; pp[NR]=$2; r[NR]=$3; c[NR]=$4; n=NR }
      END {
        intree[root]=1
        do {
          changed=0
          for (i=1;i<=n;i++)
            if (!seen[i] && (p[i]==root || intree[pp[i]])) { seen[i]=1; intree[p[i]]=1; changed=1 }
        } while (changed)
        sum=0; cnt=0
        for (i=1;i<=n;i++) if (seen[i]) { sum+=r[i]; if (c[i] ~ /bowtie2-align-[sl]/) cnt++ }
        print sum+0, cnt+0
      }')
    sum=${res%% *}; cnt=${res##* }
    [ "${sum:-0}" -gt "$peak" ] && peak="$sum"
    [ "${cnt:-0}" -gt "$maxn" ] && maxn="$cnt"
    sleep 0.3
  done
  echo "$peak $maxn" > "$out"
}

# $1 mode  $2 p  $3 total_cores ; rest = bismark_rs args (WITHOUT -p/-o/READS)
run_cell(){
  local mode="$1" p="$2" total="$3"; shift 3
  local label="${mode}_p${p}"
  local od="$OUTROOT/$label"; mkdir -p "$od"
  local tf="$od/time.txt" rf="$od/rss.txt" stop="$od/stop"
  rm -f "$stop" "$od"/*_bismark_bt2.bam 2>/dev/null
  log ">>> $label (total_cores=$total): $* -p $p -o $od $READS"
  local t0=$SECONDS
  /usr/bin/time -v -o "$tf" "$BR" "$@" -p "$p" --path_to_bowtie2 "$ENV" -o "$od" "$READS" > "$od/run.log" 2>&1 &
  local timepid=$!
  sampler "$timepid" "$rf" "$stop" & local sp=$!
  wait "$timepid"; local rc=$?
  local wall=$((SECONDS - t0))
  touch "$stop"; wait "$sp" 2>/dev/null
  local user sys peak maxn cpu
  user=$(awk -F': ' '/User time/{print $2}' "$tf" 2>/dev/null); user=${user:-0}
  sys=$(awk -F': ' '/System time/{print $2}' "$tf" 2>/dev/null); sys=${sys:-0}
  peak=$(cut -d' ' -f1 "$rf" 2>/dev/null); peak=${peak:-0}
  maxn=$(cut -d' ' -f2 "$rf" 2>/dev/null); maxn=${maxn:-0}
  cpu=$(awk -v u="$user" -v s="$sys" 'BEGIN{printf "%.1f", u+s}')
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$mode" "$p" "$total" "$rc" "$wall" "$user" "$sys" "$cpu" "$peak" "$maxn" >> "$SUMMARY"
  log "    $label  exit=$rc  wall=${wall}s  cpu_core_s=$cpu  peak_rss=$(awk -v k="$peak" 'BEGIN{printf "%.2f",k/1048576}')GB  max_concurrent_align=$maxn"
  if [ "$rc" -ne 0 ] || ! ls "$od"/*_bismark_bt2.bam >/dev/null 2>&1; then
    log "    !! $label produced no BAM (exit=$rc) — see $od/run.log"; tail -6 "$od/run.log" | sed 's/^/      /' | tee -a "$LOG"
  fi
  # reclaim scratch immediately (BAMs not needed for a perf sweep; keep logs)
  rm -f "$od"/*_bismark_bt2.bam 2>/dev/null
}

PS="2 4 8 12 16"
log "=== MODE 1: faithful directional (2 instances; total cores = 2*p) ==="
for p in $PS; do run_cell faithful_dir "$p" $((2*p)) --genome "$GENOME"; done
log "=== MODE 2: combined single-pass (b) (1 instance; total cores = p) ==="
for p in $PS; do run_cell comb1pass "$p" "$p" --genome "$GENOME" --combined_index --non_directional --combined_index_single_pass; done

log "=== SCALING SWEEP DONE ($TAG) — scaling_summary.tsv ==="
cat "$SUMMARY" | tee -a "$LOG"
