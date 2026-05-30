#!/usr/bin/env bash
# bedgraph_byte_identity.sh — real-data byte-identity gate for
# bismark2bedGraph_rs vs Perl bismark2bedGraph v0.25.1 (Phase 6, SPEC §10).
#
# Contract (SPEC §1.1 D1): compares DECOMPRESSED content. It NEVER diffs raw
# .gz bytes (zlib ≠ GNU gzip deflate; only the content is contracted).
#
# Invariants enforced here (review findings C1 / D2 / I3):
#   * Identical argv file list AND order to Perl and Rust (one ordered list
#     built once; never re-globbed per producer).
#   * Baseline runs in DEFAULT mode — no --ample_memory / --gazillion /
#     --buffer_size (those are accepted-but-ignored; not part of the gate).
#   * LC_ALL=C pinned for Perl's internal UNIX-sort collation parity.
#
# Usage:
#   bedgraph_byte_identity.sh <input_dir> [bedgraph flags...]
#
#   <input_dir>  directory containing the methylation-extractor call files
#                (CpG_*; plus CHG_*/CHH_* when --CX is passed).
# Env:
#   PERL_BG   path to Perl bismark2bedGraph     (default: bismark2bedGraph on PATH)
#   RUST_BG   path to bismark2bedGraph_rs       (default: bismark2bedGraph_rs on PATH)
#   OUTROOT   working/output root               (default: a fresh mktemp dir)
#
# Exit: 0 = byte-identical on all produced streams; 1 = a stream differed;
#       2 = setup error.
set -euo pipefail

die() { echo "error: $*" >&2; exit 2; }

[ $# -ge 1 ] || die "usage: $0 <input_dir> [flags...]"
IN_DIR="$1"; shift
FLAGS=("$@")
[ -d "$IN_DIR" ] || die "input dir not found: $IN_DIR"

PERL_BG="${PERL_BG:-bismark2bedGraph}"
RUST_BG="${RUST_BG:-bismark2bedGraph_rs}"
command -v "$PERL_BG" >/dev/null 2>&1 || [ -x "$PERL_BG" ] || die "Perl bismark2bedGraph not found: $PERL_BG"
command -v "$RUST_BG" >/dev/null 2>&1 || [ -x "$RUST_BG" ] || die "bismark2bedGraph_rs not found: $RUST_BG"

# Reject the modes that are out of the byte-identity contract (D2/D3).
for f in "${FLAGS[@]}"; do
  case "$f" in
    --gazillion|--scaffolds|--ample_memory|--buffer_size)
      die "flag '$f' is accepted-but-ignored and NOT part of the byte-identity gate (SPEC D2/D3)";;
  esac
done

# Build ONE ordered file list (sorted for determinism) used for BOTH producers.
want_cx=0
for f in "${FLAGS[@]}"; do [ "$f" = "--CX" ] || [ "$f" = "--CX_context" ] && want_cx=1; done
shopt -s nullglob
if [ "$want_cx" -eq 1 ]; then
  mapfile -t FILES < <(cd "$IN_DIR" && ls -1 CpG_*.txt* CHG_*.txt* CHH_*.txt* 2>/dev/null | LC_ALL=C sort)
else
  mapfile -t FILES < <(cd "$IN_DIR" && ls -1 CpG_*.txt* 2>/dev/null | LC_ALL=C sort)
fi
shopt -u nullglob
[ "${#FILES[@]}" -ge 1 ] || die "no input call files found in $IN_DIR (CX=$want_cx)"

OUTROOT="${OUTROOT:-$(mktemp -d)}"
P_DIR="$OUTROOT/perl"; R_DIR="$OUTROOT/rust"
rm -rf "$P_DIR" "$R_DIR"; mkdir -p "$P_DIR" "$R_DIR"
for f in "${FILES[@]}"; do cp "$IN_DIR/$f" "$P_DIR/"; cp "$IN_DIR/$f" "$R_DIR/"; done

echo "files (identical order to both): ${FILES[*]}"
echo "flags: ${FLAGS[*]:-(none)}"

( cd "$P_DIR" && LC_ALL=C perl "$PERL_BG" "${FLAGS[@]}" -o out.bedGraph "${FILES[@]}" ) >/dev/null
( cd "$R_DIR" && LC_ALL=C "$RUST_BG"      "${FLAGS[@]}" -o out.bedGraph "${FILES[@]}" ) >/dev/null

rc=0
# Decompressed gz streams.
for gz in out.bedGraph.gz out.bismark.cov.gz out.bedGraph_UCSC.bedGraph.gz; do
  if [ -f "$P_DIR/$gz" ] || [ -f "$R_DIR/$gz" ]; then
    if [ ! -f "$P_DIR/$gz" ] || [ ! -f "$R_DIR/$gz" ]; then
      echo "DIFFER: $gz produced by only one side (perl=$([ -f "$P_DIR/$gz" ]&&echo y||echo n) rust=$([ -f "$R_DIR/$gz" ]&&echo y||echo n))"; rc=1; continue
    fi
    gunzip -c "$P_DIR/$gz" > "$OUTROOT/p.txt"; gunzip -c "$R_DIR/$gz" > "$OUTROOT/r.txt"
    if diff -q "$OUTROOT/p.txt" "$OUTROOT/r.txt" >/dev/null; then echo "OK (decompressed): $gz"; else echo "DIFFER (decompressed): $gz"; diff "$OUTROOT/p.txt" "$OUTROOT/r.txt" | head; rc=1; fi
  fi
done
# Plain zero file.
z=out.bedGraph.gz.bismark.zero.cov
if [ -f "$P_DIR/$z" ] || [ -f "$R_DIR/$z" ]; then
  if diff -q "$P_DIR/$z" "$R_DIR/$z" >/dev/null 2>&1; then echo "OK (plain): $z"; else echo "DIFFER: $z"; rc=1; fi
fi

[ $rc -eq 0 ] && echo "RESULT: byte-identical (decompressed) ✓" || echo "RESULT: differences found ✗"
exit $rc
