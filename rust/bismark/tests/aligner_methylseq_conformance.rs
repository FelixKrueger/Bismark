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
//! Tier 3 (the two former v1 rejects, `--local` = GAP-1 and `--hisat2`+multicore =
//! GAP-2) are **both now RESOLVED** — each flipped to an accept-row assertion below
//! when its feature landed (fixture-free: no on-disk index, no aligner subprocess).
//!
//! **Re-scout** the align module + config on any methylseq version bump (pinned 4.2.0).

use bismark::aligner::cli::Cli;
use bismark::aligner::config::{self, Aligner, ReadFormat};
use bismark::aligner::options::build_aligner_options;
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

/// GAP-1 RESOLVED — `params.local_alignment` → `--local` is now **supported**
/// for Bowtie 2 (plan `plans/06132026_aligner-local-mode/`; the flip-detector
/// formerly here fired when the implementation landed). methylseq's `--local`
/// command now parses AND `build_aligner_options` accepts it, emitting
/// `--local --score-min G,20,8`. (HISAT2-`--local` is also supported now — L-form +
/// dropped `--no-softclip`; only minimap2-`--local` [local by design] + `--local`+combined-index
/// are rejected — those rejects live in `config::resolve`, not here.)
#[test]
fn methylseq_align_local_now_accepted() {
    let cli = Cli::try_parse_from([
        "bismark",
        "reads.fq.gz",
        "--genome",
        "idx",
        "--bam",
        "--local",
    ])
    .expect("--local parses");
    let (opts, _gp) = build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None)
        .expect("--local is now supported for Bowtie 2 (GAP-1 closed)");
    assert!(
        opts.contains("--local") && opts.contains("--score-min G,20,8"),
        "Bowtie 2 --local must emit `--local --score-min G,20,8`: {opts}"
    );
}

/// GAP-2 RESOLVED — `BISMARK_ALIGN` auto-derives `--multicore` on big (`process_high`)
/// nodes; the Rust aligner now **supports** `--hisat2 --multicore N` by interpreting it
/// as a single HISAT2 instance with `-p N --reorder` (Approach B-faithful, plan
/// `06132026_aligner-hisat2-multicore`): deterministic and byte-identical to Perl
/// `--hisat2 -p N` (NOT the fork model — HISAT2 splice discovery is not chunk-invariant).
/// The reject formerly here flipped when the route landed. (HISAT2 single-core +
/// Bowtie 2 `--multicore` fork are unaffected.)
#[test]
fn methylseq_align_hisat2_multicore_now_accepted_via_p_threading() {
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
    // The GAP-2 reject is GONE: resolve no longer fails with the "not supported with
    // --hisat2" message. It now proceeds past the multicore check (failing later only on
    // the fake `idx` genome dir — a different, expected error, not the GAP-2 reject).
    if let Err(e) = config::resolve(
        &cli,
        "bismark reads.fq.gz --genome idx --bam --hisat2 --multicore 2".to_string(),
    ) {
        assert!(
            !e.to_string().contains("not supported with --hisat2"),
            "GAP-2 must no longer reject --hisat2 + multicore; got: {e}"
        );
    }
    // And the route emits `-p N --reorder` (fixture-free; mirrors the GAP-1 `--local` flip):
    let (opts, _gp) =
        build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, Some(2))
            .expect("HISAT2 --multicore route builds options");
    assert!(
        opts.contains("-p 2") && opts.contains("--reorder"),
        "HISAT2 --multicore 2 must emit `-p 2 --reorder`: {opts}"
    );
}
