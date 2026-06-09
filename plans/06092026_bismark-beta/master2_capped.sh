#!/bin/bash
# ===========================================================================
# master2_capped.sh — runs ONLY the missing cells for the 32-CPU-capped,
# TOTAL-CORES-axis scaling benchmark (sequential, no overcommit, ≤32 cores):
#   STEP A  comb_dir  -p32  (directional 10M)  -> gives comb_dir its 32-core point  [tag scaling_comb_dir_p32]
#   STEP B  faithful_nondir -p{2,4,8}  (Sherman non-dir) -> 8/16/32 cores           [tag scaling_faithful_nondir]
#   STEP C  comb1pass -p{8,16,32}      (Sherman non-dir) -> 8/16/32 cores           [tag scaling_comb1pass]
# Already have (valid, reused): faithful_dir/perl_dir/comb_dir p{2..16}, perl_nondir p{2,4,8}.
# ===========================================================================
set -uo pipefail
BASE=/home/fkrueger/v2spike_out/bench_align_modes
DIRREADS=/home/fkrueger/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
SHERMAN_READS=/var/tmp/sherman_nondir_10M/sherman_nondir_10M_64bp.fq.gz
ML="$BASE/master2_capped.log"
log(){ echo "[$(date +%H:%M:%S)] MASTER2: $*" | tee -a "$ML"; }
: > "$ML"

if [ ! -s "$SHERMAN_READS" ]; then log "FATAL: Sherman reads missing at $SHERMAN_READS"; exit 1; fi
log "Sherman reads: $(($(zcat "$SHERMAN_READS" | wc -l)/4)) reads"

log "STEP A: comb_dir -p32 (directional, 32 cores)"
PS_LIST="32" bash "$BASE/run_scaling_rust.sh" "$DIRREADS" scaling_comb_dir_p32 comb_dir >> "$ML" 2>&1
log "STEP A done"

log "STEP B: faithful_nondir -p{2,4,8} (Sherman, 8/16/32 cores)"
PS_LIST="2 4 8" bash "$BASE/run_scaling_rust.sh" "$SHERMAN_READS" scaling_faithful_nondir faithful_nondir >> "$ML" 2>&1
log "STEP B done"

log "STEP C: comb1pass -p{8,16,32} (Sherman, 8/16/32 cores)"
PS_LIST="8 16 32" bash "$BASE/run_scaling_rust.sh" "$SHERMAN_READS" scaling_comb1pass comb1pass >> "$ML" 2>&1
log "STEP C done"

log "=== MASTER2 CAPPED DONE ==="
