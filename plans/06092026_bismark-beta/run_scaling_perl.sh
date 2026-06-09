#!/bin/bash
# ===========================================================================
# run_scaling_perl.sh <reads> <tag> <mode>  — Perl Bismark 0.25.1 -p scaling sweep
# mode ∈ {perl_dir, perl_nondir}
#   perl_dir     --genome                    2 inst -> 2*p cores
#   perl_nondir  --genome --non_directional  4 inst -> 4*p cores
# Same process-tree RSS sampler. No concordance. -p {2,4,8,12,16}.
# ===========================================================================
set -uo pipefail
READS="${1:?usage: <reads> <tag> <mode>}"
TAG="${2:?usage: <reads> <tag> <mode>}"
MODE="${3:?usage: <reads> <tag> <mode>}"

ENV=/home/fkrueger/micromamba/envs/bismark-test/bin
export PATH="$ENV:$PATH"
PERL_BM="$ENV/bismark"
GENOME=/home/fkrueger/bismark_benchmarks/genome

OUTROOT="/var/tmp/bench_core_scaling/$TAG"
BINDIR="/home/fkrueger/v2spike_out/bench_align_modes"
DURABLE="$BINDIR/$TAG"
mkdir -p "$OUTROOT" "$DURABLE"
LOG="$DURABLE/scaling.log"; SUMMARY="$DURABLE/scaling_summary.tsv"
: > "$LOG"
printf 'mode\tp\ttotal_cores\texit\twall_s\tuser_s\tsys_s\tcpu_core_s\tpeak_rss_kb\tmax_concurrent_align\n' > "$SUMMARY"
log(){ echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }

case "$MODE" in
  perl_dir)    MARGS=(--genome "$GENOME"); MULT=2 ;;
  perl_nondir) MARGS=(--genome "$GENOME" --non_directional); MULT=4 ;;
  *) echo "unknown mode: $MODE"; exit 2 ;;
esac

log "READS=$READS TAG=$TAG MODE=$MODE (cores=${MULT}*p)"
log "PERL_BM=$PERL_BM ($("$PERL_BM" --version 2>&1 | grep -i version | head -1 | tr -s ' '))"

sampler(){
  local root="$1" out="$2" stop="$3" peak=0 maxn=0 res sum cnt
  while [ ! -e "$stop" ]; do
    res=$(ps --no-headers -eo pid=,ppid=,rss=,comm= 2>/dev/null | awk -v root="$root" '
      { p[NR]=$1; pp[NR]=$2; r[NR]=$3; c[NR]=$4; n=NR }
      END { intree[root]=1
        do { changed=0
          for (i=1;i<=n;i++) if (!seen[i] && (p[i]==root || intree[pp[i]])) { seen[i]=1; intree[p[i]]=1; changed=1 }
        } while (changed)
        sum=0; cnt=0
        for (i=1;i<=n;i++) if (seen[i]) { sum+=r[i]; if (c[i] ~ /bowtie2-align-[sl]/) cnt++ }
        print sum+0, cnt+0 }')
    sum=${res%% *}; cnt=${res##* }
    [ "${sum:-0}" -gt "$peak" ] && peak="$sum"
    [ "${cnt:-0}" -gt "$maxn" ] && maxn="$cnt"
    sleep 0.3
  done
  echo "$peak $maxn" > "$out"
}

run_cell(){
  local mode="$1" p="$2" total="$3"; shift 3
  local label="${mode}_p${p}" od="$OUTROOT/${mode}_p${p}"
  mkdir -p "$od"; local tf="$od/time.txt" rf="$od/rss.txt" stop="$od/stop"
  rm -f "$stop" "$od"/*_bismark_bt2.bam 2>/dev/null
  log ">>> $label (total_cores=$total): $* -p $p"
  local t0=$SECONDS
  /usr/bin/time -v -o "$tf" "$PERL_BM" "$@" -p "$p" --path_to_bowtie2 "$ENV" --samtools_path "$ENV" -o "$od" "$READS" > "$od/run.log" 2>&1 &
  local timepid=$!; sampler "$timepid" "$rf" "$stop" & local sp=$!
  wait "$timepid"; local rc=$?; local wall=$((SECONDS - t0))
  touch "$stop"; wait "$sp" 2>/dev/null
  local user sys peak maxn cpu
  user=$(awk -F': ' '/User time/{print $2}' "$tf" 2>/dev/null); user=${user:-0}
  sys=$(awk -F': ' '/System time/{print $2}' "$tf" 2>/dev/null); sys=${sys:-0}
  peak=$(cut -d' ' -f1 "$rf" 2>/dev/null); peak=${peak:-0}
  maxn=$(cut -d' ' -f2 "$rf" 2>/dev/null); maxn=${maxn:-0}
  cpu=$(awk -v u="$user" -v s="$sys" 'BEGIN{printf "%.1f", u+s}')
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$mode" "$p" "$total" "$rc" "$wall" "$user" "$sys" "$cpu" "$peak" "$maxn" >> "$SUMMARY"
  log "    $label exit=$rc wall=${wall}s cpu_core_s=$cpu peak_rss=$(awk -v k="$peak" 'BEGIN{printf "%.2f",k/1048576}')GB max_concurrent_align=$maxn"
  [ "$rc" -ne 0 ] && { log "    !! $label FAILED"; tail -6 "$od/run.log" | sed 's/^/      /' | tee -a "$LOG"; }
  rm -f "$od"/*_bismark_bt2.bam 2>/dev/null
}

for p in 2 4 8 12 16; do run_cell "$MODE" "$p" $((MULT*p)) "${MARGS[@]}"; done
log "=== DONE $MODE ($TAG) ==="
cat "$SUMMARY" | tee -a "$LOG"
