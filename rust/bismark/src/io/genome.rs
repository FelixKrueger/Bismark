//! Whole-genome FASTA reader for Bismark genome folders, faithful to Perl
//! `read_genome_into_memory` + `extract_chromosome_name` (as used by both
//! `coverage2cytosine` and the standalone `NOMe_filtering`).
//!
//! This is a **shared, tier-parameterized** promotion of the reader that
//! `bismark-coverage2cytosine` originally carried crate-locally, so NOMe-style
//! ports stop re-deriving it. It is distinct from [`crate::io::cram_ref`] (which
//! reconstitutes a CRAM reference and does NOT uppercase).
//!
//! Quirks reproduced:
//! - **Tier-parameterized glob priority**: the caller supplies an ordered list
//!   of suffixes (e.g. `&[".fa", ".fa.gz", ".fasta", ".fasta.gz"]` for c2c, or
//!   `&[".fa", ".fasta"]` for `NOMe_filtering`); the **first tier with ≥1
//!   matching filename wins** (no union, no fall-through). Leading-dot files
//!   are excluded (Perl `<*.fa>` never matches dotfiles).
//! - **`Mus_musculus.NCBIM37.fa` skip** (a Perl-ism for the tophat mouse file).
//! - **Uppercase on load** (soft-mask safety — divergence from `cram_ref.rs`).
//! - **Chromosome name** = first whitespace token after `>` (noodles
//!   `record.name()` already yields this; trailing `\r` is auto-stripped).
//! - **Duplicate name → error**; **present-but-malformed/empty file → error**.
//! - **`u32` length guard** (downstream positions are `u32`).
//!
//! **Invariant:** [`Genome`] exposes NO public insertion-order iterator — the
//! only name accessor is [`Genome::names_sorted`] (bytewise-sorted). Callers
//! that need a deterministic order use that; nothing reads the backing
//! `HashMap`'s iteration order. Do NOT add an `iter()`/`keys()` passthrough.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// `(chromosome name, uppercased sequence)` pairs read from one FASTA file.
type FastaRecords = Vec<(Vec<u8>, Vec<u8>)>;

/// The tophat whole-mouse FASTA Perl explicitly skips.
const MUS_SKIP: &str = "Mus_musculus.NCBIM37.fa";

/// Errors raised by the Bismark whole-genome FASTA reader.
///
/// Module-local (NOT a [`crate::io::error::BismarkIoError`] variant) so that adding
/// the `genome` module is a purely additive change to `bismark-io` — it cannot
/// break a downstream crate's non-exhaustive `match` on `BismarkIoError`.
#[derive(Debug, thiserror::Error)]
pub enum GenomeError {
    /// No FASTA matched any supplied tier at the top level of the folder.
    #[error("genome folder {dir:?} does not contain any sequence files for the requested suffixes")]
    NoGenomeFasta {
        /// The genome folder searched.
        dir: PathBuf,
    },

    /// A present file in the winning tier has no valid FASTA header / yields
    /// zero records (Perl `extract_chromosome_name` dies on a non-`>`/empty
    /// first line). Also covers the bare/nameless `>` header (noodles
    /// `InvalidData`) — a documented divergence from Perl, which would store
    /// an empty-name chromosome; cannot occur on a Bowtie2-built genome.
    #[error("malformed or empty FASTA (no valid `>` header): {file:?}")]
    MalformedFastaHeader {
        /// The offending file.
        file: PathBuf,
    },

    /// Two chromosomes share a name.
    #[error("duplicate chromosome name: {name} (all chromosomes must have a unique name)")]
    DuplicateChromosomeName {
        /// The repeated name (lossy-decoded for display).
        name: String,
    },

    /// A chromosome exceeds `u32::MAX` bases (downstream positions are `u32`).
    #[error("chromosome {name} length {len} exceeds u32::MAX")]
    ChromosomeTooLong {
        /// The over-long chromosome's name.
        name: String,
        /// Its length in bases.
        len: usize,
    },

