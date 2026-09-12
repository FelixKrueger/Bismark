#!/bin/bash
# End-to-end A/B for #1120: every run is executed twice — once streaming the
# converted reads through FIFOs (the default) and once writing them as files
# (`--no_stream_converted`) — and the two BAMs must be identical.
#
# Run it with ab-run.sh (which builds/starts the container).
set -uo pipefail
cd /repo/rust

BIN=/repo/rust/target/debug
echo "### building"
cargo build --locked -p bismark --bins 2>&1 | tail -3 || exit 1

mkdir -p /work/genome
zcat /repo/test_files/NC_010473.fa.gz > /work/genome/ecoli.fa
echo "### preparing the bisulfite genome"
$BIN/bismark_genome_preparation --bowtie2 /work/genome >/dev/null 2>&1 || { echo "FAIL genome prep"; exit 1; }

R1=/repo/test_files/test_R1.fastq.gz
R2=/repo/test_files/test_R2.fastq.gz

# FastA fixtures, so the 2-line conversion core is exercised too (it has its own
# record loop, and streaming drives the same one).
FA1=/work/reads_1.fa; FA2=/work/reads_2.fa
zcat $R1 | awk 'NR%4==1 { sub(/^@/,">"); print } NR%4==2 { print }' > $FA1
zcat $R2 | awk 'NR%4==1 { sub(/^@/,">"); print } NR%4==2 { print }' > $FA2

fail=0
# $1 = case label, rest = bismark args (minus --genome/-o/--temp_dir)
ab() {
  label=$1; shift
  for arm in stream files; do
    out=/work/out_${label}_${arm}; tmp=/work/tmp_${label}_${arm}
    rm -rf "$out" "$tmp"; mkdir -p "$out" "$tmp"
    extra=""; [ "$arm" = files ] && extra="--no_stream_converted"
    # shellcheck disable=SC2086
    $BIN/bismark --genome /work/genome "$@" $extra -o "$out" --temp_dir "$tmp" \
      > "$out/stdout.log" 2> "$out/stderr.log"
    rc=$?
    if [ $rc -ne 0 ]; then
      echo "  [$label/$arm] *** bismark exited $rc ***"; tail -20 "$out/stderr.log"; fail=1; return
    fi
    samtools view "$out"/*.bam > "$out/records.sam" 2>/dev/null
    grep -vE '^(Bismark completed|Bismark version)' "$out"/*_report.txt \
      | grep -vE '/work/|/repo/' > "$out/report.filtered" 2>/dev/null
  done

  a=/work/out_${label}_stream; b=/work/out_${label}_files
  if cmp -s "$a/records.sam" "$b/records.sam"; then bam="BAM identical"; else bam="*** BAM DIFFERS ***"; fail=1; fi
  if cmp -s "$a/report.filtered" "$b/report.filtered"; then rep="report identical"; else rep="*** report DIFFERS ***"; fail=1; fi
  n=$(wc -l < "$a/records.sam")

  # The point of the exercise: the streaming arm must leave no converted temp
  # file behind, and the file arm's temp dir must have held them.
  leftover=$(find /work/tmp_${label}_stream -name '*_C_to_T*' -o -name '*_G_to_A*' -o -name '*.fifo.*' 2>/dev/null | wc -l)
  [ "$leftover" -ne 0 ] && { echo "  [$label] *** streaming arm left $leftover converted/fifo file(s) ***"; fail=1; }
  streamed=$(grep -c 'no temp file written' "$a/stderr.log")
  wrote=$(grep -c '^Created .* converted version' "$b/stderr.log")
  [ "$streamed" -eq 0 ] && { echo "  [$label] *** streaming arm printed no streaming banner ***"; fail=1; }
  [ "$wrote" -eq 0 ] && { echo "  [$label] *** file arm printed no 'Created ... converted' banner ***"; fail=1; }

  printf '  %-34s %-22s %-20s (%s records, %s streams, %s files)\n' "$label" "$bam" "$rep" "$n" "$streamed" "$wrote"
}

echo "### A/B matrix (streaming vs --no_stream_converted)"
ab pe_directional      -1 $R1 -2 $R2
ab pe_nondirectional   -1 $R1 -2 $R2 --non_directional
ab pe_pbat             -1 $R1 -2 $R2 --pbat
ab se_directional      --single_end $R1
ab se_nondirectional   --single_end $R1 --non_directional
ab se_pbat             --single_end $R1 --pbat
ab pe_gzip             -1 $R1 -2 $R2 --gzip
ab pe_threads4         -1 $R1 -2 $R2 -p 4
ab pe_multicore2       -1 $R1 -2 $R2 --multicore 2
ab se_skip_upto        --single_end $R1 --skip 100 --upto 2000
ab pe_basename         -1 $R1 -2 $R2 --prefix pfx
ab se_fasta            --fasta --single_end $FA1
ab pe_fasta            --fasta -1 $FA1 -2 $FA2
ab se_fasta_nondir     --fasta --single_end $FA1 --non_directional

echo
if [ $fail -eq 0 ]; then echo "### ALL PASS"; else echo "### FAILURES ABOVE"; fi
exit $fail
