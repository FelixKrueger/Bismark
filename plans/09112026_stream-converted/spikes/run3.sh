set -u
cd /work
recs() { grep -v '^@PG' "$1"; }
same() { if cmp -s <(recs "$1") <(recs "$2"); then echo "  RESULT: byte-identical"; else echo "  RESULT: *** DIFFERS ***"; diff <(recs "$1") <(recs "$2") | head -5; fi; }
python3 - <<'PY'
seq=[]
for l in open("ecoli.fa"):
    if l.startswith(">"): hdr=l
    else: seq.append(l.strip().upper())
s="".join(seq)
open("ecoli_CT.fa","w").write(hdr+"\n".join(s[i:i+60].replace("C","T") for i in range(0,len(s),60))+"\n")
PY
python3 convert.py R1.fq.gz ct_1.fq ct
python3 convert.py R2.fq.gz ga_2.fq ga
BT2OPTS="-q --phred33 -L 20 --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --minins 0 --maxins 500"
bowtie2-build -q ecoli_CT.fa idx >/dev/null 2>&1
hisat2-build -q ecoli_CT.fa hidx >/dev/null 2>&1
minimap2 -x map-ont -d ecoli_CT.mmi ecoli_CT.fa >/dev/null 2>&1

echo "### HISAT2 2.2.2 — PE over FIFOs"
rm -f h1 h2; mkfifo h1 h2
( timeout 30 cat ct_1.fq > h1 ) >/dev/null 2>&1 &
( timeout 30 cat ga_2.fq > h2 ) >/dev/null 2>&1 &
HS="-q --phred33 --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --norc --no-spliced-alignment"
timeout 60 hisat2 $HS -x hidx -1 h1 -2 h2 -S h_fifo.sam 2> h.err; echo "  exit=$?"
echo "  stderr: $(tr '\n' ' ' < h.err)"
sleep 31

echo; echo "### HISAT2 2.2.2 — SE over a FIFO"
rm -f hs1; mkfifo hs1
( timeout 30 cat ct_1.fq > hs1 ) >/dev/null 2>&1 &
timeout 60 hisat2 $HS -x hidx -U hs1 -S hse_fifo.sam 2> hse.err; echo "  exit=$?"
echo "  stderr: $(tr '\n' ' ' < hse.err)"
sleep 31

echo; echo "### minimap2 2.31 — SE over a FIFO"
timeout 300 minimap2 -a -x map-ont ecoli_CT.mmi ct_1.fq > m_file.sam 2>/dev/null
rm -f m1; mkfifo m1
( timeout 120 cat ct_1.fq > m1 ) >/dev/null 2>&1 &
timeout 300 minimap2 -a -x map-ont ecoli_CT.mmi m1 > m_fifo.sam 2> m.err; echo "  exit=$?"
grep -q . m_fifo.sam && same m_file.sam m_fifo.sam || echo "  RESULT: no output; stderr: $(tr '\n' ' ' < m.err)"

echo; echo "### bowtie2 wall time, regular file vs FIFO (cat writer, -p 4, 5 reps)"
for mode in file fifo file fifo; do
  tot=0
  for i in 1 2 3 4 5; do
    if [ "$mode" = file ]; then
      s=$(date +%s%N); bowtie2 $BT2OPTS -p 4 --reorder --norc -x idx -1 ct_1.fq -2 ga_2.fq -S /dev/null 2>/dev/null; e=$(date +%s%N)
    else
      rm -f t1 t2; mkfifo t1 t2
      ( cat ct_1.fq > t1 ) & ( cat ga_2.fq > t2 ) &
      s=$(date +%s%N); bowtie2 $BT2OPTS -p 4 --reorder --norc -x idx -1 t1 -2 t2 -S /dev/null 2>/dev/null; e=$(date +%s%N)
    fi
    tot=$((tot + (e-s)/1000000))
  done
  echo "  $mode: $((tot/5)) ms mean of 5"
done
