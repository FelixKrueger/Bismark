#!/usr/bin/env bash
#
# c2c_byte_identity_matrix.sh — bismark-coverage2cytosine release gate.
#
# Runs the Rust `coverage2cytosine_rs` and Perl `coverage2cytosine` (v0.25.1)
# over a 15-cell representative flag matrix against a Perl-bismark2bedGraph
# `.bismark.cov.gz` + genome, and asserts RAW-BYTE-IDENTITY Rust≡Perl on every
# in-scope output stream (gzip compared after decompression). A clean pass
# (exit 0) gates the release tag.
#   - v1.0 (Phase E): the 9 core-report cells (cx/default/zero/gzip/thr/split/
#     merge/merge_disc/merge_gzip) → gated `bismark-coverage2cytosine-v1.0`.
#   - v1.x (Phase 4): + 6 niche-mode cells (gc/nome/drach/ffs/ffs_cx/ffs_nome)
#     → gates the v1.x tag. ⚠️ The gate cov MUST be `bismark2bedGraph --CX`
#     (all-context): the gc-cell GpC require-nonempty depends on covered
#     GpC-context Cs (PLAN §8). The NOMe *GpC* streams are existence-only.
#
# Design: plans/05292026_bismark-coverage2cytosine/phase-e-byte-identity-gate/PLAN.md (rev 1)
#       + plans/05312026_bismark-c2c-niche-modes/phase4-byte-identity-gate/PLAN.md (rev 1).
# Models scripts/phase_h_se_matrix.sh; FAIL-CLOSED throughout (the count_mbias_rows lesson).
#
# Usage:
#   scripts/c2c_byte_identity_matrix.sh <COV_GZ> --genome <DIR> [options]
#     <COV_GZ>          Perl-bismark2bedGraph *.bismark.cov.gz (required, positional)
#     --genome <DIR>    FASTA genome folder (required)
#     --out <DIR>       output dir (default ./c2c_byte_identity_out; must be empty/absent)
#     --cells "a b c"   subset of cell names to run (default: all; cx runs first)
#     --disk-floor-gb N pre-flight + per-cell free-space floor in GiB (default 30)
#     --keep-all        keep large outputs even on PASS (default: purge on pass)
#     --perl-c2c PATH   Perl coverage2cytosine (default: $PERL_C2C or repo-root ./coverage2cytosine)
#     --rust-c2c PATH   Rust binary (default: $RUST_C2C or rust/target/release/coverage2cytosine_rs)
#     -h|--help
#
# Exit codes:
#   0  all cells + all differential checks byte-identical / satisfied
#   1  any byte-diff, missing/empty-where-required output, gzip-integrity failure, or differential violation
#   2  pre-flight / usage error
#
# Recommended: run inside tmux/screen (full-genome matrix is multi-hour).

set -euo pipefail

# ── bash >= 4 (associative arrays + modern set -u idioms) ──
if (( ${BASH_VERSINFO[0]} < 4 )); then
  echo "error: bash >= 4.0 required (current: $BASH_VERSION)" >&2
  echo "       macOS default /bin/bash is 3.2; install via 'brew install bash' and" >&2
  echo "       re-run with /opt/homebrew/bin/bash. oxy/Linux ships bash 5.x." >&2
  exit 2
fi

OUT_DIR="./c2c_byte_identity_out"  # set early so the trap can reference it
trap 'echo "" >&2; echo "interrupted; partial matrix output in $OUT_DIR (preserved for evidence)" >&2; exit 130' INT TERM

# ─── Args ──────────────────────────────────────────────────────────────
COV_GZ=""
GENOME=""
CELLS_ARG=""
DISK_FLOOR_GB=30
KEEP_ALL=0
PERL_C2C="${PERL_C2C:-}"
RUST_C2C="${RUST_C2C:-}"

while [[ $# -gt 0 ]]; do
  case $1 in
    --genome)        GENOME="$2"; shift 2 ;;
    --out)           OUT_DIR="$2"; shift 2 ;;
    --cells)         CELLS_ARG="$2"; shift 2 ;;
    --disk-floor-gb) DISK_FLOOR_GB="$2"; shift 2 ;;
    --keep-all)      KEEP_ALL=1; shift ;;
    --perl-c2c)      PERL_C2C="$2"; shift 2 ;;
    --rust-c2c)      RUST_C2C="$2"; shift 2 ;;
    -h|--help)       sed -n '2,/^$/p' "$0"; exit 0 ;;
    *)
      if [[ -z "$COV_GZ" ]]; then COV_GZ="$1"; shift
      else echo "error: unexpected arg: $1" >&2; exit 2; fi ;;
  esac
done

