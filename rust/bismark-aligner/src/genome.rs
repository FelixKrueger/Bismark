//! In-memory genome — a port of Perl `read_genome_into_memory` (5022–5147) +
//! `extract_chromosome_name` (5149–5159).
//!
//! Loads the raw reference FASTA(s) into memory once (before the per-file read
//! loop, Perl 273–277) so the methylation call can pull the matching genomic
//! window per read. **Consumes Phase 1's already-ordered FASTA list**
//! (`config.genome.fastas`, produced by `discovery::discover_fastas`) rather
//! than re-globbing — the byte-identity-critical `@SQ` order then has exactly
//! one source of truth. `sq_order` is the chromosome encounter order across that
//! list (and within each multi-FASTA file), which `output::generate_sam_header`
//! emits as the `@SQ` block.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use flate2::read::MultiGzDecoder;

use crate::error::{AlignerError, Result};

/// The reference genome held in memory.
pub struct Genome {
    /// Chromosome name → upper-case ASCII sequence (byte-indexed for O(1) slicing).
    pub chromosomes: HashMap<String, Vec<u8>>,
    /// Chromosome names in encounter order (the `@SQ` order — Perl `%SQ_order`).
    pub sq_order: Vec<String>,
}

impl Genome {
    /// Sequence bytes for `chr`, or `None` if absent.
    pub fn get(&self, chr: &str) -> Option<&[u8]> {
        self.chromosomes.get(chr).map(Vec::as_slice)
    }
}

/// Extract the chromosome name from a FASTA header line (already chomped /
/// `\r`-stripped). Mirrors Perl `extract_chromosome_name` (5149–5159): strip a
/// leading `>`, then return the first whitespace-delimited token.
///
/// Returns `Ok("")` for a leading-space header (`> chr1` → first token is empty,
/// exactly as Perl `split /\s+/` yields a leading empty field) — the
/// empty-name → die check lives in [`read_genome_into_memory`], the caller.
/// `Err` only when the line has no leading `>` (not FASTA).
fn extract_chromosome_name(header: &str) -> Result<&str> {
    let rest = header.strip_prefix('>').ok_or_else(|| {
        AlignerError::Validation(format!(
            "The specified chromosome ({header}) file doesn't seem to be in FASTA format as required!"
        ))
    })?;
    // First whitespace-delimited token; "" if `rest` starts with whitespace
    // (Perl `split /\s+/` leading-empty-field semantics).
    Ok(rest.split(char::is_whitespace).next().unwrap_or(""))
}

/// Read the raw genome FASTA(s) into memory, in the supplied order.
///
/// `fastas` is `config.genome.fastas` — the byte-significant, already-sorted
/// list from Phase 1 discovery. Chromosomes accumulate across files in list
/// order and within each file in record order; `sq_order` records that order.
pub fn read_genome_into_memory(fastas: &[PathBuf]) -> Result<Genome> {
    let mut chromosomes: HashMap<String, Vec<u8>> = HashMap::new();
    let mut sq_order: Vec<String> = Vec::new();

    for path in fastas {
        read_one_fasta(path, &mut chromosomes, &mut sq_order)?;
    }
    Ok(Genome {
        chromosomes,
        sq_order,
    })
}

