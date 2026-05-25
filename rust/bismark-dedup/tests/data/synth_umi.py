#!/usr/bin/env python3
# Compatible with Python 3.7+ (uses contextlib.ExitStack for the
# four-handle gzip ContextManager rather than the parenthesized
# `with (a, b, c, d):` syntax that requires 3.10). Tested against the
# Python 3.7 in oxy's bismark-test micromamba env.
"""Synthesize UMI-bearing FASTQs from a stock paired-end FASTQ pair.

This script supports Phase 0 of the bismark-dedup v1.2 UMI/RRBS epic:
generating the Perl `deduplicate_bismark` byte-identity baselines for
the `--barcode/--umi` and `--bclconvert` codepaths on a real RRBS
dataset (Olecka 2024 SRR24766921) whose qnames don't natively carry
UMIs.

Two output formats are supported (one synthetic FASTQ pair per mode):

- ``--mode barcode`` appends ``:<UMI>`` to the qname, before the
  ``/1``/``/2`` mate indicator. Perl regex
  ``deduplicate_bismark:659`` captures the UMI as the tail-of-qname
  ``[\\w\\+]+`` token after the last ``:``.

- ``--mode bclconvert`` inserts ``:<UMI>_<mate>:N:0:NNNNNNNN`` before
  the mate indicator, mimicking bcl-convert's internal-position UMI
  format. Perl regex ``deduplicate_bismark:650`` captures the UMI as
  the ``[CAGTN+]+`` token before ``_<digit>:N:<digit>:``.

UMIs are 8-mer ACGT, deterministically generated from
``pair_index // cluster_size`` so each UMI appears across roughly
``cluster_size`` consecutive read pairs (default 100). The same UMI is
attached to R1 and R2 of each pair (matching the bcl-convert + Trim
Galore convention).

The script reads two gzipped FASTQs in lockstep, writes two gzipped
FASTQs with rewritten qnames, and asserts on completion that R1 and
R2 contained the same number of records. Wall-clock on a 10M-PE
input is ~5-15 min on oxy (Python ``gzip`` is the long pole).

Per the rev 2.1 Phase 0 plan
(``~/.claude/plans/05252026_bismark-dedup-umi-v1.2/phase0-rrbs-baseline/PLAN.md``),
this script is checked into the Bismark repo for reproducibility even
though Phase 0 itself ships no Rust crate changes.

Usage::

    python synth_umi.py \\
        --mode barcode \\
        --in-r1 in_R1.fastq.gz --in-r2 in_R2.fastq.gz \\
        --out-r1 synth_barcode_R1.fastq.gz \\
        --out-r2 synth_barcode_R2.fastq.gz \\
        --subset 10000000
"""

from __future__ import annotations

import argparse
import contextlib
import gzip
import random
import sys
from pathlib import Path


def synth_umi(pair_index: int, cluster: int) -> str:
    """Generate a deterministic 8-mer ACGT UMI for ``pair_index``.

    UMIs cluster: pairs ``[k*cluster, (k+1)*cluster)`` all share the
    same seed and therefore the same UMI. Default ``cluster=100``
    gives ~100 reads per UMI for 10M-pair inputs, which approximates
    typical PCR-duplicate clustering in RRBS data.
    """
    cluster_seed = pair_index // cluster
    rng = random.Random(cluster_seed)
    return "".join(rng.choice("ACGT") for _ in range(8))


def transform_qname(qname_line: bytes, umi: str, mode: str, mate: int) -> bytes:
    """Rewrite a FASTQ qname line to carry the synthesized UMI.

    Args:
        qname_line: the raw bytes of the FASTQ qname line including the
            leading ``@`` and trailing ``\\n``.
        umi: the 8-mer ACGT UMI to insert.
        mode: either ``"barcode"`` (tail-of-qname) or ``"bclconvert"``
            (internal-position).
        mate: 1 for R1, 2 for R2 (used by bcl-convert's
            ``_<mate>:N:0:`` segment; ignored in barcode mode).
    """
    # Strip the trailing newline so we can append cleanly.
    raw = qname_line.rstrip(b"\n")
    # Track the optional /1 or /2 mate indicator. SRA-deposited qnames
    # carry it; some preprocessors strip it.
    mate_suffix = b""
    if raw.endswith(b"/1") or raw.endswith(b"/2"):
        mate_suffix = raw[-2:]
        raw = raw[:-2]

    if mode == "barcode":
        # Append :<UMI> before the mate suffix.
        new = raw + b":" + umi.encode("ascii") + mate_suffix
    elif mode == "bclconvert":
        # Append :<UMI>_<mate>:N:0:NNNNNNNN before the mate suffix.
        new = (
            raw
            + b":"
            + umi.encode("ascii")
            + b"_"
            + str(mate).encode("ascii")
            + b":N:0:NNNNNNNN"
            + mate_suffix
        )
    else:
        raise ValueError(f"unknown mode {mode!r}")

    return new + b"\n"


