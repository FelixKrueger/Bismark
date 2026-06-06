//! `bismark-bam2nuc` — Rust port of Bismark Perl's `bam2nuc`.
//!
//! Computes the **mono- and di-nucleotide coverage** of a Bismark alignment as
//! a QC metric: for every read with a clean (no `I/D/S/N`) CIGAR it tallies the
//! **genomic** sequence at the read's mapped span (NOT the read's own bases),
//! reverse-complementing for reverse-strand reads, then compares the read-
//! derived composition against the whole-genome composition (cached in
//! `genomic_nucleotide_frequencies.txt`). The binary is installed as
//! `bam2nuc_rs`.
//!
//! Acceptance gate: **byte-identity** of `*.nucleotide_stats.txt` and
//! `genomic_nucleotide_frequencies.txt` vs Perl `bam2nuc` v0.25.1. See
//! `plans/05312026_bismark-bam2nuc/SPEC.md` (rev 1) for the design contract.
//!
//! ## Status
//!
//! Phase A — crate scaffold + CLI/validation + genome reader. `run()` grows
//! through Phases B–E (genomic composition cache → per-read counting → report
//! writer → full wiring).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod count;
pub mod error;
pub mod freqs;
pub mod genome;
pub mod output_name;
pub mod report;

pub use cli::{Cli, ResolvedConfig};
pub use error::BismarkBam2nucError;
pub use genome::Genome;

/// Run the nucleotide-coverage report (Perl `bam2nuc`'s top-level flow,
/// `:36-61`):
/// 1. Load the genome into memory.
/// 2. `--genomic_composition_only` → compute + write the cache, then exit.
/// 3. Otherwise, for each input file (argv order): gate the format (accept
///    `.bam`; reject `.sam`/`.cram`), resolve the genomic composition (computed
///    once for file #1, then reused), count the reads, and write
///    `<sample>.nucleotide_stats.txt`.
pub fn run(config: &ResolvedConfig) -> Result<(), BismarkBam2nucError> {
    use std::io::Write;

    let genome = Genome::load(&config.genome_folder)?;
    eprintln!(
        "Stored sequence information of {} chromosomes/scaffolds in total",
        genome.len()
    );

    // --genomic_composition_only: compute + write the cache (or reuse), exit.
    if config.genomic_composition_only {
        freqs::get_genomic_frequencies(&genome, &config.genome_folder, &config.output_dir)?;
        eprintln!("Finished processing genomic nucleotide frequencies");
        return Ok(());
    }

    for infile in &config.inputs {
        // Input-format gate (content sniff): accept BAM; reject SAM (SPEC Q2)
        // and CRAM (SPEC Q3). Done before counting — for a `.bam` the output
        // name always derives cleanly, so the observable outcome matches Perl
        // (which derives the name after counting).
        match bismark_io::AlignmentKind::from_path(infile)? {
            bismark_io::AlignmentKind::Bam => {}
            bismark_io::AlignmentKind::Sam => return Err(BismarkBam2nucError::SamNotSupported),
            bismark_io::AlignmentKind::Cram => return Err(BismarkBam2nucError::CramNotSupported),
        }

        // Genomic composition: reused from the genome-folder cache after file #1.
        let genomic =
            freqs::get_genomic_frequencies(&genome, &config.genome_folder, &config.output_dir)?;

        let (sample, stats) = count::count_reads_in_file(&genome, infile)?;
        eprintln!(
            "Processed {} reads ({} skipped for an InDel/softclip/skip CIGAR)",
            stats.total, stats.skipped
        );

        let name = output_name::derive_output_name(infile)?;
        let out_path = format!("{}{}", config.output_dir, name);
        eprintln!("Printing nucleotide stats to >> {out_path} <<");
        let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path)?);
        report::write_stats(&mut out, &sample, &genomic)?;
        out.flush()?;
    }

    Ok(())
}

/// TG-style provenance string for the binary's `--version` output.
///
/// Format: `bam2nuc_rs <semver> (<os>/<arch>)`.
#[must_use]
pub fn version_string() -> String {
    format!(
        "bam2nuc_rs {} ({}/{})",
        bismark_meta::SUITE_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
