set -u
cd /work
# Bigger input so index load stops dominating: replicate the 5k pairs 60x = 300k pairs.
for m in 1 2; do
  : > big_R$m.fq
  for i in $(seq 60); do gzip -dc R$m.fq.gz >> big_R$m.fq; done
  gzip -c big_R$m.fq > big_R$m.fq.gz
done
echo "input: $(du -ch big_R1.fq.gz big_R2.fq.gz | tail -1 | cut -f1) gzipped, $(du -ch big_R1.fq big_R2.fq | tail -1 | cut -f1) raw, $(( $(wc -l < big_R1.fq) / 4 )) pairs"
rm -f big_R1.fq big_R2.fq

python3 - <<'PY'
seq=[]
for l in open("ecoli.fa"):
    if l.startswith(">"): hdr=l
    else: seq.append(l.strip().upper())
s="".join(seq)
open("ecoli_CT.fa","w").write(hdr+"\n".join(s[i:i+60].replace("C","T") for i in range(0,len(s),60))+"\n")
PY
bowtie2-build -q ecoli_CT.fa idx >/dev/null 2>&1
BT2OPTS="-q --phred33 -L 20 --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --minins 0 --maxins 500"
# awk converter: C-speed, stands in for the Rust converter (which is faster still)
conv() { gzip -dc "$1" | awk -v t="$2" 'NR%4==2{ if(t=="ct") gsub(/C/,"T"); else gsub(/G/,"A"); print; next } {print}' > "$3"; }

ms() { echo $(( ($2 - $1) / 1000000 )); }

echo
echo "=== SERIAL (current shape): convert both mates to disk, THEN run both instances ==="
for rep in 1 2 3; do
  s=$(date +%s%N)
  conv big_R1.fq.gz ct ct_1.fq
  conv big_R2.fq.gz ga ga_2.fq
  c=$(date +%s%N)
  peak=$(du -c ct_1.fq ga_2.fq | tail -1 | cut -f1)
  bowtie2 $BT2OPTS -p 4 --reorder --norc -x idx -1 ct_1.fq -2 ga_2.fq -S /dev/null 2>/dev/null &
  bowtie2 $BT2OPTS -p 4 --reorder --nofw -x idx -1 ct_1.fq -2 ga_2.fq -S /dev/null 2>/dev/null &
  wait
  e=$(date +%s%N)
  echo "  rep$rep: convert $(ms $s $c) ms + align $(ms $c $e) ms = $(ms $s $e) ms total; converted-file peak ${peak} KB"
  rm -f ct_1.fq ga_2.fq
done

echo
echo "=== STREAMED: conversion overlapped with alignment, nothing on disk ==="
for rep in 1 2 3; do
  rm -f p1 p2 p3 p4; mkfifo p1 p2 p3 p4
  s=$(date +%s%N)
  # one conversion pass per mate, tee'd to both instances
  gzip -dc big_R1.fq.gz | awk 'NR%4==2{gsub(/C/,"T")} 1' | tee p1 > p3 &
  gzip -dc big_R2.fq.gz | awk 'NR%4==2{gsub(/G/,"A")} 1' | tee p2 > p4 &
  bowtie2 $BT2OPTS -p 4 --reorder --norc -x idx -1 p1 -2 p2 -S /dev/null 2>/dev/null &
  bowtie2 $BT2OPTS -p 4 --reorder --nofw -x idx -1 p3 -2 p4 -S /dev/null 2>/dev/null &
  wait
  e=$(date +%s%N)
  echo "  rep$rep: $(ms $s $e) ms total; converted-file peak 0 KB"
done
