#!/usr/bin/env bash
#
# oxy_idle_gate.sh — block until oxy is idle enough to run a clean benchmark.
# Part of plans/05302026_extractor-fulldata-validation/PLAN.md.
#
# oxy is shared with sibling sessions (coverage2cytosine, alignments, dedup,
# methylation_consistency). A contended box ruins timing AND interferes with
# their work. This gate waits until no sibling heavy job is running and the
# 1-min load is below a threshold, then returns 0. Times out with exit 1.
#
# It deliberately does NOT match the methylation extractor (Perl or Rust) — that
# is OUR campaign's own workload, started only after this gate passes.
#
# Usage: oxy_idle_gate.sh [--timeout S] [--poll S] [--max-load N]
#   defaults: --timeout 21600 (6h), --poll 120, --max-load 16 (of 128 logical CPUs)

set -euo pipefail

TIMEOUT=21600 POLL=120 MAX_LOAD=16
while [[ $# -gt 0 ]]; do
  case $1 in
    --timeout) TIMEOUT="$2"; shift 2 ;;
    --poll) POLL="$2"; shift 2 ;;
    --max-load) MAX_LOAD="$2"; shift 2 ;;
    *) echo "unexpected arg: $1" >&2; exit 2 ;;
  esac
done

# Sibling heavy-job signatures (NOT the extractor — that is ours).
PATTERN='bowtie2-align|coverage2cytosine|bismark2bedGraph|deduplicate_bismark|methylation_consistency|bismark_genome_preparation'

elapsed=0
while :; do
  busy=$(ps -eo pid,args 2>/dev/null \
           | grep -E "$PATTERN" \
           | grep -vE "grep|oxy_idle_gate|overnight_driver|byteid_run|bench_run" || true)
  load1=$(awk '{print $1}' /proc/loadavg)
  load_ok=$(awk -v l="$load1" -v m="$MAX_LOAD" 'BEGIN{print (l<m)?1:0}')

  if [[ -z "$busy" && "$load_ok" -eq 1 ]]; then
    echo "$(date -u +%H:%M:%SZ) oxy idle (load1=$load1, no sibling heavy job) — proceeding" >&2
    exit 0
  fi

  if [[ "$elapsed" -ge "$TIMEOUT" ]]; then
    echo "$(date -u +%H:%M:%SZ) idle-gate TIMEOUT after ${TIMEOUT}s (load1=$load1)" >&2
    [[ -n "$busy" ]] && { echo "still-busy:" >&2; echo "$busy" | head -5 >&2; }
    exit 1
  fi

  reason=""
  [[ -n "$busy" ]] && reason="sibling job(s): $(echo "$busy" | wc -l)"
  [[ "$load_ok" -ne 1 ]] && reason="$reason load1=$load1>=$MAX_LOAD"
  echo "$(date -u +%H:%M:%SZ) waiting (${reason}); next poll in ${POLL}s" >&2
  sleep "$POLL"
  elapsed=$((elapsed + POLL))
done
