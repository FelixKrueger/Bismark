set -u
cd /work
BT2OPTS="-q --phred33 -L 20 --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --minins 0 --maxins 500"
# Bismark parses only bowtie2's ALIGNMENT RECORDS and writes its own @PG
# (output.rs:107), so the @PG CL line (which necessarily names the input path) is
# excluded from every comparison. Everything else must match byte-for-byte.
recs() { grep -v '^@PG' "$1"; }
same() { if cmp -s <(recs "$1") <(recs "$2"); then echo "  byte-identical"; else echo "  *** DIFFERS ***"; diff <(recs "$1") <(recs "$2") | head -5; fi; }
rate() { grep "overall alignment rate" "$1" | tr -d ' '; }

echo "### versions"; bowtie2 --version | head -1

# C->T converted genome, as bismark_genome_preparation builds it
python3 - <<'PY'
seq=[]
for l in open("ecoli.fa"):
    if l.startswith(">"): hdr=l
    else: seq.append(l.strip().upper())
s="".join(seq)
open("ecoli_CT.fa","w").write(hdr+"\n".join(s[i:i+60].replace("C","T") for i in range(0,len(s),60))+"\n")
PY
bowtie2-build -q ecoli_CT.fa idx >/dev/null 2>&1 || { echo "FAIL build"; exit 1; }

python3 convert.py R1.fq.gz ct_1.fq ct
python3 convert.py R2.fq.gz ga_2.fq ga
echo "converted files on disk: $(du -ch ct_1.fq ga_2.fq | tail -1 | cut -f1) for a $(du -ch R1.fq.gz R2.fq.gz | tail -1 | cut -f1) gzipped input"

echo; echo "### A/A2: baseline, regular files (the two bismark directional instances)"
bowtie2 $BT2OPTS --norc -x idx -1 ct_1.fq -2 ga_2.fq -S A.sam  2> A.err
bowtie2 $BT2OPTS --nofw -x idx -1 ct_1.fq -2 ga_2.fq -S A2.sam 2> A2.err
echo "  --norc rate: $(rate A.err)   --nofw rate: $(rate A2.err)"

echo; echo "### B: FIFOs, one writer per mate"
rm -f b1 b2; mkfifo b1 b2
cat ct_1.fq > b1 & cat ga_2.fq > b2 &
bowtie2 $BT2OPTS --norc -x idx -1 b1 -2 b2 -S B.sam 2> B.err; echo "  exit=$?"; wait
echo "  rate: $(rate B.err)"; same A.sam B.sam

echo; echo "### C: FIFOs fed straight from the converter — zero converted bytes on disk"
rm -f c1 c2; mkfifo c1 c2
python3 convert.py R1.fq.gz c1 ct & python3 convert.py R2.fq.gz c2 ga &
bowtie2 $BT2OPTS --norc -x idx -1 c1 -2 c2 -S C.sam 2> C.err; echo "  exit=$?"; wait
echo "  rate: $(rate C.err)"; same A.sam C.sam

echo; echo "### D: both instances concurrently, independent FIFO pairs (real bismark shape)"
rm -f d1 d2 d3 d4; mkfifo d1 d2 d3 d4
python3 convert.py R1.fq.gz d1 ct & python3 convert.py R2.fq.gz d2 ga &
python3 convert.py R1.fq.gz d3 ct & python3 convert.py R2.fq.gz d4 ga &
bowtie2 $BT2OPTS --norc -x idx -1 d1 -2 d2 -S D1.sam 2> D1.err &
bowtie2 $BT2OPTS --nofw -x idx -1 d3 -2 d4 -S D2.sam 2> D2.err &
wait
echo "  D1 (--norc) vs A:"; same A.sam D1.sam
echo "  D2 (--nofw) vs A2:"; same A2.sam D2.sam

echo; echo "### E: gzipped bytes through a FIFO"
gzip -c ct_1.fq > ct_1.fq.gz; gzip -c ga_2.fq > ga_2.fq.gz
rm -f e1 e2; mkfifo e1 e2
cat ct_1.fq.gz > e1 & cat ga_2.fq.gz > e2 &
bowtie2 $BT2OPTS --norc -x idx -1 e1 -2 e2 -S E.sam 2> E.err; echo "  exit=$?"; wait
echo "  rate: $(rate E.err)"; same A.sam E.sam
echo "  (for contrast, gz as a REGULAR file:)"
bowtie2 $BT2OPTS --norc -x idx -1 ct_1.fq.gz -2 ga_2.fq.gz -S E2.sam 2> E2.err
echo "  rate: $(rate E2.err)"; same A.sam E2.sam

echo; echo "### F: -p 4 --reorder over FIFOs (how bismark threads bowtie2)"
bowtie2 $BT2OPTS -p 4 --reorder --norc -x idx -1 ct_1.fq -2 ga_2.fq -S Ff.sam 2>/dev/null
rm -f f1 f2; mkfifo f1 f2
python3 convert.py R1.fq.gz f1 ct & python3 convert.py R2.fq.gz f2 ga &
bowtie2 $BT2OPTS -p 4 --reorder --norc -x idx -1 f1 -2 f2 -S Fp.sam 2> F.err; echo "  exit=$?"; wait
same Ff.sam Fp.sam
echo "  and vs the single-threaded baseline:"; same A.sam Fp.sam

echo; echo "### G: single writer filling mate1 before mate2"
rm -f g1 g2; mkfifo g1 g2
timeout 40 bash -c 'cat ct_1.fq > g1; cat ga_2.fq > g2' &
timeout 40 bowtie2 $BT2OPTS --norc -x idx -1 g1 -2 g2 -S G.sam 2> G.err; GE=$?
wait 2>/dev/null
[ "$GE" = "124" ] && echo "  DEADLOCK (exit 124) — one writer per mate is mandatory" || echo "  completed, exit=$GE"
