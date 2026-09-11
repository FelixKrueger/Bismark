#!/usr/bin/env python3
"""Minimal stand-in for bismark's in-silico conversion: stream FastQ -> converted FastQ."""
import gzip, sys, io
src, dst, kind = sys.argv[1], sys.argv[2], sys.argv[3]
tbl = str.maketrans("C", "T") if kind == "ct" else str.maketrans("G", "A")
op = gzip.open if src.endswith(".gz") else open
with op(src, "rt") as fh, open(dst, "w") as out:
    while True:
        i = fh.readline()
        if not i: break
        s, i2, q = fh.readline(), fh.readline(), fh.readline()
        out.write(i)
        out.write(s.upper().translate(tbl))
        out.write(i2); out.write(q)
