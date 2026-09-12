#!/usr/bin/env python
"""Generate the summary tables from results.tsv / C1.tsv.

Hand-transcribing 50 runs into markdown is how a write-up ends up disagreeing with
its own data. These tables are produced from the files the harness wrote.
"""
import pathlib, pandas as pd

SRC = [("/tmp/fs/run/results.tsv", "/tmp/fs/run/C1.tsv", "10M"),
       ("/tmp/fs/run2m/results.tsv", "/tmp/fs/run2m/C1.tsv", "2M")]
GiB = 1073741824

frames = []
for r, c, scale in SRC:
    p = pathlib.Path(r)
    if p.exists():
        d = pd.read_csv(p, sep="\t"); d["scale"] = scale; frames.append(d)
df = pd.concat(frames, ignore_index=True)
df = df[df.rc == 0]

print("## C3 — wall time\n")
print("| shape | scale | n | streamed median | files median | delta |")
print("|---|---|---|---|---|---|")
order = {"10M": 0, "2M": 1}
for (sh, sc), d in sorted(df.groupby(["shape", "scale"]), key=lambda kv: (order[kv[0][1]], kv[0][0])):
    s, f = d[d.arm == "stream"].wall_s, d[d.arm == "files"].wall_s
    n = min(len(s), len(f))
    if not n:
        continue
    delta = (s.median() - f.median()) / f.median() * 100
    print(f"| `{sh}` | {sc} | {n} | {s.median():.1f} s | {f.median():.1f} s | **{delta:+.2f} %** |")

print("\n## C2 — converted reads on disk (peak, sampled live)\n")
print("| shape | scale | streamed | files |")
print("|---|---|---|---|")
for (sh, sc), d in sorted(df.groupby(["shape", "scale"]), key=lambda kv: (order[kv[0][1]], kv[0][0])):
    s, f = d[d.arm == "stream"].peak_conv_kb, d[d.arm == "files"].peak_conv_kb
    if not len(f):
        continue
    print(f"| `{sh}` | {sc} | **{s.max()} KB** | {f.max()/1048576:.2f} GiB |")

print("\n## Memory (peak anonymous, streamed vs files)\n")
print("| shape | scale | streamed | files |")
print("|---|---|---|---|")
for (sh, sc), d in sorted(df.groupby(["shape", "scale"]), key=lambda kv: (order[kv[0][1]], kv[0][0])):
    s = pd.to_numeric(d[d.arm == "stream"].anon_peak_bytes, errors="coerce").max()
    f = pd.to_numeric(d[d.arm == "files"].anon_peak_bytes, errors="coerce").max()
    if pd.isna(s) or pd.isna(f):
        continue
    print(f"| `{sh}` | {sc} | {s/GiB:.2f} GiB | {f/GiB:.2f} GiB |")

tot = bad = 0
for _, c, _ in SRC:
    p = pathlib.Path(c)
    if not p.exists():
        continue
    d = pd.read_csv(p, sep="\t")
    d = d[d.bam_verdict != "NO BAM"]
    tot += len(d)
    bad += int(((d.bam_verdict != "IDENTICAL") | (d.report_verdict != "IDENTICAL")).sum())
print(f"\n## C1 — {tot} arm-pairs compared, {bad} differing\n")
print(f"Total benchmark runs recorded: {len(df)}")
