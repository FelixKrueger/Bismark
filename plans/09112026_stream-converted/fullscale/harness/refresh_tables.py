#!/usr/bin/env python
"""Regenerate the C3 table in BENCHMARKS.md from results.tsv.

The table is written between markers so the document can never drift from the data
it describes -- a row was once typed ahead of the run that would produce it.
"""
import pathlib, re, pandas as pd

frames = []
for f, scale in [("/tmp/fs/run/results.tsv", "10M"), ("/tmp/fs/run2m/results.tsv", "2M")]:
    p = pathlib.Path(f)
    if p.exists():
        d = pd.read_csv(p, sep="\t"); d["scale"] = scale; frames.append(d)
df = pd.concat(frames, ignore_index=True)
df = df[df.rc == 0]

rows = []
for (sh, sc), d in df.groupby(["shape", "scale"]):
    s, f = d[d.arm == "stream"].wall_s, d[d.arm == "files"].wall_s
    n = min(len(s), len(f))
    if not n:
        continue
    rows.append((sh, sc, n, (s.median() - f.median()) / f.median() * 100))
rows.sort(key=lambda r: r[3])   # most negative (best) first

body = ["| shape | scale | n | delta |", "|---|---|---|---|"]
for sh, sc, n, d in rows:
    body.append(f"| `{sh}` | {sc} | {n} | **{d:+.2f} %** |".replace("+", "−" if d < 0 else "+")
                if False else f"| `{sh}` | {sc} | {n} | **{d:+.2f} %** |".replace("-", "−"))
table = "\n".join(body)

bm = pathlib.Path("plans/09112026_stream-converted/BENCHMARKS.md")
t = bm.read_text()
t = re.sub(r"<!-- C3TABLE:START -->.*?<!-- C3TABLE:END -->",
           "<!-- C3TABLE:START -->\n" + table + "\n<!-- C3TABLE:END -->", t, flags=re.S)
bm.write_text(t)
print("C3 table regenerated from data:")
print(table)
