#!/usr/bin/env bash
#
# phase_h_se_matrix.sh — Phase H sub-gate 1 SE byte-identity + speedup matrix.
#
# Runs the per-cell Phase H smoke (scripts/phase_h_smoke.sh) over the 5-cell
# representative SE matrix at --parallel ∈ {1, 4} (configurable). Asserts
# SPEC §8.3 row 4 N-invariance per ignore-pair (Rust-N=1 ≡ Rust-N=4 raw-byte).
# Emits a markdown speedup table with Perl-only and Rust scaling columns.
#
# Closes #871 (under epic #798). Companion: #872 (PE matrix; separate driver).
#
# Usage:
#   scripts/phase_h_se_matrix.sh <BAM> [--out DIR] [--parallel-set "1 4"]
#
# Pre-flight checks (rev 1 I8 + I9 + I12):
#   - BAM exists + readable
#   - --out DIR is empty or doesn't exist
#   - Perl bismark_methylation_extractor version == v0.25.1
#   - Rust binary discoverable (built on-demand by phase_h_smoke.sh)
#   - nproc check + contention advisory
#
# 5-cell matrix:
#   D          (--ignore 0   --ignore_3prime 0)     — Default; M-bias = 5712 B regression guard
#   5p         (--ignore 5   --ignore_3prime 0)     — 5' trim isolated
#   3p         (--ignore 0   --ignore_3prime 5)     — 3' trim isolated
#   5p+3p      (--ignore 5   --ignore_3prime 5)     — Both trims combined
#   edge_clip  (--ignore 250 --ignore_3prime 0)     — --ignore exceeds typical read length
#
# Exit codes:
#   0  — all cells PASS byte-identity + cross-N PASS + Rust scaling ≥ SPEC §9.7's 4×
#   1  — any cell or cross-N failed byte-identity OR (D,N=1) M-bias 5712 B baseline drift
#   2  — pre-flight USAGE-ERROR
#   3  — byte-identity PASSED but Rust scaling missed the perf target (informational)
#
# Outputs:
#   <OUT>/cell_p<N>_i<5p>_i3<3p>/  — per-cell phase_h_smoke output
#   <OUT>/cross_n_summary.txt      — cross-N comparison results per ignore-pair
#   <OUT>/speedup_table.md         — markdown summary with Rust commit + crate version
#   <OUT>/matrix_verdict.txt       — PASS/FAIL with per-cell breakdown
#
# Recommended: run inside `tmux` / `screen` (1-3 h matrix runtime).

set -euo pipefail

# Rev 3 A-Er1: hard-fail loudly on bash < 4.0. macOS ships /bin/bash 3.2
# by default, and this driver uses associative arrays (`declare -A`) +
# the modern empty-array-under-set-u idiom — neither runs on bash 3.2.
if (( ${BASH_VERSINFO[0]} < 4 )); then
  echo "error: bash >= 4.0 required (current: $BASH_VERSION)" >&2
  echo "       macOS default /bin/bash is 3.2; install via 'brew install bash'" >&2
  echo "       and re-run with /opt/homebrew/bin/bash scripts/phase_h_se_matrix.sh ..." >&2
  echo "       Colossal Linux ships bash 5.x; the matrix driver targets that." >&2
  exit 2
fi

# Rev 3 (B-Med absorption): trap SIGINT/SIGTERM so user gets a clear
# message about partial state remaining in --out for evidence preservation.
trap 'echo "" >&2; echo "interrupted; partial matrix output in $OUT_DIR (preserved for evidence)" >&2; exit 130' INT TERM

# ─── Args ─────────────────────────────────────────────────────────────

BAM=""
OUT_DIR="./phase_h_se_matrix_out"
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

# 2. --out empty-or-doesn't-exist (matrix-level rejection; rev 1 I12)
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

# 3. Perl bismark_methylation_extractor version assertion (rev 1 I8)
PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"
if [[ ! -x "$PERL_BIN" ]]; then
  echo "error: Perl binary not executable: $PERL_BIN" >&2
  echo "       Activate the 'bioinf' micromamba env on colossal or set PERL_BIN." >&2
  exit 2
