#!/usr/bin/env bash
# 5-Base default-engine benchmark harness (plans/10292026_5base-default-engine/).
#
#   five_base_engine_bench.sh prep  <sample> <tag> <pairs>
#       First <pairs> read pairs of L001 then L002, Trim Galore paired, md5s,
#       and a CpG-only copy of the DRAGEN CX report.
#   five_base_engine_bench.sh cell  <sample> <tag> <arm> [rep]
#       Align (timed, pinned), verify the engine options, dedup, extract,
#       read-fate stats and deconvolution; resumable via a DONE marker.
#   five_base_engine_bench.sh score <sample> <tag> <baseline> <arm>...
#
# Arms: A (minimap2), B (bowtie2), B-loc (bowtie2 --local), B-X (bowtie2 -X 800),
# C (hisat2). Layout: $ROOT/<sample>/{fastq,dragen}, results under $ROOT/runs.
# Env: ROOT (default /var/tmp/5base_engines), RESULTS (persistent copy,
# default ~/5base_results), REFS (shared indexes, default ~/references/GRCh38),
# CPUS (default 0-31), THREADS (default 32), MIN_JOINED (score guard, default 0.5).

set -euo pipefail

ROOT=${ROOT:-/var/tmp/5base_engines}
RESULTS=${RESULTS:-$HOME/5base_results}
CPUS=${CPUS:-0-31}
THREADS=${THREADS:-32}
BIN=$ROOT/bin
GENOME=$ROOT/genome
REFS=${REFS:-$HOME/references/GRCh38}
BT2_INDEX=$REFS/Bowtie2Index/Homo_sapiens.GRCh38
HT2_INDEX=$REFS/Hisat2Index/Homo_sapiens.GRCh38
MIN_FREE_GB=150
HERE=$(cd "$(dirname "$0")" && pwd)
export PATH=$BIN:$HOME/micromamba/envs/bismark-test/bin:$PATH

die() { echo "ERROR: $*" >&2; exit 1; }
log() { echo "$(date -u +%FT%TZ) $*"; }

arm_flags() {
  case $1 in
    A) echo "" ;;
    B) echo "--bowtie2 --five_base_index $BT2_INDEX" ;;
    B-loc) echo "--bowtie2 --five_base_index $BT2_INDEX --local" ;;
    B-X) echo "--bowtie2 --five_base_index $BT2_INDEX -X 800" ;;
    C) echo "--hisat2 --five_base_index $HT2_INDEX" ;;
    *) die "unknown arm '$1'" ;;
  esac
}

# Engine options the report must show for the arm: required tokens, then forbidden ones.
verify_options() {
  local arm=$1 report=$2 line
  line=$(grep -m1 "with the specified options:" "$report") || die "no options line in $report"
  local opts=" ${line##*with the specified options: } "
  local need=() deny=()
  case $arm in
    A) need=("Bismark was run with minimap2" " -t $THREADS ") ;;
    B) need=("Bismark was run with Bowtie 2" " -p $THREADS " " --reorder " " --score-min L,0,-0.6 ")
       deny=(" --local " " --maxins ") ;;
    B-loc) need=("Bismark was run with Bowtie 2" " -p $THREADS " " --local " " --score-min G,20,8 ") ;;
    B-X) need=("Bismark was run with Bowtie 2" " -p $THREADS " " --maxins 800 ") deny=(" --local ") ;;
    C) need=("Bismark was run with HISAT2" " -p $THREADS " " --reorder " " --no-spliced-alignment ")
       deny=(" --local ") ;;
  esac
  for t in "${need[@]}"; do
    if [[ $t == Bismark* ]]; then [[ $line == *"$t"* ]] || die "$arm: report lacks '$t': $line"
    else [[ $opts == *"$t"* ]] || die "$arm: options lack '$t': $opts"; fi
  done
  for t in "${deny[@]}"; do [[ $opts != *"$t"* ]] || die "$arm: options contain '$t': $opts"; done
  if [[ $arm == C ]]; then
    [[ $(grep -o -- "--no-spliced-alignment" <<<"$opts" | wc -l) -eq 1 ]] || die "C: no-splice not exactly once"
  fi
}

free_gb() { df -Pk "$ROOT" | awk 'NR==2{print int($4/1024/1024)}'; }

