#!/usr/bin/env python3
"""One conversion pass, fanned out to N FIFOs via bounded queues + one writer
thread per FIFO. Models the proposed --stream_converted design: 1x CPU, bounded
memory, no coupling to the slowest consumer beyond the queue depth."""
import gzip, sys, threading, queue
src, kind, depth, *fifos = sys.argv[1], sys.argv[2], int(sys.argv[3]), *sys.argv[4:]
tbl = str.maketrans("C", "T") if kind == "ct" else str.maketrans("G", "A")
qs = [queue.Queue(maxsize=depth) for _ in fifos]

def writer(path, q):
    with open(path, "w") as out:
        while True:
            b = q.get()
            if b is None: break
            out.write(b)

ts = [threading.Thread(target=writer, args=(f, q), daemon=False) for f, q in zip(fifos, qs)]
for t in ts: t.start()
op = gzip.open if src.endswith(".gz") else open
with op(src, "rt") as fh:
    buf = []
    while True:
        i = fh.readline()
        if not i: break
        s, i2, qual = fh.readline(), fh.readline(), fh.readline()
        buf.append(i + s.upper().translate(tbl) + i2 + qual)
        if len(buf) >= 200:
            blk = "".join(buf); buf.clear()
            for q in qs: q.put(blk)
if buf:
    blk = "".join(buf)
    for q in qs: q.put(blk)
for q in qs: q.put(None)
for t in ts: t.join()
