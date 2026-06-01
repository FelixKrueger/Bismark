#!/usr/bin/env bash
# Phase C real-data byte-identity gate: Perl NOMe_filtering vs Rust
# NOMe_filtering_rs on a common real `--yacht` input + genome. The DECOMPRESSED
# outputs must be byte-identical (the gzip container is impl-dependent — Perl
# `gzip -c` vs Rust `flate2` — so we compare post-decompression; SPEC §6 / P8).
#
# Usage:
#   PERL_NOME=~/Bismark/NOMe_filtering \
#   RUST_NOME=~/nome-build/rust/target/release/NOMe_filtering_rs \
#   nome_gate.sh <genome_dir> <yacht_file[.gz]>
#
# NOTE on invocation: Perl checks the input's existence at the LAUNCH cwd but
# opens it relative to --dir. The only pattern that satisfies both Perl and the
# Rust port is the extractor's real one — launch from the (per-side) output dir
# with a BARE filename and no --dir. So we symlink the input into each side's
# dir and `cd` in. (Verified on oxy 2026-06-01: 10M SE, byte-identical.)
set -euo pipefail
export LC_ALL=C
: "${PERL_NOME:?set PERL_NOME=path/to/Bismark/NOMe_filtering}"
: "${RUST_NOME:?set RUST_NOME=path/to/NOMe_filtering_rs}"
GENOME=${1:?usage: nome_gate.sh <genome_dir> <yacht_file>}
YACHT=${2:?usage: nome_gate.sh <genome_dir> <yacht_file>}

OUT=${OUT:-$PWD/nome_gate_out}
rm -rf "$OUT"; mkdir -p "$OUT/perl" "$OUT/rust"
base=$(basename "$YACHT")
abs=$(readlink -f "$YACHT")
ln -sf "$abs" "$OUT/perl/$base"
ln -sf "$abs" "$OUT/rust/$base"

( cd "$OUT/perl" && /usr/bin/time -v perl "$PERL_NOME" -g "$GENOME" "$base" ) > "$OUT/perl.log" 2>&1
( cd "$OUT/rust" && /usr/bin/time -v "$RUST_NOME"      -g "$GENOME" "$base" ) > "$OUT/rust.log" 2>&1

# Output name = NOMe derivation: strip one .gz, one .txt, append .manOwar.txt.gz.
out="${base%.gz}"; out="${out%.txt}.manOwar.txt.gz"
if cmp <(gunzip -c "$OUT/perl/$out") <(gunzip -c "$OUT/rust/$out"); then
  echo "PASS  byte-identical  md5=$(gunzip -c "$OUT/rust/$out" | md5sum | cut -c1-32)  lines=$(gunzip -c "$OUT/rust/$out" | wc -l)"
else
  echo "FAIL  outputs differ — preserved at $OUT"; exit 1
fi
