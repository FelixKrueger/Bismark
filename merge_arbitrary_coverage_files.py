#!/usr/bin/env python

import os
import glob
import time
import gzip
import argparse
import sys

def merge_coverage_files(basename):
    print(f"Merging Bismark coverage files into a file called '{basename}.cov.gz'")
    cov_files = glob.glob("*.cov.gz")
    
    if not cov_files:
        print("Error: No files ending in '.cov.gz' found in the current folder.")
        sys.exit(1)

    allcov = {}  # overarching dictionary

    for file in cov_files:
        print(f"Reading methylation calls from file: {file}")

        isGzip = False
        if file.endswith("gz"):
            infile = gzip.open(file, 'rt')  # mode is 'rt' for text mode
            isGzip = True
        else:
            infile = open(file)

        for line in infile:

            if isGzip:
                line = line.rstrip()  # no need to decode if using 'rt' mode
            else:
                line = line.rstrip()

            chrom, pos, m, u = [line.split(sep="\t")[i] for i in (0, 1, 4, 5)]  # list comprehension

            if chrom in allcov.keys():
                pass
            else:
                allcov[chrom] = {}

            pos = int(pos)

            if pos in allcov[chrom].keys():
                pass
            else:
                allcov[chrom][pos] = {}
                allcov[chrom][pos]["meth"] = 0
                allcov[chrom][pos]["unmeth"] = 0

            allcov[chrom][pos]["meth"] += int(m)
            allcov[chrom][pos]["unmeth"] += int(u)

        infile.close()

    print("Now printing out a new, merged coverage file")

    with gzip.open(f"{basename}.cov.gz", "wt") as out:
        for chrom in sorted(allcov.keys()):
            for pos in sorted(allcov[chrom].keys()):
                perc = ''
                if (allcov[chrom][pos]['meth'] + allcov[chrom][pos]['unmeth'] == 0):
                    print("Both methylated and unmethylated positions were 0. Exiting...")
                    sys.exit()
                else:
                    perc = allcov[chrom][pos]['meth'] / (allcov[chrom][pos]['meth'] + allcov[chrom][pos]['unmeth']) * 100

                out.write(f"{chrom}\t{pos}\t{pos}\t{perc:.2f}\t{allcov[chrom][pos]['meth']}\t{allcov[chrom][pos]['unmeth']}\n")

    print(f"All done! The merged coverage file '{basename}.cov.gz' has been created.\n")

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Merge Bismark coverage files into a file called "basename.cov.gz".')
    parser.add_argument('-b', '--basename', default='merged_coverage_file', help='The basename for the merged coverage file.')
    args = parser.parse_args()
    merge_coverage_files(args.basename)
