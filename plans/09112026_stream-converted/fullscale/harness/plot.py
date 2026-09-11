#!/usr/bin/env python
"""Time-series plots for the #1120 full-scale A/B (streamed vs files).

One stacked figure per shape/rep — converted bytes on disk, whole --temp_dir,
output dir, memory, CPU and thread count — plus a cross-shape disk summary and a
wall-time comparison.

The streamed arm's disk curve sits flat on zero, which is the result but is also
easy to mistake for a missing line, so every panel is annotated with each arm's
peak.
"""
import sys, pathlib
import pandas as pd
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

WORK = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/fs/run")
OUT = WORK / "plots"; OUT.mkdir(exist_ok=True)

STYLE = {"stream": dict(color="#1f77b4", label="streamed (default)"),
         "files":  dict(color="#d62728", label="files (--no_stream_converted)")}

MiB, GiB = 1024, 1024**3
PANELS = [
    ("converted_kb", "Converted reads\non disk (MiB)", MiB, "{:,.0f} MiB"),
    ("temp_kb",      "--temp_dir\ntotal (MiB)",        MiB, "{:,.0f} MiB"),
    ("out_kb",       "Output dir\n(MiB)",              MiB, "{:,.0f} MiB"),
    ("mem_bytes",    "Memory\n(GiB)",                  GiB, "{:,.2f} GiB"),
    ("cpu_pct",      "CPU\n(% of one core)",           1,   "{:,.0f}%"),
    ("pids",         "Procs + threads",                1,   "{:,.0f}"),
]

def series(shape, arm, rep):
    f = WORK / "series" / f"{shape}_{arm}_r{rep}.tsv"
    if not f.exists():
        return None
    df = pd.read_csv(f, sep="\t")
    return df if len(df) > 1 else None

def index():
    found = {}
    for f in sorted((WORK / "series").glob("*.tsv")):
        for arm in ("stream", "files"):
            key = f"_{arm}_r"
            if key in f.stem:
                shape, rep = f.stem.split(key)
                found.setdefault(shape, set()).add(int(rep))
    return {s: sorted(r) for s, r in found.items()}

IDX = index()

for shape, reps in IDX.items():
    for rep in reps:
        if not any(series(shape, a, rep) is not None for a in STYLE):
            continue
        fig, axes = plt.subplots(len(PANELS), 1, figsize=(11, 1.95 * len(PANELS)),
                                 sharex=True)
        for ax, (col, ylab, div, fmt) in zip(axes, PANELS):
            notes = []
            for arm, st in STYLE.items():
                df = series(shape, arm, rep)
                if df is None or col not in df:
                    continue
                ax.plot(df["t_s"], df[col] / div, lw=1.4, **st)
                notes.append((st["color"], f"peak {fmt.format(df[col].max() / div)}"))
            ax.set_ylabel(ylab, fontsize=8.5)
            ax.grid(alpha=0.25, lw=0.5); ax.margins(x=0.01); ax.set_ylim(bottom=0)
            for i, (c, txt) in enumerate(notes):
                ax.text(0.995, 0.92 - i * 0.22, txt, transform=ax.transAxes,
                        ha="right", va="top", fontsize=8, color=c, weight="bold")
        axes[0].legend(fontsize=8.5, loc="upper left", framealpha=0.9)
        axes[-1].set_xlabel("seconds since launch")
        fig.suptitle(f"#1120 converted-read streaming — {shape}, rep {rep}", fontsize=12)
        fig.tight_layout(rect=[0, 0, 1, 0.985])
        p = OUT / f"{shape}_r{rep}.png"
        fig.savefig(p, dpi=140); plt.close(fig); print("wrote", p)

# Headline: converted-read footprint, every shape on one axis.
if IDX:
    fig, ax = plt.subplots(figsize=(11, 4.4))
    for shape, reps in IDX.items():
        for arm, st in STYLE.items():
            df = series(shape, arm, reps[0])
            if df is None:
                continue
            ax.plot(df["t_s"], df["converted_kb"] / MiB, lw=1.4, color=st["color"],
                    alpha=0.85, label=f"{shape} — {arm}")
    ax.set_xlabel("seconds since launch")
    ax.set_ylabel("converted reads on disk (MiB)")
    ax.set_title("#1120 — converted-read footprint in --temp_dir, all shapes")
    ax.grid(alpha=0.25, lw=0.5); ax.set_ylim(bottom=0)
    ax.legend(fontsize=7.5, ncol=2)
    fig.tight_layout(); fig.savefig(OUT / "converted_all_shapes.png", dpi=140)
    print("wrote", OUT / "converted_all_shapes.png")

# C3: wall time per shape, both arms, all reps.
res = WORK / "results.tsv"
if res.exists():
    df = pd.read_csv(res, sep="\t")
    if len(df):
        shapes = sorted(df["shape"].unique())
        fig, ax = plt.subplots(figsize=(max(7, 1.9 * len(shapes) + 3), 4.4))
        w = 0.36
        for i, (arm, st) in enumerate(STYLE.items()):
            xs, meds = [], []
            for j, s in enumerate(shapes):
                v = df[(df["shape"] == s) & (df["arm"] == arm)]["wall_s"]
                if not len(v):
                    continue
                x = j + (i - 0.5) * w
                xs.append(x); meds.append(v.median())
                ax.scatter([x] * len(v), v, s=16, color=st["color"], zorder=3,
                           alpha=0.75, edgecolors="none")
            if xs:
                ax.bar(xs, meds, width=w, color=st["color"], alpha=0.32,
                       label=f"{st['label']} (median)")
        ax.set_xticks(range(len(shapes)))
        ax.set_xticklabels(shapes, fontsize=8.5, rotation=12, ha="right")
        ax.set_ylabel("wall time (s)")
        ax.set_title("#1120 — wall time, median bars with individual reps")
        ax.grid(alpha=0.25, lw=0.5, axis="y"); ax.legend(fontsize=8.5)
        fig.tight_layout(); fig.savefig(OUT / "walltime.png", dpi=140)
        print("wrote", OUT / "walltime.png")
