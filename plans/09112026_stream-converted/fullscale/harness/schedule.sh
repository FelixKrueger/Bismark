#!/bin/sh
# The remaining validation, in priority order, run STRICTLY SERIALLY and
# unattended. Each phase snapshots its results into the repo and commits, so a
# lost session costs at most the phase in flight.
#
# Order is by value-per-hour: the section 7 edge probes first (cheap, and 7.2 is
# the premise of the feature), then C1/C2 byte-identity at full scale for every
# shape, then the C3 timing reps that need repetition rather than size.
set -u
export PATH=/usr/bin:/bin:/usr/local/bin
FS=/tmp/fs
LOG=$FS/schedule.log
D10_1=/fs/data/SRR24766921_10M_1.fastq.gz
D10_2=/fs/data/SRR24766921_10M_2.fastq.gz
D2_1=/fs/data/SRR24766921_2M_1.fastq.gz
D2_2=/fs/data/SRR24766921_2M_2.fastq.gz

say() { printf '\n########## %s  [%s] ##########\n' "$*" "$(date -u +%H:%M:%S)" | tee -a "$LOG"; }
snap() { sh $FS/bin/snapshot.sh "$1" 2>&1 | tee -a "$LOG"; }

# --- S1: the 2M subset, used by everything that needs reps rather than size ---
say "S1 prepare 2M subset"
if [ ! -f $FS/data/SRR24766921_2M_1.fastq.gz ]; then
  for m in 1 2; do
    zcat $FS/data/SRR24766921_10M_${m}.fastq.gz | head -n 8000000 \
      | gzip -1 > $FS/data/SRR24766921_2M_${m}.fastq.gz
  done
fi
ls -la $FS/data/ >> "$LOG" 2>&1

# --- S2: section 7 edge probes ---
say "S2 section 7 edge probes"
docker run --rm -t -v $FS:/fs -v bismark-target-189:/t \
  --tmpfs /tiny:rw,size=512m \
  -e R1=$D2_1 -e R2=$D2_2 -e GENOME=/fs/genome/GRCm39 -e WORK=/fs/edge -e TINY=/tiny \
  -w /fs bismark-ab /fs/bin/edge.sh > $FS/edge.log 2>&1
echo "section 7 exit: $?" >> "$LOG"
tail -60 $FS/edge.log >> "$LOG"
snap "section 7 edge probes"

# --- S3..S5: C1/C2 byte-identity, one rep, the widest shapes at full scale ---
say "S3 C1/C2 pe_nondirectional_p4 @10M"
FS=$FS WORK=$FS/run REPS=1 SHAPES=pe_nondirectional_p4 R1=$D10_1 R2=$D10_2 \
  sh $FS/bin/drive.sh >> "$LOG" 2>&1
snap "C1/C2 non-directional PE at 10M"

say "S4 C1/C2 pe_directional_mc2 @10M"
FS=$FS WORK=$FS/run REPS=1 SHAPES=pe_directional_mc2 R1=$D10_1 R2=$D10_2 \
  sh $FS/bin/drive.sh >> "$LOG" 2>&1
snap "C1/C2 --multicore 2 at 10M"

# -p 1 gets two cores against eight, so 10M would cost ~6 h for both arms. C1/C2
# is a property of the record stream, not of read count, and -p 1's interest is
# the fan-out's least slack, which is reached in seconds and held. Run it at 2M.
say "S5 C1/C2 pe_directional_p1 @2M"
FS=$FS WORK=$FS/run2m REPS=1 SHAPES=pe_directional_p1 R1=$D2_1 R2=$D2_2 \
  sh $FS/bin/drive.sh >> "$LOG" 2>&1
snap "C1/C2 -p 1 at 2M"

# --- S6: C3 timing. Reps and interleaving, at a size that still runs for minutes ---
say "S6 C3 timing, 5 reps @2M, three shapes"
FS=$FS WORK=$FS/run2m REPS=5 R1=$D2_1 R2=$D2_2 \
  SHAPES="pe_directional_p4 pe_nondirectional_p4 pe_directional_mc2" \
  sh $FS/bin/drive.sh >> "$LOG" 2>&1
snap "C3 timing, 5 reps at 2M"

say "S7 C3 timing, 3 more reps of -p 1 @2M"
for r in 2 3 4; do
  FS=$FS WORK=$FS/run2m REPS=1 SHAPES=pe_directional_p1 R1=$D2_1 R2=$D2_2 \
    REP_OFFSET=$r sh $FS/bin/drive.sh >> "$LOG" 2>&1
done
snap "C3 timing, -p 1 reps at 2M"

# --- S8: fill whatever time is left with full-scale reps of the headline shape ---
say "S8 C3 at full scale, headline shape, until the clock runs out"
r=2
while [ "$r" -le 6 ]; do
  # Stop if we are inside two hours of the session deadline, so the write-up
  # is never the thing that gets cut.
  now=$(date -u +%s); deadline=$(date -u -d '2026-09-12 12:00:00' +%s 2>/dev/null || echo 0)
  [ "$deadline" -gt 0 ] && [ "$now" -ge "$deadline" ] && { echo "deadline guard: stopping S8" >> "$LOG"; break; }
  FS=$FS WORK=$FS/run REPS=1 SHAPES=pe_directional_p4 R1=$D10_1 R2=$D10_2 \
    REP_OFFSET=$r sh $FS/bin/drive.sh >> "$LOG" 2>&1
  snap "C3 full-scale rep $r"
  r=$((r + 1))
done

say "SCHEDULE COMPLETE"
snap "schedule complete"
