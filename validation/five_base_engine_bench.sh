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
#   five_base_engine_bench.sh versions
#       Record the tool versions and fail unless they match the #1146 run.
#   five_base_engine_bench.sh repro-input <sample> <tag>
#   five_base_engine_bench.sh repro <sample> <tag> <arm> <ref-dir>
#       Trimmed-read md5s, then an arm against #1146's committed B-X results.
#   five_base_engine_bench.sh repro-compare <sample> <tag> <arm1> <arm2>
#       Two cells must agree on every reproduced metric.
#   five_base_engine_bench.sh rescued <sample> <tag> <old-arm> <new-arm>
#   five_base_engine_bench.sh verdict <sample> <tag> <old-arm> <new-arm>
#       Admitted-pair sets and the pre-registered criteria
#       (plans/11012026_5base-bowtie2-defaults/PREREGISTRATION.md).
#
# Arms: A (minimap2), B (bowtie2 -X 500), B-loc (bowtie2 --local -X 500), B-X
# (bowtie2 -X 800), B-new (bowtie2 defaults), C (hisat2). B-old names the rev 3
# cross-check cells and cannot be re-run.
# Layout: $ROOT/<sample>/{fastq,dragen}, results under $ROOT/runs.
# Env: ROOT (default /var/tmp/5base_engines), BIN (default $ROOT/bin), RESULTS
# (persistent copy, default ~/5base_results), REFS (shared indexes, default
# ~/references/GRCh38), CPUS (default 0-31), THREADS (default 32), MIN_JOINED
# (score guard, default 0.5).

set -euo pipefail

ROOT=${ROOT:-/var/tmp/5base_engines}
RESULTS=${RESULTS:-$HOME/5base_results}
CPUS=${CPUS:-0-31}
THREADS=${THREADS:-32}
BIN=${BIN:-$ROOT/bin}
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
    B) echo "--bowtie2 --five_base_index $BT2_INDEX -X 500" ;;
    B-loc) echo "--bowtie2 --five_base_index $BT2_INDEX --local -X 500" ;;
    B-X) echo "--bowtie2 --five_base_index $BT2_INDEX -X 800" ;;
    B-old) echo "--bowtie2 --five_base_index $BT2_INDEX -X 800 --no_dovetail --five_base_umi_qname" ;;
    B-new) echo "--bowtie2 --five_base_index $BT2_INDEX" ;;
    C) echo "--hisat2 --five_base_index $HT2_INDEX" ;;
    *) die "unknown arm '$1'" ;;
  esac
}

# Occurrences of a whole token in a space-separated option string.
count_tok() { tr ' ' '\n' <<<"$2" | grep -cx -- "$1" || true; }

# Engine options the report must show for the arm: required tokens, then forbidden ones.
verify_options() {
  local arm=$1 report=$2 line
  line=$(grep -m1 "with the specified options:" "$report") || die "no options line in $report"
  local opts=" ${line##*with the specified options: } "
  local need=() deny=()
  case $arm in
    A) need=("Bismark was run with minimap2" " -t $THREADS " " -x sr ") ;;
    B) need=("Bismark was run with Bowtie 2" " -p $THREADS " " --reorder " " --score-min L,0,-0.6 " " --maxins 500 ")
       deny=(" --local " " --dovetail ") ;;
    B-loc) need=("Bismark was run with Bowtie 2" " -p $THREADS " " --reorder " " --local " " --score-min G,20,8 " " --maxins 500 ")
       deny=(" --dovetail ") ;;
    B-X|B-old) need=("Bismark was run with Bowtie 2" " -p $THREADS " " --reorder " " --score-min L,0,-0.6 " " --maxins 800 ")
       deny=(" --local " " --dovetail ") ;;
    B-new) need=("Bismark was run with Bowtie 2" " -p $THREADS " " --reorder " " --score-min L,0,-0.6 " " --maxins 800 ")
       deny=(" --local " " --dovetail " " --no-mixed " " --no-discordant ") ;;
    C) need=("Bismark was run with HISAT2" " -p $THREADS " " --reorder " " --no-spliced-alignment ")
       deny=(" --local ") ;;
  esac
  for t in "${need[@]}"; do
    if [[ $t == Bismark* ]]; then [[ $line == *"$t"* ]] || die "$arm: report lacks '$t': $line"
    else [[ $opts == *"$t"* ]] || die "$arm: options lack '$t': $opts"; fi
  done
  for t in "${deny[@]}"; do [[ $opts != *"$t"* ]] || die "$arm: options contain '$t': $opts"; done
  local once=()
  case $arm in
    C) once=(--no-spliced-alignment) ;;
    B|B-loc|B-X|B-old|B-new) once=(--maxins) ;;
  esac
  for t in "${once[@]}"; do
    [[ $(count_tok "$t" "$opts") -eq 1 ]] || die "$arm: '$t' not exactly once: $opts"
  done
}