fi
PERL_VERSION=$("$PERL_BIN" --version 2>&1 | grep -oE 'Bismark Extractor Version: v[0-9.]+' | head -1 || true)
if [[ "$PERL_VERSION" != "Bismark Extractor Version: v0.25.1" ]]; then
  echo "error: expected Perl bismark v0.25.1; got '$PERL_VERSION'" >&2
  echo "       The locked 5712 B M-bias baseline assumes v0.25.1." >&2
  echo "       Either upgrade/downgrade the 'bioinf' env to v0.25.1, or update" >&2
  echo "       the locked baseline in SPEC §A4 + this driver before proceeding." >&2
  exit 2
fi

# 4. Rust binary discoverable — defer to phase_h_smoke.sh's check
#    (it builds on demand via `cargo build --release`).

# 5. nproc + contention advisory (rev 1 I9; rev 3 B-Med refinements)
NCORES=""
if command -v nproc >/dev/null 2>&1; then
  NCORES=$(nproc)
  echo "Available cores: $NCORES" >&2
else
  echo "warning: nproc not found; skipping core-count check + contention advisory" >&2
fi
# Hard-fail if any requested N > nproc (only when we successfully got NCORES;
# rev 3 fix: don't reject high-N requests just because nproc wasn't on PATH).
if [[ -n "$NCORES" ]]; then
  for n in $PARALLEL_SET; do
    if [[ "$n" -gt "$NCORES" ]]; then
      echo "error: requested --parallel $n exceeds available cores ($NCORES)" >&2
      echo "       Reduce --parallel-set or run on a host with more cores." >&2
      exit 2
    fi
  done
fi
# Warn if load average > nproc (speedup ratios will be noisy).
# Rev 3 B-E3: graceful-degrade across BSD vs GNU `uptime` dialects.
# - Linux GNU: "...load average: 0.50, 0.30, 0.10"
# - macOS BSD: "...load averages: 1.50 1.20 1.00"
# The regex tolerates singular/plural and comma/space separators.
if [[ -n "$NCORES" ]] && command -v uptime >/dev/null 2>&1; then
  LOAD=""
  if LOAD=$(uptime 2>/dev/null | awk -F'load average[s]?:' '{print $2}' | awk '{gsub(/,/, " "); print $1}' 2>/dev/null); then
    if [[ -n "$LOAD" ]] && awk "BEGIN {exit !($LOAD > $NCORES)}" 2>/dev/null; then
      echo "warning: 1-min load average ($LOAD) exceeds nproc ($NCORES);" >&2
      echo "         speedup ratios will be noisy. Consider 'nice -n 10 ...'." >&2
    fi
  fi
fi

# Rev 3 (B-Med absorption): warn if not inside tmux/screen — 1-3 h matrix
# runtime is too long for a raw SSH session.
if [[ -z "${TMUX:-}" && -z "${STY:-}" ]]; then
  echo "warning: not running inside tmux or screen. The matrix takes 1-3 hours;" >&2
  echo "         SSH disconnect would leave the subprocesses orphaned. Recommended:" >&2
  echo "           tmux new -s phase_h_release   (or: screen -S phase_h_release)" >&2
  echo "         then re-run this driver. Press Ctrl-C to abort + restart in tmux." >&2
fi

# ─── Matrix definition (5 cells; rev 1 §3.1) ───────────────────────────

# Each cell: "name|ignore_5p|ignore_3p"
MATRIX_CELLS=(
  "D|0|0"
  "5p|5|0"
  "3p|0|5"
  "5p+3p|5|5"
  "edge_clip|250|0"
)

# ─── Matrix execution (rev 1 §3.3.3 + §3.6) ────────────────────────────

echo "==> Phase H SE matrix: ${#MATRIX_CELLS[@]} cells × parallel-set='$PARALLEL_SET'" >&2
echo "    BAM: $BAM" >&2
echo "    OUT: $OUT_DIR" >&2

# Track per-cell results in parallel arrays.
declare -a CELL_NAMES=()
declare -a CELL_N=()
declare -a CELL_I5P=()
declare -a CELL_I3P=()
declare -a CELL_VERDICT=()   # PASS / FAIL / USAGE
declare -a CELL_PERL_S=()
declare -a CELL_RUST_S=()
declare -a CELL_SUBDIR=()

# Cross-N tracking — accumulate per ignore-pair across the N inner-loop.
declare -A IGNORE_PAIR_NS  # key="<name>|<i5p>|<i3p>" → " 1 4 ..."
declare -A IGNORE_PAIR_SUBDIRS  # key="<name>|<i5p>|<i3p>|<N>" → subdir

