#!/usr/bin/env bash
#
# phase_h_pe_matrix.sh — Phase H sub-gate 1 PE byte-identity + speedup matrix.
#
# Runs the per-cell Phase H smoke (scripts/phase_h_smoke.sh) over the 5-cell
# representative PE matrix at --parallel ∈ {1, 4} (configurable). Asserts
# SPEC §8.3 row 4 N-invariance per CELL_ID (Rust-N=1 ≡ Rust-N=4 raw-byte).
# Emits a markdown speedup table with Perl-only and Rust scaling columns +
# input BAM MD5 + properly-paired fraction headers.
#
# Closes #872 (under epic #798). Companion: #871 (SE matrix; merged at #873).
#
# Usage:
#   scripts/phase_h_pe_matrix.sh <BAM> [--out DIR] [--parallel-set "1 4"]
#
# Pre-flight checks (PHASE_H_PE_PLAN.md rev 1 §3.3.2):
#   - bash ≥ 4.0
#   - BAM exists + readable
#   - --out DIR is empty or doesn't exist
#   - PE-ness assertion via samtools direct @PG regex (mirrors phase_h_smoke.sh:159)
#   - Overlap fraction ≥ 80% (samtools view -c -f 0x2 / total reads) — rev 1 A-I1
#   - Perl bismark_methylation_extractor version == v0.25.1
#   - Rust binary discoverable (built on-demand by phase_h_smoke.sh)
#   - nproc check + contention advisory (graceful skip if nproc unavailable)
#   - tmux/screen warning
#
# 5-cell PE matrix (rev 1 §3.1):
#   D         (no flags)                                  — Default; M-bias = 11,443 B regression guard at (D, N=1) (Phase C.1)
#   r1_5p     (--ignore 5)                                — R1 5' trim isolated
#   r2_5p     (--ignore_r2 5)                             — R2 5' trim isolated (PE-specific axis SE doesn't have)
#   r1r2_3p   (--ignore_3prime 5 --ignore_3prime_r2 5)   — Both 3' trims combined
#   overlap   (--include_overlap)                         — Overrides default --no_overlap; Phase C.1 polarity-direction guard
#
# Mixed-metric differential at N=1 (rev 1 A-O3):
#   - r1_5p / r2_5p / r1r2_3p: M-bias data row count < D by ≥1 row (SE-symmetric).
#   - overlap: M-bias data COUNT-SUM (methylated + unmethylated) strictly > D.
#     Rationale: --include_overlap accumulates counts at existing M-bias positions
#     (positions are read-relative) rather than adding new rows. Row count is
#     unchanged; count-sum strictly increases (magnitude is per-library, so the
#     invariant is monotonic > D, not a fixed % floor — rev 2).
#
# Exit codes:
#   0  — all cells PASS byte-identity + cross-N PASS + differential PASS + Rust scaling ≥ SPEC §9.7's 4×
#   1  — any cell FAIL or cross-N FAIL or (D,N=1) M-bias 11,443 B drift or differential FAIL or missing M-bias file
#   2  — pre-flight USAGE-ERROR
#   3  — byte-identity PASSED but Rust scaling missed the perf target (informational; v1.0 may ship at exit 3)
#
# Outputs:
#   <OUT>/cell_p<N>_<CELL_ID>/   — per-cell phase_h_smoke output (CELL_ID ∈ D|r1_5p|r2_5p|r1r2_3p|overlap)
#   <OUT>/cross_n_summary.txt    — cross-N comparison results per CELL_ID
#   <OUT>/speedup_table.md       — markdown summary with Rust commit + crate version + BAM MD5 + overlap fraction
#   <OUT>/matrix_verdict.txt     — PASS/FAIL with per-cell breakdown + inline differential evidence (no separate row_count_diff.txt per rev 1 B-Imp-2)
#
# Recommended: run inside `tmux` / `screen` (1.5-4 h matrix runtime per rev 1 A-O2).

set -euo pipefail

# Rev 1 (mirror SE rev 3 A-Er1): hard-fail on bash < 4.0. macOS ships
# /bin/bash 3.2 by default; this driver uses associative arrays (declare -A)
# and the modern empty-array-under-set-u idiom.
if (( ${BASH_VERSINFO[0]} < 4 )); then
  echo "error: bash >= 4.0 required (current: $BASH_VERSION)" >&2
  echo "       macOS default /bin/bash is 3.2; install via 'brew install bash'" >&2
  echo "       and re-run with /opt/homebrew/bin/bash scripts/phase_h_pe_matrix.sh ..." >&2
  echo "       Colossal Linux ships bash 5.x; the matrix driver targets that." >&2
  exit 2
fi

# Rev 1 (mirror SE rev 3 B-Med): SIGINT/SIGTERM trap preserves partial state.
trap 'echo "" >&2; echo "interrupted; partial matrix output in $OUT_DIR (preserved for evidence)" >&2; exit 130' INT TERM

# ─── Args ─────────────────────────────────────────────────────────────

BAM=""
OUT_DIR="./phase_h_pe_matrix_out"
PARALLEL_SET="1 4"

while [[ $# -gt 0 ]]; do
  case $1 in
    --out)
      OUT_DIR="$2"; shift 2 ;;
    --parallel-set)
      PARALLEL_SET="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,/^$/p' "$0"; exit 0 ;;
    *)
      if [[ -z "$BAM" ]]; then BAM="$1"; shift; else
        echo "error: unexpected arg: $1" >&2; exit 2
      fi
      ;;
  esac
done

if [[ -z "$BAM" ]]; then
  echo "usage: $0 <BAM> [--out DIR] [--parallel-set \"1 4\"]" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SMOKE_SCRIPT="$REPO_ROOT/scripts/phase_h_smoke.sh"

# ─── Pre-flight checks (rev 1 §3.3.2) ─────────────────────────────────

