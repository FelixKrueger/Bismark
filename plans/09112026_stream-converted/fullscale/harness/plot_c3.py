#!/usr/bin/env python
"""C3 summary: wall time, streamed vs files, every shape and both scales.

Plotted as percent difference from each group's files median, not absolute
seconds. The effects here are under 2%; bars from zero render them as identical
rectangles and hide the entire result.
"""
import pathlib
import pandas as pd
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

OUT = pathlib.Path("/tmp/fs/plots"); OUT.mkdir(exist_ok=True)
SRC = [("/tmp/fs/run2m/results.tsv", "2M"), ("/tmp/fs/run/results.tsv", "10M")]
COL = {"stream": "#1f77b4", "files": "#d62728"}

rows = []
for path, scale in SRC:
    p = pathlib.Path(path)
    if not p.exists():
        continue
    df = pd.read_csv(p, sep="\t")
    df = df[df.rc == 0]
    for sh in df["shape"].unique():
        d = df[df["shape"] == sh]
        s, f = d[d.arm == "stream"].wall_s, d[d.arm == "files"].wall_s
        if not len(s) or not len(f):
            continue
        base = f.median()
        rows.append(dict(shape=sh, scale=scale, base=base,
                         stream=[(x / base - 1) * 100 for x in s],
                         files=[(x / base - 1) * 100 for x in f],
                         n=min(len(s), len(f)),
                         delta=(s.median() / base - 1) * 100))

rows.sort(key=lambda r: (r["scale"] != "10M", r["shape"]))
fig, ax = plt.subplots(figsize=(11, 5.6))
labels = []
for i, r in enumerate(rows):
    for arm, off in (("stream", -0.16), ("files", 0.16)):
        vals = r[arm]
        ax.scatter([i + off] * len(vals), vals, s=42, color=COL[arm], zorder=3,
                   alpha=0.8, edgecolors="white", linewidths=0.6,
                   label=("streamed (default)" if arm == "stream" else "files (--no_stream_converted)") if i == 0 else None)
        med = pd.Series(vals).median()
        ax.plot([i + off - 0.1, i + off + 0.1], [med, med], color=COL[arm], lw=2.4, zorder=4)
    ax.annotate(f"{r['delta']:+.2f}%", xy=(i, 0.965), xycoords=("data", "axes fraction"),
                ha="center", va="top", fontsize=9.5, weight="bold",
                color="#1a7a3a" if r["delta"] <= 0 else "#b03030")
    labels.append(f"{r['shape'].replace('pe_','')}\n{r['scale']} · n={r['n']}")

ax.axhline(0, color="#555", lw=1, ls="--", zorder=1)
# headroom so the per-group delta labels never sit on top of a data point
lo, hi = ax.get_ylim()
ax.set_ylim(lo, hi + (hi - lo) * 0.13)
ax.set_xticks(range(len(rows)))
ax.set_xticklabels(labels, fontsize=8.5)
ax.set_ylabel("wall time, % relative to the files median for that group")
ax.set_title("#1120 — C3: streamed vs files wall time. Below the line is faster.\n"
             "Thick ticks are medians; each dot is one rep.", fontsize=11)
ax.grid(alpha=0.25, lw=0.5, axis="y")
ax.legend(fontsize=9, loc="lower right", framealpha=0.95)
ax.margins(x=0.06)
fig.tight_layout()
fig.savefig(OUT / "c3_summary.png", dpi=150)
print("wrote", OUT / "c3_summary.png")

# Converted-bytes summary: the C2 result, which is the large one.
fig, ax = plt.subplots(figsize=(9, 4.4))
bars, names = [], []
for path, scale in SRC:
    p = pathlib.Path(path)
    if not p.exists():
        continue
    df = pd.read_csv(p, sep="\t")
    for sh in sorted(df["shape"].unique()):
        d = df[(df["shape"] == sh) & (df.rc == 0)]
        f = d[d.arm == "files"].peak_conv_kb
        s = d[d.arm == "stream"].peak_conv_kb
        if not len(f):
            continue
        bars.append((f.max() / 1048576, s.max() / 1048576))
        names.append(f"{sh.replace('pe_','')}\n{scale}")
x = range(len(bars))
ax.bar([i - 0.2 for i in x], [b[0] for b in bars], 0.4, color=COL["files"], alpha=0.85,
       label="files (--no_stream_converted)")
ax.bar([i + 0.2 for i in x], [b[1] for b in bars], 0.4, color=COL["stream"], alpha=0.85,
       label="streamed (default)")
for i, b in enumerate(bars):
    ax.text(i - 0.2, b[0], f"{b[0]:.2f}", ha="center", va="bottom", fontsize=8.5, weight="bold")
    ax.text(i + 0.2, b[1], "0", ha="center", va="bottom", fontsize=8.5, weight="bold",
            color=COL["stream"])
ax.set_xticks(list(x)); ax.set_xticklabels(names, fontsize=8.5)
ax.set_ylabel("peak converted reads on disk (GiB)")
ax.set_title("#1120 — C2: converted-read footprint in --temp_dir (peak, sampled live)")
ax.grid(alpha=0.25, lw=0.5, axis="y"); ax.legend(fontsize=9)
fig.tight_layout(); fig.savefig(OUT / "c2_summary.png", dpi=150)
print("wrote", OUT / "c2_summary.png")
