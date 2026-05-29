//! Whole-genome FASTA reader, faithful to Perl `read_genome_into_memory`
//! (`coverage2cytosine:1648-1739`) + `extract_chromosome_name` (`:1741-1751`).
//!
//! Quirks reproduced (SPEC §6):
//! - **Four-suffix glob priority**: `*.fa` → `*.fa.gz` → `*.fasta` →
//!   `*.fasta.gz`; the **first tier with ≥1 matching filename wins** (no
//!   union, no fall-through if the winning tier's files are skipped/empty).
//! - **`Mus_musculus.NCBIM37.fa` skip** (a Perl-ism for the tophat mouse file).
//! - **Uppercase on load** (soft-mask safety — divergence from `cram_ref.rs`).
//! - **Chromosome name** = first whitespace token after `>` (noodles
//!   `record.name()` already yields this).
//! - **Duplicate name → error**; **present-but-malformed/empty file → error**.
//! - **`u32` length guard** (positions are `u32`).
//!
//! **Invariant (keeps SPEC Deviation D1 airtight):** `Genome` exposes NO
//! public insertion-order iterator — the only name accessor is
//! [`Genome::names_sorted`] (bytewise-sorted). Covered-chromosome *output*
//! order is the coverage-file appearance order (a Phase-B concern), and
//! uncovered chromosomes are emitted via `names_sorted()`; neither reads this
//! map's `HashMap` order. Do NOT add an `iter()`/`keys()` passthrough.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::error::BismarkC2cError;

/// `(chromosome name, uppercased sequence)` pairs read from one FASTA file.
type FastaRecords = Vec<(Vec<u8>, Vec<u8>)>;

/// The tophat whole-mouse FASTA Perl explicitly skips (`:1678`).
const MUS_SKIP: &str = "Mus_musculus.NCBIM37.fa";

/// Glob tiers in Perl's priority order; the first tier with ≥1 matching
/// filename wins. Suffixes are disjoint by construction (`.fa` does not match
/// `.fa.gz` or `.fasta`).
const FASTA_TIERS: [&str; 4] = [".fa", ".fa.gz", ".fasta", ".fasta.gz"];

/// In-memory genome: chromosome name → uppercased sequence bytes.
///
/// Names are `Vec<u8>` for cheap byte comparison + consistency with
/// `bismark-io::cram_ref` (noodles surfaces names up-to-first-whitespace).
#[derive(Debug)]
pub struct Genome {
    chromosomes: HashMap<Vec<u8>, Vec<u8>>,
}

