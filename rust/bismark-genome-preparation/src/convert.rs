//! The byte-identity core: bisulfite conversion of FASTA sequences.
//!
//! Faithful to Perl `process_sequence_files` (lines 360–516). The transform
//! operates on **raw line bytes including the terminator**, so CRLF stays CRLF
//! and a final line without a newline keeps none — never trim-and-re-emit.
//!
//! Per sequence byte: uppercase → map anything not in `{A,T,C,G,N,\r,\n}` to
//! `N` → then `tr` (`C→T`/`G→A`, or slam `T→C`/`A→G`). Headers are rewritten as
//! `>{name}_CT_converted\n` / `>{name}_GA_converted\n` (always LF), with the
//! name's fixed suffix even in slam mode.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use flate2::read::MultiGzDecoder;

use crate::discovery::extract_chromosome_name;
use crate::error::GenomePrepError;

/// Which converted strand a record is being written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// C→T (top strand). Slam: T→C.
    Ct,
    /// G→A (bottom strand). Slam: A→G.
    Ga,
}

/// Conversion counts (for the STDOUT totals line; not byte-gated).
#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    /// Number of CT-side transliterations (`C→T`, or slam `T→C`).
    pub ct: u64,
    /// Number of GA-side transliterations (`G→A`, or slam `A→G`).
    pub ga: u64,
}

/// Convert one raw line into `out` for the given `side`. Returns the number of
/// `tr` transliterations performed (for stats). Preserves the line terminator
/// (`\r`/`\n` are in the keep-set and untouched by `tr`).
pub fn map_into(raw: &[u8], side: Side, slam: bool, out: &mut Vec<u8>) -> u64 {
    out.clear();
    out.reserve(raw.len());
    let mut conv = 0u64;
    for &b in raw {
        let u = b.to_ascii_uppercase();
        // (I) anything not A/T/C/G/N (or a line ending) → N
        let n = match u {
            b'A' | b'T' | b'C' | b'G' | b'N' | b'\r' | b'\n' => u,
            _ => b'N',
        };
        // (II) tr — C→T / G→A (bisulfite) or T→C / A→G (slam)
        let mapped = match (side, slam) {
            (Side::Ct, false) => {
                if n == b'C' {
                    conv += 1;
                    b'T'
                } else {
                    n
                }
            }
            (Side::Ct, true) => {
                if n == b'T' {
                    conv += 1;
                    b'C'
                } else {
                    n
                }
            }
            (Side::Ga, false) => {
                if n == b'G' {
                    conv += 1;
                    b'A'
                } else {
                    n
                }
            }
            (Side::Ga, true) => {
                if n == b'A' {
                    conv += 1;
                    b'G'
                } else {
                    n
                }
            }
        };
        out.push(mapped);
    }
    conv
}

/// Write the rewritten converted header `>{name}_{CT|GA}_converted\n` into
/// `out`. The `_CT_`/`_GA_` suffix is fixed **even in slam mode** (Perl never
/// changed it — the `### TODO: Change this for GrandSlam` that was never acted
/// on).
fn header_line(name: &[u8], side: Side, out: &mut Vec<u8>) {
    out.clear();
    out.push(b'>');
    out.extend_from_slice(name);
    out.extend_from_slice(match side {
        Side::Ct => b"_CT_converted\n",
        Side::Ga => b"_GA_converted\n",
    });
}

/// Open a FASTA file for reading, transparently decompressing `.gz`
/// (multi-member safe). Returns a buffered byte reader.
fn open_fasta(path: &Path) -> Result<Box<dyn BufRead>, GenomePrepError> {
    let f = File::open(path)?;
    let is_gz = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".gz"))
        .unwrap_or(false);
    if is_gz {
        Ok(Box::new(BufReader::new(MultiGzDecoder::new(f))))
    } else {
        Ok(Box::new(BufReader::new(f)))
    }
}

/// Build the per-chromosome output path `<dir>/<name>.{CT|GA}_conversion.fa`
/// from the raw name bytes (byte-faithful — names need not be valid UTF-8).
#[cfg(unix)]
fn per_chr_path(dir: &Path, name: &[u8], side: Side) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    let suffix: &[u8] = match side {
        Side::Ct => b".CT_conversion.fa",
        Side::Ga => b".GA_conversion.fa",
    };
    let mut fname = name.to_vec();
    fname.extend_from_slice(suffix);
    dir.join(std::ffi::OsStr::from_bytes(&fname))
}