    /// Underlying I/O failure (open, read, truncated gzip, …).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// In-memory genome: chromosome name → uppercased sequence bytes.
///
/// Names are `Vec<u8>` for cheap byte comparison + consistency with
/// [`crate::io::cram_ref`] (noodles surfaces names up-to-first-whitespace).
#[derive(Debug)]
pub struct Genome {
    chromosomes: HashMap<Vec<u8>, Vec<u8>>,
}

impl Genome {
    /// Load every chromosome from `genome_folder` into memory, choosing files
    /// by the ordered `tiers` glob (first non-empty tier wins).
    ///
    /// `NOMe_filtering` passes `&[".fa", ".fasta"]` (two PLAIN suffixes — Perl
    /// `NOMe_filtering` never reads gzipped FASTA, so a `.fa.gz`-only folder
    /// correctly yields [`GenomeError::NoGenomeFasta`]). `coverage2cytosine`
    /// would pass the four-tier list including `.gz` suffixes.
    ///
    /// # Errors
    /// - [`GenomeError::NoGenomeFasta`] if no file matches any tier.
    /// - [`GenomeError::MalformedFastaHeader`] if a file in the winning tier
    ///   has no valid header / yields zero records.
    /// - [`GenomeError::DuplicateChromosomeName`] on a repeated name.
    /// - [`GenomeError::ChromosomeTooLong`] if a sequence exceeds `u32::MAX`.
    pub fn load(genome_folder: &Path, tiers: &[&str]) -> Result<Self, GenomeError> {
        let files = discover_fasta_files(genome_folder, tiers)?;

        let mut chromosomes: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        let mut seen: HashSet<Vec<u8>> = HashSet::new();

        for path in &files {
            // Mus skip happens INSIDE the loop, after the tier was chosen —
            // so a Mus-only winning tier yields an empty genome with no error.
            if path.file_name().and_then(|n| n.to_str()) == Some(MUS_SKIP) {
                continue;
            }
            for (name, seq) in read_one_fasta(path)? {
                if !seen.insert(name.clone()) {
                    return Err(GenomeError::DuplicateChromosomeName {
                        name: String::from_utf8_lossy(&name).into_owned(),
                    });
                }
                check_chr_len(&name, seq.len())?;
                chromosomes.insert(name, seq);
            }
        }

        Ok(Genome { chromosomes })
    }

    /// Sequence bytes for `name`, if present.
    #[must_use]
    pub fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.chromosomes.get(name).map(Vec::as_slice)
    }

    /// Whether `name` is present.
    #[must_use]
    pub fn contains(&self, name: &[u8]) -> bool {
        self.chromosomes.contains_key(name)
    }

    /// All chromosome names, **bytewise-sorted**. The ONLY name-iterating
    /// accessor — see the module-level invariant.
    #[must_use]
    pub fn names_sorted(&self) -> Vec<&[u8]> {
        let mut names: Vec<&[u8]> = self.chromosomes.keys().map(Vec::as_slice).collect();
        names.sort_unstable();
        names
    }

    /// Number of chromosomes loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.chromosomes.len()
    }

    /// Whether no chromosomes were loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chromosomes.is_empty()
    }
}

/// Pick the winning glob tier (first supplied tier with ≥1 matching filename).
fn discover_fasta_files(dir: &Path, tiers: &[&str]) -> Result<Vec<PathBuf>, GenomeError> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();

    for tier in tiers {
        let mut tier_files: Vec<PathBuf> = entries
            .iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    // Perl's `<*.fa>` glob does not match leading-dot files;
                    // exclude dotfiles (hidden / partial-download `.x.fa.gz`)
                    // so we don't ingest a chromosome Perl would never see.
                    .is_some_and(|n| !n.starts_with('.') && n.ends_with(tier))
            })
            .cloned()
            .collect();
        if !tier_files.is_empty() {
            // Order is irrelevant to output (no public insertion-order
            // accessor); sort for test/determinism stability.
            tier_files.sort();
            return Ok(tier_files);
        }
    }

    Err(GenomeError::NoGenomeFasta {
        dir: dir.to_path_buf(),
    })
}

/// Read one FASTA file (plain or gzipped) into `(name, uppercased_seq)` pairs.
/// A present file that yields zero records is treated as malformed (Perl's
/// `extract_chromosome_name` dies on a non-`>` / empty first line).
fn read_one_fasta(path: &Path) -> Result<FastaRecords, GenomeError> {
    let file = File::open(path)?;
    let is_gz = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".gz"));

    // MultiGzDecoder decodes plain gzip (Perl `gunzip -c`) AND gzip-framed
    // BGZF; noodles' build_from_path is BGZF-only, so we decompress ourselves.
    let records = if is_gz {
        collect_records(
            BufReader::new(flate2::read::MultiGzDecoder::new(file)),
            path,
        )?
    } else {
        collect_records(BufReader::new(file), path)?
    };

    if records.is_empty() {
        return Err(GenomeError::MalformedFastaHeader {
            file: path.to_path_buf(),
        });
    }
    Ok(records)
}

fn collect_records<R: BufRead>(inner: R, path: &Path) -> Result<FastaRecords, GenomeError> {
    let mut reader = noodles_fasta::io::Reader::new(inner);
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| {
            // A non-`>` / nameless header surfaces as InvalidData → malformed;
            // anything else (truncated gzip, mid-file read error) is a genuine
            // I/O error and must NOT masquerade as "not FASTA".
            if e.kind() == std::io::ErrorKind::InvalidData {
                GenomeError::MalformedFastaHeader {
                    file: path.to_path_buf(),
                }
            } else {
                GenomeError::Io(e)
            }
        })?;
        let name = record.name().to_vec();
        let seq: Vec<u8> = record
            .sequence()
            .as_ref()
            .iter()
            .map(u8::to_ascii_uppercase)
            .collect();
        out.push((name, seq));
    }
    Ok(out)
}