# 1. BAM exists + readable
if [[ ! -r "$BAM" ]]; then
  echo "error: BAM not readable: $BAM" >&2
  exit 2
fi
BAM=$(cd "$(dirname "$BAM")" && pwd)/$(basename "$BAM")  # canonicalize

# 2. --out empty-or-doesn't-exist (matrix-level rejection; rev 1 inherits SE rev 1 I12)
if [[ -e "$OUT_DIR" ]]; then
  if [[ -d "$OUT_DIR" ]]; then
    if [[ -n "$(ls -A "$OUT_DIR" 2>/dev/null)" ]]; then
      echo "error: --out dir is not empty: $OUT_DIR" >&2
      echo "       Pass --out to a fresh dir to prevent clobbering previous evidence." >&2
      exit 2
    fi
  else
    echo "error: --out path exists and is not a directory: $OUT_DIR" >&2
    exit 2
  fi
fi
mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd)  # canonicalize

# 3. samtools available — required for PE-ness + overlap-fraction pre-flight (rev 1 A-C1 + A-I1)
if ! command -v samtools >/dev/null 2>&1; then
  echo "error: samtools not on PATH; required for PE-ness + overlap-fraction pre-flight" >&2
  echo "       Activate the 'bioinf' micromamba env (provides samtools)." >&2
  exit 2
fi

# 4. PE-ness assertion (rev 1 A-C1 — samtools-direct regex mirroring smoke at phase_h_smoke.sh:159)
if ! samtools view -H "$BAM" 2>/dev/null | grep -qE '^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]'; then
  echo "error: expected PE BAM (Bismark '-1' arg in @PG header); got header without paired-end indicator" >&2
  echo "       confirm input is a paired-end Bismark output. SE BAMs use the SE matrix driver instead." >&2
  exit 2
fi

# 5. Overlap-fraction sanity check (rev 1 A-I1)
# samtools FLAG 0x2 = "properly paired" — the count we need for overlap-eligibility.
# Counting the whole BAM takes ~30 s for a 1.2 GB file; one-time pre-flight cost.
echo "==> measuring properly-paired fraction (samtools view -c -f 0x2 + total count)..." >&2
TOTAL_READS=$(samtools view -c "$BAM" 2>/dev/null || echo 0)
PAIRED_READS=$(samtools view -c -f 0x2 "$BAM" 2>/dev/null || echo 0)
if [[ "$TOTAL_READS" -le 0 ]]; then
  echo "error: samtools view -c reported 0 total reads (or failed)" >&2
  exit 2
fi
# Compute percentage (×100 / total; integer division — fine for 80% threshold)
OVERLAP_PCT=$(( PAIRED_READS * 100 / TOTAL_READS ))
if [[ "$OVERLAP_PCT" -lt 80 ]]; then
  echo "error: BAM has ${OVERLAP_PCT}% properly-paired reads ($PAIRED_READS / $TOTAL_READS)" >&2
  echo "       overlap differential check requires ≥80% to be meaningful." >&2
  echo "       Either use a different BAM (e.g., a canonical WGBS PE library)" >&2
  echo "       or add a --skip-overlap-differential flag in a polish(extractor): follow-up." >&2
  exit 2
fi
echo "    Properly-paired fraction: ${OVERLAP_PCT}% ($PAIRED_READS / $TOTAL_READS)" >&2

# 6. Perl bismark_methylation_extractor version assertion (mirror SE rev 1 I8)
PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"
if [[ ! -x "$PERL_BIN" ]]; then
  echo "error: Perl binary not executable: $PERL_BIN" >&2
  echo "       Activate the 'bioinf' micromamba env on colossal or set PERL_BIN." >&2
  exit 2
fi
PERL_VERSION=$("$PERL_BIN" --version 2>&1 | grep -oE 'Bismark Extractor Version: v[0-9.]+' | head -1 || true)
if [[ "$PERL_VERSION" != "Bismark Extractor Version: v0.25.1" ]]; then
  echo "error: expected Perl bismark v0.25.1; got '$PERL_VERSION'" >&2
  echo "       The locked 11,443 B M-bias baseline assumes v0.25.1." >&2
  echo "       Either upgrade/downgrade the 'bioinf' env to v0.25.1, or update" >&2
  echo "       the locked baseline in SPEC §8.3 + this driver before proceeding." >&2
  exit 2
fi

# 7. Rust binary discoverable — defer to phase_h_smoke.sh's check
#    (it builds on demand via `cargo build --release`).

# 8. nproc + contention advisory (mirror SE rev 1 I9; rev 3 B-Med graceful skip)
NCORES=""
if command -v nproc >/dev/null 2>&1; then
  NCORES=$(nproc)
  echo "Available cores: $NCORES" >&2
else
  echo "warning: nproc not found; skipping core-count check + contention advisory" >&2
fi
if [[ -n "$NCORES" ]]; then
  for n in $PARALLEL_SET; do
    if [[ "$n" -gt "$NCORES" ]]; then
      echo "error: requested --parallel $n exceeds available cores ($NCORES)" >&2
      echo "       Reduce --parallel-set or run on a host with more cores." >&2
      exit 2
    fi
  done
fi
# Load advisory — BSD/GNU-tolerant regex (mirror SE rev 3 B-E3).
if [[ -n "$NCORES" ]] && command -v uptime >/dev/null 2>&1; then
  LOAD=""
  if LOAD=$(uptime 2>/dev/null | awk -F'load average[s]?:' '{print $2}' | awk '{gsub(/,/, " "); print $1}' 2>/dev/null); then
    if [[ -n "$LOAD" ]] && awk "BEGIN {exit !($LOAD > $NCORES)}" 2>/dev/null; then
      echo "warning: 1-min load average ($LOAD) exceeds nproc ($NCORES);" >&2
      echo "         speedup ratios will be noisy. Consider 'nice -n 10 ...'." >&2
    fi
  fi
