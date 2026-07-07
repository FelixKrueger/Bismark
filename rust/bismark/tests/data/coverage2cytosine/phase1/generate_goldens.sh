#!/usr/bin/env bash
# Regenerate Phase-1 (v1.x --gc/--gc_context + --nome-seq) byte-identity goldens
# from the repo's Perl coverage2cytosine (v0.25.1, self-contained). Run from this
# directory (tests/data/phase1). Creates the tiny fixtures AND the per-mode Perl
# golden output directories under ./gold/ — full provenance: re-running this
# reproduces every committed golden byte-for-byte.
#
# gzip modes (--gc --gzip, --nome-seq --gzip) have NO separate golden: the test
# decompresses the Rust .gz output and compares to the matching plain golden
# (gzip is content-invariant — same discipline as phase_d).
set -eo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
C2C="$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine"
cd "$HERE"

# ── Fixtures (committed alongside the goldens) ──
mkfa() { mkdir -p "$1"; printf '%b' "$2" > "$1/genome.fa"; }
mkfa g_primary ">chr1\nAGCAGCGCATGCGGCATTAGCTAGC\n"
mkfa g_edge    ">chr1\nGCAGCTTAGC\n"
mkfa g_gcgc    ">chr1\nAAGCGCAA\n"
mkfa g_nome    ">chr1\nTTACGTTAGCATCGTT\n"
mkfa g_multi   ">chr1\nAGCAGCGCATGCGGCATTAGCTAGC\n>chr2\nTTACGTTAGCATCGTT\n"

printf 'chr1\t2\t2\t50\t1\t1\nchr1\t3\t3\t100\t5\t0\nchr1\t6\t6\t100\t10\t0\nchr1\t7\t7\t0\t0\t5\nchr1\t8\t8\t75\t3\t1\n' > primary.cov
printf 'chr1\t1\t1\t100\t1\t0\nchr1\t2\t2\t100\t2\t0\nchr1\t4\t4\t100\t2\t0\nchr1\t5\t5\t0\t0\t3\n' > edge.cov
printf 'chr1\t3\t3\t100\t1\t0\nchr1\t4\t4\t100\t1\t0\nchr1\t5\t5\t100\t1\t0\nchr1\t6\t6\t100\t1\t0\n' > gcgc.cov
printf 'chr1\t4\t4\t75\t3\t1\nchr1\t5\t5\t50\t1\t1\nchr1\t13\t13\t100\t9\t0\n' > nome.cov
printf 'chr1\t6\t6\t100\t10\t0\nchr1\t7\t7\t0\t0\t5\nchr1\t8\t8\t75\t3\t1\nchr2\t4\t4\t75\t3\t1\nchr2\t5\t5\t50\t1\t1\nchr2\t13\t13\t100\t9\t0\n' > multi.cov
printf 'chr1\t6\t6\t100\t10\t0\nchr2\t4\t4\t75\t3\t1\nchr1\t8\t8\t75\t3\t1\n' > noncontig.cov

# ── Per-mode Perl goldens (plain output dirs under ./gold/<mode>) ──
rm -rf gold; mkdir -p gold
gen() { # $1=mode  $2=genome  $3=cov  $4=outname  $5..=flags
  local mode="$1" genome="$2" cov="$3" oname="$4"; shift 4
  local d="gold/$mode"; mkdir -p "$d"
  perl "$C2C" -o "$oname" -g "$genome" --dir "$d" "$@" "$cov" >/dev/null 2>&1
}

gen plain_primary    g_primary primary.cov   sample                            # V5/V17 (core report, no --gc)
gen gc_primary       g_primary primary.cov   sample                --gc        # V4/V5
gen gc_zero          g_primary primary.cov   sample                --gc --zero_based            # V10/V20
gen gc_thr3          g_primary primary.cov   sample                --gc --coverage_threshold 3  # B-M2
gen gc_edge          g_edge    edge.cov      sample                --gc        # V6
gen gc_gcgc          g_gcgc    gcgc.cov      sample                --gc        # V7
gen gc_rawsuffix     g_primary primary.cov   sample.CpG_report.txt --gc        # V15
gen gc_split         g_multi   multi.cov     sample                --gc --split_by_chromosome   # V9
gen gc_noncontig     g_multi   noncontig.cov sample                --gc        # V18 (single)
gen gc_noncontig_spl g_multi   noncontig.cov sample                --gc --split_by_chromosome   # V18 (split)
gen nome_primary     g_primary primary.cov   sample                --nome-seq  # V12/V13
gen nome_acgtcg      g_nome    nome.cov      sample                --nome-seq  # V11
gen nome_zero        g_nome    nome.cov      sample                --nome-seq --zero_based       # V21
gen nome_split       g_multi   multi.cov     sample                --nome-seq --split_by_chromosome  # V14/V19
gen nome_rawsuffix   g_nome    nome.cov      sample.CpG_report.txt --nome-seq  # raw-base .cov divergence

echo "Phase-1 goldens regenerated under $HERE/gold/"