def synthesize(
    in_r1: Path,
    in_r2: Path,
    out_r1: Path,
    out_r2: Path,
    mode: str,
    subset: int | None,
    cluster: int,
) -> None:
    """Drive the per-pair synthesis loop.

    Reads R1 + R2 in lockstep (4-line FASTQ blocks), rewrites the
    qname of each R1 and R2 with the same per-pair UMI, writes the
    output to two gzipped FASTQ files. Validates that R1 and R2 have
    equal record counts.
    """
    n_pairs = 0
    # compresslevel=6 (default gzip level, not Python gzip.open's default
    # of 9) is ~2x faster on the 10M-pair input with negligible output
    # size difference. Per round-1 code-review by both reviewers.
    #
    # Use contextlib.ExitStack rather than the parenthesized
    # `with (a, b, c, d):` syntax (which requires Python 3.10+) so this
    # script runs on oxy's bismark-test micromamba env (Python 3.7).
    with contextlib.ExitStack() as stack:
        fh_in_r1 = stack.enter_context(gzip.open(in_r1, "rb"))
        fh_in_r2 = stack.enter_context(gzip.open(in_r2, "rb"))
        fh_out_r1 = stack.enter_context(gzip.open(out_r1, "wb", compresslevel=6))
        fh_out_r2 = stack.enter_context(gzip.open(out_r2, "wb", compresslevel=6))
        while True:
            if subset is not None and n_pairs >= subset:
                break
            # FASTQ block: @qname / seq / + / quals
            r1_qname = fh_in_r1.readline()
            r2_qname = fh_in_r2.readline()
            if not r1_qname and not r2_qname:
                break  # clean EOF on both
            if not r1_qname or not r2_qname:
                raise RuntimeError(
                    f"R1/R2 record-count mismatch at pair {n_pairs}: "
                    f"r1_eof={not r1_qname}, r2_eof={not r2_qname}"
                )
            r1_seq = fh_in_r1.readline()
            r1_plus = fh_in_r1.readline()
            r1_qual = fh_in_r1.readline()
            r2_seq = fh_in_r2.readline()
            r2_plus = fh_in_r2.readline()
            r2_qual = fh_in_r2.readline()

            umi = synth_umi(n_pairs, cluster)
            new_r1_qname = transform_qname(r1_qname, umi, mode, mate=1)
            new_r2_qname = transform_qname(r2_qname, umi, mode, mate=2)

            fh_out_r1.write(new_r1_qname)
            fh_out_r1.write(r1_seq)
            fh_out_r1.write(r1_plus)
            fh_out_r1.write(r1_qual)
            fh_out_r2.write(new_r2_qname)
            fh_out_r2.write(r2_seq)
            fh_out_r2.write(r2_plus)
            fh_out_r2.write(r2_qual)

            n_pairs += 1
            if n_pairs % 1_000_000 == 0:
                print(f"  synthesized {n_pairs:,} pairs", file=sys.stderr)

    print(
        f"done: synthesized {n_pairs:,} pairs into {out_r1} / {out_r2}",
        file=sys.stderr,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Synthesize UMI-bearing FASTQs for Phase 0 of the "
        "bismark-dedup v1.2 UMI/RRBS epic.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--mode",
        choices=["barcode", "bclconvert"],
        required=True,
        help="UMI encoding format (which Perl extractor it targets)",
    )
    parser.add_argument("--in-r1", type=Path, required=True, help="input R1 .fastq.gz")
    parser.add_argument("--in-r2", type=Path, required=True, help="input R2 .fastq.gz")
    parser.add_argument("--out-r1", type=Path, required=True, help="output R1 .fastq.gz")
    parser.add_argument("--out-r2", type=Path, required=True, help="output R2 .fastq.gz")
    parser.add_argument(
        "--subset",
        type=int,
        default=None,
        help="cap the number of pairs to process (default: all)",
    )
    parser.add_argument(
        "--cluster",
        type=int,
        default=100,
        help="UMI clustering factor — pair_index // cluster seeds the "
        "per-cluster RNG (default: 100, giving ~100 reads per UMI for "
        "10M-pair inputs)",
    )
    args = parser.parse_args(argv)

    if args.cluster < 1:
        parser.error("--cluster must be >= 1")
    if args.subset is not None and args.subset < 0:
        parser.error("--subset must be >= 0")

    synthesize(
        in_r1=args.in_r1,
        in_r2=args.in_r2,
        out_r1=args.out_r1,
        out_r2=args.out_r2,
        mode=args.mode,
        subset=args.subset,
        cluster=args.cluster,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