/// Reject chromosomes longer than `u32::MAX` (downstream positions are `u32`).
fn check_chr_len(name: &[u8], len: usize) -> Result<(), GenomeError> {
    if len > u32::MAX as usize {
        return Err(GenomeError::ChromosomeTooLong {
            name: String::from_utf8_lossy(name).into_owned(),
            len,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    fn write(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn loads_multifasta_first_token_name_and_uppercases() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "g.fa",
            ">chr1 some description\nacgt\nACGT\n>chr2\nGGCC\n",
        );
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGTACGT");
        assert_eq!(g.get(b"chr2").unwrap(), b"GGCC");
        assert!(g.contains(b"chr1") && !g.contains(b"chrZ"));
        assert!(!g.is_empty());
    }

    #[test]
    fn two_plain_tiers_fa_beats_fasta_no_union() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "a.fa", ">chr1\nACGT\n");
        write(t.path(), "b.fasta", ">chrZ\nTTTT\n"); // wrong tier, never read
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert!(g.contains(b"chr1") && !g.contains(b"chrZ"));
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn fa_gz_invisible_with_two_plain_tiers() {
        // SPEC P14: a `.fa.gz`-only genome dies with NoGenomeFasta when the
        // tiers are the two PLAIN suffixes — the intended, Perl-faithful
        // footgun for `NOMe_filtering`.
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa.gz", "irrelevant — wrong tier");
        assert!(matches!(
            Genome::load(t.path(), &[".fa", ".fasta"]).unwrap_err(),
            GenomeError::NoGenomeFasta { .. }
        ));
    }

    #[test]
    fn fasta_tier_used_when_no_fa() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fasta", ">chr1\nACGT\n");
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
    }

    #[test]
    fn mus_only_tier_yields_empty_genome_no_error() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "Mus_musculus.NCBIM37.fa", ">chrM\nACGT\n");
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert!(g.is_empty());
    }

    #[test]
    fn mus_skipped_among_others_and_crlf_stripped() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "Mus_musculus.NCBIM37.fa", ">chrM\nAAAA\n");
        write(t.path(), "chr1.fa", ">chr1\r\nAC\r\nGT\r\n");
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
        assert!(!g.get(b"chr1").unwrap().contains(&b'\r'));
        assert!(!g.contains(b"chrM"));
    }

    #[test]
    fn duplicate_name_cross_file_errors() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "a.fa", ">chr1\nAAAA\n");
        write(t.path(), "b.fa", ">chr1\nGGGG\n");
        assert!(matches!(
            Genome::load(t.path(), &[".fa"]).unwrap_err(),
            GenomeError::DuplicateChromosomeName { name } if name == "chr1"
        ));
    }

    #[test]
    fn bare_or_nameless_header_errors() {
        // Documented divergence inherited from c2c: a bare `>` header has no
        // name. Perl stores an empty-name chromosome (no error); the Rust port
        // errors (noodles InvalidData → malformed). Cannot occur on a
        // Bowtie2-built Bismark genome.
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">\nACGT\n");
        assert!(matches!(
            Genome::load(t.path(), &[".fa"]).unwrap_err(),
            GenomeError::MalformedFastaHeader { .. }
        ));
    }

    #[test]
    fn no_fasta_anywhere_errors() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "readme.txt", "nope");
        assert!(matches!(
            Genome::load(t.path(), &[".fa", ".fasta"]).unwrap_err(),
            GenomeError::NoGenomeFasta { .. }
        ));
    }

    #[test]
    fn dotfiles_not_matched() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), ".partial.fa", ">ghost\nACGT\n");
        write(t.path(), "real.fa", ">chr1\nACGT\n");
        let g = Genome::load(t.path(), &[".fa"]).unwrap();
        assert!(g.contains(b"chr1") && !g.contains(b"ghost"));
    }

    #[test]
    fn names_sorted_is_bytewise() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chr10\nA\n>chr2\nC\n>chrX\nG\n");
        let g = Genome::load(t.path(), &[".fa"]).unwrap();
        assert_eq!(
            g.names_sorted(),
            vec![&b"chr10"[..], &b"chr2"[..], &b"chrX"[..]]
        );
    }

    #[test]
    fn loads_plain_gzip_fa_gz_when_gz_tier_supplied() {
        // The promoted reader is gz-capable; c2c-style callers pass `.fa.gz`
        // tiers. (NOMe never does, but the capability must work.)
        use flate2::{Compression, write::GzEncoder};
        let t = tempfile::tempdir().unwrap();
        let mut e = GzEncoder::new(
            std::fs::File::create(t.path().join("g.fa.gz")).unwrap(),
            Compression::default(),
        );
        e.write_all(b">chrG\nacgtACGT\n").unwrap();
        e.finish().unwrap();
        let g = Genome::load(t.path(), &[".fa", ".fa.gz"]).unwrap();
        assert_eq!(g.get(b"chrG").unwrap(), b"ACGTACGT");
    }

    #[test]
    fn u32_overflow_guard_helper() {
        assert!(matches!(
            check_chr_len(b"big", (u32::MAX as usize) + 1).unwrap_err(),
            GenomeError::ChromosomeTooLong { .. }
        ));
        assert!(check_chr_len(b"ok", 1000).is_ok());
    }
}
