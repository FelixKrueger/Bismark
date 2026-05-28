#!/bin/sh
# Fake bismark2bedGraph that emits ~1 MiB of stderr then exits 0. Exercises
# the 64 KiB ring-buffer eviction policy in RealRunner.
# Each "x" line is ~32 bytes; 32_000 lines = ~1 MB.
i=0
while [ "$i" -lt 32000 ]; do
    echo "subprocess noise line $i xxxxxxxxxxx" >&2
    i=$((i + 1))
done
exit 0