/// Parse one FASTA file (gunzipping `.gz`), appending its chromosome(s).
fn read_one_fasta(
    path: &Path,
    chromosomes: &mut HashMap<String, Vec<u8>>,
    sq_order: &mut Vec<String>,
) -> Result<()> {
    let file = std::fs::File::open(path)?;
    let reader: Box<dyn Read> = if path.extension().is_some_and(|e| e == "gz") {
        Box::new(MultiGzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut reader = BufReader::new(reader);

    // First line must be a FASTA header (Perl 5064–5071).
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf)?;
    if n == 0 {
        // Empty file → not FASTA (Perl `extract_chromosome_name` dies on undef).
        return Err(AlignerError::Validation(format!(
            "The specified chromosome ({}) file doesn't seem to be in FASTA format as required!",
            path.display()
        )));
    }
    let first_line = chomp_cr(&buf);
    let mut chromosome_name = extract_chromosome_name(&first_line)?.to_string();
    if chromosome_name.is_empty() {
        return Err(empty_name_err());
    }

    let mut sequence: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        let line = chomp_cr(&buf);
        if let Some(stripped) = line.strip_prefix('>') {
            // Store the previous chromosome (Perl 5079–5093).
            store_chromosome(&chromosome_name, &sequence, path, chromosomes, sq_order)?;
            sequence = Vec::new();
            // New chromosome name (Perl 5096–5100). `stripped` includes any
            // description; re-extract via the helper for the whitespace split.
            let header = format!(">{stripped}");
            chromosome_name = extract_chromosome_name(&header)?.to_string();
            if chromosome_name.is_empty() {
                return Err(empty_name_err());
            }
        } else {
            // Sequence line: upper-case + concatenate (Perl 5103).
            sequence.extend(line.bytes().map(|b| b.to_ascii_uppercase()));
        }
    }
    // Store the last chromosome of the file (Perl 5108–5123).
    store_chromosome(&chromosome_name, &sequence, path, chromosomes, sq_order)?;
    Ok(())
}

/// Store one chromosome, dying on a duplicate name and warning on empty
/// sequence (Perl 5080–5092 / 5109–5122).
fn store_chromosome(
    name: &str,
    sequence: &[u8],
    path: &Path,
    chromosomes: &mut HashMap<String, Vec<u8>>,
    sq_order: &mut Vec<String>,
) -> Result<()> {
    if chromosomes.contains_key(name) {
        return Err(AlignerError::Validation(format!(
            "Exiting because chromosome name already exists. Please make sure all chromosomes have a unique name! ({name})"
        )));
    }
    if sequence.is_empty() {
        eprintln!(
            "Chromosome {name} in the file {} did not contain any sequence information!",
            path.display()
        );
    }
    eprintln!("chr {name} ({} bp)", sequence.len());
    chromosomes.insert(name.to_string(), sequence.to_vec());
    sq_order.push(name.to_string());
    Ok(())
}

fn empty_name_err() -> AlignerError {
    AlignerError::Validation(
        "Chromosome names must not be empty! Please check that there are no spaces at the start of the FastA header(s) and try again".into(),
    )
}