for cell in "${MATRIX_CELLS[@]}"; do
  IFS='|' read -r NAME I5P I3P <<< "$cell"
  PAIR_KEY="$NAME|$I5P|$I3P"
  IGNORE_PAIR_NS["$PAIR_KEY"]=""

  for n in $PARALLEL_SET; do
    SUBDIR="$OUT_DIR/cell_p${n}_i${I5P}_i3${I3P}"
    echo "" >&2
    echo "==> cell $NAME @ --parallel $n (--ignore $I5P --ignore_3prime $I3P)" >&2

    EXTRA_FLAGS="--ignore $I5P --ignore_3prime $I3P"
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

    # Parse wall-clocks from the smoke's diff_summary.txt (rev 1 I2 + I4:
    # single source of truth; anchored regex).
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

    CELL_NAMES+=("$NAME")
    CELL_N+=("$n")
    CELL_I5P+=("$I5P")
    CELL_I3P+=("$I3P")
    CELL_VERDICT+=("$VERDICT")
    CELL_PERL_S+=("${PERL_S:-?}")
    CELL_RUST_S+=("${RUST_S:-?}")
    CELL_SUBDIR+=("$SUBDIR")

    # Record for cross-N pairing
    IGNORE_PAIR_NS["$PAIR_KEY"]="${IGNORE_PAIR_NS["$PAIR_KEY"]} $n"
    IGNORE_PAIR_SUBDIRS["$PAIR_KEY|$n"]="$SUBDIR"
  done
done

# ─── Cross-N byte-identity check (rev 1 C1 + §3.3.4) ───────────────────
#
# For each ignore-pair, for each (N_a < N_b) in its N values, compare every
# Rust output file across the two cells. Aggregate to cross_n_summary.txt.

CROSS_N_SUMMARY="$OUT_DIR/cross_n_summary.txt"
{
  echo "Phase H SE matrix — cross-N byte-identity check"
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "SPEC §8.3 row 4: Rust-N=1 ≡ Rust-N=4 raw-byte per ignore-pair."
  echo ""
} > "$CROSS_N_SUMMARY"

