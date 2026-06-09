#!/usr/bin/env python3
"""
Final-BAM concordance ("churn") between a baseline/oracle bismark BAM and a
combined-index bismark BAM, on the SHIPPED tool output (not the spike SAM).

A Bismark SE BAM contains exactly one record per UNIQUELY-aligned read
(ambiguous/unmapped go to side files, not the BAM). So each input stream is a
qname -> (RNAME, POS, strand) map of that mode's unique alignments.

churn = of the ORACLE's unique reads, the fraction whose combined-index result
        is absent (became ambiguous/unmapped) OR maps to a different locus.
            churn% = 100 * (absent + moved) / oracle_unique
This is the "oracle-unique-stays-unique (and same locus)" metric from the spec
§2, computed on final output. `gained` (combined-unique that the oracle did NOT
map uniquely) is reported for context but is NOT in the churn denominator.

Usage:  bam_churn.py <oracle_view> <combined_view>
        where each arg is a `samtools view` stream (no header). Process
        substitution is fine: bam_churn.py <(samtools view a.bam) <(samtools view b.bam)
"""
import sys


def load(path):
    """qname -> (rname, pos, strand) for one bismark BAM's records."""
    d = {}
    with open(path) as fh:
        for line in fh:
            f = line.split("\t", 6)
            if len(f) < 4:
                continue
            qname = f[0]
            flag = int(f[1])
            rname = f[2]
            pos = f[3]
            strand = "-" if (flag & 16) else "+"
            d[qname] = (rname, pos, strand)
    return d


def main():
    oracle = load(sys.argv[1])
    combined = load(sys.argv[2])

    o_n = len(oracle)
    absent = 0
    moved = 0
    for q, loc in oracle.items():
        c = combined.get(q)
        if c is None:
            absent += 1
        elif c != loc:
            moved += 1
    changed = absent + moved
    gained = sum(1 for q in combined if q not in oracle)

    churn = (100.0 * changed / o_n) if o_n else 0.0
    print(
        f"churn={churn:.4f}% "
        f"(oracle_unique={o_n} changed={changed} [absent={absent} moved={moved}]) "
        f"combined_unique={len(combined)} gained={gained}"
    )


if __name__ == "__main__":
    main()