usage_err() { echo "error: $1" >&2; exit 2; }

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# --disk-floor-gb must be a non-negative integer — else the later `set -u`
# arithmetic comparison aborts as exit 1 (reads as a byte-FAIL) instead of a
# clean usage error (B-M1).
[[ "$DISK_FLOOR_GB" =~ ^[0-9]+$ ]] || usage_err "--disk-floor-gb must be a non-negative integer (got: $DISK_FLOOR_GB)"

# ─── Pre-flight gates (§3.1) ───────────────────────────────────────────

# 1. bash version — done above.

# 2. COV_GZ readable + .gz suffix
[[ -n "$COV_GZ" ]] || usage_err "missing <COV_GZ>. Usage: $0 <COV_GZ> --genome <DIR> [options]"
[[ -r "$COV_GZ" ]] || usage_err "coverage file not readable: $COV_GZ"
[[ "$COV_GZ" == *.gz ]] || usage_err "coverage file must be gzipped (*.bismark.cov.gz): $COV_GZ"
COV_GZ="$(cd "$(dirname "$COV_GZ")" && pwd)/$(basename "$COV_GZ")"

# 3. genome dir readable + has a FASTA (the four-suffix c2c set; SPEC §6.1)
[[ -n "$GENOME" ]] || usage_err "missing --genome <DIR>"
[[ -d "$GENOME" && -r "$GENOME" ]] || usage_err "genome dir not readable: $GENOME"
GENOME="$(cd "$GENOME" && pwd)"
shopt -s nullglob
_fastas=( "$GENOME"/*.fa "$GENOME"/*.fa.gz "$GENOME"/*.fasta "$GENOME"/*.fasta.gz )
shopt -u nullglob
(( ${#_fastas[@]} > 0 )) || usage_err "no *.fa/*.fa.gz/*.fasta/*.fasta.gz in genome dir: $GENOME"

# 4. --out empty-or-absent
if [[ -e "$OUT_DIR" ]]; then
  if [[ -d "$OUT_DIR" ]]; then
    [[ -z "$(ls -A "$OUT_DIR" 2>/dev/null)" ]] || usage_err "--out dir is not empty: $OUT_DIR (pass a fresh dir to preserve prior evidence)"
  else
    usage_err "--out path exists and is not a directory: $OUT_DIR"
  fi
fi
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

# 5. Perl coverage2cytosine present + version == v0.25.1 (§3.1.5; A-M1/B-M1).
#    The c2c banner prints "coverage2cytosine" + a line "Version: v0.25.1" —
#    NOT the extractor's "Bismark Extractor Version:" format.
[[ -n "$PERL_C2C" ]] || PERL_C2C="$REPO_ROOT/coverage2cytosine"
[[ -r "$PERL_C2C" ]] || usage_err "Perl coverage2cytosine not found: $PERL_C2C (set --perl-c2c or \$PERL_C2C; on oxy use the bismark-test env binary)"
PERL_VERS_OUT="$("$PERL_C2C" --version 2>&1 || true)"
if ! { grep -q 'coverage2cytosine' <<<"$PERL_VERS_OUT" && grep -qE 'Version: v0\.25\.1[[:space:]]*$' <<<"$PERL_VERS_OUT"; }; then
  echo "error: expected Perl coverage2cytosine v0.25.1; --version did not match." >&2
  echo "       Got:" >&2; sed 's/^/         /' <<<"$PERL_VERS_OUT" >&2
  echo "       The byte-identity contract assumes v0.25.1." >&2
  exit 2
fi

# 6. Rust binary discoverable (build --release on demand)
if [[ -z "$RUST_C2C" ]]; then
  RUST_C2C="$REPO_ROOT/rust/target/release/coverage2cytosine_rs"
  if [[ ! -x "$RUST_C2C" ]]; then
    echo "==> building Rust coverage2cytosine_rs (--release) ..." >&2
    ( cd "$REPO_ROOT/rust" && cargo build --release -p bismark ) >&2 \
      || usage_err "cargo build --release failed"
  fi
fi
[[ -x "$RUST_C2C" ]] || usage_err "Rust binary not executable: $RUST_C2C"
RUST_VERSION="$("$RUST_C2C" --version 2>&1 | head -1 || echo '(unknown)')"
GIT_HEAD="$(cd "$REPO_ROOT" && git rev-parse HEAD 2>/dev/null || echo '(unknown)')"

# 7. Disk-headroom gate (the oxy cap; §3.1.7). free GiB on the --out filesystem.
free_gb() { df -Pk "$1" 2>/dev/null | awk 'NR==2 {print int($4/1024/1024)}'; }
disk_check() {  # $1 = context label
  local g; g="$(free_gb "$OUT_DIR")"
  if [[ -z "$g" ]]; then echo "warning: could not measure free disk on $OUT_DIR ($1)" >&2; return 0; fi
  if (( g < DISK_FLOOR_GB )); then
    echo "error: only ${g} GiB free on $OUT_DIR; floor is ${DISK_FLOOR_GB} GiB ($1)." >&2
    echo "       c2c output is genome-driven (full-hg38 CX report is tens of GB); free space or" >&2
    echo "       lower --disk-floor-gb only if you understand the footprint. Retained FAIL evidence" >&2
    echo "       under --keep-all may be consuming space." >&2
    return 1
  fi
  return 0
}
disk_check "pre-flight" || exit 2

# 8. LC_ALL=C — belt-and-suspenders (Perl + Rust sorts are already bytewise; §3.1.8/B-M2).
export LC_ALL=C

# 9. tmux/screen advisory
if [[ -z "${TMUX:-}" && -z "${STY:-}" ]]; then
  echo "warning: not in tmux/screen. The full-genome matrix is multi-hour; an SSH drop would" >&2
  echo "         orphan subprocesses. Recommended: tmux new -s c2c_release, then re-run." >&2
fi

# ─── Matrix definition (§3.2). cx first for disk (§3.7). ────────────────
# "name|flags" — flags are passed identically to Perl and Rust.
declare -a ALL_CELLS=(
  "cx|--CX --gzip"
  "default|"
  "zero|--zero_based"
  "gzip|--gzip"
  "thr|--coverage_threshold 5"
  "split|--split_by_chromosome"
  "merge|--merge_CpGs"
  "merge_disc|--merge_CpGs --discordance_filter 10"
  "merge_gzip|--merge_CpGs --gzip"
  # ── v1.x niche modes (Phase 4) ──
  "gc|--gc"
  "nome|--nome-seq"
  "drach|--drach"
  "ffs|--ffs"
  "ffs_cx|--ffs --CX --gzip"
  "ffs_nome|--ffs --nome-seq"
)

# Per-cell REQUIRE-NONEMPTY globs (Perl-side ground truth must have content).
# Streams NOT listed are existence-only / empty-tolerant (§3.4.1): the discordant
# file, the merge_disc merged-cov, and split per-chr reports (short scaffolds).
# NOTE (B-M4): these names hardcode the `c2c` stem — every cell runs `-o c2c` (below).
# The merged/discordant names are report-derived (`{stem}.CpG_report.merged_CpG_evidence.cov[.gz]`,
# Perl combine_CpGs:1766; matches report.rs merged_cov_name). If the fixed `-o c2c`
# is ever parameterised, update these patterns too, or the backstop stops matching.
declare -A REQUIRE_NONEMPTY=(
  [cx]="c2c.CX_report.txt.gz c2c.cytosine_context_summary.txt"
  [default]="c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
  [zero]="c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
  [gzip]="c2c.CpG_report.txt.gz c2c.cytosine_context_summary.txt"
  [thr]="c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
  [split]=""
  [merge]="c2c.CpG_report.merged_CpG_evidence.cov c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
  [merge_disc]="c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
  [merge_gzip]="c2c.CpG_report.merged_CpG_evidence.cov.gz c2c.cytosine_context_summary.txt"
  # ── v1.x niche modes (Phase 4). The NOMe *GpC* streams are NOT listed: they are
  # existence-only (validated by the file-set match + byte-compare), since their
  # non-emptiness depends on covered non-CG GpC positions (Assumption 8 / PLAN
  # §3.2). The gc-cell GpC streams ARE required (no ACG/TCG filter + the all-context
  # --CX gate cov ⇒ every covered GC emits). ffs_nome's .NOMe.CpG.cov is the
  # SUPPRESSED 0-byte file (rev-3 Critical) — required-EMPTY, so not listed here.
  [gc]="c2c.GpC_report.txt c2c.GpC.cov c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
  [nome]="c2c.NOMe.CpG_report.txt c2c.NOMe.CpG.cov c2c.cytosine_context_summary.txt"
  [drach]="c2c_DRACH_report.txt c2c_DRACH.cov"
  [ffs]="c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
  [ffs_cx]="c2c.CX_report.txt.gz c2c.cytosine_context_summary.txt"
  [ffs_nome]="c2c.NOMe.CpG_report.txt c2c.cytosine_context_summary.txt"
)

# Cells to run
declare -a CELLS=()
if [[ -n "$CELLS_ARG" ]]; then
  # Reject unknown cell names up front.
  for want in $CELLS_ARG; do
    printf '%s\n' "${ALL_CELLS[@]%%|*}" | grep -qx "$want" || usage_err "unknown cell in --cells: $want"
  done
  # Iterate ALL_CELLS (canonical order, cx first) and keep the requested ones — so
  # the cx-first disk-discipline invariant holds regardless of --cells arg order (A-M1).
  for c in "${ALL_CELLS[@]}"; do
    for want in $CELLS_ARG; do [[ "${c%%|*}" == "$want" ]] && { CELLS+=("$c"); break; }; done
  done
  (( ${#CELLS[@]} > 0 )) || usage_err "no matching cells for --cells '$CELLS_ARG'"
else
  CELLS=( "${ALL_CELLS[@]}" )
fi

# ─── Helpers ───────────────────────────────────────────────────────────

# Portable content hash from stdin (oxy=Linux, dev=macOS).
_hash() {
  if   command -v shasum  >/dev/null 2>&1; then shasum -a 256 | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then sha256sum | awk '{print $1}'
  elif command -v md5sum  >/dev/null 2>&1; then md5sum | awk '{print $1}'
  else cksum | awk '{print $1"-"$2}'; fi
}
hash_plain() { _hash < "$1"; }
hash_gz()    { gzip -dc "$1" | _hash; }
lines_plain(){ wc -l < "$1" | tr -d ' '; }
lines_gz()   { gzip -dc "$1" | wc -l | tr -d ' '; }
# Non-empty test: plain → -s; gz → decompressed has ≥1 byte.
nonempty() {
  local f="$1"
  if [[ "$f" == *.gz ]]; then [[ -n "$(gzip -dc "$f" 2>/dev/null | head -c1)" ]]
  else [[ -s "$f" ]]; fi
}

# ─── Per-cell run + compare ────────────────────────────────────────────
declare -a CELL_NAMES=() CELL_VERDICT=() CELL_DETAIL=() CELL_PERL_S=() CELL_RUST_S=()
# Differential stash (set during the cell loop; §3.6).
HASH_DEFAULT="" LINES_DEFAULT="" HASH_ZERO="" HASH_GZIP_DECOMP="" LINES_CX="" LINES_THR=""
HASH_MERGE_COV="" MERGE_COV_NONEMPTY="" HASH_MERGEGZIP_COV_DECOMP="" SPLIT_FILE_COUNT=""
# v1.x niche-mode differential stash (Phase 4). Init "" so the `[[ -n "$VAR" ]]`
# guards never trip `set -u` (B-Imp-4). Captured in run_cell's case BEFORE purge.
HASH_GC_CORE="" LINES_NOME_CORE="" DRACH_STANDALONE_OK="" FFS_ALL_10COL="" LINES_FFS="" FFSNOME_COV_EMPTY=""

now_s() { date +%s; }

run_cell() {
  local name="$1" flags="$2"
  local cdir="$OUT_DIR/cell_$name"
  local rdir="$cdir/rust" pdir="$cdir/perl"
  mkdir -p "$rdir" "$pdir"

  # shellcheck disable=SC2086  # $flags is an intentional word-split flag list
  local t0 t1 perl_s rust_s
  echo "" >&2; echo "==> cell '$name'  (flags: ${flags:-<none>})" >&2

  t0=$(now_s)
  set +e
  "$PERL_C2C" -o c2c -g "$GENOME" --dir "$pdir" $flags "$COV_GZ" >"$pdir/.stdout" 2>"$pdir/.stderr"
  local prc=$?
  t1=$(now_s); perl_s=$((t1 - t0))

  t0=$(now_s)
  "$RUST_C2C" -o c2c -g "$GENOME" --dir "$rdir" $flags "$COV_GZ" >"$rdir/.stdout" 2>"$rdir/.stderr"
  local rrc=$?
  t1=$(now_s); rust_s=$((t1 - t0))
  set -e

  # Strip the stdout/stderr capture files from the comparison set.
  rm -f "$rdir/.stdout" "$rdir/.stderr" "$pdir/.stdout" "$pdir/.stderr"

  local verdict="PASS" detail=""

  # (a) file-name-set match (existence guard; missing/extra ⇒ FAIL)
  local rfiles pfiles
  rfiles="$(cd "$rdir" && ls -1 2>/dev/null | sort)"
  pfiles="$(cd "$pdir" && ls -1 2>/dev/null | sort)"
  if [[ "$rfiles" != "$pfiles" ]]; then
    verdict="FAIL"; detail="file-name-set mismatch"
    { echo "FILE-NAME-SET MISMATCH (cell $name):"; diff <(echo "$pfiles") <(echo "$rfiles") || true; } >>"$cdir/diff.txt"
  fi

  # (b) per-file byte compare (gz → integrity-test both THEN decompress-compare; §3.4.2)
  if [[ "$verdict" == "PASS" ]]; then
    local f
    while IFS= read -r f; do
      [[ -z "$f" ]] && continue
      local R="$rdir/$f" P="$pdir/$f"
      if [[ "$f" == *.gz ]]; then
        # FAIL-CLOSED: cmp <(gzip -dc ...) swallows decompress failures, so a
        # truncated gz would false-PASS. Integrity-test BOTH sides first.
        if ! gzip -t "$R" 2>/dev/null || ! gzip -t "$P" 2>/dev/null; then
          verdict="FAIL"; detail="${detail:+$detail; }gzip-integrity failed: $f"
          echo "GZIP-INTEGRITY FAIL: $f" >>"$cdir/diff.txt"; continue
        fi
        if ! cmp -s <(gzip -dc "$R") <(gzip -dc "$P"); then
          verdict="FAIL"; detail="${detail:+$detail; }byte-diff (gz): $f"
          echo "BYTE-DIFF (gz, decompressed): $f" >>"$cdir/diff.txt"
        fi
      else
        if ! cmp -s "$R" "$P"; then
          verdict="FAIL"; detail="${detail:+$detail; }byte-diff: $f"
          echo "BYTE-DIFF: $f" >>"$cdir/diff.txt"
        fi
      fi
    done <<< "$(comm -12 <(echo "$rfiles") <(echo "$pfiles"))"
  fi

  # (c) require-nonempty assertions (Perl-side ground truth; §3.4.1)
  if [[ "$verdict" == "PASS" ]]; then
    local pat
    for pat in ${REQUIRE_NONEMPTY[$name]:-}; do
      if [[ ! -e "$pdir/$pat" ]]; then
        verdict="FAIL"; detail="${detail:+$detail; }required output absent: $pat"
        echo "REQUIRED OUTPUT ABSENT (perl): $pat" >>"$cdir/diff.txt"
      elif ! nonempty "$pdir/$pat"; then
        verdict="FAIL"; detail="${detail:+$detail; }required output empty: $pat"
        echo "REQUIRED OUTPUT EMPTY (perl): $pat" >>"$cdir/diff.txt"
      fi
    done
  fi

  # (d) split-specific (the file-set + per-file compare in (a)/(b) already validate
  #     content): ≥1 per-chr report present, and exactly ONE non-empty per-chr summary
  #     (the last-processed chr; SPEC §5) — an independent assertion of the quirk the
  #     byte-compare only catches transitively (A-M2/B-M3). Uses `find` not a literal
  #     glob, so a no-match exits 0 instead of aborting the matrix under set -e (A-I1).
  if [[ "$name" == "split" && "$verdict" == "PASS" ]]; then
    SPLIT_FILE_COUNT="$(find "$pdir" -maxdepth 1 -name '*.CpG_report.txt' 2>/dev/null | wc -l | tr -d ' ')"
    if [[ "${SPLIT_FILE_COUNT:-0}" -lt 1 ]]; then
      verdict="FAIL"; detail="${detail:+$detail; }split produced no per-chr reports"
      echo "SPLIT: no per-chr reports" >>"$cdir/diff.txt"
    fi
    local ne_sum=0 s
    while IFS= read -r s; do [[ -s "$s" ]] && ne_sum=$((ne_sum + 1)); done \
      < <(find "$pdir" -maxdepth 1 -name '*.cytosine_context_summary.txt' 2>/dev/null)
    if [[ "$ne_sum" -ne 1 ]]; then
      verdict="FAIL"; detail="${detail:+$detail; }split: expected exactly 1 non-empty per-chr summary, found $ne_sum"
      echo "SPLIT SUMMARY COUNT: $ne_sum non-empty (expected 1)" >>"$cdir/diff.txt"
    fi
  fi

  # (e) binary exit codes (B-I1): a non-zero exit means the output is suspect even
  #     if it happens to match (e.g. disk-full after a complete write). Fail-CLOSED;
  #     surfaces what a byte-compare alone would silently pass.
  if [[ "$verdict" == "PASS" && ( "$prc" -ne 0 || "$rrc" -ne 0 ) ]]; then
    verdict="FAIL"; detail="${detail:+$detail; }nonzero exit (perl_rc=$prc rust_rc=$rrc)"
    echo "NONZERO EXIT: perl_rc=$prc rust_rc=$rrc" >>"$cdir/diff.txt"
  fi

  # ── Stash differential inputs while files still exist (BEFORE purge; §3.6). ──
  case "$name" in
    default) [[ -f "$pdir/c2c.CpG_report.txt" ]] && { HASH_DEFAULT="$(hash_plain "$pdir/c2c.CpG_report.txt")"; LINES_DEFAULT="$(lines_plain "$pdir/c2c.CpG_report.txt")"; } ;;
    zero)    [[ -f "$pdir/c2c.CpG_report.txt" ]] && HASH_ZERO="$(hash_plain "$pdir/c2c.CpG_report.txt")" ;;
    gzip)    [[ -f "$pdir/c2c.CpG_report.txt.gz" ]] && HASH_GZIP_DECOMP="$(hash_gz "$pdir/c2c.CpG_report.txt.gz")" ;;
    cx)      [[ -f "$pdir/c2c.CX_report.txt.gz" ]] && LINES_CX="$(lines_gz "$pdir/c2c.CX_report.txt.gz")" ;;
    thr)     [[ -f "$pdir/c2c.CpG_report.txt" ]] && LINES_THR="$(lines_plain "$pdir/c2c.CpG_report.txt")" ;;
    merge)   if [[ -f "$pdir/c2c.CpG_report.merged_CpG_evidence.cov" ]]; then
               HASH_MERGE_COV="$(hash_plain "$pdir/c2c.CpG_report.merged_CpG_evidence.cov")"
               nonempty "$pdir/c2c.CpG_report.merged_CpG_evidence.cov" && MERGE_COV_NONEMPTY=1 || MERGE_COV_NONEMPTY=0
             fi ;;
    merge_gzip) [[ -f "$pdir/c2c.CpG_report.merged_CpG_evidence.cov.gz" ]] && HASH_MERGEGZIP_COV_DECOMP="$(hash_gz "$pdir/c2c.CpG_report.merged_CpG_evidence.cov.gz")" ;;
    # ── v1.x niche modes (Phase 4) — capture BEFORE the purge below (B-Imp-2). ──
    gc)   [[ -f "$pdir/c2c.CpG_report.txt" ]] && HASH_GC_CORE="$(hash_plain "$pdir/c2c.CpG_report.txt")" ;;
    nome) [[ -f "$pdir/c2c.NOMe.CpG_report.txt" ]] && LINES_NOME_CORE="$(lines_plain "$pdir/c2c.NOMe.CpG_report.txt")" ;;
    drach)
      # Standalone: a DRACH report present AND no normal CpG report / summary.
      if [[ -f "$pdir/c2c_DRACH_report.txt" && ! -e "$pdir/c2c.CpG_report.txt" && ! -e "$pdir/c2c.cytosine_context_summary.txt" ]]; then
        DRACH_STANDALONE_OK=1; else DRACH_STANDALONE_OK=0; fi ;;
    ffs)
      # 10 columns on EVERY line (NF!=10 anywhere → not-all-10) + line-count.
      if [[ -f "$pdir/c2c.CpG_report.txt" ]]; then
        if awk -F'\t' 'NF!=10{exit 1}' "$pdir/c2c.CpG_report.txt" 2>/dev/null; then FFS_ALL_10COL=1; else FFS_ALL_10COL=0; fi
        LINES_FFS="$(lines_plain "$pdir/c2c.CpG_report.txt")"
      fi ;;
    ffs_nome)
      # Present-AND-0-byte on BOTH sides (NOT a post-purge stat). Distinguishes
      # present-and-empty (PASS) from absent (the file-set match already FAILs).
      if [[ -f "$pdir/c2c.NOMe.CpG.cov" && -f "$rdir/c2c.NOMe.CpG.cov" \
            && ! -s "$pdir/c2c.NOMe.CpG.cov" && ! -s "$rdir/c2c.NOMe.CpG.cov" ]]; then
        FFSNOME_COV_EMPTY=1; else FFSNOME_COV_EMPTY=0; fi ;;
  esac

  CELL_NAMES+=("$name"); CELL_VERDICT+=("$verdict")
  CELL_DETAIL+=("${detail:-ok}"); CELL_PERL_S+=("$perl_s"); CELL_RUST_S+=("$rust_s")
  echo "    perl=${perl_s}s rust=${rust_s}s (perl_rc=$prc rust_rc=$rrc) → $verdict ${detail:+[$detail]}" >&2

  # ── Disk discipline (§3.7): purge large outputs on PASS; keep on FAIL. ──
  if [[ "$verdict" == "PASS" && "$KEEP_ALL" -eq 0 ]]; then
    find "$rdir" "$pdir" -type f \( -name '*report*.txt*' -o -name '*.cov' -o -name '*.cov.gz' \) -delete 2>/dev/null || true
  fi
}

# ─── Run the matrix (disk re-check before each cell; §3.7) ──────────────
echo "==> c2c byte-identity matrix: ${#CELLS[@]} cells" >&2
echo "    cov:    $COV_GZ" >&2
echo "    genome: $GENOME" >&2
echo "    out:    $OUT_DIR" >&2
echo "    perl:   $PERL_C2C (v0.25.1)" >&2
echo "    rust:   $RUST_C2C ($RUST_VERSION, $GIT_HEAD)" >&2

for cell in "${CELLS[@]}"; do
  if ! disk_check "before cell ${cell%%|*}"; then
    echo "error: aborting before cell '${cell%%|*}' — insufficient disk." >&2
    exit 2
  fi
  run_cell "${cell%%|*}" "${cell#*|}"
done

# ─── Cross-cell differential checks (§3.6) ─────────────────────────────
# These run from the stash; they catch "both binaries silently no-op a flag",
# which the per-cell Rust≡Perl compare cannot. Only checked when both relevant
# cells ran.
declare -a DIFF_RESULTS=()
diff_fail=0
diff_check() {  # $1=desc  $2=condition-result(0 ok / nonzero fail)  $3=detail
  if [[ "$2" -eq 0 ]]; then DIFF_RESULTS+=("PASS: $1${3:+ ($3)}")
  else DIFF_RESULTS+=("FAIL: $1${3:+ ($3)}"); diff_fail=1; fi
}
ran() { printf '%s\n' "${CELL_NAMES[@]}" | grep -qx "$1"; }

if ran cx && ran default && [[ -n "$LINES_CX" && -n "$LINES_DEFAULT" ]]; then
  diff_check "cx lines > default lines" "$([[ "$LINES_CX" -gt "$LINES_DEFAULT" ]] && echo 0 || echo 1)" "cx=$LINES_CX default=$LINES_DEFAULT"
fi
if ran zero && ran default && [[ -n "$HASH_ZERO" && -n "$HASH_DEFAULT" ]]; then
  diff_check "zero report != default report" "$([[ "$HASH_ZERO" != "$HASH_DEFAULT" ]] && echo 0 || echo 1)" "--zero_based must shift coordinates"
fi
if ran gzip && ran default && [[ -n "$HASH_GZIP_DECOMP" && -n "$HASH_DEFAULT" ]]; then
  diff_check "gzip decompressed == default report" "$([[ "$HASH_GZIP_DECOMP" == "$HASH_DEFAULT" ]] && echo 0 || echo 1)" "gzip must not alter content"
fi
if ran thr && ran default && [[ -n "$LINES_THR" && -n "$LINES_DEFAULT" ]]; then
  diff_check "thr lines < default lines" "$([[ "$LINES_THR" -lt "$LINES_DEFAULT" ]] && echo 0 || echo 1)" "thr=$LINES_THR default=$LINES_DEFAULT"
fi
if ran merge && [[ -n "$MERGE_COV_NONEMPTY" ]]; then
  diff_check "merge merged-cov non-empty" "$([[ "$MERGE_COV_NONEMPTY" -eq 1 ]] && echo 0 || echo 1)"
fi
if ran merge && ran merge_gzip && [[ -n "$HASH_MERGE_COV" && -n "$HASH_MERGEGZIP_COV_DECOMP" ]]; then
  diff_check "merge_gzip decompressed == merge merged-cov" "$([[ "$HASH_MERGEGZIP_COV_DECOMP" == "$HASH_MERGE_COV" ]] && echo 0 || echo 1)"
fi
if ran split && [[ -n "$SPLIT_FILE_COUNT" ]]; then
  diff_check "split produced >1 per-chr report" "$([[ "$SPLIT_FILE_COUNT" -gt 1 ]] && echo 0 || echo 1)" "files=$SPLIT_FILE_COUNT"
fi

# ── v1.x niche-mode differentials (Phase 4). ──
# #1 regression (NOT a no-op detector): --gc must leave the core report untouched.
if ran gc && ran default && [[ -n "$HASH_GC_CORE" && -n "$HASH_DEFAULT" ]]; then
  diff_check "gc core report == default core report (--gc leaves the core untouched)" "$([[ "$HASH_GC_CORE" == "$HASH_DEFAULT" ]] && echo 0 || echo 1)"
fi
# #2 --nome-seq's ACG/TCG filter drops CpGs → fewer lines than the full report.
if ran nome && ran default && [[ -n "$LINES_NOME_CORE" && -n "$LINES_DEFAULT" ]]; then
  diff_check "nome core lines != AND < default lines (ACG/TCG filter fired)" "$([[ "$LINES_NOME_CORE" != "$LINES_DEFAULT" && "$LINES_NOME_CORE" -lt "$LINES_DEFAULT" ]] && echo 0 || echo 1)" "nome=$LINES_NOME_CORE default=$LINES_DEFAULT"
fi
# #3 --drach is standalone (DRACH report present, no normal CpG report / summary).
if ran drach && [[ -n "$DRACH_STANDALONE_OK" ]]; then
  diff_check "drach standalone (DRACH report, no CpG report/summary)" "$([[ "$DRACH_STANDALONE_OK" -eq 1 ]] && echo 0 || echo 1)"
fi
# #4 --ffs report is 10-col on every line + same line-count as default.
if ran ffs && ran default && [[ -n "$FFS_ALL_10COL" && -n "$LINES_FFS" && -n "$LINES_DEFAULT" ]]; then
  diff_check "ffs report 10-col on every line + lines == default" "$([[ "$FFS_ALL_10COL" -eq 1 && "$LINES_FFS" -eq "$LINES_DEFAULT" ]] && echo 0 || echo 1)" "all10col=$FFS_ALL_10COL ffs=$LINES_FFS default=$LINES_DEFAULT"
fi
# #5 --ffs --nome-seq: the NOMe .cov companion is present-and-0-byte on both sides
#    (the rev-3 Critical; stash captured during the loop, NOT a post-purge stat).
if ran ffs_nome && [[ -n "$FFSNOME_COV_EMPTY" ]]; then
  diff_check "ffs_nome .NOMe.CpG.cov present-and-0-byte both sides (--ffs suppresses CYTCOV)" "$([[ "$FFSNOME_COV_EMPTY" -eq 1 ]] && echo 0 || echo 1)"
fi

# ─── Verdict + summaries + exit (§3.8) ─────────────────────────────────
pass=0 fail=0 usage=0
for v in "${CELL_VERDICT[@]}"; do
  case "$v" in PASS) pass=$((pass+1));; FAIL) fail=$((fail+1));; *) usage=$((usage+1));; esac
done

VERDICT_FILE="$OUT_DIR/matrix_verdict.txt"
SUMMARY_MD="$OUT_DIR/byte_identity_summary.md"
PERF_MD="$OUT_DIR/perf_table.md"
{
  echo "c2c byte-identity matrix verdict"
  echo ""
  echo "cov:    $COV_GZ"
  echo "genome: $GENOME"
  echo "perl:   $PERL_C2C (v0.25.1)"
  echo "rust:   $RUST_C2C ($RUST_VERSION, $GIT_HEAD)"
  echo ""
  echo "Per-cell:"
  for ((k=0;k<${#CELL_NAMES[@]};k++)); do
    printf '  %-12s %s  [%s]\n' "${CELL_NAMES[k]}" "${CELL_VERDICT[k]}" "${CELL_DETAIL[k]}"
  done
  echo ""
  echo "Cross-cell differential checks:"
  if (( ${#DIFF_RESULTS[@]} == 0 )); then echo "  (none ran — cell subset)"; fi
  for d in "${DIFF_RESULTS[@]:-}"; do [[ -n "$d" ]] && echo "  $d"; done
  echo ""
  echo "Cells: ${#CELL_NAMES[@]}  PASS=$pass FAIL=$fail USAGE=$usage  diff_fail=$diff_fail"
} > "$VERDICT_FILE"

{
  echo "# c2c byte-identity summary"
  echo ""
  echo "- Rust: \`$RUST_VERSION\` @ \`$GIT_HEAD\`"
  echo "- Perl: coverage2cytosine v0.25.1"
  echo "- Cells PASS/FAIL/USAGE: $pass/$fail/$usage; differential failures: $diff_fail"
  echo ""
  echo "| Cell | Verdict | Detail |"
  echo "|------|---------|--------|"
  for ((k=0;k<${#CELL_NAMES[@]};k++)); do
    echo "| ${CELL_NAMES[k]} | ${CELL_VERDICT[k]} | ${CELL_DETAIL[k]} |"
  done
} > "$SUMMARY_MD"

{
  echo "# c2c perf (informational — NOT gated)"
  echo ""
  echo "| Cell | Perl (s) | Rust (s) |"
  echo "|------|----------|----------|"
  for ((k=0;k<${#CELL_NAMES[@]};k++)); do
    echo "| ${CELL_NAMES[k]} | ${CELL_PERL_S[k]} | ${CELL_RUST_S[k]} |"
  done
} > "$PERF_MD"

cat "$VERDICT_FILE"
echo ""
echo "=== matrix complete: verdict=$VERDICT_FILE summary=$SUMMARY_MD perf=$PERF_MD ==="

EXIT=0; REASON=""
if   (( usage > 0 ));    then EXIT=2; REASON="usage/runtime error in $usage cell(s)"
elif (( fail > 0 ));     then EXIT=1; REASON="$fail cell(s) failed byte-identity"
elif (( diff_fail > 0 ));then EXIT=1; REASON="cross-cell differential check failed"
else EXIT=0; REASON="all cells byte-identical + all differential checks satisfied"
fi
echo "Verdict: $REASON (exit $EXIT)"
exit "$EXIT"
