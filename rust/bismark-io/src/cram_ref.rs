//! Reconstitute a multi-FASTA reference from a Bismark genome directory.
//!
//! Equivalent to Perl Bismark's auto-reconstitution at `bismark:5129-5141`
//! when `--cram_ref` is not supplied: walks the Bismark genome directory,
//! concatenates the chromosomes into a single multi-FASTA file, and
//! writes it to `output`.
//!
//! Differences from Perl:
//!
//! - **Deterministic ordering.** Perl iterates `%chromosomes` in
//!   hash order (randomised since Perl 5.18); we sort chromosomes
//!   alphabetically by name for reproducibility.
//! - **Byte-identity vs Perl is NOT a goal** (per `PLAN.md` V8). The
//!   acceptance criterion is per-chromosome sequence identity.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::BismarkIoError;

/// Walk `bismark_genome_dir` for FASTA files at the top level, extract
/// chromosomes from each, and write them to `output` as a single
/// multi-FASTA. Chromosomes are sorted alphabetically by name for
/// deterministic output across runs.
///
/// Subdirectories (e.g. `Bisulfite_Genome/`, which contains the
/// bisulfite-converted CT and GA reference indices) are NOT descended
/// into — only the original-strand FASTAs at the top level are used.
///
/// Accepted FASTA extensions: `.fa`, `.fasta`, `.fna`, `.ffn` (case-insensitive).
///
/// # Errors
///
/// - I/O errors reading the directory or any FASTA file.
/// - I/O errors writing the output file.
/// - `Io(NotFound)` if no FASTA files are found at the top level.
pub fn reconstitute_cram_reference_from_bismark_genome(
    bismark_genome_dir: &Path,
    output: &Path,
) -> Result<(), BismarkIoError> {
    let mut fasta_files: Vec<PathBuf> = std::fs::read_dir(bismark_genome_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && is_fasta_extension(p))
        .collect();
    fasta_files.sort();

    if fasta_files.is_empty() {
        return Err(BismarkIoError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "no FASTA files (.fa/.fasta/.fna/.ffn) at top level of {}",
                bismark_genome_dir.display()
            ),
        )));
    }

    // Collect (chr_name, sequence) pairs from all FASTA files.
    let mut chromosomes: Vec<(String, Vec<u8>)> = Vec::new();
    for fasta_path in &fasta_files {
        let mut reader = noodles_fasta::io::reader::Builder.build_from_path(fasta_path)?;
        for record in reader.records() {
            let record = record?;
            let name = std::str::from_utf8(record.name())
                .map(String::from)
                .unwrap_or_else(|_| String::from_utf8_lossy(record.name()).into_owned());
            let sequence: Vec<u8> = record.sequence().as_ref().to_vec();
            chromosomes.push((name, sequence));
        }
    }

    // Sort alphabetically by chromosome name for deterministic output.
    chromosomes.sort_by(|a, b| a.0.cmp(&b.0));

    // Write multi-FASTA.
    let file = std::fs::File::create(output)?;
    let mut writer = std::io::BufWriter::new(file);
    for (name, seq) in &chromosomes {
        writeln!(writer, ">{name}")?;
        writer.write_all(seq)?;
        writeln!(writer)?;
    }
    writer.flush()?;
    Ok(())
}

fn is_fasta_extension(p: &Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("fa") | Some("fasta") | Some("fna") | Some("ffn")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    fn write_fasta(dir: &Path, filename: &str, content: &str) {
        std::fs::write(dir.join(filename), content).unwrap();
    }

    #[test]
    fn reconstitute_single_fasta_file() {
        let tmp = TempDir::new().unwrap();
        write_fasta(tmp.path(), "genome.fa", ">chr1\nACGT\n>chr2\nGGCC\n");
        let out = tmp.path().join("out.mfa");
        reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap();

        let mut content = String::new();
        std::fs::File::open(&out)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        // Sorted alphabetically: chr1 before chr2.
        assert!(content.contains(">chr1\n"));
        assert!(content.contains(">chr2\n"));
        let chr1_pos = content.find(">chr1\n").unwrap();
        let chr2_pos = content.find(">chr2\n").unwrap();
        assert!(
            chr1_pos < chr2_pos,
            "chromosomes should be alphabetically sorted in output"
        );
    }

    #[test]
    fn reconstitute_multiple_fasta_files_concatenates_chromosomes() {
        let tmp = TempDir::new().unwrap();
        write_fasta(tmp.path(), "chr1.fa", ">chr1\nACGT\n");
        write_fasta(tmp.path(), "chr2.fa", ">chr2\nGGCC\n");
        let out = tmp.path().join("out.mfa");
        reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap();

        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains(">chr1\nACGT"));
        assert!(content.contains(">chr2\nGGCC"));
    }

    #[test]
    fn reconstitute_skips_non_fasta_extensions() {
        let tmp = TempDir::new().unwrap();
        write_fasta(tmp.path(), "genome.fa", ">chr1\nAAAA\n");
        write_fasta(tmp.path(), "ignored.txt", "irrelevant content");
        write_fasta(tmp.path(), "ignored.bam", "fake bam");
        let out = tmp.path().join("out.mfa");
        reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap();

        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains(">chr1\nAAAA"));
        // The non-FASTA files' contents should not appear.
        assert!(!content.contains("irrelevant"));
        assert!(!content.contains("fake bam"));
    }

    #[test]
    fn reconstitute_skips_subdirectories() {
        let tmp = TempDir::new().unwrap();
        write_fasta(tmp.path(), "genome.fa", ">chr1\nAAAA\n");
        // Create a Bisulfite_Genome subdir with a FASTA inside — should be ignored.
        std::fs::create_dir(tmp.path().join("Bisulfite_Genome")).unwrap();
        write_fasta(
            &tmp.path().join("Bisulfite_Genome"),
            "ct_conversion.fa",
            ">chr1_CT_converted\nAATT\n",
        );
        let out = tmp.path().join("out.mfa");
        reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap();

        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains(">chr1\nAAAA"));
        assert!(
            !content.contains("chr1_CT_converted"),
            "FASTAs inside subdirectories must not be included"
        );
    }

    #[test]
    fn reconstitute_empty_dir_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let out = tmp.path().join("out.mfa");
        let err = reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap_err();
        assert!(
            matches!(err, BismarkIoError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound)
        );
    }

    #[test]
    fn reconstitute_output_chromosome_order_is_deterministic() {
        // Build a directory with chromosomes in reverse alphabetical name order.
        // Output should still be in alphabetical order.
        let tmp = TempDir::new().unwrap();
        write_fasta(
            tmp.path(),
            "z_input.fa",
            ">chrZ\nZZZ\n>chrA\nAAA\n>chrM\nMMM\n",
        );
        let out = tmp.path().join("out.mfa");
        reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap();

        let content = std::fs::read_to_string(&out).unwrap();
        let chra = content.find(">chrA").unwrap();
        let chrm = content.find(">chrM").unwrap();
        let chrz = content.find(">chrZ").unwrap();
        assert!(chra < chrm && chrm < chrz, "expected alphabetical order");
    }
}
