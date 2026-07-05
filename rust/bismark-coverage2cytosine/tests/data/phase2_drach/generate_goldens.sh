#!/usr/bin/env bash
# Regenerate Phase-2 (v1.x --drach/--m6A) byte-identity goldens from the repo's
# Perl coverage2cytosine (v0.25.1, self-contained). Run from this directory
# (tests/data/phase2_drach). Creates the tiny fixtures AND the per-mode Perl
# golden output dirs under ./gold/ — full provenance.
#
# ⚠️ SLOW: Perl's generate_DRACH_report sleeps 20 s at the start of every run
# (the warn/sleep banner — STDERR, exempt from byte-identity). ~9 runs ≈ 3 min.
#
# gzip / --zero_based / --CX modes have NO separate golden: the test decompresses
# (gzip) or directly compares (zero/CX) against the matching plain golden, since
# --drach ignores --zero_based and --CX (early-exit) — same discipline as phase1.
set -eo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
C2C="$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine"
cd "$HERE"

mkfa() { mkdir -p "$1"; printf '%b' "$2" > "$1/genome.fa"; }
mkfa g_top   ">chr1\nTTTGAACATTTGTACATTTGAACGTTTGAACNTTTCAACATTT\n"  # top filter arms + non-ACGT pos5
mkfa g_bot   ">chr1\nAAATGTTCAAAGTACGTACGT\n"                          # bottom strand, pos-1 anchor
mkfa g_wrap  ">chrA\nACAAA\n"                                         # top-strand pos<4 wrap (V15)
mkfa g_trunc ">chrT\nAAAGTA\n"                                        # bottom truncated-5-mer emit (V10)
mkfa g_multi ">chr1\nTTTGAACATTT\n>chr2\nAAATGTTCAAA\n"               # split + single-file ordering

printf 'chr1\t7\t7\t50\t4\t2\nchr1\t15\t15\t50\t3\t3\nchr1\t23\t23\t50\t9\t0\nchr1\t31\t31\t50\t1\t1\nchr1\t39\t39\t50\t2\t2\n' > top.cov
printf 'chr1\t5\t5\t50\t3\t3\nchr1\t12\t12\t50\t1\t0\nchr1\t16\t16\t50\t2\t2\nchr1\t20\t20\t50\t5\t0\n' > bot.cov
printf 'chrA\t2\t2\t90\t9\t1\n' > wrap.cov
printf 'chrT\t4\t4\t100\t5\t0\n' > trunc.cov
printf 'chr1\t7\t7\t50\t4\t2\nchr2\t5\t5\t50\t3\t3\n' > multi.cov
printf 'chr2\t5\t5\t50\t3\t3\nchr1\t7\t7\t50\t4\t2\n' > multi_rev.cov   # single-file: chr2 before chr1
: > empty.cov

rm -rf gold; mkdir -p gold
gen() { # mode genome cov oname flags...
  local mode="$1" g="$2" cov="$3" on="$4"; shift 4
  local d="gold/$mode"; mkdir -p "$d"
  perl "$C2C" -o "$on" -g "$g" --dir "$d" "$@" "$cov" >/dev/null 2>&1
}

gen top          g_top   top.cov       s                 --drach                       # V5 (top report+cov)
gen bottom       g_bot   bot.cov       s                 --drach                       # V6 (bottom, pos-1)
gen wrap         g_wrap  wrap.cov      s                 --drach                       # V15 (top pos<4 wrap)
gen trunc        g_trunc trunc.cov     s                 --drach                       # V10 (bottom truncation)
gen thr5         g_top   top.cov       s                 --drach --coverage_threshold 5 # V2 (explicit threshold)
gen rawsuffix    g_top   top.cov       s.CpG_report.txt  --drach                       # V4 (raw -o no strip)
gen split        g_multi multi.cov     s                 --drach --split_by_chromosome  # V12 (per-chr + ordering)
gen single_order g_multi multi_rev.cov s                 --drach                       # V16 (single-file ordering)
gen empty        g_top   empty.cov     s                 --drach                       # V8 (empty cov → empty files)

echo "Phase-2 DRACH goldens regenerated under $HERE/gold/"
