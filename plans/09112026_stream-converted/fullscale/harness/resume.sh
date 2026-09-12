#!/bin/sh
# Resume after the WORK-passthrough bug. Phases S5 onward, with:
#   - the -p 1 shape replaced by -p 2 (bismark rejects -p 1)
#   - WORK correctly translated into the container
#   - S8 reallocated: full-scale reps of BOTH 10M shapes, not just the headline
#     one, because non-directional showed a 2.4% streamed regression at n=1 and
#     that is now the most valuable thing left to measure.
set -u
export PATH=/usr/bin:/bin:/usr/local/bin
FS=/tmp/fs
LOG=$FS/schedule.log
D10_1=/fs/data/SRR24766921_10M_1.fastq.gz
D10_2=/fs/data/SRR24766921_10M_2.fastq.gz
D2_1=/fs/data/SRR24766921_2M_1.fastq.gz
D2_2=/fs/data/SRR24766921_2M_2.fastq.gz
GUARD=$(date -u -d '2026-09-12 12:00:00' +%s)

say() { printf '\n########## %s  [%s] ##########\n' "$*" "$(date -u +%H:%M:%S)" | tee -a "$LOG"; }
snap() { sh $FS/bin/snapshot.sh "$1" 2>&1 | tee -a "$LOG"; }
past_guard() { [ "$(date -u +%s)" -ge "$GUARD" ]; }

say "S5 C1/C2 pe_directional_p2 @2M (FULLSCALE asks -p 1; bismark requires >=2)"
FS=$FS WORK=$FS/run2m REPS=1 SHAPES=pe_directional_p2 R1=$D2_1 R2=$D2_2 \
  sh $FS/bin/drive.sh >> "$LOG" 2>&1
snap "C1/C2 -p 2 at 2M"

say "S6 C3 timing, 5 reps @2M, all four shapes"
FS=$FS WORK=$FS/run2m REPS=5 R1=$D2_1 R2=$D2_2 \
  SHAPES="pe_directional_p4 pe_nondirectional_p4 pe_directional_mc2" \
  sh $FS/bin/drive.sh >> "$LOG" 2>&1
snap "C3 timing, 5 reps at 2M, three shapes"

say "S7 C3 timing, -p 2 reps @2M"
FS=$FS WORK=$FS/run2m REPS=3 REP_OFFSET=2 SHAPES=pe_directional_p2 R1=$D2_1 R2=$D2_2 \
  sh $FS/bin/drive.sh >> "$LOG" 2>&1
snap "C3 timing, -p 2 reps at 2M"

# --- S8: full-scale reps, alternating the two 10M shapes -------------------
# Non-directional first in each round: it carries the open question.
say "S8 full-scale reps of both 10M shapes, until the 12:00 guard"
r=2
while [ "$r" -le 6 ]; do
  for shape in pe_nondirectional_p4 pe_directional_p4; do
    past_guard && { echo "deadline guard: stopping S8" >> "$LOG"; break 2; }
    FS=$FS WORK=$FS/run REPS=1 REP_OFFSET=$r SHAPES=$shape R1=$D10_1 R2=$D10_2 \
      sh $FS/bin/drive.sh >> "$LOG" 2>&1
    snap "full-scale $shape rep $r"
  done
  r=$((r + 1))
done

say "SCHEDULE COMPLETE"
snap "schedule complete"
