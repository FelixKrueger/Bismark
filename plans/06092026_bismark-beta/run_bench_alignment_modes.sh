#!/bin/bash
# ===========================================================================
# run_bench_alignment_modes.sh  —  Bismark alignment-mode benchmark harness
# (forked from v2spike_out/phase8gate + phase9gate run_with_rss scaffold).
#
# Per BENCHMARK_SPEC_alignment_modes.md: ONE apples-to-apples table over 11
# alignment modes on the SAME real read set + genome, fixed ~16-core budget.
# Captures per cell: wall(s) · CPU core-s (User+System) · peak PROCESS-TREE RSS
# (GB) · max concurrent bowtie2-align* · and writes BAMs for concordance.
#
# RSS sampler = TRUE process-tree walk rooted at the launched PID (NOT a comm
# name match): robust on a SHARED K8s node where a co-tenant could also run
# bowtie2. Matches BOTH bowtie2-align-s (.bt2 per-strand) AND -align-l (.bt2l
# combined) for the concurrent-index count. Do NOT use /usr/bin/time MaxRSS
# (it only sees the wrapper, not the index-holding bowtie2 children).
#
# Usage:  run_bench_alignment_modes.sh <READS.fq.gz> <TAG>
#   <READS> : the SE read file used for EVERY row (same file, comparable)
#   <TAG>   : output subdir under $OUTROOT (e.g. smoke200k | full10M)
# ===========================================================================
set -uo pipefail

READS="${1:?usage: run_bench_alignment_modes.sh <READS> <TAG>}"
TAG="${2:?usage: run_bench_alignment_modes.sh <READS> <TAG>}"

ENV=/home/fkrueger/micromamba/envs/bismark-test/bin
export PATH="$ENV:$PATH"                       # so Perl bismark finds bowtie2/samtools
PERL_BM="$ENV/bismark"                          # Perl Bismark v0.25.1 (gate oracle)
BR=/home/fkrueger/Bismark-bench/rust/target/release/bismark_rs   # shipped 0b6bb8b
ST="$ENV/samtools"
GENOME=/home/fkrueger/bismark_benchmarks/genome

OUTROOT="/var/tmp/bench_align_modes/$TAG"       # big BAMs: ephemeral scratch
BINDIR="/home/fkrueger/v2spike_out/bench_align_modes"        # harness + churn live here
DURABLE="$BINDIR/$TAG"                          # logs/summary per TAG: persist
CHURN="$BINDIR/bam_churn.py"
mkdir -p "$OUTROOT" "$DURABLE"
LOG="$DURABLE/bench.log"
SUMMARY="$DURABLE/summary.tsv"
: > "$LOG"
printf 'label\tengine\texit\twall_s\tuser_s\tsys_s\tcpu_core_s\tpeak_rss_kb\tmax_concurrent_align\n' > "$SUMMARY"

log(){ echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }

log "READS=$READS ($(($(zcat -f "$READS" 2>/dev/null | head -400000 | wc -l)/4))+ reads sampled-count; full count skipped)"
log "BR=$BR ($("$BR" --version 2>&1 | grep -i version | head -1 | tr -s ' '))"
log "PERL_BM=$PERL_BM ($("$PERL_BM" --version 2>&1 | grep -i version | head -1 | tr -s ' '))"
log "bowtie2=$("$ENV"/bowtie2-align-s --version 2>/dev/null | head -1)"
log "OUTROOT=$OUTROOT  DURABLE=$DURABLE"

# --- process-tree peak-RSS + max-concurrent-align sampler ------------------
# $1 root_pid  $2 outfile("peak_kb maxn")  $3 stopfile
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

