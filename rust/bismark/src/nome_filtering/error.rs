//! Typed errors for `bismark-nome-filtering`.
//!
//! Produced at the CLI / orchestration boundary. Genome-load failures
//! propagate from the promoted [`crate::io::genome`] reader via the
//! [`BismarkNomeError::Genome`] `#[from]` variant — note these come from the
//! module-local `GenomeError`, NOT from `crate::io::BismarkIoError` (keeping
//! the genome promotion a purely additive, non-breaking change to `bismark-io`).

use crate::io::genome::GenomeError;

/// All errors raised by the NOMe-filtering orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum BismarkNomeError {
    /// Genome-load failure from the promoted `crate::io::genome` reader.
    #[error(transparent)]
    Genome(#[from] GenomeError),

    /// `--genome_folder` was not supplied (Perl dies; the folder is mandatory).
    #[error("Please specify a genome folder to proceed (full path only)")]
    MissingGenomeFolder,

    /// The positional input file does not exist (Perl's `-e` check), or no
    /// positional input was supplied at all.
    #[error("File did not exist in the current directory.")]
    InfileNotFound,

    /// The input yielded zero data lines (empty / all-`^Bismark`). Per SPEC
    /// §D4 this is raised **after** the output header has been written, leaving
    /// a header-only `.gz` on disk (Phase B wires the header-first ordering).
    #[error(
        "No last read was defined, something must have gone wrong while reading the data in \
         (e.g. was the input file empty?). Please check your command!"
    )]
    EmptyInput,

    /// `--merge_CpGs` combined with `--CX` — the one reachable Perl die
    /// (`NOMe_filtering:498-500`).
    #[error(
        "Merging individual CpG calls into a single CpG dinucleotide entity is currently only \
         supported if CpG-context is selected only (lose the option --CX)"
    )]
    MergeCpgsWithCx,

    /// Direct I/O error from the orchestration layer (yacht read / output write).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