impl Genome {
    /// Load every chromosome from the genome folder into memory.
    ///
    /// # Errors
    /// - [`BismarkC2cError::NoGenomeFasta`] if no `.fa`/`.fa.gz`/`.fasta`/
    ///   `.fasta.gz` file is present at the top level.
    /// - [`BismarkC2cError::MalformedFastaHeader`] if a present file in the
    ///   winning tier has no valid FASTA header / yields zero records.
    /// - [`BismarkC2cError::DuplicateChromosomeName`] on a repeated name.
    /// - [`BismarkC2cError::ChromosomeTooLong`] if a sequence exceeds `u32::MAX`.
    pub fn load(genome_folder: &Path) -> Result<Self, BismarkC2cError> {
        let files = discover_fasta_files(genome_folder)?;

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
                    return Err(BismarkC2cError::DuplicateChromosomeName {
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

    /// All chromosome names, **bytewise-sorted** (the uncovered-chromosome
    /// emission order; matches Perl `sort keys %processed`). This is the ONLY
    /// name-iterating accessor — see the module-level invariant.
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

/// Pick the winning glob tier (first tier with ≥1 matching filename).
fn discover_fasta_files(dir: &Path) -> Result<Vec<PathBuf>, BismarkC2cError> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();

    for tier in FASTA_TIERS {
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
            // Order is irrelevant to output (D1); sort for test stability.
            tier_files.sort();
            return Ok(tier_files);
        }
    }

    Err(BismarkC2cError::NoGenomeFasta {
        dir: dir.to_path_buf(),
    })
}

/// Read one FASTA file (plain or gzipped) into `(name, uppercased_seq)` pairs.
/// A present file that yields zero records is treated as malformed (Perl's
/// `extract_chromosome_name` dies on a non-`>` / empty first line).
fn read_one_fasta(path: &Path) -> Result<FastaRecords, BismarkC2cError> {
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
        return Err(BismarkC2cError::MalformedFastaHeader {
            file: path.to_path_buf(),
        });
    }
    Ok(records)
}

fn collect_records<R: BufRead>(inner: R, path: &Path) -> Result<FastaRecords, BismarkC2cError> {
    let mut reader = noodles_fasta::io::Reader::new(inner);
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| {
            // A non-`>` / nameless header surfaces as InvalidData → malformed;
            // anything else (truncated gzip, mid-file read error) is a genuine
            // I/O error and must NOT masquerade as "not FASTA".
            if e.kind() == std::io::ErrorKind::InvalidData {
                BismarkC2cError::MalformedFastaHeader {
                    file: path.to_path_buf(),
                }
            } else {
                BismarkC2cError::Io(e)
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

/// Reject chromosomes longer than `u32::MAX` (positions are `u32`).
fn check_chr_len(name: &[u8], len: usize) -> Result<(), BismarkC2cError> {
    if len > u32::MAX as usize {
        return Err(BismarkC2cError::ChromosomeTooLong {
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

    // ── Task 6: basics ──

    #[test]
    fn loads_multifasta_first_token_name_and_uppercases() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "g.fa",
            ">chr1 some description\nacgt\nACGT\n>chr2\nGGCC\n",
        );
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGTACGT"); // uppercased + joined
        assert_eq!(g.get(b"chr2").unwrap(), b"GGCC");
        assert!(g.contains(b"chr1") && !g.contains(b"chrZ"));
        assert!(!g.is_empty());
    }

    #[test]
    fn names_sorted_is_bytewise() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chr10\nA\n>chr2\nC\n>chrX\nG\n");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(
            g.names_sorted(),
            vec![&b"chr10"[..], &b"chr2"[..], &b"chrX"[..]]
        );
    }

    // ── Task 7: glob priority + Mus skip + empty-dir ──

    #[test]
    fn glob_priority_fa_beats_fa_gz() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "chr.fa", ">chr1\nACGT\n");
        write(t.path(), "chr.fa.gz", "ignored — wrong tier, never read");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
    }

    #[test]
    fn mus_only_tier_yields_empty_genome_no_error() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "Mus_musculus.NCBIM37.fa", ">chrM\nACGT\n");
        let g = Genome::load(t.path()).unwrap(); // NOT an error
        assert!(g.is_empty());
    }

    #[test]
    fn mus_skipped_among_others() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "Mus_musculus.NCBIM37.fa", ">chrM\nAAAA\n");
        write(t.path(), "chr1.fa", ">chr1\nACGT\n");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.len(), 1);
        assert!(g.contains(b"chr1") && !g.contains(b"chrM"));
    }

    #[test]
    fn no_fasta_anywhere_errors() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "readme.txt", "nope");
        assert!(matches!(
            Genome::load(t.path()).unwrap_err(),
            BismarkC2cError::NoGenomeFasta { .. }
        ));
    }

    #[test]
    fn fasta_tier_chosen_when_no_fa() {
        // No `.fa`/`.fa.gz` present → the `.fasta` tier wins.
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fasta", ">chr1\nACGT\n");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
    }

    #[test]
    fn dotfiles_are_not_matched_by_glob() {
        // Code-review B-1: Perl `<*.fa>` skips leading-dot files; a
        // partial-download `.x.fa` must NOT be ingested as a chromosome.
        let t = tempfile::tempdir().unwrap();
        write(t.path(), ".partial.fa", ">ghost\nACGT\n");
        write(t.path(), "real.fa", ">chr1\nACGT\n");
        let g = Genome::load(t.path()).unwrap();
        assert!(g.contains(b"chr1") && !g.contains(b"ghost"));
    }

    #[test]
    fn bare_or_nameless_header_errors() {
        // Code-review A-M1: a bare `>` header has no name. Perl stores an
        // empty-name chromosome (no error); the Rust port errors. This cannot
        // occur on a Bowtie2-built Bismark genome (clean `>chrN` headers); the
        // test pins the documented divergence (noodles InvalidData → malformed).
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">\nACGT\n");
        assert!(matches!(
            Genome::load(t.path()).unwrap_err(),
            BismarkC2cError::MalformedFastaHeader { .. }
        ));
    }

    // ── Task 8: gz support ──

    #[test]
    fn loads_plain_gzip_fa_gz() {
        use flate2::{Compression, write::GzEncoder};
        let t = tempfile::tempdir().unwrap();
        let mut e = GzEncoder::new(
            std::fs::File::create(t.path().join("g.fa.gz")).unwrap(),
            Compression::default(),
        );
        e.write_all(b">chrG\nacgtACGT\n").unwrap();
        e.finish().unwrap();
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.get(b"chrG").unwrap(), b"ACGTACGT");
    }

    #[test]
    fn loads_bgzf_fa_gz() {
        let t = tempfile::tempdir().unwrap();
        let mut w =
            noodles_bgzf::io::Writer::new(std::fs::File::create(t.path().join("g.fa.gz")).unwrap());
        w.write_all(b">chrB\nACGT\n").unwrap();
        w.finish().unwrap();
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.get(b"chrB").unwrap(), b"ACGT");
    }

    // ── Task 9: edge cases ──

    #[test]
    fn duplicate_name_cross_file_errors() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "a.fa", ">chr1\nAAAA\n");
        write(t.path(), "b.fa", ">chr1\nGGGG\n");
        assert!(matches!(
            Genome::load(t.path()).unwrap_err(),
            BismarkC2cError::DuplicateChromosomeName { name } if name == "chr1"
        ));
    }

    #[test]
    fn empty_file_in_winning_tier_errors() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "bad.fa", "");
        assert!(matches!(
            Genome::load(t.path()).unwrap_err(),
            BismarkC2cError::MalformedFastaHeader { .. }
        ));
    }

    #[test]
    fn headerless_file_errors() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "bad.fa", "no-header-line\nACGT\n");
        assert!(matches!(
            Genome::load(t.path()).unwrap_err(),
            BismarkC2cError::MalformedFastaHeader { .. }
        ));
    }

    #[test]
    fn crlf_sequence_has_no_carriage_return() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chr1\r\nAC\r\nGT\r\n");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
        assert!(!g.get(b"chr1").unwrap().contains(&b'\r'));
    }

    #[test]
    fn empty_sequence_record_kept() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chrEmpty\n>chr1\nACGT\n");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.get(b"chrEmpty").unwrap(), b"");
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
    }

    #[test]
    fn u32_overflow_guard_helper() {
        assert!(matches!(
            check_chr_len(b"big", (u32::MAX as usize) + 1).unwrap_err(),
            BismarkC2cError::ChromosomeTooLong { .. }
        ));
        assert!(check_chr_len(b"ok", 1000).is_ok());
    }
}
