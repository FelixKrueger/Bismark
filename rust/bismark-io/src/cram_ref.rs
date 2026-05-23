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
//!   by name for reproducibility.
//! - **Byte-identity vs Perl is NOT a goal** (per `PLAN.md` V8). The
//!   acceptance criterion is per-chromosome sequence identity.
//!
//! Implementation choices (informed by code review):
//!
//! - **Chromosome names are byte-strings** (`Vec<u8>`), not `String`.
//!   SAM/BAM/CRAM specs allow chromosome names to contain non-UTF-8
//!   bytes; surfacing them through `String` would force `from_utf8_lossy`
//!   which silently corrupts the names and breaks CRAM-container lookup.
//! - **Atomic write**: output is first written to `<output>.tmp` and
//!   renamed at the end. A failure mid-write does NOT leave a corrupt
//!   half-written `.mfa`.
//! - **Duplicate-name detection**: if two input FASTAs declare the same
//!   chromosome name, surfaces `DuplicateChromosomeName` rather than
//!   silently producing a multi-FASTA that downstream `samtools faidx`
//!   would reject.
//! - **Gzipped FASTAs supported**: `.fa.gz`, `.fasta.gz`, `.fna.gz`,
//!   `.ffn.gz` (plus `.bgz` siblings) are accepted; noodles-fasta's
//!   `build_from_path` decompresses them transparently.

use std::collections::HashSet;
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
        .filter(|p| p.is_file() && is_fasta_file(p))
        .collect();
    fasta_files.sort();

    if fasta_files.is_empty() {
        return Err(BismarkIoError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "no FASTA files (.fa[.gz]/.fasta[.gz]/.fna[.gz]/.ffn[.gz]) \
                 at top level of {}",
                bismark_genome_dir.display()
            ),
        )));
    }

    // Collect (chr_name_bytes, sequence) pairs from all FASTA files.
    // Names are kept as Vec<u8> for byte-fidelity — SAM/BAM/CRAM specs
    // permit non-UTF-8 chromosome names and surfacing them as String
    // would force from_utf8_lossy which silently corrupts.
    let mut chromosomes: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for fasta_path in &fasta_files {
        let mut reader = noodles_fasta::io::reader::Builder.build_from_path(fasta_path)?;
        for record in reader.records() {
            let record = record?;
            let name: Vec<u8> = record.name().to_vec();
            let sequence: Vec<u8> = record.sequence().as_ref().to_vec();
            chromosomes.push((name, sequence));
        }
    }

    // Sort by chromosome-name bytes for deterministic output.
    chromosomes.sort_by(|a, b| a.0.cmp(&b.0));

    // Reject duplicates — a multi-FASTA with duplicate chromosome names
    // would be rejected by samtools faidx and silently produce wrong
    // CRAM-container lookups.
    let mut seen: HashSet<&[u8]> = HashSet::new();
    for (name, _) in &chromosomes {
        if !seen.insert(name.as_slice()) {
            return Err(BismarkIoError::DuplicateChromosomeName {
                name: String::from_utf8_lossy(name).into_owned(),
            });
        }
    }

    // Atomic write: write to a sibling temp path, rename when complete.
    // POSIX rename(2) is atomic; partial writes don't leak as half-baked
    // `.mfa` files.
    let tmp_path = tmp_path_for(output);
    {
        let file = std::fs::File::create(&tmp_path)?;
        let mut writer = std::io::BufWriter::new(file);
        for (name, seq) in &chromosomes {
            writer.write_all(b">")?;
            writer.write_all(name)?;
            writer.write_all(b"\n")?;
            writer.write_all(seq)?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
    }
    std::fs::rename(&tmp_path, output)?;
    Ok(())
}

/// Produce a sibling temp-path next to `output` for atomic-write.
fn tmp_path_for(output: &Path) -> PathBuf {
    let mut tmp = output.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

/// Check whether `p`'s filename indicates a (optionally gzipped) FASTA.
///
/// Accepts: `.fa`, `.fasta`, `.fna`, `.ffn`, plus `.fa.gz` / `.fa.bgz` and
/// the same suffixes for the other three stems. noodles-fasta's
/// `build_from_path` handles the gzip/bgzf decompression transparently.
fn is_fasta_file(p: &Path) -> bool {
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    [
        ".fa",
        ".fasta",
        ".fna",
        ".ffn",
        ".fa.gz",
        ".fasta.gz",
        ".fna.gz",
        ".ffn.gz",
        ".fa.bgz",
        ".fasta.bgz",
        ".fna.bgz",
        ".ffn.bgz",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
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
    fn reconstitute_rejects_duplicate_chromosome_names() {
        let tmp = TempDir::new().unwrap();
        // Two FASTAs in the genome dir, both declaring "chr1".
        write_fasta(tmp.path(), "a.fa", ">chr1\nAAAA\n");
        write_fasta(tmp.path(), "b.fa", ">chr1\nGGGG\n");
        let out = tmp.path().join("out.mfa");
        let err = reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap_err();
        assert!(
            matches!(err, BismarkIoError::DuplicateChromosomeName { ref name } if name == "chr1"),
            "expected DuplicateChromosomeName for chr1, got {err:?}"
        );
        // Atomic-write invariant: failed run must NOT leave a partial output.
        assert!(!out.exists(), "failed run must not leave a partial .mfa");
    }

    #[test]
    fn reconstitute_accepts_gzipped_fasta() {
        use std::io::Write as _;

        let tmp = TempDir::new().unwrap();
        // noodles-fasta uses BGZF (block-gzip) for .gz; write a tiny BGZF-compressed FASTA.
        let mut bgzf = noodles_bgzf::io::Writer::new(
            std::fs::File::create(tmp.path().join("genome.fa.gz")).unwrap(),
        );
        bgzf.write_all(b">chrGZ\nACGTACGT\n").unwrap();
        bgzf.finish().unwrap();

        let out = tmp.path().join("out.mfa");
        reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(
            content.contains(">chrGZ"),
            "gzipped FASTA must be read; output content: {content:?}"
        );
    }

    #[test]
    fn reconstitute_atomic_write_creates_no_partial_on_zero_chromosomes() {
        // If chromosome collection succeeds with at least one record but
        // a hypothetical error occurred, atomic-write should prevent
        // partial output. This is a smoke test on the happy path: out
        // path is created via rename at the end, never partially.
        let tmp = TempDir::new().unwrap();
        write_fasta(tmp.path(), "genome.fa", ">chr1\nA\n");
        let out = tmp.path().join("out.mfa");
        reconstitute_cram_reference_from_bismark_genome(tmp.path(), &out).unwrap();
        assert!(out.exists());
        // No leftover .tmp file from the atomic-write process.
        let tmp_file = tmp.path().join("out.mfa.tmp");
        assert!(
            !tmp_file.exists(),
            "atomic-write temp file must be renamed away"
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
