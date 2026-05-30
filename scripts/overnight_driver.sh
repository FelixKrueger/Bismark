#!/usr/bin/env bash
#
# overnight_driver.sh — unattended full-dataset benchmark + byte-identity campaign.
# Part of plans/05302026_extractor-fulldata-validation/PLAN.md.
#
# Sequence:
#   STAGE   — copy the 3 S3-symlinked BAMs to local disk; dedup the WGBS-SE for
#             parity with WGBS-PE (RRBS stays non-dedup'd per RRBS convention).
#   GATE    — wait until oxy is idle (oxy_idle_gate.sh).
#   PHASE 1 — byte-identity (parity) per dataset (byteid_run.sh). STOP on genuine FAIL.
#             The Perl --multicore 1 run here is reused as the serial perf anchor.
#   PHASE 2 — Rust perf sweep + Perl --multicore 12 anchor (bench_run.sh), in priority
#             order so a short night still banks the headline. Resumable (skip-completed).
#   REPORT  — write FINDINGS.md (median tables + footprint table; analysis in Phase 3).
#
# Designed to run under nohup/tmux. CSV-append + skip-completed ⇒ safe to re-run.
#
# Usage: overnight_driver.sh [--out DIR] [--skip-gate]
set -uo pipefail   # NOTE: not -e — one failed config must not abort the whole night.

OUT_DIR="$HOME/fulldata_bench"
SKIP_GATE=0
while [[ $# -gt 0 ]]; do
  case $1 in
    --out) OUT_DIR="$2"; shift 2 ;;
    --skip-gate) SKIP_GATE=1; shift ;;
    *) echo "unexpected arg: $1" >&2; exit 2 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STAGE="$OUT_DIR/staged"; CSV="$OUT_DIR/results.csv"; LOG="$OUT_DIR/driver.log"
mkdir -p "$STAGE" "$OUT_DIR/byteid" "$OUT_DIR/perf"
export RUST_BIN="${RUST_BIN:-$HOME/Github/Bismark/rust/target/release/bismark-methylation-extractor-rs}"
export PERL_BIN="${PERL_BIN:-$HOME/micromamba/envs/bismark-test/bin/bismark_methylation_extractor}"
DEDUP_BIN="${DEDUP_BIN:-$HOME/micromamba/envs/bismark-test/bin/deduplicate_bismark}"

