#!/usr/bin/env python3
"""
bismark2bedgraph.py - Convert Bismark methylation call files to bedGraph and coverage formats.

Reads CpG (and optionally CHG/CHH) methylation call files produced by
bismark_methylation_extractor, aggregates counts per (chr, pos), and writes:
  1. A bedGraph file (0-based half-open coordinates, gzipped)
  2. A coverage file (1-based coordinates, gzipped)

This replaces the Perl bismark2bedGraph script.
"""
import argparse
import gzip
import sys
from collections import defaultdict


def parse_methylation_calls(input_files, no_header=False):
    """Parse Bismark methylation extractor output files.

    Each line (after optional header) has:
        read_id  methylation_state(+/-)  chromosome  position  context

    Returns dict: (chr, pos) -> [methylated_count, unmethylated_count]
    """
    counts = defaultdict(lambda: [0, 0])

    for fpath in input_files:
        opener = gzip.open if fpath.endswith(".gz") else open
        with opener(fpath, "rt") as fh:
            if not no_header:
                # Skip Bismark version header line
                header = fh.readline()
                if header and not header.startswith("Bismark"):
                    # Not a header line -- reprocess it
                    _process_line(header.rstrip("\n"), counts)

            for line in fh:
                line = line.rstrip("\n")
                if not line or line.startswith("Bismark"):
                    continue
                _process_line(line, counts)

    return counts


def _process_line(line, counts):
    fields = line.split("\t")
    if len(fields) < 4:
        return
    meth_state = fields[1]
    chrom = fields[2]
    try:
        pos = int(fields[3])
    except ValueError:
        return
    if meth_state == "+":
        counts[(chrom, pos)][0] += 1
    elif meth_state == "-":
        counts[(chrom, pos)][1] += 1


def write_outputs(counts, bedgraph_path, coverage_path, coverage_threshold=1):
    """Write bedGraph and coverage files from aggregated counts."""
    # Sort by chromosome then position
    sorted_keys = sorted(counts.keys())

    with gzip.open(bedgraph_path, "wt") as bg, gzip.open(coverage_path, "wt") as cov:
        bg.write("track type=bedGraph\n")
        for chrom, pos in sorted_keys:
            meth, unmeth = counts[(chrom, pos)]
            total = meth + unmeth
            if total < coverage_threshold:
                continue
            pct = (meth / total) * 100
            bed_pos = pos - 1  # bedGraph is 0-based
            # bedGraph: 0-based start, 1-based end (half-open)
            bg.write(f"{chrom}\t{bed_pos}\t{pos}\t{pct:.6f}\n")
            # coverage: 1-based start and end
            cov.write(f"{chrom}\t{pos}\t{pos}\t{pct:.6f}\t{meth}\t{unmeth}\n")


def main():
    parser = argparse.ArgumentParser(
        description="Convert Bismark methylation calls to bedGraph and coverage formats"
    )
    parser.add_argument("input_files", nargs="+", help="Methylation call files from bismark_methylation_extractor")
    parser.add_argument("-o", "--output", required=True, help="Output prefix (produces <prefix>.bedGraph.gz and <prefix>.bismark.cov.gz)")
    parser.add_argument("--cx", action="store_true", help="Process all contexts (CpG, CHG, CHH), not just CpG")
    parser.add_argument("--no_header", action="store_true", help="Input files have no Bismark header line")
    parser.add_argument("--coverage_threshold", type=int, default=1, help="Minimum coverage to report a position (default: 1)")
    args = parser.parse_args()

    # Filter to CpG-only files unless --cx
    if not args.cx:
        cpg_files = [f for f in args.input_files if "CpG_context" in f or "CpG" in f.split("/")[-1]]
        if not cpg_files:
            print("ERROR: No CpG context files found. Use --cx to include all contexts.", file=sys.stderr)
            sys.exit(1)
        input_files = cpg_files
    else:
        input_files = args.input_files

    bedgraph_path = f"{args.output}.bedGraph.gz"
    coverage_path = f"{args.output}.bismark.cov.gz"

    counts = parse_methylation_calls(input_files, no_header=args.no_header)
    write_outputs(counts, bedgraph_path, coverage_path, args.coverage_threshold)

    print(f"bedGraph written to: {bedgraph_path}", file=sys.stderr)
    print(f"Coverage written to: {coverage_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
