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

echo "### baselines from disk"
bowtie2 $BT2OPTS -p 2 --reorder --norc -x idx -1 ct_1.fq -2 ga_2.fq -S A.sam  2>/dev/null
bowtie2 $BT2OPTS -p 2 --reorder --nofw -x idx -1 ct_1.fq -2 ga_2.fq -S A2.sam 2>/dev/null
echo "  done"

# The directional PE shape: 2 instances x 2 mates = 4 FIFOs, fed by 2 conversion
# passes (one per mate), each tee'd to both instances.
for DEPTH in 1 4 64; do
  echo; echo "### tee fan-out, queue depth $DEPTH record-blocks"
  rm -f q1 q2 q3 q4; mkfifo q1 q2 q3 q4
  timeout 120 python3 tee_convert.py R1.fq.gz ct $DEPTH q1 q3 &
  timeout 120 python3 tee_convert.py R2.fq.gz ga $DEPTH q2 q4 &
  timeout 120 bowtie2 $BT2OPTS -p 2 --reorder --norc -x idx -1 q1 -2 q2 -S T1.sam 2>/dev/null &
  B1=$!
  timeout 120 bowtie2 $BT2OPTS -p 2 --reorder --nofw -x idx -1 q3 -2 q4 -S T2.sam 2>/dev/null &
  B2=$!
  wait $B1; E1=$?; wait $B2; E2=$?; wait
  echo "  exits: $E1 $E2"
  echo "  instance 1 (--norc):"; same A.sam T1.sam
  echo "  instance 2 (--nofw):"; same A2.sam T2.sam
done

echo; echo "### asymmetric consumers: one instance deliberately starved (depth 1, -p 1 vs -p 4)"
rm -f r1 r2 r3 r4; mkfifo r1 r2 r3 r4
timeout 180 python3 tee_convert.py R1.fq.gz ct 1 r1 r3 &
timeout 180 python3 tee_convert.py R2.fq.gz ga 1 r2 r4 &
timeout 180 bowtie2 $BT2OPTS -p 1 --reorder --norc -x idx -1 r1 -2 r2 -S S1.sam 2>/dev/null &
P1=$!
timeout 180 bowtie2 $BT2OPTS -p 4 --reorder --nofw -x idx -1 r3 -2 r4 -S S2.sam 2>/dev/null &
P2=$!
wait $P1; F1=$?; wait $P2; F2=$?; wait
echo "  exits: $F1 $F2 (124 would mean deadlock/starvation)"
same A.sam S1.sam; same A2.sam S2.sam
