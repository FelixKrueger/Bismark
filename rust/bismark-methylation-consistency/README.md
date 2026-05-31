# bismark-methylation-consistency

Rust port of Bismark's Perl `methylation_consistency` script. Binary:
**`methylation_consistency_rs`**.

Splits a Bismark alignment BAM into **three** BAMs by the *read-level*
consistency of its methylation calls, and writes a small text report:

| Output | Contents |
|--------|----------|
| `<root>_all_meth.bam`   | reads (PE: pairs) that are `>= --upper_threshold`% methylated |
| `<root>_all_unmeth.bam` | reads that are `<= --lower_threshold`% methylated |
| `<root>_mixed_meth.bam` | everything in between |
| `<root>_consistency_report.txt` | bucket counts + percentages |

`<root>` is the input path with a trailing `.bam` removed — outputs are written
**next to the input file**. With `--chh`, a `_CHH` infix is added to every name.
Methylation level per read = `Z / (Z + z)` counted in the `XM:Z:` tag (CpG), or
`H / (H + h)` with `--chh`. For paired-end input, R1 and R2 counts are summed and
both mates are written to the chosen bucket.

## Usage

```
methylation_consistency_rs [OPTIONS] <FILES>...

  -p, --paired_end          force paired-end (default: auto-detect from @PG)
  -s, --single_end          force single-end
      --chh                 classify on CHH (H/h) context instead of CpG (Z/z)
      --lower_threshold <N> unmethylated cutoff, 0–49   [default: 10]
      --upper_threshold <N> methylated cutoff, 51–100   [default: 90]
  -m, --min-count <N>       min cytosine calls per read [default: 5]
      --samtools_path <P>   accepted for compatibility; ignored (pure-Rust I/O)
      --quiet               suppress STDERR diagnostics
  -V, --version             print version and exit
```

Library mode auto-detects from the Bismark `@PG` line when neither `-s`/`-p`
is given; a BAM with no Bismark `@PG` is treated as single-end.

## Relationship to the Perl original

The acceptance contract is **byte-identical output** vs Perl Bismark: the
`_consistency_report.txt` is byte-for-byte identical, and the three BAMs are
identical at the decompressed-record level (same records, same order; PE: R1
then R2). Two intentional, documented divergences:

- **Output BAM headers** lack the `@PG ID:samtools*` provenance lines that the
  Perl pipeline's `samtools` subprocesses inject — the pure-Rust port (built on
  [`noodles`] via `bismark-io`) writes the input header verbatim and adds no
  `@PG`. Records are unaffected.
- **Empty buckets** are written as valid empty BAMs (header + BGZF EOF), where
  Perl leaves a 0-byte, unreadable file.

Built on the shared `bismark-io` crate (pure-Rust BAM/SAM/CRAM I/O, no
`samtools` subprocess, no `htslib`).

[`noodles`]: https://github.com/zaeleus/noodles

## Status

v1.0.0-beta.1. Single-threaded streaming. SE, PE, and `--chh` are implemented
and validated byte-identical to Perl on synthetic data (incl. an automated
Perl-vs-Rust test that runs the original script when `perl`+`samtools` are
present). The large 10M-read real-data byte-identity gate runs on the cluster
(see `plans/05292026_bismark-methylation-consistency/`).
