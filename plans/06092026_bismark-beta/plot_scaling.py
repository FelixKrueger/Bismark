#!/usr/bin/env python3
"""
Plot Bismark alignment core-scaling: wall time, peak RAM, CPU core-seconds vs -p,
for two modes (faithful directional 2-instance vs combined single-pass).

Usage:  plot_scaling.py <scaling_summary.tsv> <out_prefix>
  writes <out_prefix>.png  (1x3 panel overview)
"""
import sys
import csv
from collections import defaultdict

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

SERIES = {
    "faithful_dir": ("Faithful directional (2 instances)", "#1f77b4", "o"),
    "comb1pass": ("Combined single-pass (1 instance)", "#d62728", "s"),
}


def load(path):
    data = defaultdict(lambda: {"p": [], "wall": [], "rss": [], "cpu": [], "total": []})
    with open(path) as fh:
        for row in csv.DictReader(fh, delimiter="\t"):
            if row.get("exit") not in ("0", 0):
                continue
            m = row["mode"]
            data[m]["p"].append(int(row["p"]))
            data[m]["total"].append(int(row["total_cores"]))
            data[m]["wall"].append(float(row["wall_s"]))
            data[m]["rss"].append(float(row["peak_rss_kb"]) / 1048576.0)
            data[m]["cpu"].append(float(row["cpu_core_s"]))
    # sort each series by p
    for m in data:
        order = sorted(range(len(data[m]["p"])), key=lambda i: data[m]["p"][i])
        for k in ("p", "total", "wall", "rss", "cpu"):
            data[m][k] = [data[m][k][i] for i in order]
    return data


def main():
    tsv, prefix = sys.argv[1], sys.argv[2]
    data = load(tsv)

    fig, axes = plt.subplots(1, 3, figsize=(16, 5))
    panels = [
        ("wall", "Wall-clock time (s)", "Wall time vs threads  (lower = better)"),
        ("rss", "Peak process-tree RSS (GB)", "Peak memory vs threads"),
        ("cpu", "CPU core-seconds (User+System)", "Total CPU work vs threads"),
    ]
    for ax, (key, ylab, title) in zip(axes, panels):
        for m, (label, color, marker) in SERIES.items():
            if m not in data or not data[m]["p"]:
                continue
            ax.plot(
                data[m]["p"], data[m][key],
                marker=marker, color=color, label=label, linewidth=2, markersize=7,
            )
        ax.set_xlabel("Bowtie 2 threads  -p  (per instance)")
        ax.set_ylabel(ylab)
        ax.set_title(title)
        ax.set_xticks([2, 4, 8, 12, 16])
        ax.grid(True, alpha=0.3)
        ax.legend(fontsize=8)
        if key in ("wall", "cpu"):
            ax.set_ylim(bottom=0)

    fig.suptitle(
        "Bismark Rust aligner — core scaling, 10M SE WGBS reads (GRCh38, oxy)\n"
        "faithful directional uses 2×-p total cores; combined single-pass uses -p total cores",
        fontsize=11,
    )
    fig.tight_layout(rect=[0, 0, 1, 0.93])
    out = f"{prefix}.png"
    fig.savefig(out, dpi=130)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
