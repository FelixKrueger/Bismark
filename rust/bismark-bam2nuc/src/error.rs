//! Typed errors for `bismark-bam2nuc`.
//!
//! Produced at the CLI-validation, genome-reading, per-read-counting, and
//! report-writing boundaries. Error strings echo Perl `bam2nuc`'s `die`/`warn`
//! wording where it exists (`process_commandline`, `read_genome_into_memory`,
//! `calc_single_end`, `generate_nucleotide_report`).

use std::path::PathBuf;

/// All errors raised by the `bismark-bam2nuc` orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum BismarkBam2nucError {
    /// Underlying I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Format-detection error from `bismark-io` (the `.bam` accept gate uses
    /// `AlignmentKind::from_path`'s magic-byte sniff). Covers too-short /
    /// unrecognized-format inputs.
    #[error(transparent)]
    BamIo(#[from] bismark_io::BismarkIoError),

    /// `-g/--genome_folder` is mandatory (Perl `process_commandline`:
    /// `die "Please specify a genome folder to proceed (full path only)"`).
    #[error("please specify a genome folder to proceed (-g/--genome_folder <PATH>)")]
    MissingGenomeFolder,

    /// No input alignment file supplied AND not `--genomic_composition_only`
    /// (Perl prints help + exits; the Rust port surfaces a clear error).
    #[error(
        "you need to provide one or more BAM files to continue — usage: bam2nuc_rs -g <genome_dir> <input.bam> [more.bam ...]"
    )]
    MissingInput,

    /// No FASTA files (`.fa`/`.fa.gz`/`.fasta`/`.fasta.gz`) in the genome dir
    /// (Perl `read_genome_into_memory`).
    #[error(
        "the genome folder {dir} does not contain any sequence files (.fa, .fa.gz, .fasta or .fasta.gz)"
    )]
    NoGenomeFasta {
        /// The genome directory that was searched.
        dir: PathBuf,
    },

    /// Two FASTA records declared the same chromosome name (Perl: `die
    /// "Exiting because chromosome name already exists ..."`).
    #[error("duplicate chromosome name {name:?} — every chromosome must have a unique name")]
    DuplicateChromosomeName {
        /// The duplicated chromosome name (lossy-UTF-8 for display).
        name: String,
    },

    /// A present file in the winning glob tier had no valid FASTA header /
    /// produced zero records (Perl `extract_chromosome_name` `die`).
    #[error("file {file} does not look like FASTA (no '>' header / empty)")]
    MalformedFastaHeader {
        /// The offending file.
        file: PathBuf,
    },

    /// A chromosome exceeds `u32::MAX` bp (positions are `u32`). Practically
    /// unreachable for known genomes.
    #[error("chromosome {name:?} is {len} bp which exceeds the u32 position limit")]
    ChromosomeTooLong {
        /// Chromosome name.
        name: String,
        /// Its length in bp.
        len: usize,
    },

    /// A `genomic_nucleotide_frequencies.txt` cache line could not be parsed
    /// (not `<word>\t<count>`, or a word length other than 1/2). Accepted
    /// divergence from Perl's lenient parse — cannot occur on a Perl-written
    /// cache.
    #[error(
        "malformed genomic_nucleotide_frequencies.txt line {line_no} (expected: <word>\\t<count>)"
    )]
    MalformedCacheLine {
        /// 1-based line number in the cache file.
        line_no: usize,
    },

    /// A `@SQ SN:...` chromosome name contains non-ASCII bytes. Bismark's
    /// downstream tools cannot round-trip non-ASCII names safely (mirrors
    /// `bismark-extractor::header`).
    #[error("non-ASCII chromosome name in BAM header: {name:?}")]
    NonAsciiChromosomeName {
        /// The offending name (lossy-UTF-8).
        name: String,
    },

    /// A single-end read carried a FLAG other than 0 or 16 (Perl
    /// `calc_single_end`: `die "failed to detect valid Bismark FLAG tag: $flag"`).
    /// Reachable: Perl's SE branch genuinely dies on an unexpected flag (unlike
    /// the buggy always-true PE branch).
    #[error("failed to detect valid Bismark FLAG tag: {flag}")]
    InvalidSeFlag {
        /// The offending SAM FLAG value.
        flag: u16,
    },

    /// Could not determine single-end vs paired-end from the BAM `@PG` header
    /// (Perl `test_file`: `die "Failed to figure out SE or PE..."`).
    #[error("failed to determine single-end vs paired-end from the BAM @PG header")]
    SePeUndetermined,

    /// The input filename does not end in `bam` or `cram`, so the output name
    /// cannot be derived (Perl: `die "File needs to be in BAM or CRAM format
    /// (ending in .bam or .cram). Terminating process..."`).
    #[error(
        "input file must be in BAM or CRAM format (filename ending in .bam or .cram); cannot derive an output name for {name:?}"
    )]
    NotBamOrCram {
        /// The offending input basename.
        name: String,
    },

    /// A mono/di total was zero (empty / all-skipped sample) or a genomic word
    /// count was zero (degenerate genome). Perl dies "Illegal division by zero"
    /// mid-`calculate_averages`, leaving a partial stats file; the Rust port
    /// errors (exit 1, never panics) and likewise does not clean up the partial.
    #[error(
        "illegal division by zero while calculating averages ({detail}) — the sample has no usable reads or the genome composition is degenerate"
    )]
    ZeroDivision {
        /// Which denominator was zero (sample/genomic, mono/di).
        detail: String,
    },

    /// A `.sam` input was supplied. Perl can read SAM via plain `open` but then
    /// dies deriving the output name (`s/(bam|cram)$/.../` fails on `.sam`); the
    /// Rust port rejects it up front with a clearer message (SPEC Q2).
    #[error(
        "SAM input is not supported (only BAM is read in v1.0); convert to BAM with `samtools view -b`"
    )]
    SamNotSupported,

    /// A `.cram` input was supplied. Perl supports CRAM via samtools; the Rust
    /// port defers CRAM to a later release (SPEC Q3).
    #[error("CRAM input is not yet supported in the Rust port (v1.x); use Perl bam2nuc for CRAM")]
    CramNotSupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_strings_present() {
        assert!(
            BismarkBam2nucError::MissingGenomeFolder
                .to_string()
                .contains("genome folder")
        );
        assert!(
            BismarkBam2nucError::InvalidSeFlag { flag: 4 }
                .to_string()
                .contains("FLAG tag: 4")
        );
        assert!(
            BismarkBam2nucError::NotBamOrCram {
                name: "x.sam".into()
            }
            .to_string()
            .contains("BAM or CRAM")
        );
        assert!(
            BismarkBam2nucError::ZeroDivision {
                detail: "sample mono total".into()
            }
            .to_string()
            .contains("division by zero")
        );
    }
}