fi

# 9. tmux/screen warning (mirror SE rev 3 B-Med)
if [[ -z "${TMUX:-}" && -z "${STY:-}" ]]; then
  echo "warning: not running inside tmux or screen. The matrix takes 1.5-4 hours;" >&2
  echo "         SSH disconnect would leave the subprocesses orphaned. Recommended:" >&2
  echo "           tmux new -s phase_h_pe_release   (or: screen -S phase_h_pe_release)" >&2
  echo "         then re-run this driver. Press Ctrl-C to abort + restart in tmux." >&2
fi

# 10. Input BAM MD5 (rev 1 B-Opt-4 — fixture-drift detector)
echo "==> computing input BAM MD5 (fixture-drift detector; ~5-10 s for 1 GB)..." >&2
BAM_MD5=""
if command -v md5sum >/dev/null 2>&1; then
  BAM_MD5=$(md5sum "$BAM" 2>/dev/null | awk '{print $1}')
elif command -v md5 >/dev/null 2>&1; then
  BAM_MD5=$(md5 -q "$BAM" 2>/dev/null || true)
fi
if [[ -z "$BAM_MD5" ]]; then
  BAM_MD5="(md5 unavailable)"
fi
echo "    Input BAM MD5: $BAM_MD5" >&2

# ─── Matrix definition (5 cells; rev 1 §3.1) ───────────────────────────

# Mnemonic cell-id naming (rev 1 B-Imp-3): PE's 5-dimensional flag space
# makes parameter-encoded names unwieldy. Mnemonic trades cross-plan
# symmetry vs SE (parameter-encoded) for readability.
MATRIX_CELLS=("D" "r1_5p" "r2_5p" "r1r2_3p" "overlap")

declare -A CELL_FLAGS=(
  ["D"]=""
  ["r1_5p"]="--ignore 5"
  ["r2_5p"]="--ignore_r2 5"
  ["r1r2_3p"]="--ignore_3prime 5 --ignore_3prime_r2 5"
  ["overlap"]="--include_overlap"
)

# ─── Matrix execution (rev 1 §3.3.3 + §3.6) ────────────────────────────

echo "==> Phase H PE matrix: ${#MATRIX_CELLS[@]} cells × parallel-set='$PARALLEL_SET'" >&2
echo "    BAM: $BAM" >&2
echo "    OUT: $OUT_DIR" >&2

declare -a CELL_NAMES=()
declare -a CELL_N=()
declare -a CELL_FLAGS_STR=()
declare -a CELL_VERDICT=()
declare -a CELL_PERL_S=()
declare -a CELL_RUST_S=()
declare -a CELL_SUBDIR=()

# Cross-N tracking — accumulate per CELL_ID across the N inner-loop.
declare -A CELL_NS              # key=CELL_ID → " 1 4 ..."
declare -A CELL_NS_SUBDIRS      # key="CELL_ID|N" → subdir

for cell_id in "${MATRIX_CELLS[@]}"; do
  CELL_NS["$cell_id"]=""
  EXTRA_FLAGS="${CELL_FLAGS[$cell_id]}"

  for n in $PARALLEL_SET; do
    SUBDIR="$OUT_DIR/cell_p${n}_${cell_id}"
    echo "" >&2
    echo "==> cell $cell_id @ --parallel $n (flags: ${EXTRA_FLAGS:-<none>})" >&2

    # Invoke per-cell smoke. Allow non-zero exit (we record + continue).
    set +e
    bash "$SMOKE_SCRIPT" "$BAM" \
      --parallel "$n" \
      --mode default \
      --out "$SUBDIR" \
      --extra-rust "$EXTRA_FLAGS" \
      --extra-perl "$EXTRA_FLAGS" \
      >&2
    SMOKE_RC=$?
    set -e

    # Parse wall-clocks from the smoke's diff_summary.txt (anchored regex).
    PERL_S=""
    RUST_S=""
    if [[ -f "$SUBDIR/diff_summary.txt" ]]; then
      PERL_S=$(grep -E '^Perl: [0-9]+s$' "$SUBDIR/diff_summary.txt" | sed -E 's/^Perl: ([0-9]+)s$/\1/' | head -1 || true)
      RUST_S=$(grep -E '^Rust: [0-9]+s$' "$SUBDIR/diff_summary.txt" | sed -E 's/^Rust: ([0-9]+)s$/\1/' | head -1 || true)
    fi

    case "$SMOKE_RC" in
      0) VERDICT="PASS" ;;
      1) VERDICT="FAIL" ;;
      *) VERDICT="USAGE" ;;
    esac

    CELL_NAMES+=("$cell_id")
    CELL_N+=("$n")
    CELL_FLAGS_STR+=("$EXTRA_FLAGS")
    CELL_VERDICT+=("$VERDICT")
    CELL_PERL_S+=("${PERL_S:-?}")
    CELL_RUST_S+=("${RUST_S:-?}")
    CELL_SUBDIR+=("$SUBDIR")

    CELL_NS["$cell_id"]="${CELL_NS["$cell_id"]} $n"
    CELL_NS_SUBDIRS["$cell_id|$n"]="$SUBDIR"
  done
done

# ─── Cross-N byte-identity check (rev 1 C1 + §3.3.4) ───────────────────
#
# For each CELL_ID, for each (N_a < N_b) in its N values, compare every
# Rust output file across the two cells. Aggregate to cross_n_summary.txt.
# Runs UNCONDITIONALLY per cell, even if byte-identity-vs-Perl FAILed
# (rev 1 B-Imp-4 — preserves diagnostic signal: a simultaneous cross-N
# failure points specifically at Phase F's worker-reduce path; an
# isolated byte-identity FAIL with cross-N PASS points elsewhere).