#[cfg(not(unix))]
fn per_chr_path(dir: &Path, name: &[u8], side: Side) -> PathBuf {
    let suffix = match side {
        Side::Ct => ".CT_conversion.fa",
        Side::Ga => ".GA_conversion.fa",
    };
    dir.join(format!("{}{}", String::from_utf8_lossy(name), suffix))
}

/// Process a header line: extract + uniqueness-check the chromosome name,
/// (re)open the per-chromosome writers in `--single_fasta` mode, and write the
/// two converted header lines.
#[allow(clippy::too_many_arguments)]
fn handle_header(
    line: &[u8],
    file: &Path,
    seen: &mut HashSet<Vec<u8>>,
    single_fasta: bool,
    ct_dir: &Path,
    ga_dir: &Path,
    ct_w: &mut Box<dyn Write>,
    ga_w: &mut Box<dyn Write>,
    hbuf: &mut Vec<u8>,
) -> Result<(), GenomePrepError> {
    let name = extract_chromosome_name(line, file)?.to_vec();
    if !seen.insert(name.clone()) {
        return Err(GenomePrepError::DuplicateChromosome(
            String::from_utf8_lossy(&name).into_owned(),
        ));
    }
    if single_fasta {
        // Flush + replace the per-chromosome writers (drop flushes too, but do
        // it explicitly to surface any error rather than swallow it on Drop).
        ct_w.flush()?;
        ga_w.flush()?;
        *ct_w = Box::new(BufWriter::new(File::create(per_chr_path(
            ct_dir,
            &name,
            Side::Ct,
        ))?));
        *ga_w = Box::new(BufWriter::new(File::create(per_chr_path(
            ga_dir,
            &name,
            Side::Ga,
        ))?));
    }
    header_line(&name, Side::Ct, hbuf);
    ct_w.write_all(hbuf)?;
    header_line(&name, Side::Ga, hbuf);
    ga_w.write_all(hbuf)?;
    Ok(())
}

/// **Step II — the standard conversion.** Stream each FASTA file once, writing
/// the C→T-converted copy to the CT output and the G→A-converted copy to the GA
/// output. MFA mode writes the two combined `genome_mfa.*` files;
/// `--single_fasta` writes per-chromosome files. Chromosome names must be
/// unique across all inputs.
pub fn convert_split(
    files: &[PathBuf],
    ct_dir: &Path,
    ga_dir: &Path,
    single_fasta: bool,
    slam: bool,
) -> Result<Counts, GenomePrepError> {
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut counts = Counts::default();
    let mut ctbuf = Vec::new();
    let mut gabuf = Vec::new();
    let mut hbuf = Vec::new();

    // MFA writers are created once and reused; in single_fasta mode they start
    // as sinks and are replaced at each header by `handle_header`.
    let (mut ct_w, mut ga_w): (Box<dyn Write>, Box<dyn Write>) = if single_fasta {
        (Box::new(std::io::sink()), Box::new(std::io::sink()))
    } else {
        (
            Box::new(BufWriter::new(File::create(
                ct_dir.join("genome_mfa.CT_conversion.fa"),
            )?)),
            Box::new(BufWriter::new(File::create(
                ga_dir.join("genome_mfa.GA_conversion.fa"),
            )?)),
        )
    };

    for file in files {
        let mut reader = open_fasta(file)?;
        let mut line: Vec<u8> = Vec::new();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            // Empty file: Perl's first `<IN>` is undef → dies "not in FASTA".
            return Err(GenomePrepError::NotFasta(file.clone()));
        }
        handle_header(
            &line,
            file,
            &mut seen,
            single_fasta,
            ct_dir,
            ga_dir,
            &mut ct_w,
            &mut ga_w,
            &mut hbuf,
        )?;
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            if line.first() == Some(&b'>') {
                handle_header(
                    &line,
                    file,
                    &mut seen,
                    single_fasta,
                    ct_dir,
                    ga_dir,
                    &mut ct_w,
                    &mut ga_w,
                    &mut hbuf,
                )?;
            } else {
                counts.ct += map_into(&line, Side::Ct, slam, &mut ctbuf);
                ct_w.write_all(&ctbuf)?;
                counts.ga += map_into(&line, Side::Ga, slam, &mut gabuf);
                ga_w.write_all(&gabuf)?;
            }
        }
    }
    ct_w.flush()?;
    ga_w.flush()?;
    Ok(counts)
}

