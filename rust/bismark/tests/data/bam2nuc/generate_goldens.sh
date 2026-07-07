#!/usr/bin/env bash
#
# generate_goldens.sh — produce byte-identity goldens for bismark-bam2nuc by
# running the REAL Perl `bam2nuc` (v0.25.1) over synthetic fixtures.
#
# Provenance: re-running this script regenerates every fixture (genome dirs,
# SAM→BAM inputs) AND every golden byte-for-byte. The committed `goldens/` are
# this script's output; the `tests/golden.rs` integration tests then run the
# Rust `bam2nuc_rs` over the SAME fixtures and assert byte-identity — hermetically
# (no Perl/samtools needed in CI).
#
# NB the genome MUST contain all 16 di-words + all 4 mono, else Perl `bam2nuc`
# dies "Illegal division by zero" computing coverage = freqs/genomic_freqs for a
# di-word absent from the genome. chr1 is a de Bruijn B(4,2) sequence
# ("AACAGATCCGCTGGTTA") which contains each 2-mer exactly once.
#
# Requirements (host): Perl + samtools on PATH (dev box: Perl 5.34,
# samtools 1.21 at /opt/homebrew/bin/samtools). Pins LC_ALL=C so Perl's
# `sort keys` cache order is the bytewise order the Rust port reproduces.
#
# Usage:  bash tests/data/generate_goldens.sh
set -euo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
PERL_BAM2NUC="$REPO_ROOT/bam2nuc"
SAMTOOLS="${SAMTOOLS:-$(command -v samtools)}"

echo "repo root      : $REPO_ROOT"
echo "perl bam2nuc   : $PERL_BAM2NUC"
echo "samtools       : $SAMTOOLS ($("$SAMTOOLS" --version | head -1))"
echo "perl           : $(perl -e 'print $]')"

DATA="$SCRIPT_DIR"
GOLD="$DATA/goldens"
rm -rf "$GOLD"
mkdir -p "$GOLD"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

tab=$'\t'

# ── genome fixtures (clean — NO cache; computed fresh at run time) ──
mkdir -p "$DATA/genome_acgtn" "$DATA/genome_iupac" "$DATA/genome_mus" "$DATA/genome_reuse"

# chr1: de Bruijn B(4,2), all 16 di-words present (mono A5 C4 G4 T4).
# chr2: an N-run to exercise N-skipping in the genomic composition.
cat > "$DATA/genome_acgtn/chr.fa" <<'EOF'
>chr1
AACAGATCCGCTGGTTA
>chr2
ACGTNNNNACGT
EOF

# IUPAC R → mono R + di CR/RG rows in the cache.
cat > "$DATA/genome_iupac/chr.fa" <<'EOF'
>chrI
ACRGTACGTACGT
EOF

# Mus-only dir → empty genome → 0-byte cache (Perl skips Mus_musculus.NCBIM37.fa).
cat > "$DATA/genome_mus/Mus_musculus.NCBIM37.fa" <<'EOF'
>chrM
ACGTACGT
EOF

# Reuse-cell genome + a PLANTED synthetic cache (×1000 / small ints, all 20
# words non-zero so no div-by-zero) that could NOT arise from the genome — so a
# recompute-instead-of-reuse bug visibly fails.
cat > "$DATA/genome_reuse/chr.fa" <<'EOF'
>chr1
AACAGATCCGCTGGTTA
EOF
cat > "$DATA/genome_reuse/genomic_nucleotide_frequencies.txt" <<EOF
A${tab}1000
AA${tab}10
AC${tab}11
AG${tab}12
AT${tab}13
C${tab}2000
CA${tab}14
CC${tab}15
CG${tab}16
CT${tab}17
G${tab}3000
GA${tab}18
GC${tab}19
GG${tab}20
GT${tab}21
T${tab}4000
TA${tab}22
TC${tab}23
TG${tab}24
TT${tab}25
EOF

