#!/bin/sh
# Extra full-scale reps of pe_directional_p4, the shape whose per-rep spread is
# comparable to the effect being measured. Hard stop well before the session ends:
# a rep that cannot finish AND be written up is worse than no rep.
set -u
FS=/tmp/fs
STOP=$(date -u -d '2026-09-12 10:15:00' +%s)
for r in 4 5; do
  now=$(date -u +%s)
  # each pair is ~100 min; do not start one that cannot finish by STOP
  [ $((now + 6300)) -ge "$STOP" ] && { echo "guard: no time for rep $r" >> $FS/schedule.log; break; }
  printf '\n########## EXTRA pe_directional_p4 rep %s  [%s] ##########\n' "$r" "$(date -u +%H:%M:%S)" >> $FS/schedule.log
  FS=$FS WORK=$FS/run REPS=1 REP_OFFSET=$r SHAPES=pe_directional_p4 \
    R1=/fs/data/SRR24766921_10M_1.fastq.gz R2=/fs/data/SRR24766921_10M_2.fastq.gz \
    sh $FS/bin/drive.sh >> $FS/schedule.log 2>&1
  sh $FS/bin/snapshot.sh "extra full-scale rep $r" >> $FS/schedule.log 2>&1
done
echo "EXTRA REPS DONE $(date -u +%H:%M:%S)" >> $FS/schedule.log
