#!/bin/bash
# C1 — byte-identity between the arms. Runs after the fact over whatever pairs of
# runs exist, so it is cheap to re-run and never blocks a benchmark.
#
# Compares `samtools view` record bodies, NOT raw BAM bytes: the header's @PG CL:
# line is the verbatim argv and differs between the arms by design (one carries
# --no_stream_converted). Reports are compared with absolute paths filtered out,
# for the same reason.
#
# The per-arm digest is cached beside the BAM, so re-running costs nothing for
# pairs already done.
set -uo pipefail
WORK=${WORK:?}
OUTV=$WORK/C1.tsv
[ -f "$OUTV" ] || printf 'shape\trep\trecords\trecords_md5_stream\trecords_md5_files\tbam_verdict\treport_verdict\n' > "$OUTV"

digest() { # $1 = run dir -> prints "md5 count", cached
  d=$1
  if [ -s "$d/records.md5" ]; then cat "$d/records.md5"; return; fi
  b=$(ls "$d"/*.bam 2>/dev/null | head -1)
  [ -z "$b" ] && { echo "NOBAM 0"; return; }
  # One decompression pass produces both the digest and the count.
  samtools view "$b" 2>/dev/null \
    | tee >(wc -l > "$d/.n") \
    | md5sum | cut -d' ' -f1 > "$d/.md5"
  echo "$(cat "$d/.md5") $(tr -d ' ' < "$d/.n")" > "$d/records.md5"
  rm -f "$d/.md5" "$d/.n"
  cat "$d/records.md5"
}

filtered_report() { # $1 = run dir
  grep -hvE '^(Bismark completed|Bismark version)' "$1"/*_report.txt 2>/dev/null \
    | grep -vE '/fs/|/work/|/repo/|/tmp/'
}

for sdir in "$WORK"/out/*_stream_r*; do
  [ -d "$sdir" ] || continue
  base=$(basename "$sdir")
  tag=${base%_stream_r*}; rep=${base##*_stream_r}
  fdir=$WORK/out/${tag}_files_r${rep}
  [ -d "$fdir" ] || continue
  grep -q "^$tag	$rep	" "$OUTV" && continue          # already recorded

  read -r m_s n_s <<< "$(digest "$sdir")"
  read -r m_f n_f <<< "$(digest "$fdir")"
  if [ "$m_s" = "NOBAM" ] || [ "$m_f" = "NOBAM" ]; then
    bam="NO BAM"
  elif [ "$m_s" = "$m_f" ] && [ "$n_s" = "$n_f" ]; then
    bam="IDENTICAL"
  else
    bam="*** DIFFERS ***"
  fi

  if diff -q <(filtered_report "$sdir") <(filtered_report "$fdir") >/dev/null 2>&1; then
    rep_v="IDENTICAL"
  else
    rep_v="*** DIFFERS ***"
    diff <(filtered_report "$sdir") <(filtered_report "$fdir") > "$WORK/report_diff_${tag}_r${rep}.txt" 2>&1
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$tag" "$rep" "$n_s" "$m_s" "$m_f" "$bam" "$rep_v" >> "$OUTV"
  printf '  %-24s rep%-3s %12s records  BAM %-16s report %s\n' "$tag" "$rep" "$n_s" "$bam" "$rep_v"
done

echo
awk -F'\t' 'NR>1 && ($6 != "IDENTICAL" || $7 != "IDENTICAL") { bad++ }
  END { if (bad) printf "### C1: %d PAIR(S) DIFFER\n", bad; else print "### C1: all pairs identical" }' "$OUTV"