/// `chomp` + `s/\r//`: drop a trailing `\n`, then remove the FIRST `\r` anywhere
/// (Perl 5065–5066 / 5075–5076 — `s/\r//` has no `/g`, so only one is removed).
/// Returns a `String` (FASTA is ASCII; lossy is inert for valid references).
fn chomp_cr(buf: &[u8]) -> String {
    let end = if buf.last() == Some(&b'\n') {
        buf.len() - 1
    } else {
        buf.len()
    };
    let line = &buf[..end];
    let s = String::from_utf8_lossy(line);
    match s.find('\r') {
        Some(i) => {
            let mut out = String::with_capacity(s.len() - 1);
            out.push_str(&s[..i]);
            out.push_str(&s[i + 1..]);
            out
        }
        None => s.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::File::create(&p)
            .unwrap()
            .write_all(content)
            .unwrap();
        p
    }

    #[test]
    fn extract_name_first_token() {
        assert_eq!(extract_chromosome_name(">chr1").unwrap(), "chr1");
        assert_eq!(extract_chromosome_name(">chr1 some desc").unwrap(), "chr1");
        // leading space → empty first field (caller dies)
        assert_eq!(extract_chromosome_name("> chr1").unwrap(), "");
        // no '>' → error
        assert!(extract_chromosome_name("chr1").is_err());
    }

    #[test]
    fn single_chromosome_uppercased() {
        let tmp = TempDir::new().unwrap();
        let f = write_file(tmp.path(), "g.fa", b">chr1\nacgtACGT\nNNnn\n");
        let g = read_genome_into_memory(&[f]).unwrap();
        assert_eq!(g.sq_order, vec!["chr1".to_string()]);
        assert_eq!(g.get("chr1").unwrap(), b"ACGTACGTNNNN");
    }

    #[test]
    fn multi_fasta_records_in_file_order() {
        let tmp = TempDir::new().unwrap();
        let f = write_file(tmp.path(), "g.fa", b">chrB\nAAAA\n>chrA\nCC\n>chrC\nG\n");
        let g = read_genome_into_memory(&[f]).unwrap();
        // record order within the file is preserved (NOT sorted)
        assert_eq!(g.sq_order, vec!["chrB", "chrA", "chrC"]);
        assert_eq!(g.get("chrA").unwrap(), b"CC");
    }

    #[test]
    fn multi_file_order_follows_input_list() {
        let tmp = TempDir::new().unwrap();
        let f1 = write_file(tmp.path(), "1.fa", b">chr1\nAA\n");
        let f2 = write_file(tmp.path(), "2.fa", b">chr2\nCC\n");
        // sq_order = input list order (Phase 1 already sorted it)
        let g = read_genome_into_memory(&[f1.clone(), f2.clone()]).unwrap();
        assert_eq!(g.sq_order, vec!["chr1", "chr2"]);
        // reversed input → reversed sq_order (proves we honour the list, not a re-sort)
        let g2 = read_genome_into_memory(&[f2, f1]).unwrap();
        assert_eq!(g2.sq_order, vec!["chr2", "chr1"]);
    }

    #[test]
    fn gzipped_fasta() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        let tmp = TempDir::new().unwrap();
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(b">chrG\nACGTacgt\n").unwrap();
        let gz = enc.finish().unwrap();
        let f = write_file(tmp.path(), "g.fa.gz", &gz);
        let g = read_genome_into_memory(&[f]).unwrap();
        assert_eq!(g.get("chrG").unwrap(), b"ACGTACGT");
    }

    #[test]
    fn duplicate_name_dies() {
        let tmp = TempDir::new().unwrap();
        let f = write_file(tmp.path(), "g.fa", b">chr1\nAA\n>chr1\nCC\n");
        assert!(read_genome_into_memory(&[f]).is_err());
    }

    #[test]
    fn duplicate_name_across_files_dies() {
        let tmp = TempDir::new().unwrap();
        let f1 = write_file(tmp.path(), "1.fa", b">chr1\nAA\n");
        let f2 = write_file(tmp.path(), "2.fa", b">chr1\nCC\n");
        assert!(read_genome_into_memory(&[f1, f2]).is_err());
    }

    #[test]
    fn leading_space_header_dies() {
        let tmp = TempDir::new().unwrap();
        let f = write_file(tmp.path(), "g.fa", b"> chr1\nAA\n");
        assert!(read_genome_into_memory(&[f]).is_err());
    }

    #[test]
    fn non_fasta_first_line_dies() {
        let tmp = TempDir::new().unwrap();
        let f = write_file(tmp.path(), "g.fa", b"ACGT\nACGT\n");
        assert!(read_genome_into_memory(&[f]).is_err());
    }

    #[test]
    fn crlf_stripped() {
        let tmp = TempDir::new().unwrap();
        let f = write_file(tmp.path(), "g.fa", b">chr1\r\nACGT\r\nTTTT\r\n");
        let g = read_genome_into_memory(&[f]).unwrap();
        assert_eq!(g.sq_order, vec!["chr1".to_string()]);
        assert_eq!(g.get("chr1").unwrap(), b"ACGTTTTT"); // no stray \r
    }

    #[test]
    fn empty_sequence_warns_not_dies() {
        // a header immediately followed by another header → empty sequence (warn, not die)
        let tmp = TempDir::new().unwrap();
        let f = write_file(tmp.path(), "g.fa", b">empty\n>chr2\nAC\n");
        let g = read_genome_into_memory(&[f]).unwrap();
        assert_eq!(g.sq_order, vec!["empty", "chr2"]);
        assert_eq!(g.get("empty").unwrap(), b"");
        assert_eq!(g.get("chr2").unwrap(), b"AC");
    }
}
