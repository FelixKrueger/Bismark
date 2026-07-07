#!/usr/bin/env bash
# Regenerate Phase-B byte-identity goldens from the repo's Perl NOMe_filtering
# (v0.25.1, self-contained Perl). Run from this directory (tests/data/phase_b).
#
# The Rust binary's DECOMPRESSED `.manOwar.txt.gz` output must be raw-byte-
# identical to each `.golden` (the gzip container itself is impl-dependent, so
# we compare post-decompression — SPEC §6/P8).
set -eo pipefail
NOME="$(cd "$(dirname "$0")/../../../../.." && pwd)/NOMe_filtering"

run() {  # <case-stem>  [extra perl flags...]
  local stem="$1"; shift
  # Perl derives the output name from the input; --dir . writes it here. Empty
  # / edge inputs may exit non-zero (D4) — tolerate it and snapshot the artifact.
  perl "$NOME" -g genome --dir . "$@" "${stem}.yacht.txt" >/dev/null 2>&1 || true
  gunzip -c "${stem}.yacht.manOwar.txt.gz" > "${stem}.golden"
  rm -f "${stem}.yacht.manOwar.txt.gz"
}

run main
run edge
run empty
run ncontext
run rev       # reverse read that COUNTS a G-strand call (col6 > col7)
run multichr  # chr2 read then chr1 read → emission order = input order, not sorted

# gz-input parity fixture: a gzipped copy of main.yacht.txt (same derived output
# name + same decompressed content as the plain input).
gzip -kf main.yacht.txt   # → main.yacht.txt.gz

echo "goldens regenerated:"
ls -l ./*.golden main.yacht.txt.gz