# ── BAM fixtures (built from SAM via samtools) ──
make_bam () { "$SAMTOOLS" view -b "$1" -o "$2"; }

se_pg='@PG'"$tab"'ID:Bismark'"$tab"'PN:Bismark'"$tab"'VN:v0.25.1'"$tab"'CL:bismark --genome /g reads.fq.gz'
pe_pg='@PG'"$tab"'ID:Bismark'"$tab"'PN:Bismark'"$tab"'VN:v0.25.1'"$tab"'CL:bismark --genome /g -1 R1.fq.gz -2 R2.fq.gz'

# SE BAM: flags 0/16 on both chromosomes, an InDel read (skipped), and a
# read running off chr2's end (substr truncation).
{
  printf '@HD\tVN:1.6\tSO:unsorted\n'
  printf '@SQ\tSN:chr1\tLN:17\n'
  printf '@SQ\tSN:chr2\tLN:12\n'
  printf '%s\n' "$se_pg"
  printf 'r1\t0\tchr1\t1\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
  printf 'r2\t16\tchr1\t9\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
  printf 'r3\t0\tchr2\t1\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
  printf 'r4\t16\tchr2\t5\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
  printf 'r5\t0\tchr1\t1\t40\t3M1I4M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
  printf 'r6\t0\tchr2\t10\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
} > "$WORK/se.sam"
make_bam "$WORK/se.sam" "$DATA/se.bam"

# Coordinate-sorted SE BAM (gap #4): samtools sort appends its OWN @PG AFTER
# Bismark's, so detect_paired_from_header still sees ID:Bismark → SE. bam2nuc
# tallies are order-independent, so its stats MUST equal the unsorted se golden.
"$SAMTOOLS" sort -o "$DATA/se_sorted.bam" "$DATA/se.bam"

# PE BAM: canonical flags 99 / 147 / 83 / 163.
{
  printf '@HD\tVN:1.6\tSO:unsorted\n'
  printf '@SQ\tSN:chr1\tLN:17\n'
  printf '%s\n' "$pe_pg"
  printf 'p1\t99\tchr1\t1\t40\t8M\t=\t9\t16\tACGTACGT\tIIIIIIII\n'
  printf 'p2\t147\tchr1\t9\t40\t8M\t=\t1\t-16\tACGTACGT\tIIIIIIII\n'
  printf 'p3\t83\tchr1\t1\t40\t8M\t=\t9\t16\tACGTACGT\tIIIIIIII\n'
  printf 'p4\t163\tchr1\t9\t40\t8M\t=\t1\t-16\tACGTACGT\tIIIIIIII\n'
} > "$WORK/pe.sam"
make_bam "$WORK/pe.sam" "$DATA/pe.bam"

# PE BAM with a NON-CANONICAL flag (65 = paired+first, forward). Perl's
# `elsif ($flag == 83 or 163)` is always-true → flag 65 is revcomp'd (NOT
# treated as forward); this cell proves the Rust port replicates that bug.
{
  printf '@HD\tVN:1.6\tSO:unsorted\n'
  printf '@SQ\tSN:chr1\tLN:17\n'
  printf '%s\n' "$pe_pg"
  printf 'n1\t65\tchr1\t1\t40\t8M\t=\t9\t16\tACGTACGT\tIIIIIIII\n'
} > "$WORK/pe_noncanonical.sam"
make_bam "$WORK/pe_noncanonical.sam" "$DATA/pe_noncanonical.bam"

# All-InDel BAM: every read has an InDel → all skipped → sample empty → mono
# total 0 → Perl dies "Illegal division by zero". Exit-code cell (no golden).
{
  printf '@HD\tVN:1.6\tSO:unsorted\n'
  printf '@SQ\tSN:chr1\tLN:17\n'
  printf '%s\n' "$se_pg"
  printf 'd1\t0\tchr1\t1\t40\t3M1I4M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
  printf 'd2\t0\tchr1\t1\t40\t4M2D4M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
} > "$WORK/all_indel.sam"
make_bam "$WORK/all_indel.sam" "$DATA/all_indel.bam"

