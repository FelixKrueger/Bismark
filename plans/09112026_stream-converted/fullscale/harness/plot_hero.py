#!/usr/bin/env python
"""The four resources, one figure: disk, CPU, memory, output — over the life of a
full-scale run, streamed against files.

Compact enough to paste into a PR comment, where a six-panel tower is unreadable.
"""
import pathlib, pandas as pd
import matplotlib; matplotlib.use("Agg")
import matplotlib.pyplot as plt

OUT = pathlib.Path("/tmp/fs/plots"); OUT.mkdir(exist_ok=True)
ST = {"stream": ("#1f77b4", "streamed (default)"),
      "files":  ("#d62728", "files (--no_stream_converted)")}

def draw(tag, title, fname, rep=1):
    fig, ax = plt.subplots(2, 2, figsize=(13, 6.4))
    panels = [
        (ax[0][0], "converted_kb", 1048576, "Converted reads on disk (GiB)", "{:.2f} GiB"),
        (ax[0][1], "cpu_pct",      1,       "CPU (% of one core)",           "{:.0f}%"),
        (ax[1][0], "anon_bytes",   1073741824, "Memory (GiB)\nsolid: allocation · dashed: incl. page cache", "{:.2f} GiB"),
        (ax[1][1], "out_kb",       1048576, "Output written (GiB)",          "{:.2f} GiB"),
    ]
    ok = False
    for a, col, div, ylab, fmt in panels:
        for arm, (c, lab) in ST.items():
            f = pathlib.Path(f"/tmp/fs/run/series/{tag}_{arm}_r{rep}.tsv")
            if not f.exists():
                continue
            df = pd.read_csv(f, sep="\t")
            if col not in df:
                a.text(.5, .5, "not recorded for this run", ha="center", va="center",
                       transform=a.transAxes, fontsize=9, color="#888"); continue
            ok = True
            a.plot(df.t_s / 60, df[col] / div, lw=1.2, color=c, label=lab)
            if col == "anon_bytes" and "mem_bytes" in df:
                # cgroup memory.current counts page cache, so the file arm's total runs
                # gigabytes above its allocation purely because its own converted files
                # are cached. Showing both makes that unmistakable.
                a.plot(df.t_s / 60, df.mem_bytes / div, lw=1.0, color=c, ls="--", alpha=.55)
            a.text(0.985, 0.93 if arm == "stream" else 0.79,
                   f"peak {fmt.format(df[col].max()/div)}", transform=a.transAxes,
                   ha="right", va="top", fontsize=9, color=c, weight="bold")
        a.set_ylabel(ylab, fontsize=9.5); a.grid(alpha=.25, lw=.5)
        a.set_ylim(bottom=0); a.margins(x=.01)
        a.set_xlabel("minutes since launch", fontsize=9)
    if not ok:
        plt.close(fig); return
    ax[0][0].legend(fontsize=9, loc="center right")
    fig.suptitle(title, fontsize=12.5)
    fig.tight_layout(rect=[0, 0, 1, 0.96])
    fig.savefig(OUT / fname, dpi=145); plt.close(fig)
    print("wrote", OUT / fname)

draw("pe_directional_p4",
     "#1120 — directional PE, 10M read pairs, mouse GRCm39: disk, CPU, memory, output",
     "hero_directional_10M.png", rep=4)
draw("pe_nondirectional_p4",
     "#1120 — non-directional PE, 10M read pairs: disk, CPU, memory, output",
     "hero_nondirectional_10M.png", rep=1)

# Peak anonymous memory per shape, both arms.
frames = []
for f, sc in [("/tmp/fs/run/results.tsv", "10M"), ("/tmp/fs/run2m/results.tsv", "2M")]:
    p = pathlib.Path(f)
    if p.exists():
        d = pd.read_csv(p, sep="\t"); d["scale"] = sc; frames.append(d)
df = pd.concat(frames, ignore_index=True); df = df[df.rc == 0]
df["anon"] = pd.to_numeric(df.anon_peak_bytes, errors="coerce")
groups = [g for g in sorted(df.groupby(["shape", "scale"]),
                            key=lambda kv: (kv[0][1] != "10M", kv[0][0]))]
names, sv, fv = [], [], []
for (sh, sc), d in groups:
    a = d[d.arm == "stream"].anon.max(); b = d[d.arm == "files"].anon.max()
    if pd.isna(a) or pd.isna(b):
        continue
    names.append(f"{sh.replace('pe_','')}\n{sc}"); sv.append(a/1073741824); fv.append(b/1073741824)
fig, ax = plt.subplots(figsize=(10, 4.2))
x = range(len(names))
ax.bar([i-0.2 for i in x], fv, 0.4, color=ST["files"][0], alpha=.85, label=ST["files"][1])
ax.bar([i+0.2 for i in x], sv, 0.4, color=ST["stream"][0], alpha=.85, label=ST["stream"][1])
for i, (a, b) in enumerate(zip(sv, fv)):
    ax.text(i-0.2, b, f"{b:.1f}", ha="center", va="bottom", fontsize=8.5)
    ax.text(i+0.2, a, f"{a:.1f}", ha="center", va="bottom", fontsize=8.5, color=ST["stream"][0], weight="bold")
ax.set_xticks(list(x)); ax.set_xticklabels(names, fontsize=8.5)
ax.set_ylabel("peak anonymous memory (GiB)")
ax.set_title("#1120 — peak memory, allocation only (page cache excluded)")
ax.grid(alpha=.25, lw=.5, axis="y"); ax.legend(fontsize=9)
fig.tight_layout(); fig.savefig(OUT / "memory_summary.png", dpi=145)
print("wrote", OUT / "memory_summary.png")