# Trimmed mate files in a prep dir, gzipped or plain; fails if neither pair exists.
trimmed() {
  local w=$1 ext
  for ext in fq.gz fq; do
    if [[ -s $w/sub_R1_val_1.$ext && -s $w/sub_R2_val_2.$ext ]]; then
      echo "$w/sub_R1_val_1.$ext $w/sub_R2_val_2.$ext"; return 0
    fi
  done
  return 1
}

cmd_prep() {
  local s=$1 tag=$2 pairs=$3
  local in=$ROOT/$s/fastq w=$ROOT/runs/$s/$tag/input
  mkdir -p "$w"
  if [[ -f $w/DONE ]]; then log "prep $s/$tag already done"; return; fi
  local r1=( "$in"/*_L001_R1_001.fastq.gz "$in"/*_L002_R1_001.fastq.gz )
  local r2=( "$in"/*_L001_R2_001.fastq.gz "$in"/*_L002_R2_001.fastq.gz )
  for f in "${r1[@]}" "${r2[@]}"; do [[ -f $f ]] || die "missing $f"; done
  local lines=$(( pairs * 4 ))
  if trimmed "$w" > /dev/null && grep -Eq "^Pairs analyzed: +$pairs$" "$w/trim_galore.log" 2>/dev/null; then
    log "prep $s/$tag: reusing the finished trim"
  else
  log "prep $s/$tag: subsetting $pairs pairs"
  ( zcat "${r1[@]}" | head -n "$lines" > "$w/sub_R1.fq" ) &
  ( zcat "${r2[@]}" | head -n "$lines" > "$w/sub_R2.fq" ) &
  wait
  for m in 1 2; do
    local n; n=$(wc -l < "$w/sub_R$m.fq")
    [[ $n -eq $lines ]] || die "sub_R$m.fq has $n lines, expected $lines"
  done
  log "prep $s/$tag: Trim Galore"
  trim_galore --paired --gzip --cores 8 -o "$w" "$w/sub_R1.fq" "$w/sub_R2.fq" > "$w/trim_galore.log" 2>&1
  rm -f "$w/sub_R1.fq" "$w/sub_R2.fq"
  fi
  local reads; reads=$(trimmed "$w") || die "Trim Galore output missing in $w"
  # shellcheck disable=SC2086
  ( cd "$w" && md5sum $(basename -a $reads) > input.md5 )
  local cx; cx=$(ls "$ROOT/$s"/dragen/*.CX_report.txt.gz | grep -v '\.control\.')
  if [[ ! -s $ROOT/$s/dragen/dragen.cpg.txt ]]; then
    log "prep $s: DRAGEN CpG-only copy"
    zcat "$cx" | awk -F'\t' '$6=="CG"' > "$ROOT/$s/dragen/dragen.cpg.txt.tmp"
    mv "$ROOT/$s/dragen/dragen.cpg.txt.tmp" "$ROOT/$s/dragen/dragen.cpg.txt"
  fi
  mkdir -p "$RESULTS/$s/$tag/input"
  cp "$w"/input.md5 "$w"/*trimming_report.txt "$w/trim_galore.log" "$RESULTS/$s/$tag/input/" 2>/dev/null || true
  touch "$w/DONE"
  log "prep $s/$tag done"
}

# Process-tree peak RSS (kB) and the aligner's CPU seconds, sampled every 2 s.
sample_tree() {
  local sid=$1 out=$2 peak=0 acpu=0
  while kill -0 "$sid" 2>/dev/null; do
    local rss cpu
    rss=$(ps -s "$sid" -o rss= 2>/dev/null | awk '{s+=$1} END {print s+0}')
    cpu=$(ps -s "$sid" -o comm=,times= 2>/dev/null \
          | awk '$1 ~ /^(minimap2|bowtie2-align|hisat2-align)/ {s+=$2} END {print s+0}')
    (( rss > peak )) && peak=$rss
    (( cpu > acpu )) && acpu=$cpu
    printf '%s\t%s\n' "$peak" "$acpu" > "$out"
    sleep 2
  done
}

cmd_cell() {
  local s=$1 tag=$2 arm=$3 rep=${4:-1}
  local in=$ROOT/runs/$s/$tag/input d=$ROOT/runs/$s/$tag/$arm.rep$rep
  [[ -f $in/DONE ]] || die "run 'prep $s $tag' first"
  local commit; commit=$(bismark --version | grep -o '([0-9a-f]\{7,\}' | tr -d '(')
  local flags; flags=$(arm_flags "$arm")
  local key; key=$(printf '%s|%s|%s|%s' "$commit" "$flags" "$THREADS" "$(cat "$in/input.md5")" | sha256sum | cut -c1-16)
  if [[ -f $d/DONE ]] && [[ $(cat "$d/DONE") == "$key" ]]; then log "cell $s/$tag/$arm.rep$rep already done"; return; fi
  [[ $(free_gb) -ge $MIN_FREE_GB ]] || die "only $(free_gb) GB free on $ROOT (need $MIN_FREE_GB)"
  rm -rf "$d"; mkdir -p "$d/tmp"
  local reads r1 r2
  reads=$(trimmed "$in") || die "no trimmed reads in $in"
  read -r r1 r2 <<<"$reads"
  log "cell $s/$tag/$arm.rep$rep: align ($flags)"
  local load0; load0=$(cut -d' ' -f1 /proc/loadavg)
  # shellcheck disable=SC2086
  setsid taskset -c "$CPUS" /usr/bin/time -v -o "$d/time.txt" \
    bismark --illumina_5base $flags --five_base_deconvolution -p "$THREADS" \
      --genome "$GENOME" -o "$d" --temp_dir "$d/tmp" -1 "$r1" -2 "$r2" \
      > "$d/bismark.log" 2>&1 &
  local pid=$!
  sample_tree "$pid" "$d/tree.tsv" &
  local spid=$!
  local rc=0; wait "$pid" || rc=$?
  wait "$spid" 2>/dev/null || true
  local load1; load1=$(cut -d' ' -f1 /proc/loadavg)
  [[ $rc -eq 0 ]] || die "bismark exited $rc (see $d/bismark.log)"
  local bam report deconv
  bam=$(ls "$d"/*_pe.bam); report=$(ls "$d"/*_PE_report.txt); deconv=$(ls "$d"/*_pe.5base_deconvolution.txt)
  verify_options "$arm" "$report"

  log "cell $s/$tag/$arm.rep$rep: dedup + extract"
  deduplicate_bismark -p --bam --output_dir "$d" "$bam" > "$d/dedup.log" 2>&1
  local dbam; dbam=$(ls "$d"/*_pe.deduplicated.bam)
  bismark_methylation_extractor -p --gzip --cytosine_report --genome_folder "$GENOME" \
    --parallel 8 -o "$d" "$dbam" > "$d/extract.log" 2>&1
  local cpg; cpg=$(ls "$d"/*CpG_report.txt* | head -1)
  [[ -s $cpg ]] || die "no CpG report in $d"

  log "cell $s/$tag/$arm.rep$rep: read fate + deconvolution"
  samtools view -@ 4 "$bam" | awk -F'\t' '
    { n++; if ($5 == 0) q0++ }
    int($2 / 64) % 2 == 1 { t = $9 < 0 ? -$9 : $9; r1++; if (t < length($10)) rt++; print t > "/dev/stderr" }
    END { printf "%d\t%d\t%d\t%d\n", n, q0, r1, rt }' > "$d/readfate.tsv" 2> "$d/tlen.txt"
  sort -n "$d/tlen.txt" | awk '{a[NR]=$1} END {printf "%d\t%d\t%d\n", a[int(NR*0.1)+1], a[int(NR*0.5)+1], a[int(NR*0.9)+1]}' > "$d/tlen_q.tsv"
  rm -f "$d/tlen.txt"
  python3 -I "$HERE/concordance.py" deconv --variants "$deconv" \
    --vcf "$(ls "$ROOT/$s"/dragen/*.hard-filtered.vcf.gz)" > "$d/deconv.txt"

  local wall maxrss pin uniq meff cpgp chg chh dup
  wall=$(awk -F': ' '/Elapsed \(wall clock\)/ {n=split($2,t,":"); s=0; for(i=1;i<=n;i++) s=s*60+t[i]; print s}' "$d/time.txt")
  maxrss=$(awk -F': ' '/Maximum resident set size/ {print $2}' "$d/time.txt")
  pin=$(awk -F'\t' '/Sequence pairs analysed in total/ {print $2}' "$report")
  uniq=$(awk -F'\t' '/paired-end alignments with a unique best hit/ {print $2}' "$report")
  meff=$(awk -F'\t' '/Mapping efficiency/ {gsub(/[% ]/,"",$2); print $2}' "$report")
  cpgp=$(awk -F'\t' '/C methylated in CpG context/ {sub(/%/,"",$2); print $2}' "$report")
  chg=$(awk -F'\t' '/C methylated in CHG context/ {sub(/%/,"",$2); print $2}' "$report")
  chh=$(awk -F'\t' '/C methylated in CHH context/ {sub(/%/,"",$2); print $2}' "$report")
  dup=$(grep -m1 "duplicated alignments removed" "$d"/*deduplication_report.txt | grep -o '([0-9.]*%)' | tr -d '()%')
  local prec rec; prec=$(grep -m1 precision "$d/deconv.txt" | grep -o '[0-9.]*%$' | tr -d %)
  rec=$(grep -m1 "recall:" "$d/deconv.txt" | grep -o '[0-9.]*%$' | tr -d %)
  local tree; tree=$(cat "$d/tree.tsv")
  {
    printf 'sample\ttag\tarm\trep\tpairs_in\tunique_pairs\tmap_eff\tcpg_pct\tchg_pct\tchh_pct\tdup_pct\t'
    printf 'records\tmapq0\tr1_records\treadthrough\ttlen_p10\ttlen_p50\ttlen_p90\t'
    printf 'deconv_precision\tdeconv_recall\twall_s\ttime_maxrss_kb\ttree_peak_rss_kb\taligner_cpu_s\tload_start\tload_end\tcpus\tkey\n'
    printf '%s\t' "$s" "$tag" "$arm" "$rep" "$pin" "$uniq" "$meff" "$cpgp" "$chg" "$chh" "$dup"
    printf '%s\t' $(cat "$d/readfate.tsv") $(cat "$d/tlen_q.tsv")
    printf '%s\t' "$prec" "$rec" "$wall" "$maxrss" $tree "$load0" "$load1" "$CPUS"
    printf '%s\n' "$key"
  } > "$d/metrics.tsv"
  # The raw BAM, CpG report, .cov and bedGraph stay; the rest is re-creatable from them.
  rm -rf "$d/tmp" "$dbam" "$d"/CpG_O[TB]_*.txt.gz "$d"/CHG_O[TB]_*.txt.gz "$d"/CHH_O[TB]_*.txt.gz
  gzip -f "$deconv"
  local keep=$RESULTS/$s/$tag/$arm.rep$rep
  mkdir -p "$keep"
  cp "$d"/metrics.tsv "$d"/time.txt "$d"/tree.tsv "$report" "$d"/*deduplication_report.txt \
     "$d"/deconv.txt "$d"/bismark.log "$keep"/
  echo "$key" > "$d/DONE"
  log "cell $s/$tag/$arm.rep$rep done: $(tail -1 "$d/metrics.tsv" | cut -f5-11)"
}

cmd_score() {
  local s=$1 tag=$2 base=$3; shift 3
  local args=()
  for arm in "$@"; do
    local cpg; cpg=$(ls "$ROOT/runs/$s/$tag/$arm.rep1"/*CpG_report.txt* | head -1)
    args+=(--arm "$arm=$cpg")
  done
  local out=$ROOT/runs/$s/$tag/score.tsv
  python3 -I "$HERE/five_base_compare.py" compare --dragen "$ROOT/$s/dragen/dragen.cpg.txt" \
    "${args[@]}" --baseline "$base" --out "$out"
  local bad
  bad=$(awk -F'\t' 'NR>1 && $4 > 0 && $3 == 0 {print $1":"$2}' "$out.contigs" | head -5)
  [[ -z $bad ]] || die "zero joined CpGs on a DRAGEN-covered contig: $bad"
  awk -F'\t' -v min="${MIN_JOINED:-0.5}" 'NR>1 && $4 < min {print "ERROR: " $1 " joins only " $4 " of DRAGEN CpGs"; e=1} END {exit e}' "$out.joined"
  mkdir -p "$RESULTS/$s/$tag"
  cp "$out" "$out.contigs" "$out.joined" "$RESULTS/$s/$tag/"
  log "score $s/$tag written"
}

case ${1:-} in
  prep) shift; cmd_prep "$@" ;;
  cell) shift; cmd_cell "$@" ;;
  score) shift; cmd_score "$@" ;;
  *) sed -n '2,13p' "$0"; exit 2 ;;
esac