# Non-Bismark @PG BAM (gap #3): header has @SQ + a bowtie2 @PG (NO ID:Bismark),
# so detect_paired_from_header returns None → SePeUndetermined. Rust error cell
# (no golden); Perl test_file likewise dies "Failed to figure out SE or PE".
{
  printf '@HD\tVN:1.6\tSO:unsorted\n'
  printf '@SQ\tSN:chr1\tLN:17\n'
  printf '@PG\tID:bowtie2\tPN:bowtie2\tVN:2.5.0\tCL:bowtie2 -x g -U reads.fq\n'
  printf 'r1\t0\tchr1\t1\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
} > "$WORK/no_bismark_pg.sam"
make_bam "$WORK/no_bismark_pg.sam" "$DATA/no_bismark_pg.bam"

# ── run Perl bam2nuc into a fresh copy of each genome, harvest goldens ──
# Returns the run dir on stdout; ALL diagnostics go to stderr so the
# command-substitution caller captures only the path.
run_perl () {
  local label="$1"; local genome_fixture="$2"; shift 2
  local run="$WORK/run_$label"
  rm -rf "$run"; mkdir -p "$run/genome" "$run/out"
  cp "$genome_fixture"/* "$run/genome/"
  if ! ( cd "$run" && perl "$PERL_BAM2NUC" --genome_folder "$run/genome" --dir "$run/out/" \
         --samtools_path "$SAMTOOLS" "$@" ) > "$run/stdout.log" 2> "$run/stderr.log"; then
    { echo "PERL FAILED for '$label':"; cat "$run/stderr.log"; } >&2
    return 1
  fi
  echo "$run"
}

run_dir="$(run_perl cache_acgtn "$DATA/genome_acgtn" --genomic_composition_only)"
cp "$run_dir/genome/genomic_nucleotide_frequencies.txt" "$GOLD/cache_acgtn.golden"

run_dir="$(run_perl cache_iupac "$DATA/genome_iupac" --genomic_composition_only)"
cp "$run_dir/genome/genomic_nucleotide_frequencies.txt" "$GOLD/cache_iupac.golden"

run_dir="$(run_perl cache_mus "$DATA/genome_mus" --genomic_composition_only)"
cp "$run_dir/genome/genomic_nucleotide_frequencies.txt" "$GOLD/cache_mus.golden"

run_dir="$(run_perl se "$DATA/genome_acgtn" "$DATA/se.bam")"
cp "$run_dir/out/se.nucleotide_stats.txt" "$GOLD/se_stats.golden"
cp "$run_dir/genome/genomic_nucleotide_frequencies.txt" "$GOLD/se_cache.golden"

# Coordinate-sorted SE (gap #4): same composition as unsorted (order-independent).
run_dir="$(run_perl se_sorted "$DATA/genome_acgtn" "$DATA/se_sorted.bam")"
cp "$run_dir/out/se_sorted.nucleotide_stats.txt" "$GOLD/se_sorted_stats.golden"

run_dir="$(run_perl pe "$DATA/genome_acgtn" "$DATA/pe.bam")"
cp "$run_dir/out/pe.nucleotide_stats.txt" "$GOLD/pe_stats.golden"

run_dir="$(run_perl pe_nc "$DATA/genome_acgtn" "$DATA/pe_noncanonical.bam")"
cp "$run_dir/out/pe_noncanonical.nucleotide_stats.txt" "$GOLD/pe_noncanonical_stats.golden"

run_dir="$(run_perl reuse "$DATA/genome_reuse" "$DATA/se.bam")"
cp "$run_dir/out/se.nucleotide_stats.txt" "$GOLD/reuse_stats.golden"

echo
echo "Goldens written to $GOLD:"
ls -l "$GOLD"
echo "DONE."
