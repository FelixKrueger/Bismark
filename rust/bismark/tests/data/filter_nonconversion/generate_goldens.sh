#!/usr/bin/env bash
#
# Generate hermetic byte-identity goldens for filter_non_conversion_rs by
# running the REAL Perl `filter_non_conversion` (v0.25.1) + samtools on tiny
# synthetic Bismark BAM fixtures. Commits, per success case:
#   tests/data/<case>/in.bam            (or a non-.bam name)
#   tests/data/<case>/exp_filtered.sam  (samtools view of Perl's .nonCG_filtered.bam)
#   tests/data/<case>/exp_removed.sam   (samtools view of Perl's .nonCG_removed_seqs.bam)
#   tests/data/<case>/exp_report.txt    (Perl's .non-conversion_filtering.txt)
# and tests/data/cases.tsv:  name<TAB>input_basename<TAB>flags
#
# The integration test (tests/byte_identity.rs) runs the Rust binary on the
# committed in.bam, renders its outputs with samtools, and `cmp`s against the
# goldens (normalizing the non-deterministic run-time line of the report).
#
# Each non-header stdin line of a case is shorthand:  qname  flag  xm
# (whitespace-separated); emit_rec expands it into a full SAM record with
# SEQ/QUAL of the XM's length and XR:Z:CT XG:Z:CT.
#
# Run locally (needs perl + samtools on PATH):
#   bash tests/data/generate_goldens.sh
#
# Die/special cases (PE lone-R1, empty-.bam) are covered by dedicated Rust
# tests, not this table.
set -euo pipefail
export LC_ALL=C

PERL_FNC="${PERL_FNC:-/Users/fkrueger/Github/Bismark/filter_non_conversion}"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

command -v samtools >/dev/null || { echo "samtools not found" >&2; exit 1; }
[ -f "$PERL_FNC" ] || { echo "Perl filter_non_conversion not found at $PERL_FNC" >&2; exit 1; }

CASES_TSV="$HERE/cases.tsv"
: > "$CASES_TSV"

repeat_char() { head -c "$1" /dev/zero | tr '\0' "$2"; }

