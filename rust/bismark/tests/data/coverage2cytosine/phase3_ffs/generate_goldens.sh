#!/usr/bin/env bash
# Regenerate Phase-3 (v1.x --ffs) byte-identity goldens from the repo's Perl
# coverage2cytosine (v0.25.1, self-contained). Run from this directory
# (tests/data/phase3_ffs). Creates the tiny fixtures AND the per-mode Perl
# golden output dirs under ./gold/ — full provenance.
#
# --ffs --gzip has NO separate golden (the test decompresses + compares to the
# plain ffs_cpg golden). --ffs --zero_based DOES get a golden (the core report's
# pos shifts; only the 3 context columns are coordinate-invariant).
set -eo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
C2C="$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine"
cd "$HERE"

mkfa() { mkdir -p "$1"; printf '%b' "$2" > "$1/genome.fa"; }
# Main fixture (PLAN V0): chr1 + chrM covered, chrC UNCOVERED → exercises the
# uncovered-chromosome pass emitting 10-col `0 0` lines (V13) + the forward-hexa
# negative-wrap on chrC i=0 (hexa=CC).
mkfa g_main  ">chr1\nGCCGTGAAACACGGCTTT\n>chrM\nAACGCCAAGGCC\n>chrC\nCGTAAACCC\n"
# CpG-pair fixture for the --merge_CpGs interaction (V6).
mkfa g_merge ">chr1\nAACGTTAACGTT\n"
# N-containing fixture: ffs windows span an N (V15 — Perl does NOT filter Ns).
mkfa g_nwin  ">chrN\nACGNTGCGNAACG\n"
# NOMe fixture: covered ACG/TCG-upstream CpGs (so the NOMe core report + cov
# would normally be non-empty). Exercises the --ffs × --nome-seq interaction:
# under --ffs the NOMe `.cov` companion is SUPPRESSED (0-byte) — Perl's $tetra
# branch short-circuits before `print CYTCOV` (Phase-3 dual-review Critical).
mkfa g_nome  ">chr1\nTTACGTTAGCATCGTT\n"

printf 'chr1\t3\t3\t50\t4\t2\nchr1\t12\t12\t50\t1\t1\nchrM\t3\t3\t50\t2\t0\n' > main.cov
printf 'chr1\t3\t3\t50\t8\t2\nchr1\t4\t4\t50\t1\t1\nchr1\t9\t9\t50\t3\t3\nchr1\t10\t10\t50\t0\t4\n' > merge.cov
printf 'chrN\t1\t1\t50\t3\t1\nchrN\t6\t6\t50\t2\t2\nchrN\t12\t12\t50\t5\t0\n' > nwin.cov
printf 'chr1\t4\t4\t75\t3\t1\nchr1\t5\t5\t50\t1\t1\nchr1\t13\t13\t100\t9\t0\n' > nome.cov

rm -rf gold; mkdir -p gold
gen() { # mode genome cov oname flags...
  local mode="$1" g="$2" cov="$3" on="$4"; shift 4
  local d="gold/$mode"; mkdir -p "$d"
  perl "$C2C" -o "$on" -g "$g" --dir "$d" "$@" "$cov" >/dev/null 2>&1
}

gen ffs_cpg    g_main  main.cov  s --ffs                          # V8 (CpG; incl. uncovered chrC 10-col + hexa wrap)
gen ffs_cx     g_main  main.cov  s --ffs --CX                     # V9 (CX, 10-col across CG/CHG/CHH)
gen ffs_zero   g_main  main.cov  s --ffs --zero_based             # V10 (pos shifts; context cols frozen)
gen ffs_split  g_main  main.cov  s --ffs --split_by_chromosome    # V11 (per-chr 10-col)
gen ffs_merge  g_merge merge.cov s --ffs --merge_CpGs             # V6 (merged cov drops ffs cols)
gen plain_merge g_merge merge.cov s --merge_CpGs                  # V6 baseline (no ffs)
gen ffs_nwin   g_nwin  nwin.cov  s --ffs --CX                     # V15 (N-window emitted verbatim)
gen ffs_nome   g_nome  nome.cov  s --ffs --nome-seq               # V16 (--ffs × --nome-seq: NOMe .cov suppressed → 0-byte)

echo "Phase-3 FFS goldens regenerated under $HERE/gold/"
