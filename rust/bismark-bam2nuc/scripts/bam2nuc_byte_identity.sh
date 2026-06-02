#!/usr/bin/env bash
#
# bam2nuc_byte_identity.sh — real-data byte-identity gate for bismark-bam2nuc.
#
# Runs the Rust `bam2nuc_rs` AND the Perl `bam2nuc` v0.25.1 over the same real
# genome + BAM(s) and asserts both output files are byte-for-byte identical:
#   - <sample>.nucleotide_stats.txt
#   - genomic_nucleotide_frequencies.txt
#
# Fail-CLOSED: any missing file, size mismatch, or `diff` difference → non-zero
# exit. Also confirms the `%.2f`/`%.3f` rounding contract on the TARGET platform
# (Linux/oxy): Perl and Rust are run on the same host, so any libc rounding shift
# would affect both — byte-identity holds unless Rust-core and the host libc
# disagree on a tie.
#
# Designed for oxy ([[reference_colossal_access]]):
#   dcli ssh oxy ; build the crate locally (rustup toolchain), then:
#     LC_ALL=C bash scripts/bam2nuc_byte_identity.sh
#
# Env overrides (defaults target oxy):
#   GENOME    genome FASTA dir            (default ~/bismark_benchmarks/genome)
#   SE_BAM    a single-end Bismark BAM    (REQUIRED — exact path confirmed at run time, OI-3)
#   PE_BAM    a paired-end Bismark BAM    (REQUIRED)
#   SORTED_BAM optional samtools-sorted/reprocessed BAM (covers the @PG-detection
#             divergence: prepends a non-Bismark @PG line). If unset, that cell is
#             skipped and the gate notes the omission.
#   PERL_BAM2NUC  path to Perl bam2nuc    (default <repo>/bam2nuc)
#   RUST_BAM2NUC  path to bam2nuc_rs      (default <repo>/rust/target/release/bam2nuc_rs)
#   PERL          perl interpreter        (default: PATH; prepend
#                 ~/micromamba/envs/bismark-test/bin so Perl bam2nuc v0.25.1 wins)
#   SAMTOOLS      samtools for the Perl side (default: PATH)
set -euo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

GENOME="${GENOME:-$HOME/bismark_benchmarks/genome}"
PERL_BAM2NUC="${PERL_BAM2NUC:-$REPO_ROOT/bam2nuc}"
RUST_BAM2NUC="${RUST_BAM2NUC:-$REPO_ROOT/rust/target/release/bam2nuc_rs}"
PERL="${PERL:-perl}"
SAMTOOLS="${SAMTOOLS:-$(command -v samtools || true)}"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }
[ -d "$GENOME" ]        || fail "genome dir not found: $GENOME"
[ -x "$RUST_BAM2NUC" ]  || fail "rust binary not found/executable: $RUST_BAM2NUC (cargo build --release -p bismark-bam2nuc)"
[ -f "$PERL_BAM2NUC" ]  || fail "perl bam2nuc not found: $PERL_BAM2NUC"
[ -n "$SAMTOOLS" ]      || fail "samtools not found (needed by the Perl side)"

echo "genome      : $GENOME"
echo "rust        : $RUST_BAM2NUC"
echo "perl        : $PERL_BAM2NUC  (interp: $PERL)"
echo "samtools    : $SAMTOOLS"

WORK="$(mktemp -d)"
trap 'chmod -R u+w "$WORK" 2>/dev/null || true; rm -rf "$WORK"' EXIT

# One read-only genome copy (FASTA only, NO cache) → both tools compute the cache
# independently and fall back to writing it into their own --dir (so the two
# caches can be diffed). chmod -w forces the genome-folder write to fail.
mkdir -p "$WORK/genome"
shopt -s nullglob
cp "$GENOME"/*.fa "$GENOME"/*.fasta "$GENOME"/*.fa.gz "$GENOME"/*.fasta.gz "$WORK/genome/" 2>/dev/null || true
shopt -u nullglob
[ -n "$(ls -A "$WORK/genome")" ] || fail "no FASTA files copied from $GENOME"
chmod -R a-w "$WORK/genome"

PASS=0; SKIP=0
# compare_cell <label> <perl-args...> ::: <rust-args...>  (shared --genome_folder)
run_cell () {
  local label="$1"; shift
  local pdir="$WORK/${label}_perl" rdir="$WORK/${label}_rust"
  mkdir -p "$pdir" "$rdir"
  echo "── cell: $label ──"
  PATH="$(dirname "$SAMTOOLS"):$PATH" "$PERL" "$PERL_BAM2NUC" \
      --genome_folder "$WORK/genome" --dir "$pdir/" --samtools_path "$SAMTOOLS" "$@" \
      > "$pdir/stdout.log" 2> "$pdir/stderr.log" || fail "$label: Perl run failed (see $pdir/stderr.log)"
  "$RUST_BAM2NUC" --genome_folder "$WORK/genome" --dir "$rdir/" "$@" \
      > "$rdir/stdout.log" 2> "$rdir/stderr.log" || fail "$label: Rust run failed (see $rdir/stderr.log)"
  # Compare every output file the Perl side produced.
  local diffed=0
  for pf in "$pdir"/*.txt; do
    [ -e "$pf" ] || continue
    local base; base="$(basename "$pf")"
    local rf="$rdir/$base"
    [ -f "$rf" ] || fail "$label: Rust missing output $base"
    if ! cmp -s "$pf" "$rf"; then
      echo "  DIFF in $base:"; diff "$pf" "$rf" | head -20; fail "$label: byte mismatch in $base"
    fi
    echo "  OK  $base ($(wc -c < "$pf") bytes)"
    diffed=$((diffed+1))
  done
  [ "$diffed" -gt 0 ] || fail "$label: Perl produced no output files to compare"
  PASS=$((PASS+1))
}

# Cell 1: genomic composition only (cache).
run_cell genome_comp --genomic_composition_only

# Cell 2: SE stats (+ cache).
[ -n "${SE_BAM:-}" ] || fail "set SE_BAM to a single-end Bismark BAM"
[ -f "$SE_BAM" ]     || fail "SE_BAM not found: $SE_BAM"
run_cell se "$SE_BAM"

# Cell 3: PE stats (+ cache).
[ -n "${PE_BAM:-}" ] || fail "set PE_BAM to a paired-end Bismark BAM"
[ -f "$PE_BAM" ]     || fail "PE_BAM not found: $PE_BAM"
run_cell pe "$PE_BAM"

# Cell 4 (optional): a samtools-reprocessed BAM (prepends a non-Bismark @PG) —
# exercises the detect_paired_from_header (ID:Bismark-scoped) vs Perl test_file
# (first-@PG-any-ID) divergence on real data.
if [ -n "${SORTED_BAM:-}" ] && [ -f "$SORTED_BAM" ]; then
  run_cell sorted "$SORTED_BAM"
else
  echo "── cell: sorted — SKIPPED (set SORTED_BAM to a samtools-reprocessed BAM) ──"
  SKIP=$((SKIP+1))
fi

echo
echo "GATE PASS: $PASS cell(s) byte-identical, $SKIP skipped."
