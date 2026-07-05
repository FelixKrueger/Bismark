#!/usr/bin/env bash
#
# p4_matrix.sh — Phase 4 (extractor inline epic) downstream byte-identity matrix.
#
# Drives scripts/phase_h_smoke.sh over the downstream bedGraph/coverage2cytosine
# cells for the 10M + full_size datasets on oxy, comparing the Rust extractor's
# in-process `--bedGraph`/`--cytosine_report` output to Perl v0.25.1 (ORDERED
# decompressed compare — closes "Phase H sub-gate 2"). Logs one PASS/FAIL line
# per cell to the aggregate so a fresh session can read the verdict at a glance.
#
# Run detached: tmux new -d -s p4 'scripts/p4_matrix.sh'
# Watch:        tail -f /var/tmp/p4_matrix_aggregate.txt
# Per-cell:     /var/tmp/p4_<lib>_<cell>/diff_summary.txt  (+ .log)
#
# Env: PAR (parallel/multicore, default 16). The micromamba `bismark-test` env
# (samtools+perl) + ~/.cargo/env must be active in the launching shell.
set -uo pipefail   # NOT -e: a FAIL cell must log and the matrix must continue.

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$REPO/rust/target/release/bismark-methylation-extractor-rs"

# Self-activate the oxy toolchain so the tmux launch needs no env setup:
# micromamba `bismark-test` (samtools + perl) + the Rust toolchain.
if [ -x "$HOME/bin/micromamba" ]; then
  export MAMBA_ROOT_PREFIX="$HOME/micromamba"
  eval "$("$HOME/bin/micromamba" shell hook -s bash)" 2>/dev/null || true
  micromamba activate bismark-test 2>/dev/null || true
fi
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env" 2>/dev/null || true

# Build the release binary (incremental; ~0s if already current at this commit).
echo "==> building bismark-methylation-extractor-rs (release)..." >&2
( cd "$REPO/rust" && cargo build --release -p bismark-extractor ) > /var/tmp/p4_build.log 2>&1 \
  || { echo "FATAL: cargo build failed — see /var/tmp/p4_build.log" >&2; exit 1; }
G="$HOME/bismark_benchmarks/genome"
SE_FULL="$HOME/bismark_benchmarks/full_size/SRR24827373_Homo_sapiens_Bisulfite-Seq_SE_trimmed_full_size_bismark_bt2.bam"
PE_FULL="$HOME/bismark_benchmarks/full_size/SRR24827373_GSM7445361_32F_NB3_p28_p2n2p_p10_rep1_Homo_sapiens_Bisulfite-Seq_R1_val_1_bismark_bt2_pe.deduplicated.bam"
PE_10M="$HOME/bismark_benchmarks/10M_PE/SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam"
PAR="${PAR:-16}"
AGG=/var/tmp/p4_matrix_aggregate.txt

: > "$AGG"
echo "Phase 4 matrix START $(date -u +%FT%TZ)  par=$PAR" | tee -a "$AGG"
echo "bin=$BIN ($(cd "$REPO" && git rev-parse --short HEAD))" | tee -a "$AGG"
[ -x "$BIN" ] || { echo "FATAL: rust binary missing — build it first" | tee -a "$AGG"; exit 1; }

run_cell () {  # lib bam cell
  local lib="$1" bam="$2" cell="$3" o="/var/tmp/p4_${1}_${3}"
  if [ ! -f "$bam" ]; then echo "$(date -u +%T)  $lib $cell  ->  SKIP (no BAM)" | tee -a "$AGG"; return; fi
  rm -rf "$o"
  RUST_BIN="$BIN" "$REPO/scripts/phase_h_smoke.sh" "$bam" --mode "$cell" \
      --genome "$G" --parallel "$PAR" --out "$o" > "$o.log" 2>&1
  local res spd
  res="$(grep -E '^(PASS|FAIL)' "$o/diff_summary.txt" 2>/dev/null | head -1 || echo 'NORESULT (see log)')"
  spd="$(grep -E '^Speedup' "$o/diff_summary.txt" 2>/dev/null | head -1 || true)"
  echo "$(date -u +%T)  $lib $cell  ->  $res    $spd" | tee -a "$AGG"
}

CELLS="bg cr cr_cx cr_split cutoff2 zero ucsc"

# Quick 10M_PE sanity first (fast feedback before the slow full_size cells).
for c in bg cr; do run_cell 10M_PE "$PE_10M" "$c"; done
# Full-size at-scale gate.
for c in $CELLS; do run_cell SE_full "$SE_FULL" "$c"; done
for c in $CELLS; do run_cell PE_full "$PE_FULL" "$c"; done

echo "Phase 4 matrix ALL_DONE $(date -u +%FT%TZ)" | tee -a "$AGG"
