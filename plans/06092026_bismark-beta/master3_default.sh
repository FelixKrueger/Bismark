#!/bin/bash
# ===========================================================================
# master3_default.sh — single-threaded DEFAULT (no -p) baseline cell for each
# of the 6 modes, to anchor the left end of the scaling graphs at the real
# default config (1 thread per Bowtie 2 instance). p=1 in the harness => omit
# -p (Bowtie 2 single-thread); total cores = instances x 1.
#   directional (10M dir reads):  faithful_dir(2c)  perl_dir(2c)  comb_dir(1c)
#   non-dir (10M Sherman reads):  faithful_nondir(4c)  perl_nondir(4c)  comb1pass(1c)
# Sequential, <=4 cores each. comb1pass single-thread (20M tagged) is the long pole (~1.5h) -> last.
# ===========================================================================
set -uo pipefail
BASE=/home/fkrueger/v2spike_out/bench_align_modes
DIRREADS=/home/fkrueger/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
SHERMAN_READS=/var/tmp/sherman_nondir_10M/sherman_nondir_10M_64bp.fq.gz
ML="$BASE/master3_default.log"
log(){ echo "[$(date +%H:%M:%S)] MASTER3: $*" | tee -a "$ML"; }
: > "$ML"
[ -s "$SHERMAN_READS" ] || { log "FATAL: Sherman reads missing"; exit 1; }

log "STEP 1: faithful_dir default (2 cores)"
PS_LIST="1" bash "$BASE/run_scaling_rust.sh" "$DIRREADS" scaling_faithful_dir_p1 faithful_dir >> "$ML" 2>&1
log "STEP 2: perl_dir default (2 cores)"
PS_LIST="1" bash "$BASE/run_scaling_perl.sh" "$DIRREADS" scaling_perl_dir_p1 perl_dir >> "$ML" 2>&1
log "STEP 3: comb_dir default (1 core)"
PS_LIST="1" bash "$BASE/run_scaling_rust.sh" "$DIRREADS" scaling_comb_dir_p1 comb_dir >> "$ML" 2>&1
log "STEP 4: faithful_nondir default (4 cores)"
PS_LIST="1" bash "$BASE/run_scaling_rust.sh" "$SHERMAN_READS" scaling_faithful_nondir_p1 faithful_nondir >> "$ML" 2>&1
log "STEP 5: perl_nondir default (4 cores)"
PS_LIST="1" bash "$BASE/run_scaling_perl.sh" "$SHERMAN_READS" scaling_perl_nondir_p1 perl_nondir >> "$ML" 2>&1
log "STEP 6: comb1pass default (1 core) — long pole"
PS_LIST="1" bash "$BASE/run_scaling_rust.sh" "$SHERMAN_READS" scaling_comb1pass_p1 comb1pass >> "$ML" 2>&1

log "=== MASTER3 DEFAULT DONE ==="
