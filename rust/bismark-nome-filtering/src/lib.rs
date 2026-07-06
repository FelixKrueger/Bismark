//! `bismark-nome-filtering` — Rust port of Bismark Perl's **standalone**
//! `NOMe_filtering` (v0.25.1). A per-read NOMe-Seq classifier that consumes the
//! methylation extractor's `--yacht` output and emits a per-read CG/GC
//! methylation tally; byte-identical to Perl v0.25.1.
//!
//! ⚠️ This is the standalone tool, **NOT** `coverage2cytosine --nome-seq` (a
//! separate in-c2c flag). See `plans/05312026_bismark-nome-filtering/SPEC.md`.
//!
//! ## Status
//! **Phase B** (core): the clap CLI + validation ([`cli`]), the promoted
//! [`bismark_io::genome`] reader, output-filename derivation ([`filename`]), the
//! [`substr::perl_substr`] helper, typed errors ([`error`]), and the per-read
//! filtering pipeline ([`nome`]) that streams the `--yacht` input and writes the
//! always-gzipped `.manOwar.txt.gz` report. Phase C is the real-data
//! byte-identity gate.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod error;
pub mod filename;
pub mod nome;
pub mod substr;

use crate::cli::Cli;
use crate::error::BismarkNomeError;

/// The uniform suite `--version` one-liner via [`bismark_meta::version_line`]:
/// `NOMe_filtering (Bismark Rust suite) v<version> (<hash> — <os>/<arch> — built <ts>)`.
#[must_use]
pub fn version_string() -> String {
    bismark_meta::version_line("NOMe_filtering")
}

/// End-to-end entry point: validate the CLI, create the output directory,
/// resolve the `--dir`-relative input/output paths, verify the input exists,
/// load the genome via the promoted [`bismark_io::genome`] reader (two plain
/// tiers), then run the per-read NOMe filter and write the always-gzipped
/// `.manOwar.txt.gz` report via [`nome::write_report`].
///
/// # Errors
/// Propagates [`BismarkNomeError`] for invalid flags (`--merge_CpGs`+`--CX`), a
/// missing genome folder, a non-existent input, a genome-load failure, or an
/// empty / all-`^Bismark` input ([`error::BismarkNomeError::EmptyInput`], raised
/// after the header is written — see §D4).
pub fn run(cli: Cli) -> Result<(), BismarkNomeError> {
    let cfg = cli.validate()?;

    // Create the output directory if needed (Perl writes the report into --dir).
    if !cfg.output_dir.as_os_str().is_empty() && cfg.output_dir != std::path::Path::new(".") {
        std::fs::create_dir_all(&cfg.output_dir)?;
    }

    // Perl opens the input by bare filename relative to --dir; we resolved the
    // same path in `validate`. Mirror Perl's `-e` existence check.
    if !cfg.input_path.exists() {
        return Err(BismarkNomeError::InfileNotFound);
    }

    // Two PLAIN tiers (`.fa` → `.fasta`) — NOMe never reads gzipped FASTA, so a
    // `.fa.gz`-only folder correctly errors (the intended Perl-faithful footgun).
    let genome = bismark_io::genome::Genome::load(&cfg.genome_folder, &[".fa", ".fasta"])?;
    eprintln!(
        "Stored sequence information of {} chromosomes/scaffolds in total",
        genome.len()
    );

    // Per-read NOMe filtering → always-gzipped `.manOwar.txt.gz` report. The
    // ORDER is byte-identity-critical (SPEC §D4 / pitfall P11): `write_report`
    // opens the writer + writes the header BEFORE the read loop and `finish()`es
    // the encoder even on the empty-input error path, so Perl's header-only
    // `.gz` artifact still lands on disk.
    crate::nome::write_report(&cfg.input_path, &cfg.output_path, &genome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_string_has_binary_name_and_semver() {
        let v = version_string();
        assert!(
            v.starts_with("NOMe_filtering (Bismark Rust suite) v"),
            "got: {v}"
        );
        // Reports the SUITE version (single source: rust/VERSION), not the crate's own.
        assert!(v.contains(bismark_meta::SUITE_VERSION), "got: {v}");
        assert!(v.contains(std::env::consts::OS), "got: {v}");
    }
}