# Emit one mapped SAM record. Args: qname flag xm
emit_rec() {
  local qname="$1" flag="$2" xm="$3"
  local len=${#xm}
  printf '%s\t%s\tchr1\t100\t40\t%dM\t*\t0\t0\t%s\t%s\tXM:Z:%s\tXR:Z:CT\tXG:Z:CT\n' \
    "$qname" "$flag" "$len" "$(repeat_char "$len" A)" "$(repeat_char "$len" I)" "$xm"
}

# Emit an UNMAPPED record (flag 4): no XM.
emit_unmapped() {
  printf '%s\t4\t*\t0\t0\t*\t*\t0\t0\tAAAA\tIIII\n' "$1"
}

SE_HDR=$'@HD\tVN:1.6\tSO:unsorted\n@SQ\tSN:chr1\tLN:10000\n@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark --genome /g reads.fq.gz"\n'
PE_HDR=$'@HD\tVN:1.6\tSO:unsorted\n@SQ\tSN:chr1\tLN:10000\n@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark --genome /g -1 R1.fq.gz -2 R2.fq.gz"\n'

# Run Perl on a built input + capture goldens. Args: name inbase flags
capture() {
  local name="$1" inbase="$2" flags="$3" cdir="$4"
  ( cd "$cdir" && perl "$PERL_FNC" $flags "$inbase" >/dev/null 2>&1 ) || true
  local stem="${inbase%.bam}"
  local out="$HERE/$name"
  mkdir -p "$out"
  cp "$cdir/$inbase" "$out/$inbase"
  samtools view "$cdir/${stem}.nonCG_filtered.bam"     > "$out/exp_filtered.sam"
  samtools view "$cdir/${stem}.nonCG_removed_seqs.bam" > "$out/exp_removed.sam"
  cp "$cdir/${stem}.non-conversion_filtering.txt" "$out/exp_report.txt"
  printf '%s\t%s\t%s\n' "$name" "$inbase" "$flags" >> "$CASES_TSV"
  echo "  built $name ($inbase, flags='$flags')"
}

# Build a shorthand-driven case. Args: name inbase flags header ; body on stdin.
build_case() {
  local name="$1" inbase="$2" flags="$3" hdr="$4"
  local cdir="$WORK/$name"; mkdir -p "$cdir"
  local sam="$cdir/body.sam"
  {
    printf '%s' "$hdr"
    while read -r qname flag xm; do
      [ -z "${qname:-}" ] && continue
      emit_rec "$qname" "$flag" "$xm"
    done
  } > "$sam"
  samtools view -bS "$sam" > "$cdir/$inbase" 2>/dev/null
  capture "$name" "$inbase" "$flags" "$cdir"
}

echo "Generating goldens into $HERE ..."

# ── SE, default threshold 3 ──────────────────────────────────────────────
build_case se_default in.bam "-s" "$SE_HDR" <<'EOF'
r_keep0 0 ..........
r_keep2 0 H.X.......
r_remove3 0 HXH.......
r_remove_many 16 HHHHHHHHHH
r_cpg_only 0 ZZZZzzzz..
EOF

# ── SE, --consecutive ────────────────────────────────────────────────────
build_case se_consecutive in.bam "-s --consecutive" "$SE_HDR" <<'EOF'
c_keep_broken 0 HHhHH.....
c_remove_run3 0 HHH.......
c_keep_z_breaks 0 HHzHH.....
c_remove_Z_transparent 0 HHZH......
EOF

# ── SE, --percentage_cutoff 20 ───────────────────────────────────────────
build_case se_percentage in.bam "-s --percentage_cutoff 20" "$SE_HDR" <<'EOF'
p_below_min 0 HHHH......
p_at_cutoff 0 Hhhhh.....
p_over_cutoff 0 HHHHH.....
p_under_cutoff 0 HHHhhhhhhhhhhhhhhhhh
EOF

# ── SE, --threshold 5 ────────────────────────────────────────────────────
build_case se_threshold5 in.bam "-s --threshold 5" "$SE_HDR" <<'EOF'
t_keep4 0 HHHH......
t_remove5 0 HHHHH.....
EOF

# ── PE, default ──────────────────────────────────────────────────────────
build_case pe_default in.bam "-p" "$PE_HDR" <<'EOF'
pairA 99 ..........
pairA 147 ..........
pairB 99 ..........
pairB 147 HHH.......
pairC 99 HXH.......
pairC 147 ..........
EOF

# ── PE, --percentage_cutoff 20 ───────────────────────────────────────────
build_case pe_percentage in.bam "-p --percentage_cutoff 20" "$PE_HDR" <<'EOF'
qA 99 HHHH......
qA 147 ..........
qB 99 HHHHH.....
qB 147 ..........
EOF

# ── Auto-detect PE from @PG (no -s/-p) ───────────────────────────────────
build_case autodetect_pe in.bam "" "$PE_HDR" <<'EOF'
adA 99 ..........
adA 147 HHH.......
EOF

# ── SE with an unmapped read (no XM → kept, verbatim) ────────────────────
( cdir="$WORK/se_unmapped"; mkdir -p "$cdir"
  { printf '%s' "$SE_HDR"
    emit_rec u_keep 0 "H.X......."
    emit_unmapped u_unmapped
    emit_rec u_remove 0 "HXH......."
  } > "$cdir/body.sam"
  samtools view -bS "$cdir/body.sam" > "$cdir/in.bam" 2>/dev/null
  capture se_unmapped in.bam "-s" "$cdir"
)

# ── N/A branch: header-only BAM named ending in `bam` but WITHOUT a dot
#    (passes the top `/bam$/` gate yet skips the dotted `\.bam$` empty check,
#    so Perl processes it with count==0 → an N/A report; SPEC §4.3 C1). ──
( cdir="$WORK/na_nondotted"; mkdir -p "$cdir"
  printf '%s' "$SE_HDR" > "$cdir/body.sam"
  samtools view -bS "$cdir/body.sam" > "$cdir/emptyfoobam" 2>/dev/null
  capture na_nondotted emptyfoobam "-s" "$cdir"
)

echo "Done. cases.tsv:"
cat "$CASES_TSV"