log(){ echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $*" | tee -a "$LOG"; }

# ── Dataset source BAMs (S3 symlinks). label : src : layout : dedup ──────
FS="$HOME/bismark_benchmarks/full_size"
RR="$HOME/bismark_benchmarks/RRBS_PE"
WGBS_PE_SRC="$FS/SRR24827373_GSM7445361_32F_NB3_p28_p2n2p_p10_rep1_Homo_sapiens_Bisulfite-Seq_R1_val_1_bismark_bt2_pe.deduplicated.bam"
WGBS_SE_SRC="$FS/SRR24827373_Homo_sapiens_Bisulfite-Seq_SE_trimmed_full_size_bismark_bt2.bam"
RRBS_PE_SRC="$RR/SRR24766921_GSM7433369_Colon_3_Months_Rep1_1_val_1_bismark_bt2_pe.bam"

# stage_local <src> <dest_basename> [dedup:se|none] → echoes the staged local path
stage_local(){
  local src="$1" name="$2" dedup="${3:-none}" dst="$STAGE/$2"
  if [[ ! -e "$src" ]]; then log "MISSING source: $src"; return 1; fi
  if [[ ! -f "$dst" ]]; then log "staging $name (cp -L from S3)…"; cp -L "$src" "$dst" || { log "stage FAILED: $name"; return 1; }; fi
  samtools view "$dst" 2>/dev/null | head -1 >/dev/null || true   # warm-up read
  if [[ "$dedup" == "se" ]]; then
    local dd="$STAGE/${name%.bam}.deduplicated.bam"
    if [[ ! -f "$dd" ]]; then
      log "deduplicating SE $name for PE-parity…"
      "$DEDUP_BIN" -s --output_dir "$STAGE" "$dst" >>"$LOG" 2>&1 || { log "SE dedup FAILED"; return 1; }
    fi
    # deduplicate_bismark names it <base>.deduplicated.bam
    dst="$(ls "$STAGE"/*deduplicated.bam 2>/dev/null | grep -i "${name%.bam}" | head -1 || echo "$dd")"
  fi
  echo "$dst"
}

# ── have_config: skip if CSV already has >=reps rows for tool/dataset/mode/parallel ─
have_config(){ local t="$1" d="$2" m="$3" p="$4" r="$5"
  [[ -f "$CSV" ]] || return 1
  local c; c=$(awk -F, -v t="$t" -v d="$d" -v m="$m" -v p="$p" \
       '$1==t&&$2==d&&$3==m&&$4==p{n++} END{print n+0}' "$CSV")
  [[ "$c" -ge "$r" ]]
}

# ── STAGE ────────────────────────────────────────────────────────────────
log "=== STAGE ==="
command -v samtools >/dev/null || export PATH="$HOME/micromamba/envs/bismark-test/bin:$PATH"
WGBS_PE=$(stage_local "$WGBS_PE_SRC" "wgbs_pe.deduplicated.bam" none) || exit 1
WGBS_SE=$(stage_local "$WGBS_SE_SRC" "wgbs_se.bam" se) || exit 1
RRBS_PE=$(stage_local "$RRBS_PE_SRC" "rrbs_pe.bam" none) || exit 1
log "staged: PE=$WGBS_PE SE=$WGBS_SE RRBS=$RRBS_PE"
for b in "$WGBS_PE" "$WGBS_SE" "$RRBS_PE"; do
  log "  $(basename "$b"): $(samtools view -c "$b" 2>/dev/null || echo '?') reads"
done

# ── GATE ───────────────────────────────────────────────────────────────────
if [[ "$SKIP_GATE" -eq 0 ]]; then
  log "=== GATE (waiting for oxy idle) ==="
  "$SCRIPT_DIR/oxy_idle_gate.sh" || { log "idle-gate timed out — aborting"; exit 1; }
fi

# ── PHASE 1: byte-identity (HARD GATE) ──────────────────────────────────────
log "=== PHASE 1: byte-identity ==="
declare -A DS_BAM=( [wgbs_pe]="$WGBS_PE" [wgbs_se]="$WGBS_SE" [rrbs_pe]="$RRBS_PE" )
declare -A DS_MODES=( [wgbs_pe]="gzip plain" [wgbs_se]="gzip" [rrbs_pe]="gzip" )
for ds in wgbs_pe wgbs_se rrbs_pe; do
  if ! "$SCRIPT_DIR/byteid_run.sh" "${DS_BAM[$ds]}" --dataset "$ds" --out "$OUT_DIR/byteid" \
        --modes "${DS_MODES[$ds]}" --sweep "1 2 4 8 16"; then
    log "BYTEID FAIL for $ds — HARD GATE: stopping campaign. Triage $OUT_DIR/byteid/byteid_${ds}.status"
    exit 1
  fi
  # Reuse the Phase-1 Perl --multicore 1 wall as the serial perf anchor.
  ps=$(grep -E '^Perl: [0-9]+s$' "$OUT_DIR/byteid/parity_${ds}_gzip/diff_summary.txt" 2>/dev/null | grep -oE '[0-9]+' | head -1 || true)
  [[ -n "$ps" ]] && echo "perl,$ds,gzip,1,1,$ps,NA,NA,NA,NA,0" >> "$CSV" && log "Perl serial anchor $ds: ${ps}s (reused from byteid)"
done
log "PHASE 1 PASS — all datasets parity + worker-invariant"

# ── PHASE 2: perf (priority order; resumable) ───────────────────────────────
log "=== PHASE 2: perf ==="
run_cfg(){ local tool="$1" ds="$2" bam="$3" mode="$4" par="$5" reps="$6"
  if have_config "$tool" "$ds" "$mode" "$par" "$reps"; then log "skip (done): $tool $ds $mode p$par"; return; fi
  log "run: $tool $ds $mode p$par x$reps"
  "$SCRIPT_DIR/bench_run.sh" "$bam" --tool "$tool" --mode "$mode" --parallel "$par" \
      --reps "$reps" --dataset "$ds" --out "$OUT_DIR/perf" --csv "$CSV" || log "  (config had failures — recorded)"
}

# (i) WGBS-PE Rust sweep (primary headline)
for m in gzip plain mbias_only; do for p in 1 2 4 8 16; do run_cfg rust wgbs_pe "$WGBS_PE" "$m" "$p" 3; done; done
# (ii) WGBS-SE + RRBS-PE Rust
for ds_bam in "wgbs_se:$WGBS_SE" "rrbs_pe:$RRBS_PE"; do
  ds="${ds_bam%%:*}"; bam="${ds_bam#*:}"
  for m in gzip mbias_only; do for p in 1 4 16; do run_cfg rust "$ds" "$bam" "$m" "$p" 2; done; done
done
# (iii) Perl --multicore 12 anchor (gzip = realistic), 1 rep, all datasets
run_cfg perl wgbs_pe "$WGBS_PE" gzip 12 1
run_cfg perl wgbs_se "$WGBS_SE" gzip 12 1
run_cfg perl rrbs_pe "$RRBS_PE" gzip 12 1

# ── REPORT ──────────────────────────────────────────────────────────────────
log "=== REPORT ==="
FINDINGS="$OUT_DIR/FINDINGS.md"
{
  echo "# Full-dataset benchmark — raw results"
  echo ""
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ). CSV: \`$CSV\`."
  echo ""
  echo "## Wall (s) by tool/dataset/mode/parallel (all reps; medians computed in Phase 3)"
  echo '```'
  awk -F, 'NR>1{k=$1" "$2" "$3" p"$4; w[k]=w[k]" "$6}
    END{for(k in w) printf "%-34s reps:%s\n",k,w[k]}' "$CSV" | sort
  echo '```'
  echo ""
  echo "## Resource footprint (peak threads / peak fds / cores / max RSS KB) — from rep1 rows"
  echo '```'
  awk -F, 'NR>1&&$5==1{printf "%-28s p%-3s %-10s cores=%-5s rss=%-9s threads=%-4s fds=%-4s exit=%s\n",$1" "$2,$4,$3,$7,$8,$9,$10,$11}' "$CSV" | sort
  echo '```'
  echo ""
  echo "_Note: cores=(user+sys)/real is deflated in gzip mode by write-back — treat gzip-mode cores as a floor; the ~3-core headline is from mbias_only/plain. Default-gzip fds should confirm ~12 open .gz files._"
} > "$FINDINGS"
log "wrote $FINDINGS"
log "=== DONE ==="