# --- run one cell: time -v (CPU) + SECONDS (wall) + tree sampler (RSS) ------
# $1 label  $2 engine(perl|rust)  ; rest = full command WITHOUT trailing -o/READS
run_cell(){
  local label="$1" engine="$2"; shift 2
  local od="$OUTROOT/$label"; mkdir -p "$od"
  local tf="$od/time.txt" rf="$od/rss.txt" stop="$od/stop"
  rm -f "$stop" "$od"/*_bismark_bt2.bam 2>/dev/null
  log ">>> CELL $label ($engine): $* -o $od $READS"
  local t0=$SECONDS
  /usr/bin/time -v -o "$tf" "$@" --path_to_bowtie2 "$ENV" -o "$od" "$READS" > "$od/run.log" 2>&1 &
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
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$label" "$engine" "$rc" "$wall" "$user" "$sys" "$cpu" "$peak" "$maxn" >> "$SUMMARY"
  log "    $label  exit=$rc  wall=${wall}s  cpu_core_s=$cpu  peak_rss=$(awk -v k="$peak" 'BEGIN{printf "%.2f",k/1048576}')GB  max_concurrent_align=$maxn"
  if [ "$rc" -ne 0 ] || ! ls "$od"/*_bismark_bt2.bam >/dev/null 2>&1; then
    log "    !! $label produced no BAM (exit=$rc) — see $od/run.log"; tail -6 "$od/run.log" | sed 's/^/      /' | tee -a "$LOG"
  fi
}

# ===========================================================================
# THE 11 ROWS (fixed ~16-core budget: instances x -p = 16)
# ===========================================================================
# NOTE: pbat rows (perl_pbat / rust_pbat / comb_pbat) DROPPED — the dataset is
# DIRECTIONAL WGBS, so --pbat aligns near-nothing (0.4% mapped in smoke); pbat
# throughput + concordance are not benchmarkable on this read set.
# Perl 0.25.1 (oracle): 2 / 4 concurrent instances
run_cell perl_dir          perl "$PERL_BM" --genome "$GENOME" --samtools_path "$ENV" -p 8
run_cell perl_nondir       perl "$PERL_BM" --genome "$GENOME" --samtools_path "$ENV" --non_directional -p 4
# Rust faithful (expect byte-identical to matching Perl row)
run_cell rust_dir          rust "$BR" --genome "$GENOME" -p 8
run_cell rust_nondir       rust "$BR" --genome "$GENOME" --non_directional -p 4
# Rust combined-index (concordance-gated, NOT byte-identical except sequential)
run_cell comb_dir          rust "$BR" --genome "$GENOME" --combined_index -p 16
run_cell comb_nondir_parA  rust "$BR" --genome "$GENOME" --combined_index --non_directional -p 8
run_cell comb_nondir_seq   rust "$BR" --genome "$GENOME" --combined_index --non_directional --combined_index_sequential -p 16
run_cell comb_nondir_1pass rust "$BR" --genome "$GENOME" --combined_index --non_directional --combined_index_single_pass -p 16

log "=== ALL CELLS DONE — summary.tsv ==="
cat "$SUMMARY" | tee -a "$LOG"

# ===========================================================================
# CONCORDANCE
# ===========================================================================
bam(){ ls "$OUTROOT/$1"/*_bismark_bt2.bam 2>/dev/null | head -1; }
recmd5(){ "$ST" view "$1" 2>/dev/null | md5sum | cut -d' ' -f1; }   # decompressed records, header/@PG excluded
CONC="$DURABLE/concordance.tsv"
printf 'row\tmetric\tvalue\tdetail\n' > "$CONC"

log "=== concordance: faithful rows vs Perl (expect byte-identical) ==="
for pair in "rust_dir:perl_dir:4" "rust_nondir:perl_nondir:5"; do
  r=${pair%%:*}; rest=${pair#*:}; p=${rest%%:*}; row=${rest##*:}
  rb=$(bam "$r"); pb=$(bam "$p")
  if [ -n "$rb" ] && [ -n "$pb" ]; then
    m1=$(recmd5 "$rb"); m2=$(recmd5 "$pb")
    if [ "$m1" = "$m2" ]; then v="byte-identical"; else v="DIFFER"; fi
    printf '%s\t%s\t%s\t%s\n' "row$row($r)" "md5_vs_$p" "$v" "$m1 vs $m2" >> "$CONC"
    log "    row$row $r vs $p: $v ($m1 / $m2)"
  else
    printf '%s\t%s\t%s\t%s\n' "row$row($r)" "md5_vs_$p" "MISSING_BAM" "$rb|$pb" >> "$CONC"
    log "    row$row $r vs $p: MISSING BAM"
  fi
done

log "=== concordance: sequential vs parallel-a (expect byte-identical) ==="
sb=$(bam comb_nondir_seq); ab=$(bam comb_nondir_parA)
if [ -n "$sb" ] && [ -n "$ab" ]; then
  ms=$(recmd5 "$sb"); ma=$(recmd5 "$ab")
  if [ "$ms" = "$ma" ]; then v="byte-identical"; else v="DIFFER"; fi
  printf 'row9(comb_nondir_seq)\tmd5_vs_parA\t%s\t%s\n' "$v" "$ms vs $ma" >> "$CONC"
  log "    row9 seq vs parA: $v ($ms / $ma)"
fi

log "=== concordance: combined churn vs matching faithful (final-BAM, oracle-unique-stays-unique) ==="
# $1 oracle-bam-label  $2 combined-bam-label  $3 rownum  $4 oracle-name
churn(){
  local ol="$1" cl="$2" row="$3" oname="$4"
  local ob cb; ob=$(bam "$ol"); cb=$(bam "$cl")
  if [ -z "$ob" ] || [ -z "$cb" ]; then
    printf 'row%s(%s)\tchurn_vs_%s\tMISSING_BAM\t%s|%s\n' "$row" "$cl" "$oname" "$ob" "$cb" >> "$CONC"; return
  fi
  local res
  res=$(python3 "$CHURN" <("$ST" view "$ob") <("$ST" view "$cb") 2>>"$LOG")
  printf 'row%s(%s)\tchurn_vs_%s\t%s\n' "$row" "$cl" "$oname" "$res" >> "$CONC"
  log "    row$row $cl vs $ol: $res"
}
churn rust_dir          comb_dir          7  rust_dir
churn rust_nondir       comb_nondir_parA  8  rust_nondir
churn comb_nondir_parA  comb_nondir_1pass 10 comb_nondir_parA

log "=== concordance.tsv ==="
cat "$CONC" | tee -a "$LOG"
log "=== BENCHMARK COMPLETE ($TAG) ==="