CROSS_N_FAILS=0
for pair_key in "${!IGNORE_PAIR_NS[@]}"; do
  IFS='|' read -r PNAME PI5P PI3P <<< "$pair_key"
  NS=$(echo "${IGNORE_PAIR_NS["$pair_key"]}" | tr -s ' ' '\n' | grep -v '^$' | sort -n | uniq)
  # Build array
  NSARR=()
  for n in $NS; do NSARR+=("$n"); done

  if [[ ${#NSARR[@]} -lt 2 ]]; then
    echo "[$PNAME (--ignore $PI5P --ignore_3prime $PI3P)] only 1 N value; cross-N skipped" >> "$CROSS_N_SUMMARY"
    continue
  fi

  # All-pairs comparison.
  PAIR_FAILED=0
  for ((i = 0; i < ${#NSARR[@]}; i++)); do
    for ((j = i + 1; j < ${#NSARR[@]}; j++)); do
      NA="${NSARR[i]}"
      NB="${NSARR[j]}"
      SUBDIR_A="${IGNORE_PAIR_SUBDIRS["$pair_key|$NA"]}"
      SUBDIR_B="${IGNORE_PAIR_SUBDIRS["$pair_key|$NB"]}"
      RUST_DIR_A="$SUBDIR_A/rust"
      RUST_DIR_B="$SUBDIR_B/rust"

      if [[ ! -d "$RUST_DIR_A" || ! -d "$RUST_DIR_B" ]]; then
        echo "[$PNAME] N=$NA vs N=$NB: missing rust output dirs — SKIP" >> "$CROSS_N_SUMMARY"
        continue
      fi

      # Compare every file present in BOTH dirs.
      FILES_A=$(cd "$RUST_DIR_A" && ls -1 | sort)
      FILES_B=$(cd "$RUST_DIR_B" && ls -1 | sort)
      DIFFS=0
      for f in $(comm -12 <(echo "$FILES_A") <(echo "$FILES_B")); do
        if ! cmp -s "$RUST_DIR_A/$f" "$RUST_DIR_B/$f"; then
          DIFFS=$((DIFFS + 1))
          echo "[$PNAME] N=$NA vs N=$NB: $f BYTE-DIFFERS" >> "$CROSS_N_SUMMARY"
        fi
      done

      # File-name set diff
      NAME_DIFF=$(diff <(echo "$FILES_A") <(echo "$FILES_B") || true)
      if [[ -n "$NAME_DIFF" ]]; then
        DIFFS=$((DIFFS + 1))
        echo "[$PNAME] N=$NA vs N=$NB: FILE-NAME-SET MISMATCH" >> "$CROSS_N_SUMMARY"
        echo "$NAME_DIFF" >> "$CROSS_N_SUMMARY"
      fi

      if [[ "$DIFFS" -eq 0 ]]; then
        echo "[$PNAME] N=$NA vs N=$NB: PASS (all files byte-identical)" >> "$CROSS_N_SUMMARY"
      else
        PAIR_FAILED=1
      fi
    done
  done

  if [[ "$PAIR_FAILED" -ne 0 ]]; then
    CROSS_N_FAILS=$((CROSS_N_FAILS + 1))
  fi
done

# ─── M-bias baseline + row-count differential checks (rev 3 absorption) ─

# Rev 3 (A-L1 / B-L2 consensus): fail-CLOSED design.
# - MBIAS_GATE_APPLIES: 1 iff the matrix has a (D, N=1) cell that produced
#   an M-bias.txt. If N=1 was omitted from --parallel-set, the gate doesn't
#   apply (so does not contribute to FAIL).
# - MBIAS_BASELINE_OK: 0 by default; flipped to 1 only on POSITIVE
#   confirmation of size==5712 B at (D, N=1). Missing-file / missing-cell
#   / size-drift all keep this at 0.

DEFAULT_N1_SUBDIR=""
for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
  if [[ "${CELL_NAMES[k]}" == "D" && "${CELL_N[k]}" == "1" ]]; then
    DEFAULT_N1_SUBDIR="${CELL_SUBDIR[k]}"
    break
  fi
done
MBIAS_GATE_APPLIES=0
MBIAS_BASELINE_OK=0   # rev 3: fail-CLOSED default
MBIAS_ACTUAL_SIZE=""
MBIAS_FILE=""
if [[ -n "$DEFAULT_N1_SUBDIR" && -d "$DEFAULT_N1_SUBDIR/rust" ]]; then
  MBIAS_GATE_APPLIES=1
  MBIAS_FILE=$(ls "$DEFAULT_N1_SUBDIR"/rust/*M-bias.txt 2>/dev/null | head -1 || true)
  if [[ -n "$MBIAS_FILE" && -f "$MBIAS_FILE" ]]; then
    MBIAS_ACTUAL_SIZE=$(wc -c < "$MBIAS_FILE" | tr -d ' ')
    if [[ "$MBIAS_ACTUAL_SIZE" == "5712" ]]; then
      MBIAS_BASELINE_OK=1
    fi
  fi
fi

# Rev 3 (Coverage §3.4 #4 PARTIAL absorption): row-count differential check
# for ignore-flag cells at N=1. Catches "both binaries silently ignore the
# --ignore flag" — the rarer failure mode that per-cell cmp doesn't catch
# because Rust-vs-Perl would still match each other.
#
# Expected behaviour (per SPEC §7.6 --ignore semantics):
#   D (--ignore 0 --ignore_3prime 0):     full row count (baseline)
#   5p (--ignore 5):                       fewer rows than D
#   3p (--ignore_3prime 5):                fewer rows than D
#   5p+3p (--ignore 5 --ignore_3prime 5):  fewer than D; <= 5p AND <= 3p
#   edge_clip (--ignore 250):              near-zero rows (typical read 100 bp;
#                                          all positions filtered)
#
# Counts data rows by skipping the section headers (which start with
# capital letters or are empty). M-bias.txt rows that count as "data" are
# tab-separated lines starting with a digit (the 1-based position).

ROW_COUNT_OK=1
ROW_COUNT_DETAIL=""

count_mbias_rows() {
  local f="$1"
  if [[ ! -f "$f" ]]; then echo "0"; return; fi
  # Data rows start with `<position>\t` where position is a digit.
  # Back-port from PE driver #874 rev-3 absorption (consensus code-review
  # finding B-H1 ≡ A-M1): use awk instead of `grep -cE ... || echo "0"`.
  # The grep pattern fail-opens: grep -c prints "0" AND exits 1 when 0
  # matches, triggering the || echo "0" fallback. Result is two lines
  # "0\n0" — downstream integer compare `[[ N -ge D ]]` hits "bad math
  # expression" (swallowed by 2>/dev/null inside if-test), the violation
  # goes unrecorded, PASS_FLAG stays 1, and the matrix emits a false PASS.
  # Latent on the canonical 10M SE BAM (D cell always has rows) but real
  # fail-open in a check whose entire purpose (rev 3 absorption) is
  # fail-closed. The awk form emits a single integer (including 0) on
  # stdout, exits 0, and parses cleanly.
  awk '/^[0-9]+\t/ { c++ } END { print c+0 }' "$f"
}

get_cell_mbias_file() {
  local name="$1" n="$2"
  for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
    if [[ "${CELL_NAMES[k]}" == "$name" && "${CELL_N[k]}" == "$n" ]]; then
      ls "${CELL_SUBDIR[k]}"/rust/*M-bias.txt 2>/dev/null | head -1 || true
      return
    fi
  done
}

# Only run the differential check if (D, N=1) cell exists + has M-bias.
if [[ "$MBIAS_GATE_APPLIES" -eq 1 ]]; then
  ROWS_D=$(count_mbias_rows "$MBIAS_FILE")
  ROWS_5P=$(count_mbias_rows "$(get_cell_mbias_file '5p' 1)")
  ROWS_3P=$(count_mbias_rows "$(get_cell_mbias_file '3p' 1)")
  ROWS_5P3P=$(count_mbias_rows "$(get_cell_mbias_file '5p+3p' 1)")
  ROWS_EDGE=$(count_mbias_rows "$(get_cell_mbias_file 'edge_clip' 1)")

  ROW_COUNT_DETAIL="D=$ROWS_D 5p=$ROWS_5P 3p=$ROWS_3P 5p+3p=$ROWS_5P3P edge_clip=$ROWS_EDGE"

  # Assert decreases
  if [[ -n "$ROWS_5P" && "$ROWS_5P" -ge "$ROWS_D" ]] 2>/dev/null; then
    ROW_COUNT_OK=0
    ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: 5p ($ROWS_5P) not < D ($ROWS_D)]"
  fi
  if [[ -n "$ROWS_3P" && "$ROWS_3P" -ge "$ROWS_D" ]] 2>/dev/null; then
    ROW_COUNT_OK=0
    ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: 3p ($ROWS_3P) not < D ($ROWS_D)]"
  fi
  if [[ -n "$ROWS_5P3P" && "$ROWS_5P3P" -ge "$ROWS_D" ]] 2>/dev/null; then
    ROW_COUNT_OK=0
    ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: 5p+3p ($ROWS_5P3P) not < D ($ROWS_D)]"
  fi
  # edge_clip threshold: data row count should be < 10% of D (mostly empty).
  # Use bash integer arithmetic; threshold = D/10.
  if [[ -n "$ROWS_EDGE" && "$ROWS_D" -gt 0 ]]; then
    THRESH=$(( ROWS_D / 10 ))
    if [[ "$ROWS_EDGE" -gt "$THRESH" ]]; then
      ROW_COUNT_OK=0
      ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: edge_clip ($ROWS_EDGE) > D/10 ($THRESH)]"
    fi
  fi
fi

# ─── Speedup table (rev 1 §3.3.5 + I10) ────────────────────────────────

SPEEDUP_TABLE="$OUT_DIR/speedup_table.md"
GIT_HEAD=$(cd "$REPO_ROOT" && git rev-parse HEAD 2>/dev/null || echo "(unknown)")
CRATE_VERSION=$(grep -E '^version = ' "$REPO_ROOT/rust/bismark/Cargo.toml" | head -1 | sed -E 's/^version = "([^"]+)"$/\1/' || echo "(unknown)")
BAM_SIZE=$(wc -c < "$BAM" | tr -d ' ')

{
  echo "# Phase H SE speedup table"
  echo ""
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "Input BAM: $BAM ($BAM_SIZE bytes)"
  echo "Bismark Perl version: v0.25.1 (asserted by pre-flight)"
  echo "Rust commit: $GIT_HEAD"
  echo "Rust crate version: $CRATE_VERSION"
  echo "Parallel set: $PARALLEL_SET"
  echo "Available cores: $NCORES"
  echo ""
  echo "## Per-cell wall-clock"
  echo ""
  echo "| Cell | N | --ignore | --ignore_3prime | Perl (s) | Rust (s) | Rust/Perl | Verdict |"
  echo "|------|---|----------|-----------------|----------|----------|-----------|---------|"
} > "$SPEEDUP_TABLE"

for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
  P="${CELL_PERL_S[k]}"
  R="${CELL_RUST_S[k]}"
  RATIO="?"
  # Rev 3 B-L1 Critical fix: column header is "Rust/Perl", so compute
  # R/P (not P/R as rev 2 did). Sub-second cells where Perl=0 are
  # flagged "?" instead of inducing a divide-by-zero.
  if [[ "$P" =~ ^[0-9]+$ && "$R" =~ ^[0-9]+$ && "$P" -gt 0 ]]; then
    # ×100 for two decimals
    RATIO_X100=$(( R * 100 / P ))
    INT=$(( RATIO_X100 / 100 ))
    FRAC=$(( RATIO_X100 % 100 ))
    RATIO=$(printf "%d.%02d×" "$INT" "$FRAC")
  fi
  # Rev 3 Low: sub-minute cells produce noisy ratios; annotate.
  SUBSECOND_NOTE=""
  if [[ "$P" =~ ^[0-9]+$ && "$R" =~ ^[0-9]+$ ]]; then
    if [[ "$P" -lt 2 || "$R" -lt 2 ]]; then
      SUBSECOND_NOTE=" ⚠️ sub-2s"
    fi
  fi
  echo "| ${CELL_NAMES[k]} | ${CELL_N[k]} | ${CELL_I5P[k]} | ${CELL_I3P[k]} | $P | $R | ${RATIO}${SUBSECOND_NOTE} | ${CELL_VERDICT[k]} |" >> "$SPEEDUP_TABLE"
done

# Per-N aggregate
{
  echo ""
  echo "## Per-N aggregate"
  echo ""
  echo "| N   | Avg Perl (s) | Avg Rust (s) | Avg Rust/Perl | Perl scaling | Rust scaling | Cells |"
  echo "|-----|--------------|--------------|---------------|--------------|--------------|-------|"
} >> "$SPEEDUP_TABLE"

# Compute per-N averages
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

# Baseline: smallest N in PARALLEL_SET
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
    # Rev 3 B-L1: "Avg Rust/Perl" column = R/P (not P/R).
    if [[ "$PAVG" -gt 0 ]]; then
      PR_X100=$(( RAVG * 100 / PAVG ))
      PR_RATIO=$(printf "%d.%02d×" "$(( PR_X100 / 100 ))" "$(( PR_X100 % 100 ))")
    fi
  else
    PR_RATIO=""
    # Rev 3 B-L1: "Avg Rust/Perl" column = R/P (not P/R).
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
  # Target is 4.00× (≥4×)
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
      echo "Measured: Rust scaling at N=$HIGHEST_N = $PERF_TARGET_VALUE. ❌ Below 4× target."
      echo "(Byte-identity gate is independent; file perf(extractor): sub-issue per #871.)"
    fi
  else
    echo "Measured: (insufficient data — single N value or no valid cells)"
  fi
  echo ""
  echo "## Cross-N N-invariance (SPEC §8.3 row 4)"
  echo ""
  if [[ "$CROSS_N_FAILS" -eq 0 ]]; then
    echo "✅ All ignore-pairs PASS cross-N byte-identity. See cross_n_summary.txt."
  else
    echo "❌ $CROSS_N_FAILS ignore-pair(s) FAILED cross-N. See cross_n_summary.txt."
  fi
  echo ""
  echo "## M-bias baseline (D, N=1) — Phase C.1 regression guard"
  echo ""
  # Rev 3 (A-L1 / B-L2): fail-CLOSED reporting; missing/missing-N=1 surfaces
  # as either "gate doesn't apply" or "FAIL: missing file".
  if [[ "$MBIAS_GATE_APPLIES" -eq 0 ]]; then
    echo "⚠️ Gate does not apply: --parallel-set omits N=1 (no (D, N=1) cell). The"
    echo "   5712 B baseline check requires N=1 in the matrix; matrix verdict ignores"
    echo "   the gate when it doesn't apply. To verify the baseline, re-run with"
    echo "   '--parallel-set \"1 4\"'."
  elif [[ -z "$MBIAS_ACTUAL_SIZE" ]]; then
    echo "❌ FAIL: M-bias.txt could not be located in (D, N=1) cell. Either Rust"
    echo "   suppressed the file (regression) or the cell crashed. Investigate"
    echo "   cell_p1_i0_i30/rust/. Matrix exits 1."
  elif [[ "$MBIAS_BASELINE_OK" -eq 1 ]]; then
    echo "✅ M-bias.txt size = $MBIAS_ACTUAL_SIZE B (matches locked 5712 B baseline)."
  else
    echo "❌ FAIL: M-bias.txt size = $MBIAS_ACTUAL_SIZE B (expected 5712 B —"
    echo "   Phase C.1 regression guard violated). Matrix exits 1."
  fi
  echo ""
  echo "## M-bias row-count differential (ignore-flag cells at N=1) — rev 3"
  echo ""
  # Rev 3 (Coverage §3.4 #4 PARTIAL absorption + B-Med): asserts the
  # ignore-flag cells produce fewer M-bias data rows than the (D, N=1)
  # baseline. Catches the "both binaries silently ignore --ignore" failure
  # mode that per-cell Perl-vs-Rust cmp doesn't detect.
  if [[ "$MBIAS_GATE_APPLIES" -eq 0 ]]; then
    echo "⚠️ Gate does not apply: --parallel-set omits N=1."
  elif [[ "$ROW_COUNT_OK" -eq 1 ]]; then
    echo "✅ Row-counts decrease as expected across ignore-flag cells."
    echo "   Counts: $ROW_COUNT_DETAIL"
  else
    echo "❌ FAIL: Row-count differential violated. Matrix exits 1."
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
  echo "Phase H SE matrix verdict"
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo ""
  echo "Per-cell breakdown:"
  for ((k = 0; k < ${#CELL_NAMES[@]}; k++)); do
    echo "  ${CELL_NAMES[k]} N=${CELL_N[k]} (--ignore ${CELL_I5P[k]} --ignore_3prime ${CELL_I3P[k]}): ${CELL_VERDICT[k]}"
  done
  echo ""
  echo "Aggregates:"
  echo "  Total cells:        ${#CELL_NAMES[@]}"
  echo "  PASS:               $PASS_COUNT"
  echo "  FAIL:               $FAIL_COUNT"
  echo "  USAGE:              $USAGE_COUNT"
  echo "  Cross-N fails:      $CROSS_N_FAILS"
  echo "  M-bias gate applies: $MBIAS_GATE_APPLIES (1=N=1 in matrix, 0=skipped)"
  echo "  M-bias baseline OK:  $MBIAS_BASELINE_OK (1=size==5712, 0=missing or drift)"
  echo "  M-bias row-count OK: $ROW_COUNT_OK (1=monotonic, 0=violation)"
  echo "  Row-count detail:    ${ROW_COUNT_DETAIL:-(gate did not apply)}"
  echo "  Perf target met:     $PERF_TARGET_MET (1=≥4× Rust scaling, 0=below)"
  echo ""
} > "$VERDICT_FILE"

# Decide exit code. Rev 3: M-bias gate only enforced when MBIAS_GATE_APPLIES=1
# (i.e. --parallel-set includes N=1). Row-count differential also gated.
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
elif [[ "$MBIAS_GATE_APPLIES" -eq 1 && "$MBIAS_BASELINE_OK" -eq 0 ]]; then
  EXIT=1
  REASON="FAIL: M-bias baseline 5712 B drift (or missing file) at (D, N=1) cell"
elif [[ "$MBIAS_GATE_APPLIES" -eq 1 && "$ROW_COUNT_OK" -eq 0 ]]; then
  EXIT=1
  REASON="FAIL: M-bias row-count differential violated across ignore-flag cells"
elif [[ "$PERF_TARGET_MET" -eq 0 ]]; then
  EXIT=3
  REASON="WARN: byte-identity PASSED but Rust scaling missed §9.7's 4× target"
else
  EXIT=0
  REASON="PASS: all cells byte-identical, cross-N invariant holds, M-bias baseline + row-count OK, perf target met"
fi

echo "Verdict: $REASON (exit $EXIT)" >> "$VERDICT_FILE"
cat "$VERDICT_FILE"

echo ""
echo "=== Phase H SE matrix complete ==="
echo "  Output dir:      $OUT_DIR"
echo "  Speedup table:   $SPEEDUP_TABLE"
echo "  Cross-N summary: $CROSS_N_SUMMARY"
echo "  Matrix verdict:  $VERDICT_FILE"
echo "  Exit code:       $EXIT"

exit "$EXIT"
