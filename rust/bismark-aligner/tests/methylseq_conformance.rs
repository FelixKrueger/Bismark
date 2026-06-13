//! methylseq CLI-surface conformance — `BISMARK_ALIGN` (nf-core/methylseq **4.2.0**).
//!
//! Asserts the Rust `bismark` aligner CLI accepts every command shape methylseq's
//! `BISMARK_ALIGN` module emits, so a drop-in gap is caught HERE (fast CI test)
//! instead of in a production pipeline run. This is the proactive net for the
//! recurring "methylseq emits a flag the Rust CLI rejects" class (already hit 4×:
//! aligner `--bam`, c2c `--genome`, extractor `--CX`).
//! Plan: `plans/06122026_methylseq-cli-conformance/`.
//!
//! **Command template** (`modules/nf-core/bismark/align/main.nf`
//! + `conf/modules/bismark_align.config` ext.args):
//! ```text
//!   bismark <reads|-1 -2> --genome <idx> --bam <ext.args> [--multicore N] [--prefix P]
//!   ext.args: --bowtie2 | --hisat2 ; --pbat ; --non_directional ; --unmapped ;
//!             --score_min L,0,-N ; --local(*) ; --minins/--maxins (PE)
//! ```
//! `--multicore N` is auto-derived on multi-core nodes; `--prefix` set when
//! `task.ext.prefix` is present.
//!
//! **Tiers** (see plan): Tier 1 = clap parse for the accept rows (the aligner has
//! no `Cli::validate()`; its checks live in `config::resolve`/`build_aligner_options`).
//! Tier 3 = the two deliberate v1 rejects (`--local`, `--hisat2`+multicore), asserted
//! via the pub inner fns — fixture-free (no on-disk index, no `bowtie2` subprocess).
//!
//! **Re-scout** the align module + config on any methylseq version bump (pinned 4.2.0).

use bismark_aligner::cli::Cli;
use bismark_aligner::config::{self, Aligner, ReadFormat};
use bismark_aligner::options::build_aligner_options;
use clap::Parser;

/// Tier 1 — every methylseq-emitted `BISMARK_ALIGN` command must clap-parse
/// (no unknown-flag rejection — that is the `--bam`-class drop-in gap).
#[test]
fn methylseq_align_accept_rows_parse() {
    let rows: Vec<(&str, Vec<&str>)> = vec![
        (
            "SE directional bowtie2 (default + auto --multicore)",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--multicore",
                "4",
            ],
        ),
        (
            "SE non-directional (single_cell/non_directional/zymo)",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--non_directional",
            ],
        ),
        (
            "SE pbat",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--pbat",
            ],
        ),
        (
            "SE unmapped",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--unmapped",
            ],
        ),
        (
            "SE relax_mismatches (--score_min underscore form methylseq emits)",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--score_min",
                "L,0,-0.6",
            ],
        ),
        (
            "SE hisat2 single-core (--aligner bismark_hisat, normal node)",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--hisat2",
            ],
        ),
        (
            "SE hisat2 + --known-splicesite-infile (bismark_hisat && params.known_splices)",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--hisat2",
                "--known-splicesite-infile",
                "splice_sites.txt",
            ],
        ),
        (
            "SE with --prefix (task.ext.prefix)",
            vec![
                "bismark",
                "reads.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--prefix",
                "sample1",
            ],
        ),
        (
            "PE directional (--minins/--maxins)",
            vec![
                "bismark",
                "-1",
                "r1.fq.gz",
                "-2",
                "r2.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--minins",
                "1",
                "--maxins",
                "500",
            ],
        ),
        (
            "PE non-directional + unmapped",
            vec![
                "bismark",
                "-1",
                "r1.fq.gz",
                "-2",
                "r2.fq.gz",
                "--genome",
                "idx",
                "--bam",
                "--bowtie2",
                "--non_directional",
                "--unmapped",
            ],
        ),
    ];
    for (label, argv) in rows {
        assert!(
            Cli::try_parse_from(&argv).is_ok(),
            "BISMARK_ALIGN methylseq command must parse [{label}]: {argv:?}\n\
             (a parse rejection here = a methylseq drop-in gap)"
        );
    }
}

/// Tier 3 / GAP-1 (KnownUnsupported, flip-detecting). `params.local_alignment`
/// emits `--local`; the Rust aligner rejects it (v1 is end-to-end only).
/// Asserted via the pub `build_aligner_options` (fixture-free — mirrors the
/// crate's own `rejects_local_in_v1`). When a future `--local` epic lands, this
/// fails → forces a deliberate update + opening the gap.
#[test]
fn methylseq_align_local_known_unsupported() {
    let cli = Cli::try_parse_from([
        "bismark",
        "reads.fq.gz",
        "--genome",
        "idx",
        "--bam",
        "--local",
    ])
    .expect("--local PARSES (it is rejected at build time, not at parse time)");
    let err = build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false)
        .expect_err("--local must be rejected (KnownUnsupported GAP-1)");
    let msg = err.to_string();
    // Message-specific: GAP-1 and GAP-2 share AlignerError::Unsupported, so the
    // variant alone is insufficient (the two rows would alias each other).
    assert!(
        msg.contains("local mode is not supported"),
        "GAP-1 must reject with the --local-specific message; got: {msg}"
    );
}

/// Tier 3 / GAP-2 (KnownUnsupported, flip-detecting). `BISMARK_ALIGN` auto-derives
/// `--multicore` on big (`process_high`) nodes; the Rust aligner rejects
/// `--hisat2` with multicore>1 (HISAT2 splice discovery is not chunk-invariant).
/// The reject (`config.rs:251`) fires BEFORE any disk I/O / `bowtie2 --version`
/// subprocess inside `resolve()`, so this is fixture-free.
#[test]
fn methylseq_align_hisat2_multicore_known_unsupported() {
    let cli = Cli::try_parse_from([
        "bismark",
        "reads.fq.gz",
        "--genome",
        "idx",
        "--bam",
        "--hisat2",
        "--multicore",
        "2",
    ])
    .expect("--hisat2 --multicore 2 parses");
    let err = config::resolve(
        &cli,
        "bismark reads.fq.gz --genome idx --bam --hisat2 --multicore 2".to_string(),
    )
    .expect_err("--hisat2 + multicore>1 must be rejected (KnownUnsupported GAP-2)");
    let msg = err.to_string();
    assert!(
        msg.contains("not supported with --hisat2"),
        "GAP-2 must reject with the --hisat2-specific message; got: {msg}"
    );
}
