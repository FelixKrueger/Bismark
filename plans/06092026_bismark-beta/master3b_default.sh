#!/bin/bash
# ===========================================================================
# master3b_default.sh — RESUME the single-threaded default baseline after the
# Perl-harness PS_LIST fix. faithful_dir_p1 already done (kept). Runs the 5
# remaining p=1 (no -p, single-threaded) cells, sequentially, <=4 cores each:
#   perl_dir(2c, FIRST: validates the PS_LIST fix)  comb_dir(1c)
#   faithful_nondir(4c)  perl_nondir(4c)  comb1pass(1c, long pole ~1.5h, LAST)
# ===========================================================================
set -uo pipefail
BASE=/home/fkrueger/v2spike_out/bench_align_modes
DIRREADS=/home/fkrueger/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
SHERMAN_READS=/var/tmp/sherman_nondir_10M/sherman_nondir_10M_64bp.fq.gz
ML="$BASE/master3b_default.log"
log(){ echo "[$(date +%H:%M:%S)] MASTER3b: $*" | tee -a "$ML"; }
: > "$ML"
[ -s "$SHERMAN_READS" ] || { log "FATAL: Sherman reads missing"; exit 1; }

log "STEP 1: perl_dir default (2 cores) — validates Perl PS_LIST fix (expect ONE p1 row)"
PS_LIST="1" bash "$BASE/run_scaling_perl.sh" "$DIRREADS" scaling_perl_dir_p1 perl_dir >> "$ML" 2>&1
log "STEP 2: comb_dir default (1 core)"
PS_LIST="1" bash "$BASE/run_scaling_rust.sh" "$DIRREADS" scaling_comb_dir_p1 comb_dir >> "$ML" 2>&1
log "STEP 3: faithful_nondir default (4 cores)"
PS_LIST="1" bash "$BASE/run_scaling_rust.sh" "$SHERMAN_READS" scaling_faithful_nondir_p1 faithful_nondir >> "$ML" 2>&1
log "STEP 4: perl_nondir default (4 cores)"
PS_LIST="1" bash "$BASE/run_scaling_perl.sh" "$SHERMAN_READS" scaling_perl_nondir_p1 perl_nondir >> "$ML" 2>&1
log "STEP 5: comb1pass default (1 core) — long pole"
PS_LIST="1" bash "$BASE/run_scaling_rust.sh" "$SHERMAN_READS" scaling_comb1pass_p1 comb1pass >> "$ML" 2>&1

log "=== MASTER3b DEFAULT DONE ==="
