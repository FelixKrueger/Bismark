#!/bin/bash
# ===========================================================================
# master_remaining.sh — runs the rest of the scaling benchmark sequentially
# (launch AFTER perl_dir finishes, so directional timing has no self-contention):
#   STEP 1  comb_dir sweep            (directional 10M reads)            [Part A]
#   STEP 2  Sherman: simulate 10M non-directional 64bp reads
#   STEP 3  perl_nondir sweep         (Sherman non-dir reads)           [Part B]
#   STEP 4  faithful_nondir sweep     (Sherman non-dir reads)           [Part B]
#   STEP 5  comb1pass sweep           (Sherman non-dir reads)           [Part B]
# Each sub-sweep writes its own DURABLE/<tag>/scaling_summary.tsv.
# ===========================================================================
set -uo pipefail
BASE=/home/fkrueger/v2spike_out/bench_align_modes
DIRREADS=/home/fkrueger/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz
GENOME=/home/fkrueger/bismark_benchmarks/genome
SHERMAN=/home/fkrueger/Sherman/Sherman
SIMDIR=/var/tmp/sherman_nondir_10M
SIM="$SIMDIR/sherman_nondir_10M_64bp.fq.gz"
ML="$BASE/master_remaining.log"
log(){ echo "[$(date +%H:%M:%S)] MASTER: $*" | tee -a "$ML"; }
: > "$ML"

# (perl_dir sweep already complete before this master is launched — no wait needed.)
log "STEP 1: comb_dir sweep (directional 10M)"
bash "$BASE/run_scaling_rust.sh" "$DIRREADS" scaling_comb_dir comb_dir >> "$ML" 2>&1
log "STEP 1 done"

log "STEP 2: Sherman simulate 10M non-directional 64bp reads -> $SIMDIR"
mkdir -p "$SIMDIR"; cd "$SIMDIR"
"$SHERMAN" --genome_folder "$GENOME" -l 64 -n 10000000 --non_directional -o "$SIMDIR" >> "$ML" 2>&1
SIMFQ=$(ls "$SIMDIR"/simulated.fastq "$SIMDIR"/*.fastq 2>/dev/null | head -1)
if [ -z "$SIMFQ" ] || [ ! -s "$SIMFQ" ]; then log "FATAL: Sherman produced no fastq"; ls -la "$SIMDIR" | tee -a "$ML"; exit 1; fi
log "Sherman produced $SIMFQ ($(($(wc -l < "$SIMFQ")/4)) reads); gzipping -> $SIM"
gzip -c "$SIMFQ" > "$SIM"
rm -f "$SIMFQ"   # reclaim the uncompressed copy
log "STEP 2 done: $SIM ($(($(zcat "$SIM" | wc -l)/4)) reads)"

log "STEP 3: perl_nondir sweep (Sherman non-dir)"
bash "$BASE/run_scaling_perl.sh" "$SIM" scaling_perl_nondir perl_nondir >> "$ML" 2>&1
log "STEP 3 done"

log "STEP 4: faithful_nondir sweep (Sherman non-dir)"
bash "$BASE/run_scaling_rust.sh" "$SIM" scaling_faithful_nondir faithful_nondir >> "$ML" 2>&1
log "STEP 4 done"

log "STEP 5: comb1pass sweep (Sherman non-dir)"
bash "$BASE/run_scaling_rust.sh" "$SIM" scaling_comb1pass comb1pass >> "$ML" 2>&1
log "STEP 5 done"

log "=== MASTER REMAINING DONE ==="
