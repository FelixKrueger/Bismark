#!/usr/bin/env python

# Copyright 2025, Felix Krueger (fkrueger@altoslabs.com)

# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

import os
import glob
import gzip
import sys
import argparse


def parse_options():
    parser = argparse.ArgumentParser(description="Merge Bismark coverage files into a file called 'basename.cov.gz'. "
                                     "Merges the files given as arguments, or all '*.cov.gz' files in the current folder if none are given.")
    parser.add_argument("-b","--basename", type=str, help="Basename of file to write merged coverage output to (default 'merged_coverage_file', i.e. 'merged_coverage_file.cov.gz')", default="merged_coverage_file")
    parser.add_argument("filenames", help="The Bismark coverage files to merge (default: all '*.cov.gz' files in the current folder)", nargs='*')

    options = parser.parse_args()
    return options

def main():

    options = parse_options()
    outfile = f"{options.basename}.cov.gz"
    print (f"Merging Bismark coverage files into a file called '{outfile}'")
    cov_files = options.filenames or glob.glob("*.cov.gz")
    # Never read our own output (e.g. from a previous run), or it is counted twice.
    cov_files = [f for f in cov_files if os.path.abspath(f) != os.path.abspath(outfile)]
    if not cov_files:
        print ("Error: No coverage files given and no files ending in '.cov.gz' found in the current folder.", file=sys.stderr)
        sys.exit(1)
    print (f"Merging the following files: {cov_files}")
    allcov = {} # overarching dictionary

    for filename in cov_files:
        print (f"Reading methylation calls from file: {filename}")

        if filename.endswith("gz"):
            infile = gzip.open(filename, 'rt') # text mode, no decoding needed
        else:
            infile = open(filename)

        for line in infile:
            line = line.rstrip()
            chrom, pos, m, u = [line.split(sep="\t")[i] for i in (0,1,4,5)] # list comprehension

            if chrom not in allcov:
                allcov[chrom] = {}
            # converting the position to int() right here, so sorted() sorts numerically
            pos = int(pos)

            if pos not in allcov[chrom]:
                allcov[chrom][pos] = {}
                allcov[chrom][pos]["meth"] = 0
                allcov[chrom][pos]["unmeth"] = 0

            allcov[chrom][pos]["meth"] += int(m)
            allcov[chrom][pos]["unmeth"] += int(u)

        infile.close()

    # Checked before writing, so a bad input leaves no partial output file.
    for chrom in allcov:
        for pos in allcov[chrom]:
            if (allcov[chrom][pos]['meth'] + allcov[chrom][pos]['unmeth'] == 0):
                # This should not happen in real data, as coverage files by definition only show covered positions
                print (f"Error: Both methylated and unmethylated counts were 0 at {chrom}:{pos}. Dying now...", file=sys.stderr)
                sys.exit(1)

    print (f"Now printing out a new, merged coverage file to >>{outfile}<<")

    with gzip.open(outfile,"wt") as out:
        for chrom in sorted(allcov.keys()):
            for pos in sorted(allcov[chrom].keys()):
                perc = allcov[chrom][pos]['meth'] / (allcov[chrom][pos]['meth'] + allcov[chrom][pos]['unmeth']) * 100

                # percentage is displayed with 2 decimals with f-string formatting
                out.write(f"{chrom}\t{pos}\t{pos}\t{perc:.2f}\t{allcov[chrom][pos]['meth']}\t{allcov[chrom][pos]['unmeth']}\n")

    print (f"All done! The merged coverage file '{outfile}' has been created.\n")

if __name__ == '__main__':
    main()