CROSS_N_SUMMARY="$OUT_DIR/cross_n_summary.txt"
{
  echo "Phase H PE matrix — cross-N byte-identity check"
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "SPEC §8.3 row 4: Rust-N=1 ≡ Rust-N=4 raw-byte per CELL_ID."
  echo "(Runs unconditionally per rev 1 B-Imp-4, even on cells with FAILed byte-identity vs Perl.)"
  echo ""
} > "$CROSS_N_SUMMARY"

CROSS_N_FAILS=0
for cell_id in "${MATRIX_CELLS[@]}"; do
  NS=$(echo "${CELL_NS["$cell_id"]}" | tr -s ' ' '\n' | grep -v '^$' | sort -n | uniq)
  NSARR=()
  for n in $NS; do NSARR+=("$n"); done

  if [[ ${#NSARR[@]} -lt 2 ]]; then
    echo "[$cell_id] only 1 N value; cross-N skipped" >> "$CROSS_N_SUMMARY"
    continue
  fi

  CELL_FAILED=0
  for ((i = 0; i < ${#NSARR[@]}; i++)); do
    for ((j = i + 1; j < ${#NSARR[@]}; j++)); do
      NA="${NSARR[i]}"
      NB="${NSARR[j]}"
      SUBDIR_A="${CELL_NS_SUBDIRS["$cell_id|$NA"]}"
      SUBDIR_B="${CELL_NS_SUBDIRS["$cell_id|$NB"]}"
      RUST_DIR_A="$SUBDIR_A/rust"
      RUST_DIR_B="$SUBDIR_B/rust"

      if [[ ! -d "$RUST_DIR_A" || ! -d "$RUST_DIR_B" ]]; then
        echo "[$cell_id] N=$NA vs N=$NB: missing rust output dirs — SKIP" >> "$CROSS_N_SUMMARY"
        continue
      fi

      FILES_A=$(cd "$RUST_DIR_A" && ls -1 | sort)
      FILES_B=$(cd "$RUST_DIR_B" && ls -1 | sort)
      DIFFS=0
      for f in $(comm -12 <(echo "$FILES_A") <(echo "$FILES_B")); do
        if ! cmp -s "$RUST_DIR_A/$f" "$RUST_DIR_B/$f"; then
          DIFFS=$((DIFFS + 1))
          echo "[$cell_id] N=$NA vs N=$NB: $f BYTE-DIFFERS" >> "$CROSS_N_SUMMARY"
        fi
      done

      NAME_DIFF=$(diff <(echo "$FILES_A") <(echo "$FILES_B") || true)
      if [[ -n "$NAME_DIFF" ]]; then
        DIFFS=$((DIFFS + 1))
        echo "[$cell_id] N=$NA vs N=$NB: FILE-NAME-SET MISMATCH" >> "$CROSS_N_SUMMARY"
        echo "$NAME_DIFF" >> "$CROSS_N_SUMMARY"
      fi

      if [[ "$DIFFS" -eq 0 ]]; then
        echo "[$cell_id] N=$NA vs N=$NB: PASS (all files byte-identical)" >> "$CROSS_N_SUMMARY"
      else
        CELL_FAILED=1
      fi
    done
  done

  if [[ "$CELL_FAILED" -ne 0 ]]; then
    CROSS_N_FAILS=$((CROSS_N_FAILS + 1))
  fi
done

# ─── M-bias baseline + mixed-metric differential (rev 1 A-I3 + A-O3 + B-Imp-1) ─

# Rev 1 (mirror SE rev 3 A-L1 / B-L2): fail-CLOSED M-bias baseline.
# BASELINE_GATE_APPLIES: 1 iff (D, N=1) cell exists.
# MBIAS_BASELINE_OK: 0 init; flipped to 1 only on positive size==11443 confirmation.

DEFAULT_N1_SUBDIR=""
for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
  if [[ "${CELL_NAMES[k]}" == "D" && "${CELL_N[k]}" == "1" ]]; then
    DEFAULT_N1_SUBDIR="${CELL_SUBDIR[k]}"
    break
  fi
done
BASELINE_GATE_APPLIES=0
MBIAS_BASELINE_OK=0
MBIAS_ACTUAL_SIZE=""
MBIAS_FILE=""
if [[ -n "$DEFAULT_N1_SUBDIR" && -d "$DEFAULT_N1_SUBDIR/rust" ]]; then
  BASELINE_GATE_APPLIES=1
  MBIAS_FILE=$(ls "$DEFAULT_N1_SUBDIR"/rust/*M-bias.txt 2>/dev/null | head -1 || true)
  if [[ -n "$MBIAS_FILE" && -f "$MBIAS_FILE" ]]; then
    MBIAS_ACTUAL_SIZE=$(wc -c < "$MBIAS_FILE" | tr -d ' ')
    if [[ "$MBIAS_ACTUAL_SIZE" == "11443" ]]; then
      MBIAS_BASELINE_OK=1
    fi
  fi
fi

# Rev 1 (A-O3 mixed-metric differential; B-Imp-1 fail-closed init).
# - r1_5p / r2_5p / r1r2_3p: row count < D (SE-symmetric)
# - overlap: count-sum strictly > D (PE-specific; --include_overlap accumulates counts at existing positions; rev 2 dropped the +5% floor)
#
# ROW_COUNT_OK initialized to 0 (fail-closed; rev 1 B-Imp-1). Flipped to 1
# only on positive completion of all four assertions AND all 5 cells'
# M-bias files present + readable. Missing file → forced FAIL.

ROW_COUNT_OK=0
ROW_COUNT_DETAIL=""

count_mbias_rows() {
  local f="$1"
  if [[ ! -f "$f" ]]; then echo ""; return; fi
  # Data rows start with `<position>\t` where position is a digit.
  # Rev 3 (B-H1 ≡ A-M1 consensus): use awk instead of `grep -cE ... || echo "0"`.
  # The grep pattern fail-opens: grep -c prints "0" AND exits 1 when 0 matches,
  # triggering the || echo "0" fallback. Result is two lines "0\n0" — downstream
  # integer compare `[[ N -ge D ]]` hits "bad math expression" (swallowed by
  # 2>/dev/null inside if-test), the violation goes unrecorded, PASS_FLAG stays
  # 1, and the matrix emits a false PASS. The awk form emits a single integer
  # (including 0) on stdout, exits 0, and parses cleanly.
  awk '/^[0-9]+\t/ { c++ } END { print c+0 }' "$f"
}

sum_mbias_counts() {
  # Sum of methylated (col 2) + unmethylated (col 3) across all data rows.
  # Rev 1 A-O3: the count-sum metric for the `overlap` cell (M-bias positions
  # are read-relative; --include_overlap accumulates counts at existing
  # positions without adding rows; count-sum strictly increases).
  local f="$1"
  if [[ ! -f "$f" ]]; then echo ""; return; fi
  awk -F'\t' '/^[0-9]+\t/ { sum += $2 + $3 } END { print sum+0 }' "$f" 2>/dev/null || echo "0"
}

get_cell_mbias_file() {
  local cell_id="$1" n="$2"
  for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
    if [[ "${CELL_NAMES[k]}" == "$cell_id" && "${CELL_N[k]}" == "$n" ]]; then
      ls "${CELL_SUBDIR[k]}"/rust/*M-bias.txt 2>/dev/null | head -1 || true
      return
    fi
  done
}

if [[ "$BASELINE_GATE_APPLIES" -eq 1 ]]; then
  # All 5 cells at N=1 must be present + readable.
  D_FILE=$(get_cell_mbias_file "D" 1)
  R1_5P_FILE=$(get_cell_mbias_file "r1_5p" 1)
  R2_5P_FILE=$(get_cell_mbias_file "r2_5p" 1)
  R1R2_3P_FILE=$(get_cell_mbias_file "r1r2_3p" 1)
  OVERLAP_FILE=$(get_cell_mbias_file "overlap" 1)

  MISSING=""
  for fpair in "D|$D_FILE" "r1_5p|$R1_5P_FILE" "r2_5p|$R2_5P_FILE" "r1r2_3p|$R1R2_3P_FILE" "overlap|$OVERLAP_FILE"; do
    IFS='|' read -r CN CF <<< "$fpair"
    if [[ -z "$CF" || ! -f "$CF" ]]; then
      MISSING="$MISSING $CN"
    fi
  done

  if [[ -n "$MISSING" ]]; then
    # Rev 1 B-Imp-1: missing file → forced FAIL (fail-closed).
    ROW_COUNT_DETAIL="[FAIL: M-bias.txt missing/unreadable for cell(s):$MISSING]"
  else
    # All present; compute metrics.
    D_ROWS=$(count_mbias_rows "$D_FILE")
    D_COUNTS=$(sum_mbias_counts "$D_FILE")
    R1_5P_ROWS=$(count_mbias_rows "$R1_5P_FILE")
    R2_5P_ROWS=$(count_mbias_rows "$R2_5P_FILE")
    R1R2_3P_ROWS=$(count_mbias_rows "$R1R2_3P_FILE")
    OVERLAP_COUNTS=$(sum_mbias_counts "$OVERLAP_FILE")

    ROW_COUNT_DETAIL="D rows=$D_ROWS count-sum=$D_COUNTS | r1_5p rows=$R1_5P_ROWS | r2_5p rows=$R2_5P_ROWS | r1r2_3p rows=$R1R2_3P_ROWS | overlap count-sum=$OVERLAP_COUNTS"

    PASS_FLAG=1
    # Three `<D` row-count assertions (SE-symmetric)
    if [[ -n "$R1_5P_ROWS" && -n "$D_ROWS" && "$R1_5P_ROWS" -ge "$D_ROWS" ]] 2>/dev/null; then
      PASS_FLAG=0
      ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: differential r1_5p rows=$R1_5P_ROWS not < D=$D_ROWS]"
    fi
    if [[ -n "$R2_5P_ROWS" && -n "$D_ROWS" && "$R2_5P_ROWS" -ge "$D_ROWS" ]] 2>/dev/null; then
      PASS_FLAG=0
      ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: differential r2_5p rows=$R2_5P_ROWS not < D=$D_ROWS]"
    fi
    if [[ -n "$R1R2_3P_ROWS" && -n "$D_ROWS" && "$R1R2_3P_ROWS" -ge "$D_ROWS" ]] 2>/dev/null; then
      PASS_FLAG=0
      ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: differential r1r2_3p rows=$R1R2_3P_ROWS not < D=$D_ROWS]"
    fi
    # overlap count-sum strictly > D (rev 2, 2026-05-29: dropped the +5% floor).
    # The magnitude of the --include_overlap bump scales with the mate-overlap-
    # base fraction (insert size vs read length) — a per-library property, not a
    # fixed constant. On SRR24827378_10M the real bump is +2.28% (byte-identical
    # Perl≡Rust), which the old ≥5% floor wrongly FAILed. The only always-true
    # invariant is monotonic increase; the 80%-properly-paired pre-flight gate
    # ensures overlap is meaningful. See plans/05262026_bismark-extractor/
    # MATRIX_REV2_OVERLAP_DIFFERENTIAL_PLAN.md + evidence dir on colossal.
    # `-le` fails on count-sum == D intentionally: properly-paired (FLAG 0x2)
    # does NOT imply mate overlap, but zero net overlap on a WGBS library means
    # --include_overlap was a no-op = a genuine regression worth failing on.
    if [[ -n "$OVERLAP_COUNTS" && -n "$D_COUNTS" && "$D_COUNTS" -gt 0 ]]; then
      if [[ "$OVERLAP_COUNTS" -le "$D_COUNTS" ]]; then
        PASS_FLAG=0
        ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: differential overlap count-sum=$OVERLAP_COUNTS not > D=$D_COUNTS]"
      fi
    else
      PASS_FLAG=0
      ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: overlap count-sum=$OVERLAP_COUNTS or D count-sum=$D_COUNTS unreadable]"
    fi

    if [[ "$PASS_FLAG" -eq 1 ]]; then
      ROW_COUNT_OK=1
    fi
  fi
fi

# ─── Speedup table (rev 1 §3.3.5 + I10 + B-Opt-4) ──────────────────────

SPEEDUP_TABLE="$OUT_DIR/speedup_table.md"
GIT_HEAD=$(cd "$REPO_ROOT" && git rev-parse HEAD 2>/dev/null || echo "(unknown)")
CRATE_VERSION=$(grep -E '^version = ' "$REPO_ROOT/rust/bismark/Cargo.toml" | head -1 | sed -E 's/^version = "([^"]+)"$/\1/' || echo "(unknown)")
BAM_SIZE=$(wc -c < "$BAM" | tr -d ' ')

{
  echo "# Phase H PE speedup table"
  echo ""
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "Input BAM: $BAM ($BAM_SIZE bytes)"
  echo "Input BAM MD5: $BAM_MD5"
  echo "Properly-paired fraction: ${OVERLAP_PCT}% ($PAIRED_READS / $TOTAL_READS, asserted ≥80%)"
  echo "Bismark Perl version: v0.25.1 (asserted by pre-flight)"
  echo "Rust commit: $GIT_HEAD"
  echo "Rust crate version: $CRATE_VERSION"
  echo "Library: PE (asserted via samtools @PG pre-flight check; see driver's pre-flight step 4)"
  echo "Parallel set: $PARALLEL_SET"
  echo "Available cores: $NCORES"
  echo ""
  echo "## Per-cell wall-clock"
  echo ""
  echo "| Cell | N | Flags | Perl (s) | Rust (s) | Rust/Perl | Verdict |"
  echo "|------|---|-------|----------|----------|-----------|---------|"
} > "$SPEEDUP_TABLE"

for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
  P="${CELL_PERL_S[k]}"
  R="${CELL_RUST_S[k]}"
  RATIO="?"
  # Mirror SE rev 3 B-L1: column header is "Rust/Perl", compute R/P (not P/R).
  if [[ "$P" =~ ^[0-9]+$ && "$R" =~ ^[0-9]+$ && "$P" -gt 0 ]]; then
    RATIO_X100=$(( R * 100 / P ))
    INT=$(( RATIO_X100 / 100 ))
    FRAC=$(( RATIO_X100 % 100 ))
    RATIO=$(printf "%d.%02d×" "$INT" "$FRAC")
  fi
  # Sub-2s annotation (mirror SE rev 3 Low).
  SUBSECOND_NOTE=""
  if [[ "$P" =~ ^[0-9]+$ && "$R" =~ ^[0-9]+$ ]]; then
    if [[ "$P" -lt 2 || "$R" -lt 2 ]]; then
      SUBSECOND_NOTE=" ⚠️ sub-2s"
    fi
  fi
  FLAGS_DISPLAY="${CELL_FLAGS_STR[k]:-<none>}"
  echo "| ${CELL_NAMES[k]} | ${CELL_N[k]} | $FLAGS_DISPLAY | $P | $R | ${RATIO}${SUBSECOND_NOTE} | ${CELL_VERDICT[k]} |" >> "$SPEEDUP_TABLE"
done

# Per-N aggregate
{
  echo ""
  echo "## Per-N aggregate"
  echo ""
  echo "| N   | Avg Perl (s) | Avg Rust (s) | Avg Rust/Perl | Perl scaling | Rust scaling | Cells |"
  echo "|-----|--------------|--------------|---------------|--------------|--------------|-------|"
} >> "$SPEEDUP_TABLE"

declare -A N_PERL_SUM
declare -A N_RUST_SUM
declare -A N_CELL_COUNT
declare -A N_PERL_AVG
declare -A N_RUST_AVG
for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
  n="${CELL_N[k]}"
  P="${CELL_PERL_S[k]}"
  R="${CELL_RUST_S[k]}"
  if [[ "$P" =~ ^[0-9]+$ && "$R" =~ ^[0-9]+$ ]]; then
    N_PERL_SUM[$n]=$(( ${N_PERL_SUM[$n]:-0} + P ))
    N_RUST_SUM[$n]=$(( ${N_RUST_SUM[$n]:-0} + R ))
    N_CELL_COUNT[$n]=$(( ${N_CELL_COUNT[$n]:-0} + 1 ))
  fi
done

BASELINE_N=$(echo "$PARALLEL_SET" | tr ' ' '\n' | sort -n | head -1)
BASELINE_PERL_AVG=""
BASELINE_RUST_AVG=""

for n in $(echo "$PARALLEL_SET" | tr ' ' '\n' | sort -n); do
  COUNT="${N_CELL_COUNT[$n]:-0}"
  if [[ "$COUNT" -eq 0 ]]; then
    echo "| $n   | (no valid cells)              |              |               |              |              |       |" >> "$SPEEDUP_TABLE"
    continue
  fi
  PAVG=$(( ${N_PERL_SUM[$n]} / COUNT ))
  RAVG=$(( ${N_RUST_SUM[$n]} / COUNT ))
  N_PERL_AVG[$n]=$PAVG
  N_RUST_AVG[$n]=$RAVG

  if [[ "$n" == "$BASELINE_N" ]]; then
    BASELINE_PERL_AVG=$PAVG
    BASELINE_RUST_AVG=$RAVG
    PSCALE="(baseline)"
    RSCALE="(baseline)"
    PR_RATIO=""
    if [[ "$PAVG" -gt 0 ]]; then
      PR_X100=$(( RAVG * 100 / PAVG ))
      PR_RATIO=$(printf "%d.%02d×" "$(( PR_X100 / 100 ))" "$(( PR_X100 % 100 ))")
    fi
  else
    PR_RATIO=""
    if [[ "$PAVG" -gt 0 ]]; then
      PR_X100=$(( RAVG * 100 / PAVG ))
      PR_RATIO=$(printf "%d.%02d×" "$(( PR_X100 / 100 ))" "$(( PR_X100 % 100 ))")
    fi
    PSCALE=""
    RSCALE=""
    if [[ -n "$BASELINE_PERL_AVG" && "$PAVG" -gt 0 ]]; then
      PSCALE_X100=$(( BASELINE_PERL_AVG * 100 / PAVG ))
      PSCALE=$(printf "%d.%02d×" "$(( PSCALE_X100 / 100 ))" "$(( PSCALE_X100 % 100 ))")
    fi
    if [[ -n "$BASELINE_RUST_AVG" && "$RAVG" -gt 0 ]]; then
      RSCALE_X100=$(( BASELINE_RUST_AVG * 100 / RAVG ))
      RSCALE=$(printf "%d.%02d×" "$(( RSCALE_X100 / 100 ))" "$(( RSCALE_X100 % 100 ))")
    fi
  fi

  echo "| $n   | $PAVG          | $RAVG          | $PR_RATIO         | $PSCALE   | $RSCALE   | $COUNT     |" >> "$SPEEDUP_TABLE"
done

# SPEC §9.7 target check (Rust scaling at highest N)
HIGHEST_N=$(echo "$PARALLEL_SET" | tr ' ' '\n' | sort -n | tail -1)
PERF_TARGET_MET=1
PERF_TARGET_VALUE=""
if [[ "$HIGHEST_N" != "$BASELINE_N" && -n "${N_RUST_AVG[$HIGHEST_N]:-}" && -n "${N_RUST_AVG[$BASELINE_N]:-}" && "${N_RUST_AVG[$HIGHEST_N]}" -gt 0 ]]; then
  SCALE_X100=$(( ${N_RUST_AVG[$BASELINE_N]} * 100 / ${N_RUST_AVG[$HIGHEST_N]} ))
  PERF_TARGET_VALUE=$(printf "%d.%02d×" "$(( SCALE_X100 / 100 ))" "$(( SCALE_X100 % 100 ))")
  if [[ "$SCALE_X100" -lt 400 ]]; then
    PERF_TARGET_MET=0
  fi
fi

{
  echo ""
  echo "## SPEC §9.7 target check"
  echo ""
  echo "Target: Rust --parallel $HIGHEST_N ≥ 4× Rust --parallel $BASELINE_N."
  if [[ -n "$PERF_TARGET_VALUE" ]]; then
    if [[ "$PERF_TARGET_MET" -eq 1 ]]; then
      echo "Measured: Rust scaling at N=$HIGHEST_N = $PERF_TARGET_VALUE. ✅ Target met."
    else
      echo "Measured: Rust scaling at N=$HIGHEST_N = $PERF_TARGET_VALUE. ⚠️ Below 4× target."
      echo "(Byte-identity gate is independent; v1.0 may legitimately ship at exit 3 per PHASE_H_PE_PLAN §1."
      echo "File perf(extractor): sub-issue per #872.)"
    fi
  else
    echo "Measured: (insufficient data — single N value or no valid cells)"
  fi
  echo ""
  echo "## Cross-N N-invariance (SPEC §8.3 row 4)"
  echo ""
  if [[ "$CROSS_N_FAILS" -eq 0 ]]; then
    echo "✅ All cells PASS cross-N byte-identity. See cross_n_summary.txt."
  else
    echo "❌ $CROSS_N_FAILS cell(s) FAILED cross-N. See cross_n_summary.txt."
  fi
  echo ""
  echo "## M-bias baseline (D, N=1) — Phase C.1 polarity regression guard"
  echo ""
  if [[ "$BASELINE_GATE_APPLIES" -eq 0 ]]; then
    echo "⚠️ Gate does not apply: --parallel-set omits N=1 (no (D, N=1) cell). The"
    echo "   11,443 B baseline check requires N=1 in the matrix; matrix verdict ignores"
    echo "   the gate when it doesn't apply. To verify the baseline, re-run with"
    echo "   '--parallel-set \"1 4\"'."
  elif [[ -z "$MBIAS_ACTUAL_SIZE" ]]; then
    echo "❌ FAIL: M-bias.txt could not be located in (D, N=1) cell. Either Rust"
    echo "   suppressed the file (regression) or the cell crashed. Investigate"
    echo "   cell_p1_D/rust/. Matrix exits 1."
  elif [[ "$MBIAS_BASELINE_OK" -eq 1 ]]; then
    echo "✅ M-bias.txt size = $MBIAS_ACTUAL_SIZE B (matches locked 11,443 B baseline)."
  else
    echo "❌ FAIL: M-bias.txt size = $MBIAS_ACTUAL_SIZE B (expected 11,443 B —"
    echo "   Phase C.1 polarity regression guard violated). Matrix exits 1."
    echo "   See RELEASE_CHECKLIST escalation path for colossal-vs-planner baseline drift."
  fi
  echo ""
  echo "## Mixed-metric differential (N=1, rev 1 A-O3) — semantic regression guard"
  echo ""
  echo "Cells:"
  echo "  - r1_5p / r2_5p / r1r2_3p: M-bias row count strictly < D (--ignore removes positions)"
  echo "  - overlap: M-bias count-sum (methylated + unmethylated) strictly > D"
  echo "    (--include_overlap accumulates counts at existing positions; rows unchanged)"
  echo ""
  if [[ "$BASELINE_GATE_APPLIES" -eq 0 ]]; then
    echo "⚠️ Gate does not apply: --parallel-set omits N=1."
  elif [[ "$ROW_COUNT_OK" -eq 1 ]]; then
    echo "✅ All four differential assertions PASS."
    echo "   $ROW_COUNT_DETAIL"
  else
    echo "❌ FAIL: differential check violated. Matrix exits 1."
    echo "   $ROW_COUNT_DETAIL"
  fi
} >> "$SPEEDUP_TABLE"

# ─── Matrix verdict + exit code (rev 1 §3.3.6 + I16) ───────────────────

PASS_COUNT=0
FAIL_COUNT=0
USAGE_COUNT=0
for v in "${CELL_VERDICT[@]}"; do
  case "$v" in
    PASS) PASS_COUNT=$((PASS_COUNT + 1)) ;;
    FAIL) FAIL_COUNT=$((FAIL_COUNT + 1)) ;;
    USAGE) USAGE_COUNT=$((USAGE_COUNT + 1)) ;;
  esac
done

VERDICT_FILE="$OUT_DIR/matrix_verdict.txt"
{
  echo "Phase H PE matrix verdict"
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "Input BAM: $BAM"
  echo "Input BAM MD5: $BAM_MD5"
  echo "Properly-paired fraction: ${OVERLAP_PCT}% ($PAIRED_READS / $TOTAL_READS)"
  echo "Rust commit: $GIT_HEAD"
  echo "Rust crate version: $CRATE_VERSION"
  echo ""
  echo "Per-cell breakdown:"
  for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
    FLAGS_DISP="${CELL_FLAGS_STR[k]:-<none>}"
    echo "  ${CELL_NAMES[k]} N=${CELL_N[k]} (flags: $FLAGS_DISP): ${CELL_VERDICT[k]}"
  done
  echo ""
  echo "Aggregates:"
  echo "  Total cells:              ${#CELL_NAMES[@]}"
  echo "  PASS:                     $PASS_COUNT"
  echo "  FAIL:                     $FAIL_COUNT"
  echo "  USAGE:                    $USAGE_COUNT"
  echo "  Cross-N fails:            $CROSS_N_FAILS"
  echo "  Baseline gate applies:    $BASELINE_GATE_APPLIES (1=N=1 in matrix, 0=skipped)"
  echo "  M-bias baseline OK:       $MBIAS_BASELINE_OK (1=size==11443 at (D,N=1), 0=missing or drift)"
  echo "  Row-count/count-sum OK:   $ROW_COUNT_OK (1=all 4 differentials PASS + all files present, 0=violation/missing)"
  echo "  Differential detail:      ${ROW_COUNT_DETAIL:-(gate did not apply)}"
  echo "  Perf target met:          $PERF_TARGET_MET (1=≥4× Rust scaling, 0=below)"
  echo ""
} > "$VERDICT_FILE"

EXIT=0
REASON=""
if [[ "$USAGE_COUNT" -gt 0 ]]; then
  EXIT=2
  REASON="USAGE: $USAGE_COUNT cell(s) had usage errors"
elif [[ "$FAIL_COUNT" -gt 0 ]]; then
  EXIT=1
  REASON="FAIL: $FAIL_COUNT cell(s) failed byte-identity"
elif [[ "$CROSS_N_FAILS" -gt 0 ]]; then
  EXIT=1
  REASON="FAIL: cross-N byte-identity (SPEC §8.3 row 4) violated"
elif [[ "$BASELINE_GATE_APPLIES" -eq 1 && "$MBIAS_BASELINE_OK" -eq 0 ]]; then
  EXIT=1
  REASON="FAIL: M-bias baseline 11,443 B drift (or missing file) at (D, N=1) cell"
elif [[ "$BASELINE_GATE_APPLIES" -eq 1 && "$ROW_COUNT_OK" -eq 0 ]]; then
  EXIT=1
  # Rev 1 B-Imp-5: distinct verdict line for differential FAIL (disambiguates from byte-identity FAIL).
  REASON="FAIL: differential check violated (mixed-metric: row-count for <D cells, count-sum>D for overlap) — see Differential detail above"
elif [[ "$PERF_TARGET_MET" -eq 0 ]]; then
  EXIT=3
  REASON="WARN: byte-identity PASSED but Rust scaling missed §9.7's 4× target (v1.0 may ship at exit 3)"
else
  EXIT=0
  # Rev 3 (B-M2): distinguish full-gate PASS from weakened-checks PASS.
  # When --parallel-set lacks N=1, the (D, N=1)-anchored gates (M-bias baseline,
  # cross-N pairings against N=1, differential) don't apply — verdict should
  # NOT claim "baseline + differential OK" because those gates were skipped.
  if [[ "$BASELINE_GATE_APPLIES" -eq 1 ]]; then
    REASON="PASS: all cells byte-identical, cross-N invariant holds, M-bias baseline (11,443 B) + differential OK, perf target met"
  else
    REASON="PASS (weakened — --parallel-set lacks N=1; baseline + differential gates not applied): all cells byte-identical, cross-N invariant holds where applicable, perf target met"
  fi
fi

echo "Verdict: $REASON (exit $EXIT)" >> "$VERDICT_FILE"
cat "$VERDICT_FILE"

echo ""
echo "=== Phase H PE matrix complete ==="
echo "  Output dir:      $OUT_DIR"
echo "  Speedup table:   $SPEEDUP_TABLE"
echo "  Cross-N summary: $CROSS_N_SUMMARY"
echo "  Matrix verdict:  $VERDICT_FILE"
echo "  Exit code:       $EXIT"

exit "$EXIT"
