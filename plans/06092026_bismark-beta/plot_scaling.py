#!/usr/bin/env python3
"""
Plot Bismark alignment core-scaling vs TOTAL CORES (32-CPU pod budget), two
figures (each 1x3: wall time, peak RAM, CPU core-seconds):
  directional.png : faithful_dir (Rust) vs perl_dir (Perl) vs comb_dir (Rust combined), directional 10M
  nondir.png      : faithful_nondir vs perl_nondir vs comb1pass, non-directional 10M (Sherman)

x = total cores = (Bowtie 2 instances) x (-p). Every series capped at 32 cores
(the pod's CPU allocation) so nothing oversubscribes; modes line up at the
shared 8/16/32-core budgets for a fair, equal-budget comparison.

Usage:  plot_scaling.py <merged_summary.tsv> <out_dir>
"""
import sys, csv, os
from collections import defaultdict

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

# section -> {mode: (label, color, marker, linestyle)}
SECTIONS = {
    "directional": {
        "title": "DIRECTIONAL (10M real WGBS SE)",
        "modes": {
            "faithful_dir": ("Rust faithful directional (2 inst)", "#1f77b4", "o", "-"),
            "perl_dir":     ("Perl 0.25.1 directional (2 inst)",   "#2ca02c", "^", "--"),
            "comb_dir":     ("Rust combined directional (1 inst)",  "#d62728", "s", "-"),
        },
        "xticks": [2, 4, 8, 12, 16, 24, 32],
    },
    "nondir": {
        "title": "NON-DIRECTIONAL (10M Sherman-simulated SE)",
        "modes": {
            "faithful_nondir": ("Rust faithful non-dir (4 inst)",            "#1f77b4", "o", "-"),
            "perl_nondir":     ("Perl 0.25.1 non-dir (4 inst)",              "#2ca02c", "^", "--"),
            "comb1pass":       ("Rust combined single-pass non-dir (1 inst)", "#d62728", "s", "-"),
        },
        "xticks": [8, 16, 32],
    },
}

PANELS = [
    ("wall", "Wall-clock time (s)", "Wall time  (lower = better)"),
    ("rss", "Peak process-tree RSS (GB)", "Peak memory"),
    ("cpu", "CPU core-seconds (User+System)", "Total CPU work"),
]


def load(path):
    rows = defaultdict(lambda: {"cores": [], "wall": [], "rss": [], "cpu": []})
    with open(path) as fh:
        for r in csv.DictReader(fh, delimiter="\t"):
            if r.get("exit") not in ("0", 0):
                continue
            m = r["mode"]
            rows[m]["cores"].append(int(r["total_cores"]))
            rows[m]["wall"].append(float(r["wall_s"]))
            rows[m]["rss"].append(float(r["peak_rss_kb"]) / 1048576.0)
            rows[m]["cpu"].append(float(r["cpu_core_s"]))
    for m in rows:
        order = sorted(range(len(rows[m]["cores"])), key=lambda i: rows[m]["cores"][i])
        for k in ("cores", "wall", "rss", "cpu"):
            rows[m][k] = [rows[m][k][i] for i in order]
    return rows


def plot_section(data, sec_key, out_dir):
    sec = SECTIONS[sec_key]
    fig, axes = plt.subplots(1, 3, figsize=(16, 5))
    for ax, (key, ylab, title) in zip(axes, PANELS):
        for m, (label, color, marker, ls) in sec["modes"].items():
            if m not in data or not data[m]["cores"]:
                continue
            ax.plot(data[m]["cores"], data[m][key], marker=marker, color=color,
                    label=label, linewidth=2, markersize=7, linestyle=ls)
        ax.set_xlabel("Total cores allocated  (instances x -p)")
        ax.set_ylabel(ylab)
        ax.set_title(title)
        ax.set_xticks(sec["xticks"])
        ax.set_xlim(left=0)
        ax.grid(True, alpha=0.3)
        ax.legend(fontsize=8)
        if key in ("wall", "cpu"):
            ax.set_ylim(bottom=0)
    fig.suptitle(
        f"Bismark core scaling vs total cores — {sec['title']}\n"
        "GRCh38, oxy (32-CPU pod). Solid = Rust, dashed = Perl 0.25.1. "
        "Equal-budget comparison: every mode at the same total cores (≤32).",
        fontsize=11,
    )
    fig.tight_layout(rect=[0, 0, 1, 0.92])
    out = os.path.join(out_dir, f"{sec_key}.png")
    fig.savefig(out, dpi=130)
    print(f"wrote {out}")
    plt.close(fig)


def main():
    tsv, out_dir = sys.argv[1], sys.argv[2]
    data = load(tsv)
    for sec_key in SECTIONS:
        plot_section(data, sec_key, out_dir)


if __name__ == "__main__":
    main()