# The Bismark @PG command line must carry --illumina_5base, the thread count and every arm flag.
check_pg_line() {
  local arm=$1 line=$2 t
  [[ $line == *"ID:Bismark"* ]] || die "$arm: not a Bismark @PG line: $line"
  local cl=" ${line#*CL:} "; cl=${cl//\"/ }
  local flags; flags=$(arm_flags "$arm")
  for t in --illumina_5base "-p $THREADS" ${flags:+"$flags"}; do
    [[ $cl == *" $t "* ]] || die "$arm: @PG command line lacks '$t': $cl"
  done
  # B-new must reach its options through the defaults alone.
  if [[ $arm == B-new ]]; then
    for t in -X --maxins --dovetail --no_dovetail; do
      [[ $cl != *" $t "* ]] || die "$arm: @PG command line sets '$t': $cl"
    done
  fi
}

verify_pg() {
  local header line
  header=$(samtools view -H "$2") || die "$1: cannot read the header of $2"
  line=$(grep -m1 $'^@PG\tID:Bismark' <<<"$header") || die "$1: no Bismark @PG in $2"
  check_pg_line "$1" "$line"
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
  local cx; cx=$(ls "$ROOT/$s"/dragen/*.CX_report.txt.gz 2>/dev/null | grep -v '\.control\.' || true)
  [[ -n $cx && $(wc -l <<<"$cx") -eq 1 ]] || die "expected one DRAGEN CX report in $ROOT/$s/dragen, found: '$cx'"
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

# PIDs of a process and all its descendants, comma-separated.
tree_pids() {
  ps -e -o pid=,ppid= | awk -v root="$1" '
    { kids[$2] = kids[$2] " " $1 }
    END { q[1] = root; n = 1; out = root
          for (i = 1; i <= n; i++) { m = split(kids[q[i]], c, " ")
            for (j = 1; j <= m; j++) { q[++n] = c[j]; out = out "," c[j] } }
          print out }'
}

# Process-tree peak RSS (KiB) and the aligner's CPU seconds, sampled every 2 s.
sample_tree() {
  local sid=$1 out=$2 peak=0 acpu=0
  while kill -0 "$sid" 2>/dev/null; do
    local rss cpu pids
    pids=$(tree_pids "$sid")
    rss=$(ps -o rss= -p "$pids" 2>/dev/null | awk '{s+=$1} END {print s+0}')
    cpu=$(ps -o comm=,times= -p "$pids" 2>/dev/null \
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
  [[ $arm != B-old ]] || die "B-old is the rev 3 cross-check arm; not re-runnable"
  [[ -f $in/DONE ]] || die "run 'prep $s $tag' first"
  local commit; commit=$(bismark --version | grep -o '([0-9a-f]\{7,\}' | tr -d '(') || true
  [[ -n $commit ]] || die "no commit hash in 'bismark --version'"
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
  taskset -c "$CPUS" /usr/bin/time -v -o "$d/time.txt" \
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
  [[ -s $d/tree.tsv && $(cut -f1 "$d/tree.tsv") -gt 0 ]] || die "the process-tree sampler recorded no memory"
  local bam report deconv
  bam=$(ls "$d"/*_pe.bam); report=$(ls "$d"/*_PE_report.txt); deconv=$(ls "$d"/*_pe.5base_deconvolution.txt)
  verify_options "$arm" "$report"
  verify_pg "$arm" "$bam"

  log "cell $s/$tag/$arm.rep$rep: dedup + extract"
  deduplicate_bismark -p --bam --output_dir "$d" "$bam" > "$d/dedup.log" 2>&1
  local dbam; dbam=$(ls "$d"/*_pe.deduplicated.bam)
  bismark_methylation_extractor -p --gzip --cytosine_report --genome_folder "$GENOME" \
    --parallel 8 -o "$d" "$dbam" > "$d/extract.log" 2>&1
  local cpg; cpg=$(ls "$d"/*CpG_report.txt* | head -1)
  [[ -s $cpg ]] || die "no CpG report in $d"
  local mbias; mbias=$(ls "$d"/*M-bias.txt) || die "no M-bias file in $d"

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
     "$d"/deconv.txt "$d"/bismark.log "$mbias" "$keep"/
  echo "$key" > "$d/DONE"
  log "cell $s/$tag/$arm.rep$rep done: $(tail -1 "$d/metrics.tsv" | cut -f5-11)"
}

cmd_score() {
  local s=$1 tag=$2 base=$3; shift 3
  local args=() pairs=()
  for arm in "$@"; do
    local d=$ROOT/runs/$s/$tag/$arm.rep1 cpg
    [[ -s $d/DONE ]] || die "cell $s/$tag/$arm.rep1 has not finished"
    [[ $(cat "$d/DONE") == "$(awk -F'\t' 'NR==2 {print $NF}' "$d/metrics.tsv")" ]] \
      || die "cell $s/$tag/$arm.rep1: DONE key does not match metrics.tsv"
    cpg=$(ls "$d"/*CpG_report.txt*)
    [[ $(wc -l <<<"$cpg") -eq 1 ]] || die "expected one CpG report in $d, found: $cpg"
    local n; n=$(awk -F'\t' 'NR==2 {print $5}' "$d/metrics.tsv")
    [[ $n =~ ^[0-9]+$ && $n -gt 0 ]] || die "cell $s/$tag/$arm.rep1: no read count in metrics.tsv"
    pairs+=("$n")
    args+=(--arm "$arm=$cpg")
  done
  [[ $(printf '%s\n' "${pairs[@]}" | sort -u | wc -l) -eq 1 ]] \
    || die "arms analysed different read counts: ${pairs[*]}"
  local out=$ROOT/runs/$s/$tag/score.tsv
  python3 -I "$HERE/five_base_compare.py" compare --dragen "$ROOT/$s/dragen/dragen.cpg.txt" \
    "${args[@]}" --baseline "$base" --out "$out" --min-joined "${MIN_JOINED:-0.5}"
  mkdir -p "$RESULTS/$s/$tag"
  cp "$out" "$out.contigs" "$out.joined" "$RESULTS/$s/$tag/"
  log "score $s/$tag written"
}

# stdout only: bowtie2 prints a CPU-feature warning on stderr before its version.
first_line() { "$@" 2>/dev/null | sed -n 1p; }

cmd_versions() {
  mkdir -p "$RESULTS"
  local commit bin b out
  commit=$(bismark --version | grep -o '([0-9a-f]\{7,\}' | tr -d '(') || true
  [[ -n $commit ]] || die "no commit hash in 'bismark --version'"
  out=$RESULTS/versions_$commit.txt
  bin=$(readlink -f "$(command -v bismark)")
  {
    printf 'bismark\t%s\t%s\n' "$(first_line bismark --version)" "$bin"
    for b in deduplicate_bismark bismark_methylation_extractor; do
      printf '%s\t%s\n' "$b" "$(readlink -f "$(command -v "$b")")"
    done
    printf 'trim_galore\t%s\n' "$(trim_galore --version 2>&1 | grep -m1 -o '[0-9]\+\.[0-9]\+\.[0-9]\+')"
    printf 'bowtie2\t%s\n' "$(first_line bowtie2 --version)"
    printf 'cutadapt\t%s\n' "$(first_line cutadapt --version)"
    printf 'samtools\t%s\n' "$(first_line samtools --version)"
  } > "$out"
  cat "$out"
  local -A want=([trim_galore]="2.2.0" [bowtie2]="*version 2.5.5" [cutadapt]="2.6" [samtools]="samtools 1.23.1")
  local tool got
  for tool in "${!want[@]}"; do
    got=$(awk -F'\t' -v t="$tool" '$1 == t {print $2}' "$out")
    # shellcheck disable=SC2053
    [[ $got == ${want[$tool]} ]] || die "$tool is '$got', expected ${want[$tool]}"
  done
  for b in deduplicate_bismark bismark_methylation_extractor; do
    [[ $(awk -F'\t' -v t="$b" '$1 == t {print $2}' "$out") == "$bin" ]] \
      || die "$b is not the bismark build at $bin"
  done
}

# #1146's trimmed-read md5s (plain .fq) for the 100M-pair screen.
declare -A MD5_R1=([NA12878]=337ffa049ef5cff27fa5e30a555eb50a [HG002]=d055be973c675edbeee5897aa7cc6341)
declare -A MD5_R2=([NA12878]=5229b07724d84db29bc6ae9b36276649 [HG002]=f0770c478ceb160f5d50edeec7619bf8)

cmd_repro_input() {
  local s=$1 tag=$2 w=$ROOT/runs/$1/$2/input reads r1 r2 m1 m2
  [[ -n ${MD5_R1[$s]:-} ]] || die "no reference md5s for $s"
  reads=$(trimmed "$w") || die "no trimmed reads in $w"
  read -r r1 r2 <<<"$reads"
  m1=$(zcat -f "$r1" | md5sum | cut -d' ' -f1)
  m2=$(zcat -f "$r2" | md5sum | cut -d' ' -f1)
  mkdir -p "$RESULTS/$s/$tag"
  printf 'R1\t%s\t%s\nR2\t%s\t%s\n' "$m1" "${MD5_R1[$s]}" "$m2" "${MD5_R2[$s]}" > "$RESULTS/$s/$tag/repro_input.tsv"
  [[ $m1 == "${MD5_R1[$s]}" && $m2 == "${MD5_R2[$s]}" ]] \
    || die "$s trimmed reads differ from #1146: R1 $m1, R2 $m2"
  log "repro-input $s/$tag: trimmed reads match #1146"
}

REPRO_COLS=(pairs_in unique_pairs map_eff cpg_pct chg_pct chh_pct dup_pct records mapq0 r1_records
            readthrough tlen_p10 tlen_p50 tlen_p90 deconv_precision deconv_recall)

col() { awk -F'\t' -v k="$1" 'NR == 1 { for (i = 1; i <= NF; i++) if ($i == k) c = i } NR == 2 { print $c }' "$2"; }
deconv_fracs() { grep -E 'precision|recall' "$1" | grep -o '[0-9]\+/[0-9]\+' | paste -sd' ' -; }

# Own-set cov>=10 "n r" of a finished cell, scored alone against DRAGEN.
own_cov10() {
  local s=$1 tag=$2 arm=$3 d=$ROOT/runs/$1/$2/$3.rep1 score cpg
  score=$ROOT/runs/$s/$tag/repro_score_$arm.tsv
  if [[ ! -s $score ]]; then
    cpg=$(ls "$d"/*CpG_report.txt*)
    python3 -I "$HERE/five_base_compare.py" compare --dragen "$ROOT/$s/dragen/dragen.cpg.txt" \
      --arm "$arm=$cpg" --baseline "$arm" --out "$score" --min-joined "${MIN_JOINED:-0.5}" >&2
    mkdir -p "$RESULTS/$s/$tag"
    cp "$score" "$RESULTS/$s/$tag/"
  fi
  awk -F'\t' -v a="$arm" '$1 == a && $2 == "own" && $3 == 10 {print $4, $5}' "$score"
}

# Compare two (label, metrics, deconv, own-set) tuples row by row; writes <out>, returns 1 on any difference.
compare_cells() {
  local out=$1 la=$2 ma=$3 da=$4 oa=$5 lb=$6 mb=$7 db=$8 ob=$9 bad=0 c a b
  printf 'metric\t%s\t%s\n' "$la" "$lb" > "$out"
  for c in "${REPRO_COLS[@]}"; do
    a=$(col "$c" "$ma"); b=$(col "$c" "$mb")
    printf '%s\t%s\t%s\n' "$c" "$a" "$b" >> "$out"
    [[ -n $a && $a == "$b" ]] || { echo "$c: $la $a, $lb $b"; bad=1; }
  done
  a=$(deconv_fracs "$da"); b=$(deconv_fracs "$db")
  printf 'deconv_counts\t%s\t%s\n' "$a" "$b" >> "$out"
  [[ -n $a && $a == "$b" ]] || { echo "deconvolution counts: $la $a, $lb $b"; bad=1; }
  printf 'own_cov10_n_r\t%s\t%s\n' "$oa" "$ob" >> "$out"
  [[ -n $oa && $oa == "$ob" ]] || { echo "own-set cov>=10: $la $oa, $lb $ob"; bad=1; }
  return "$bad"
}

# An arm against #1146's committed B-X: every committed metric, deconvolution counts and the own set.
cmd_repro() {
  local s=$1 tag=$2 arm=$3 ref=$4 d=$ROOT/runs/$1/$2/$3.rep1
  [[ -s $d/DONE ]] || die "cell $s/$tag/$arm.rep1 has not finished"
  [[ -s $ref/B-X.rep1/metrics.tsv && -s $ref/B-X.rep1/deconv.txt && -s $ref/score.tsv ]] \
    || die "no #1146 B-X results in $ref"
  local own ref_own out=$RESULTS/$s/$tag/repro_$arm.tsv
  own=$(own_cov10 "$s" "$tag" "$arm")
  ref_own=$(awk -F'\t' '$1 == "B-X" && $2 == "own" && $3 == 10 {print $4, $5}' "$ref/score.tsv")
  mkdir -p "$RESULTS/$s/$tag"
  compare_cells "$out" "$arm" "$d/metrics.tsv" "$d/deconv.txt" "$own" \
    "#1146 B-X" "$ref/B-X.rep1/metrics.tsv" "$ref/B-X.rep1/deconv.txt" "$ref_own" \
    || die "$arm does not reproduce #1146 B-X (see $out)"
  log "repro $s/$tag: $arm reproduces #1146 B-X"
}

cmd_repro_compare() {
  local s=$1 tag=$2 a1=$3 a2=$4 d1=$ROOT/runs/$1/$2/$3.rep1 d2=$ROOT/runs/$1/$2/$4.rep1
  [[ -s $d1/DONE && -s $d2/DONE ]] || die "cells $s/$tag/$a1 and $a2 must both have finished"
  local o1 o2 out=$RESULTS/$s/$tag/repro_compare_${a1}_$a2.tsv
  o1=$(own_cov10 "$s" "$tag" "$a1"); o2=$(own_cov10 "$s" "$tag" "$a2")
  mkdir -p "$RESULTS/$s/$tag"
  compare_cells "$out" "$a1" "$d1/metrics.tsv" "$d1/deconv.txt" "$o1" \
    "$a2" "$d2/metrics.tsv" "$d2/deconv.txt" "$o2" || die "$a1 and $a2 differ (see $out)"
  log "repro-compare $s/$tag: $a1 and $a2 agree"
}

cmd_rescued() {
  local s=$1 tag=$2 old=$3 new=$4 base=$ROOT/runs/$1/$2 arm
  for arm in "$old" "$new"; do
    [[ -s $base/$arm.rep1/DONE ]] || die "cell $s/$tag/$arm.rep1 has not finished"
  done
  local reads r1 r2 out=$base/rescued_${old}_$new keep=$RESULTS/$s/$tag/rescued_${old}_$new
  reads=$(trimmed "$base/input") || die "no trimmed reads in $base/input"
  read -r r1 r2 <<<"$reads"
  rm -rf "$out"; mkdir -p "$out"
  log "rescued $s/$tag $old -> $new"
  python3 -I "$HERE/five_base_rescued.py" rescued --fastq "$r1" --all-bases \
    --old "$(ls "$base/$old.rep1"/*_pe.bam)" --new "$(ls "$base/$new.rep1"/*_pe.bam)" \
    --dragen "$ROOT/$s/dragen/dragen.cpg.txt" --out "$out" > "$out/rescued.log" 2>&1 \
    || die "rescued $s/$tag failed (see $out/rescued.log)"
  mkdir -p "$keep"
  cp "$out/rescued.json" "$out/rescued.log" "$keep/"
  log "rescued $s/$tag done: $(tail -1 "$out/rescued.log")"
}

cmd_verdict() {
  local s=$1 tag=$2 old=$3 new=$4 r=$RESULTS/$1/$2 rc=0
  python3 -I "$HERE/five_base_rescued.py" verdict --sample "$s" \
    --rescued "$r/rescued_${old}_$new/rescued.json" --old-arm "$old" --new-arm "$new" \
    --old-dir "$r/$old.rep1" --new-dir "$r/$new.rep1" --score "$r/score.tsv" \
    --out "$r/verdict_${old}_$new.tsv" || rc=$?
  [[ $rc -eq 0 ]] && log "verdict $s/$tag: every criterion passes" || log "verdict $s/$tag: NOT all criteria pass"
  return "$rc"
}

# Option checks accept each arm's real options and reject a wrong one.
cmd_selftest() {
  local t; t=$(mktemp -d "${TMPDIR:-/tmp}/fbe_selftest.XXXXXX")
  local ok=0 bad=0
  expect() {  # expect pass|fail <arm> <report-options> <pg-command-line>
    local want=$1 arm=$2 got=pass
    printf 'Bismark was run with %s against the unconverted genome of /g with the specified options: %s\n' \
      "$(engine_of "$arm")" "$3" > "$t/report"
    ( verify_options "$arm" "$t/report" && check_pg_line "$arm" $'@PG\tID:Bismark\tVN:v0.25.1\tCL:"'"$4"'"' ) \
      > /dev/null 2>&1 || got=fail
    if [[ $got == "$want" ]]; then ok=$((ok + 1)); else bad=$((bad + 1)); echo "selftest: $arm expected $want: $3 | $4"; fi
  }
  engine_of() { case $1 in A) echo minimap2 ;; C) echo HISAT2 ;; *) echo "Bowtie 2" ;; esac; }
  local p="-p $THREADS" i5=--illumina_5base bt="--bowtie2 --five_base_index $BT2_INDEX" ht="--hisat2 --five_base_index $HT2_INDEX"
  expect pass A "-a --MD --secondary=no -t $THREADS -x sr -K 250K" "bismark $i5 $p --genome /g"
  expect fail A "-a --MD --secondary=no -t 8 -x sr -K 250K" "bismark $i5 $p --genome /g"
  expect fail A "-a --MD --secondary=no -t $THREADS -x sr -K 250K" "bismark $i5 -p 8 --genome /g"
  local nd=--no_dovetail uq=--five_base_umi_qname
  expect pass B "-q --score-min L,0,-0.6 --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt -X 500 $p"
  expect fail B "-q --score-min L,0,-0.6 --maxins 500 -p $THREADS" "bismark $i5 $bt -X 500 $p"
  expect fail B "-q --score-min L,0,-0.6 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt -X 500 $p"
  expect fail B "-q --score-min L,0,-0.6 --dovetail --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt -X 500 $p"
  expect fail B "-q --score-min L,0,-0.6 --maxins 500 --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt -X 500 $p"
  expect fail B "-q --score-min L,0,-0.6 --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect pass B-loc "-q --local --score-min G,20,8 --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt --local -X 500 $p"
  expect fail B-loc "-q --local --score-min G,20,8 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt --local -X 500 $p"
  expect fail B-loc "-q --local --score-min G,20,8 --dovetail --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt --local -X 500 $p"
  expect fail B-loc "-q --local --score-min G,20,8 --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt --local $p"
  expect fail B-loc "-q --score-min L,0,-0.6 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect pass B-X "-q --score-min L,0,-0.6 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt -X 800 $p"
  expect fail B-X "-q --score-min L,0,-0.6 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect fail B-X "-q --score-min L,0,-0.6 --dovetail --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt -X 800 $p"
  local old="-q --score-min L,0,-0.6 --maxins 800 -p $THREADS --reorder"
  expect pass B-old "$old" "bismark $i5 $bt -X 800 $nd $uq $p"
  expect fail B-old "$old" "bismark $i5 $bt -X 800 $nd $p"
  expect fail B-old "-q --score-min L,0,-0.6 --dovetail --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt -X 800 $nd $uq $p"
  expect fail B-old "-q --score-min L,0,-0.6 --maxins 800 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt -X 800 $nd $uq $p"
  local new="-q --score-min L,0,-0.6 --maxins 800 -p $THREADS --reorder"
  expect pass B-new "$new" "bismark $i5 $bt $p"
  expect fail B-new "-q --score-min L,0,-0.6 --dovetail --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect fail B-new "-q --score-min L,0,-0.6 --maxins 800 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect fail B-new "-q --score-min L,0,-0.6 --maxins 500 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect fail B-new "-q --score-min L,0,-0.6 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect fail B-new "-q --score-min L,0,-0.6 --no-mixed --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect fail B-new "-q --local --score-min G,20,8 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt $p"
  expect fail B-new "$new" "bismark $i5 $bt $p -X 800"
  expect fail B-new "$new" "bismark $i5 $bt $p --maxins 800"
  expect fail B-new "$new" "bismark $i5 $bt $p --dovetail"
  expect fail B-new "$new" "bismark $i5 $bt $p --no_dovetail"
  expect fail B-new "$new" "bismark $i5 --hisat2 --five_base_index $HT2_INDEX $p"
  expect pass C "-q --score-min L,0,-0.6 --no-spliced-alignment -p $THREADS --reorder" "bismark $i5 $ht $p"
  expect fail C "-q --score-min L,0,-0.6 --no-spliced-alignment --no-spliced-alignment -p $THREADS --reorder" "bismark $i5 $ht $p"
  expect fail C "-q --score-min L,0,-0.6 --no-spliced-alignment -p $THREADS --reorder" "bismark $ht $p"
  expect fail A "-a --MD --secondary=no -t $THREADS -x map-ont -K 250K" "bismark $i5 $p --genome /g"
  expect fail B-loc "-q --local --score-min G,20,8 --maxins 500 -p $THREADS" "bismark $i5 $bt --local -X 500 $p"
  expect fail B-X "-q --local --score-min G,20,8 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt -X 800 $p"
  expect fail C "-q --local --score-min L,0,-0.6 --no-spliced-alignment -p $THREADS --reorder" "bismark $i5 $ht $p"
  expect fail B-X "-q --score-min L,0,-0.6 --maxins 800 -p $THREADS --reorder" "bismark $i5 $bt -X 8000 $p"
  check() { if "$@"; then ok=$((ok + 1)); else bad=$((bad + 1)); echo "selftest: $*"; fi; }
  check [ "$(count_tok --maxins "-q --maxins 800 --maxins")" -eq 2 ]
  check [ "$(count_tok --dovetail "--dovetail --dovetail")" -eq 2 ]
  check [ "$(count_tok --dovetail "-q --no_dovetail")" -eq 0 ]
  # B-old can never be re-run, so its cross-check cells cannot be wiped.
  local msg; msg=$( (cmd_cell NA12878 screen B-old) 2>&1 ) && bad=$((bad + 1))
  check [ "$msg" = "ERROR: B-old is the rev 3 cross-check arm; not re-runnable" ]
  # compare_cells: equal tuples pass; any one differing metric, count or own-set fails.
  local c; for c in "${REPRO_COLS[@]}"; do printf '%s\t' "$c"; done > "$t/m1.tsv"
  { echo; for c in "${REPRO_COLS[@]}"; do printf '1\t'; done; echo; } >> "$t/m1.tsv"
  sed 's/^1\t1\t/1\t2\t/' "$t/m1.tsv" > "$t/m2.tsv"
  printf '  precision (any DRAGEN C>T/G>A):  9/10 = 90%%\n  recall:   5/10 = 50%%\n' > "$t/d1.txt"
  sed 's|9/10|8/10|' "$t/d1.txt" > "$t/d2.txt"
  check compare_cells "$t/o" a "$t/m1.tsv" "$t/d1.txt" "5 0.9" b "$t/m1.tsv" "$t/d1.txt" "5 0.9"
  check [ "$(grep -c . "$t/o")" -eq $(( ${#REPRO_COLS[@]} + 3 )) ]
  check eval '! compare_cells "$t/o" a "$t/m1.tsv" "$t/d1.txt" "5 0.9" b "$t/m2.tsv" "$t/d1.txt" "5 0.9" > /dev/null'
  check eval '! compare_cells "$t/o" a "$t/m1.tsv" "$t/d1.txt" "5 0.9" b "$t/m1.tsv" "$t/d2.txt" "5 0.9" > /dev/null'
  check eval '! compare_cells "$t/o" a "$t/m1.tsv" "$t/d1.txt" "5 0.9" b "$t/m1.tsv" "$t/d1.txt" "6 0.9" > /dev/null'
  check eval '! compare_cells "$t/o" a "$t/m1.tsv" "$t/d1.txt" "" b "$t/m1.tsv" "$t/d1.txt" "" > /dev/null'
  check [ "$(deconv_fracs "$t/d1.txt")" = "9/10 5/10" ]
  rm -r "$t"
  echo "selftest: $ok passed, $bad failed"
  [[ $bad -eq 0 ]]
}

case ${1:-} in
  prep) shift; cmd_prep "$@" ;;
  cell) shift; cmd_cell "$@" ;;
  score) shift; cmd_score "$@" ;;
  versions) cmd_versions ;;
  repro-input) shift; cmd_repro_input "$@" ;;
  repro) shift; cmd_repro "$@" ;;
  repro-compare) shift; cmd_repro_compare "$@" ;;
  rescued) shift; cmd_rescued "$@" ;;
  verdict) shift; cmd_verdict "$@" ;;
  selftest) cmd_selftest ;;
  *) sed -n '2,30p' "$0"; exit 2 ;;
esac
