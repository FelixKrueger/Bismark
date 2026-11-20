#!/usr/bin/env python3
"""Write umi_tools_directional_golden.tsv: UMI-tools directional clusters for seeded random UMI sets.

Usage: python3 make_umi_tools_golden.py > umi_tools_directional_golden.tsv  (needs umi_tools)
Columns: case, threshold, umi, count, cluster (index of the cluster within the case).
"""

import random
import sys

import umi_tools
from umi_tools.network import UMIClusterer

ALPHABET = "ACGTN"


def mutate(umi, rng, n):
    umi = list(umi)
    for pos in rng.sample(range(len(umi)), n):
        umi[pos] = rng.choice([b for b in ALPHABET if b != umi[pos]])
    return "".join(umi)


def make_case(rng):
    length = rng.choice([6, 8, 10])
    seeds = ["".join(rng.choice("ACGT") for _ in range(length)) for _ in range(rng.randint(1, 3))]
    counts = {}
    for seed in seeds:
        counts[seed] = counts.get(seed, 0) + rng.randint(1, 30)
        for _ in range(rng.randint(0, 6)):
            child = mutate(seed, rng, rng.choice([1, 1, 1, 2]))
            counts[child] = counts.get(child, 0) + rng.randint(1, 6)
    return counts


def main():
    rng = random.Random(20261120)
    clusterer = UMIClusterer(cluster_method="directional")
    print(f"# UMI-tools {umi_tools.__version__} UMIClusterer('directional'); counts inserted in byte order")
    print("case\tthreshold\tumi\tcount\tcluster")
    for case in range(300):
        counts = make_case(rng)
        ordered = {u.encode(): counts[u] for u in sorted(counts)}
        for threshold in (1, 2):
            clusters = clusterer(ordered, threshold=threshold)
            for index, cluster in enumerate(clusters):
                for umi in cluster:
                    print(f"{case}\t{threshold}\t{umi.decode()}\t{ordered[umi]}\t{index}")


if __name__ == "__main__":
    sys.exit(main())
