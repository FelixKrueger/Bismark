//! Typed errors for `bismark-coverage2cytosine`.
//!
//! Produced at the CLI-validation and genome-reading boundaries. Error
//! strings echo Perl `coverage2cytosine`'s `die` wording where it exists
//! (`process_commandline:1990-2197`, `read_genome_into_memory:1648-1751`).

use std::path::PathBuf;

/// All errors raised by the `bismark-coverage2cytosine` orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum BismarkC2cError {
    /// Underlying I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// No positional coverage infile was supplied. Perl prints help + exits
    /// (`:2059`); the Rust port surfaces a clear error instead.
    #[error(
        "no coverage input file supplied — usage: coverage2cytosine_rs -o <out> -g <genome_dir> <input.bismark.cov[.gz]>"
    )]
    MissingCovInput,

    /// `-o/--output` is mandatory (Perl `:2078`).
    #[error("please provide the name of the output file using -o/--output <filename>")]
    MissingOutput,

    /// `--genome_folder` is mandatory; the Perl hardcoded-mouse default is NOT
    /// honoured in the Rust port (Perl `:2134`; SPEC §15).
    #[error("please specify a genome folder to proceed (-g/--genome_folder <PATH>)")]
    MissingGenomeFolder,

    /// A flag deferred to a later (v1.x) release was supplied. Rejected rather
    /// than silently ignored (silent acceptance would produce wrong output).
    #[error(
        "{flag} is not supported in the Rust port yet (v1.x); use Perl coverage2cytosine for this mode"
    )]
    UnsupportedFlag {
        /// The rejected flag, e.g. `--nome-seq`.
        flag: &'static str,
    },

    /// `--merge_CpGs` + `--CX` (Perl `:2140`).
    #[error(
        "merging individual CpG calls into a single CpG dinucleotide entity is only supported in CpG context (lose the option --CX)"
    )]
    MergeCpgsWithCx,

    /// `--merge_CpGs` + `--split_by_chromosome` (Perl `:2143`).
    #[error(
        "merging individual CpG calls into a single CpG dinucleotide entity requires a single genome-wide report (lose the option --split_by_chromosome)"
    )]
    MergeCpgsWithSplit,

    /// `--merge_CpGs` + `--coverage_threshold` (Perl `:2176`).
    #[error("a coverage threshold cannot be specified together with --merge_CpGs")]
    MergeCpgsWithThreshold,

    /// `--discordance_filter` without `--merge_CpGs` (Perl `:2165`).
    #[error("--discordance_filter requires the option --merge_CpGs as well")]
    DiscordanceWithoutMerge,

    /// `--discordance_filter` value out of `1..=100` (Perl `:2168`).
    #[error(
        "the discordance between top/bottom strand methylation must be a percentage difference between 1 and 100 (got {value})"
    )]
    DiscordanceOutOfRange {
        /// The supplied (invalid) value.
        value: u8,
    },

    /// `--coverage_threshold 0` explicitly supplied (Perl `:2178`; absence ⇒
    /// default 0 meaning "report all", but an explicit 0 is an error).
    #[error("coverage threshold must be a positive integer greater than 0")]
    ThresholdNotPositive,

    /// No FASTA files (`.fa`/`.fa.gz`/`.fasta`/`.fasta.gz`) in the genome dir
    /// (Perl `:1671-1673`).
    #[error(
        "the genome folder {dir} does not contain any sequence files (.fa, .fa.gz, .fasta or .fasta.gz)"
    )]
    NoGenomeFasta {
        /// The genome directory that was searched.
        dir: PathBuf,
    },

    /// Two FASTA records (within or across files) declared the same chromosome
    /// name (Perl `:1702-1705`).
    #[error("duplicate chromosome name {name:?} — every chromosome must have a unique name")]
    DuplicateChromosomeName {
        /// The duplicated chromosome name (lossy-UTF-8 for display).
        name: String,
    },

    /// A present file in the winning glob tier had no valid FASTA header /
    /// produced zero records (Perl `extract_chromosome_name` `die`, `:1749`).
    #[error("file {file} does not look like FASTA (no '>' header / empty)")]
    MalformedFastaHeader {
        /// The offending file.
        file: PathBuf,
    },

    /// A chromosome exceeds `u32::MAX` bp; positions are `u32` (SPEC §15
    /// overflow guard). Practically unreachable for known genomes.
    #[error("chromosome {name:?} is {len} bp which exceeds the u32 position limit")]
    ChromosomeTooLong {
        /// Chromosome name.
        name: String,
        /// Its length in bp.
        len: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_strings_present() {
        assert!(
            BismarkC2cError::MissingOutput
                .to_string()
                .contains("output")
        );
        assert!(
            BismarkC2cError::MergeCpgsWithCx
                .to_string()
                .contains("--CX")
        );
        assert!(
            BismarkC2cError::UnsupportedFlag { flag: "--nome-seq" }
                .to_string()
                .contains("--nome-seq")
        );
        assert!(
            BismarkC2cError::DiscordanceOutOfRange { value: 101 }
                .to_string()
                .contains("101")
        );
    }
}