/// **Combined reference (Bismark-Rust extension).** Write a single FASTA = the
/// CT-converted records of all files (glob order), then the GA-converted
/// records of all files. Built directly from the converted stream, so it is
/// well-defined in both MFA and `--single_fasta` modes. No uniqueness re-check
/// (already validated by [`convert_split`]).
pub fn write_combined(
    files: &[PathBuf],
    combined_path: &Path,
    slam: bool,
) -> Result<(), GenomePrepError> {
    let mut w = BufWriter::new(File::create(combined_path)?);
    let mut buf = Vec::new();
    for side in [Side::Ct, Side::Ga] {
        for file in files {
            let mut reader = open_fasta(file)?;
            let mut line: Vec<u8> = Vec::new();
            let n = reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                return Err(GenomePrepError::NotFasta(file.clone()));
            }
            let name = extract_chromosome_name(&line, file)?.to_vec();
            header_line(&name, side, &mut buf);
            w.write_all(&buf)?;
            loop {
                line.clear();
                let n = reader.read_until(b'\n', &mut line)?;
                if n == 0 {
                    break;
                }
                if line.first() == Some(&b'>') {
                    let name = extract_chromosome_name(&line, file)?.to_vec();
                    header_line(&name, side, &mut buf);
                    w.write_all(&buf)?;
                } else {
                    map_into(&line, side, slam, &mut buf);
                    w.write_all(&buf)?;
                }
            }
        }
    }
    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn ct(raw: &[u8], slam: bool) -> Vec<u8> {
        let mut o = Vec::new();
        map_into(raw, Side::Ct, slam, &mut o);
        o
    }
    fn ga(raw: &[u8], slam: bool) -> Vec<u8> {
        let mut o = Vec::new();
        map_into(raw, Side::Ga, slam, &mut o);
        o
    }

    #[test]
    fn uppercases_and_converts() {
        // lowercase → upper, then C→T (CT side): "acgt" → "ACGT" → "ATGT".
        assert_eq!(ct(b"acgt\n", false), b"ATGT\n");
        assert_eq!(ga(b"acgt\n", false), b"ACAT\n");
    }

    #[test]
    fn ct_ga_basic() {
        assert_eq!(ct(b"ACGT\n", false), b"ATGT\n");
        assert_eq!(ga(b"ACGT\n", false), b"ACAT\n");
    }

    #[test]
    fn ambiguity_and_non_ascii_to_n() {
        // IUPAC ambiguity codes → N (after uppercasing); high byte → N.
        assert_eq!(ct(b"RYSWKMBDHV\n", false), b"NNNNNNNNNN\n");
        assert_eq!(ct(&[0xC3, 0x28, b'A', b'\n'], false), b"NNA\n");
    }

    #[test]
    fn crlf_preserved() {
        // \r is in the keep-set → CRLF stays CRLF.
        assert_eq!(ct(b"ACGT\r\n", false), b"ATGT\r\n");
        assert_eq!(ga(b"ACGT\r\n", false), b"ACAT\r\n");
    }

    #[test]
    fn final_line_without_newline_preserved() {
        assert_eq!(ct(b"ACGT", false), b"ATGT");
        assert_eq!(ct(b"", false), b"");
    }

    #[test]
    fn interior_whitespace_becomes_n() {
        // A stray space/tab is not in the keep-set → N (then tr applies).
        assert_eq!(ct(b"AC GT\n", false), b"ATNGT\n");
        assert_eq!(ct(b"AC\tGT\n", false), b"ATNGT\n");
    }

    #[test]
    fn empty_seq_line_passthrough() {
        assert_eq!(ct(b"\n", false), b"\n");
    }

    #[test]
    fn slam_direction() {
        // slam: CT file does T→C, GA file does A→G.
        assert_eq!(ct(b"ACGT\n", true), b"ACGC\n");
        assert_eq!(ga(b"ACGT\n", true), b"GCGT\n");
    }

    #[test]
    fn header_line_fixed_suffix() {
        let mut o = Vec::new();
        header_line(b"chr1", Side::Ct, &mut o);
        assert_eq!(o, b">chr1_CT_converted\n");
        header_line(b"chr1", Side::Ga, &mut o);
        assert_eq!(o, b">chr1_GA_converted\n");
        // empty name (bare `>` / leading whitespace) still produces a header.
        header_line(b"", Side::Ct, &mut o);
        assert_eq!(o, b">_CT_converted\n");
    }

    #[test]
    fn convert_split_mfa_byte_exact() {
        let d = tempdir().unwrap();
        let gdir = d.path().join("genome");
        let ct_dir = d.path().join("CT");
        let ga_dir = d.path().join("GA");
        for p in [&gdir, &ct_dir, &ga_dir] {
            fs::create_dir_all(p).unwrap();
        }
        // Two records, second has lowercase + an ambiguity code + no trailing \n.
        fs::write(gdir.join("g.fa"), b">chr1 desc\nACGTN\nacgt\n>chr2\nGGCCR").unwrap();
        let files = vec![gdir.join("g.fa")];
        convert_split(&files, &ct_dir, &ga_dir, false, false).unwrap();

        let ct = fs::read(ct_dir.join("genome_mfa.CT_conversion.fa")).unwrap();
        assert_eq!(
            ct,
            b">chr1_CT_converted\nATGTN\nATGT\n>chr2_CT_converted\nGGTTN".to_vec()
        );
        let ga = fs::read(ga_dir.join("genome_mfa.GA_conversion.fa")).unwrap();
        assert_eq!(
            ga,
            b">chr1_GA_converted\nACATN\nACAT\n>chr2_GA_converted\nAACCN".to_vec()
        );
    }

    #[test]
    fn convert_split_single_fasta_byte_exact() {
        let d = tempdir().unwrap();
        let gdir = d.path().join("genome");
        let ct_dir = d.path().join("CT");
        let ga_dir = d.path().join("GA");
        for p in [&gdir, &ct_dir, &ga_dir] {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(gdir.join("g.fa"), b">chr1\nACGT\n>chr2\nTTAA\n").unwrap();
        let files = vec![gdir.join("g.fa")];
        convert_split(&files, &ct_dir, &ga_dir, true, false).unwrap();
        assert_eq!(
            fs::read(ct_dir.join("chr1.CT_conversion.fa")).unwrap(),
            b">chr1_CT_converted\nATGT\n".to_vec()
        );
        assert_eq!(
            fs::read(ct_dir.join("chr2.CT_conversion.fa")).unwrap(),
            b">chr2_CT_converted\nTTAA\n".to_vec()
        );
        assert_eq!(
            fs::read(ga_dir.join("chr1.GA_conversion.fa")).unwrap(),
            b">chr1_GA_converted\nACAT\n".to_vec()
        );
    }

    #[test]
    fn combined_equals_ct_concat_ga_in_mfa_mode() {
        let d = tempdir().unwrap();
        let gdir = d.path().join("genome");
        let ct_dir = d.path().join("CT");
        let ga_dir = d.path().join("GA");
        for p in [&gdir, &ct_dir, &ga_dir] {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(gdir.join("a.fa"), b">chr1\nACGT\n").unwrap();
        fs::write(gdir.join("b.fa"), b">chr2\nGGCC\n").unwrap();
        let files = vec![gdir.join("a.fa"), gdir.join("b.fa")];
        convert_split(&files, &ct_dir, &ga_dir, false, false).unwrap();
        let combined_path = d.path().join("combined.fa");
        write_combined(&files, &combined_path, false).unwrap();

        let mut expected = fs::read(ct_dir.join("genome_mfa.CT_conversion.fa")).unwrap();
        expected.extend(fs::read(ga_dir.join("genome_mfa.GA_conversion.fa")).unwrap());
        assert_eq!(fs::read(&combined_path).unwrap(), expected);
    }

    #[test]
    fn duplicate_chromosome_name_errors() {
        let d = tempdir().unwrap();
        let gdir = d.path().join("genome");
        let ct_dir = d.path().join("CT");
        let ga_dir = d.path().join("GA");
        for p in [&gdir, &ct_dir, &ga_dir] {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(gdir.join("g.fa"), b">chr1\nACGT\n>chr1\nTTAA\n").unwrap();
        let files = vec![gdir.join("g.fa")];
        let r = convert_split(&files, &ct_dir, &ga_dir, false, false);
        assert!(matches!(r, Err(GenomePrepError::DuplicateChromosome(_))));
    }

    #[test]
    fn empty_file_errors() {
        let d = tempdir().unwrap();
        let gdir = d.path().join("genome");
        let ct_dir = d.path().join("CT");
        let ga_dir = d.path().join("GA");
        for p in [&gdir, &ct_dir, &ga_dir] {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(gdir.join("empty.fa"), b"").unwrap();
        let files = vec![gdir.join("empty.fa")];
        let r = convert_split(&files, &ct_dir, &ga_dir, false, false);
        assert!(matches!(r, Err(GenomePrepError::NotFasta(_))));
    }
}
