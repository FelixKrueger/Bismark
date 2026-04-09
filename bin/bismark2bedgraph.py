#!/usr/bin/env python3
"""Convert Bismark methylation extractor output to bedGraph and coverage formats."""

import argparse
import gzip
import sys
from collections import defaultdict

# Valid methylation call pairs: (strand, context)
VALID_CALLS = {
    ("+", "Z"),  # methylated CpG
    ("-", "z"),  # unmethylated CpG
    ("+", "X"),  # methylated CHG
    ("-", "x"),  # unmethylated CHG
    ("+", "H"),  # methylated CHH
    ("-", "h"),  # unmethylated CHH
    ("+", "C"),  # methylated (generic)
    ("-", "c"),  # unmethylated (generic)
}

CPG_CONTEXTS = {"Z", "z"}


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_files", nargs="+", help="Bismark methylation extractor output files")
    parser.add_argument("--output", "-o", required=True, help="Output bedGraph filename (will be gzipped)")
    parser.add_argument("--cutoff", type=int, default=1, help="Minimum coverage to report a position (default: 1)")
    parser.add_argument("--cx", action="store_true", help="Process all cytosine contexts (default: CpG only)")
    return parser.parse_args(argv)


def open_file(path):
    """Open a file, transparently handling gzip."""
    if path.endswith(".gz"):
        return gzip.open(path, "rt")
    return open(path)


def read_methylation_calls(files, cx_context):
    """Read methylation call files and aggregate counts per (chr, pos).

    Returns dict[(chr, pos)] -> [methylated_count, unmethylated_count]
    """
    counts = defaultdict(lambda: [0, 0])

    for filepath in files:
        with open_file(filepath) as fh:
            for line in fh:
                if line.startswith("Bismark"):
                    continue
                fields = line.rstrip("\n").split("\t")
                if len(fields) < 5:
                    continue
                _, strand, chrom, pos_str, context = fields[:5]

                # Validate call
                if (strand, context) not in VALID_CALLS:
                    continue

                # CpG-only filtering
                if not cx_context and context not in CPG_CONTEXTS:
                    continue

                pos = int(pos_str)
                if strand == "+":
                    counts[(chrom, pos)][0] += 1
                else:
                    counts[(chrom, pos)][1] += 1

    return counts


def write_outputs(counts, output_path, cutoff):
    """Write bedGraph and coverage files."""
    if output_path.endswith(".bedGraph.gz"):
        cov_path = output_path.replace(".bedGraph.gz", ".bismark.cov.gz")
    else:
        cov_path = output_path + ".bismark.cov.gz"

    sorted_positions = sorted(counts.keys())

    with gzip.open(output_path, "wt") as bg, gzip.open(cov_path, "wt") as cov:
        bg.write("track type=bedGraph\n")

        for chrom, pos in sorted_positions:
            meth, unmeth = counts[(chrom, pos)]
            total = meth + unmeth
            if total < cutoff:
                continue

            pct = (meth / total) * 100

            # bedGraph: 0-based start, 1-based end
            bed_start = pos - 1
            bg.write(f"{chrom}\t{bed_start}\t{pos}\t{pct}\n")

            # coverage: 1-based start, 1-based end
            cov.write(f"{chrom}\t{pos}\t{pos}\t{pct}\t{meth}\t{unmeth}\n")


def main(argv=None):
    args = parse_args(argv)

    output = args.output
    if not output.endswith(".gz"):
        output += ".gz"

    counts = read_methylation_calls(args.input_files, args.cx)
    if not counts:
        print("Warning: no methylation calls found in input files", file=sys.stderr)

    write_outputs(counts, output, args.cutoff)


if __name__ == "__main__":
    main()
