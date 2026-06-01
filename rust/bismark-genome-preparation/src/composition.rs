//! `--genomic_composition`: the mono- + di-nucleotide frequency table
//! `<genome>/genomic_nucleotide_frequencies.txt`, **byte-identical** to Perl
//! `bismark_genome_preparation`'s `get_genomic_frequencies` / `process_sequence`
//! / `read_genome_into_memory` (lines 518–570, 665–751).
//!
//! **This is NOT the conversion path** (load-bearing). `read_genome_into_memory`
//! `uc`s the sequence but does **NOT** apply the conversion's `[^ATCGN]→N`
//! substitution. So the counter sees the *raw uppercased bytes*:
//! - **Mono:** every byte is counted **unless it is `N`** — IUPAC ambiguity
//!   codes (`R`,`Y`,`S`,`W`,`K`,`M`,`B`,`D`,`H`,`V`) and even stray bytes (a
//!   space) become their own keys; only a literal `N` is skipped.
//! - **Di:** each adjacent pair within a chromosome is counted **unless either
//!   base is `N`** (Perl `index($di,'N') < 0`). Di-mers span line boundaries but
//!   **NOT** chromosome/file boundaries.
//!
//! Perl runs this pass **before** the bisulfite conversion, and it `die`s in
//! `read_genome_into_memory` *before* `get_genomic_frequencies` writes the
//! table. So a duplicate-chromosome-name or not-FASTA error here must fire
//! **before any table is written** — never leave an orphan file Perl wouldn't.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::convert::open_fasta;
use crate::discovery::extract_chromosome_name;
use crate::error::GenomePrepError;
use crate::logging::Logger;

/// The legacy TopHat mouse-genome filename Perl explicitly skips in the
/// frequency pass (`next if ... eq 'Mus_musculus.NCBIM37.fa'`, line 694) —
/// excluded from **counting**, but NOT from the conversion. Matched on raw
/// `file_name()` bytes.
const SKIP_FILENAME: &[u8] = b"Mus_musculus.NCBIM37.fa";

/// The frequency table's fixed basename, written into the **genome folder**
/// (not `Bisulfite_Genome/`).
const FREQ_FILENAME: &str = "genomic_nucleotide_frequencies.txt";

/// Count mono- and di-nucleotides across all `files` and write the frequency
/// table to `<genome_folder>/genomic_nucleotide_frequencies.txt`.
///
/// Mirrors Perl's order (`get_genomic_frequencies` → `process_sequence_files`):
/// run **before** the conversion. A `DuplicateChromosome` / `NotFasta` error
/// raised while counting propagates **before** [`write_table`] is reached, so no
/// table is written — matching Perl's `die` in `read_genome_into_memory`.
pub fn write_genomic_composition(
    files: &[PathBuf],
    genome_folder: &Path,
    logger: &Logger,
) -> Result<(), GenomePrepError> {
    // Array-indexed counters: NO per-base allocation on a multi-Gbp genome.
    // `di` is flat-indexed `prev*256 + cur`. Emitting them in byte order (see
    // `write_table`) reproduces Perl's `sort keys %freqs` exactly.
    let mut mono = [0u64; 256];
    let mut di = vec![0u64; 256 * 256];
    // Cross-file uniqueness check (Perl's global `%chromosomes`).
    let mut seen: HashSet<Vec<u8>> = HashSet::new();

    for file in files {
        // Perl: `next if ($chromosome_filename eq 'Mus_musculus.NCBIM37.fa')`.
        if file.file_name().map(|n| n.as_encoded_bytes()) == Some(SKIP_FILENAME) {
            continue;
        }
        count_file(file, &mut seen, &mut mono, &mut di)?;
    }

    // Only now — after every file counted without error — write the table.
    write_table(genome_folder, &mono, &di, logger);
    Ok(())
}

/// Stream one FASTA file, accumulating mono/di counts. The first line is the
/// header **unconditionally** (Perl reads `<CHR_IN>` and feeds it straight to
/// `extract_chromosome_name`, which `die`s if it isn't a `>` header — so an
/// empty file or a non-`>` first line is [`GenomePrepError::NotFasta`]); the
/// header is never counted as sequence. A repeated chromosome name is
/// [`GenomePrepError::DuplicateChromosome`].
fn count_file(
    file: &Path,
    seen: &mut HashSet<Vec<u8>>,
    mono: &mut [u64; 256],
    di: &mut [u64],
) -> Result<(), GenomePrepError> {
    let mut reader = open_fasta(file)?;
    let mut line: Vec<u8> = Vec::new();

    // First line = header. Empty file → Perl's `<CHR_IN>` is undef →
    // `extract_chromosome_name` die → NotFasta (same as `convert_split`).
    let n = reader.read_until(b'\n', &mut line)?;
    if n == 0 {
        return Err(GenomePrepError::NotFasta(file.to_path_buf()));
    }
    check_header(&line, file, seen)?;

    // Di never spans chromosomes or files: `prev` is per-file and reset at each
    // in-file header.
    let mut prev: Option<u8> = None;
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        if line.first() == Some(&b'>') {
            check_header(&line, file, seen)?;
            prev = None;
            continue;
        }
        count_sequence_line(&line, &mut prev, mono, di);
    }
    Ok(())
}

/// Extract + uniqueness-check a chromosome name (errors **before** any table is
/// written). Inserting at header-read time (rather than at Perl's
/// store-on-next-header time) detects exactly the same set of duplicates —
/// every name that appears ≥2 times — and the same as [`crate::convert`].
fn check_header(
    line: &[u8],
    file: &Path,
    seen: &mut HashSet<Vec<u8>>,
) -> Result<(), GenomePrepError> {
    let name = extract_chromosome_name(line, file)?;
    if !seen.insert(name.to_vec()) {
        return Err(GenomePrepError::DuplicateChromosome(
            String::from_utf8_lossy(name).into_owned(),
        ));
    }
    Ok(())
}

/// Count one sequence line. Perl does `chomp` (strip a single trailing `\n`)
/// then `s/\r//` (remove the **first** `\r` *anywhere*, not all) then `uc`,
/// appending to the chromosome sequence. Removing only the first `\r` makes the
/// byte before it and the byte after it adjacent, so the di-carry must continue
/// across the removed byte — handled by counting the two segments in order with
/// the same `prev`.
fn count_sequence_line(line: &[u8], prev: &mut Option<u8>, mono: &mut [u64; 256], di: &mut [u64]) {
    // chomp: drop a single trailing '\n' if present (no-op on a final line that
    // ends without one).
    let body = match line.last() {
        Some(&b'\n') => &line[..line.len() - 1],
        _ => line,
    };
    // s/\r//: remove the FIRST '\r' only. Common (no-\r) case allocates nothing.
    match body.iter().position(|&b| b == b'\r') {
        None => count_bytes(body, prev, mono, di),
        Some(i) => {
            count_bytes(&body[..i], prev, mono, di);
            count_bytes(&body[i + 1..], prev, mono, di);
        }
    }
}

/// Tally `bytes` into the counters, uppercasing each byte and carrying `prev`
/// for the di-mer. `prev` advances for **every** byte (including `N`), because a
/// di-mer at index `i` uses `seq[i]` and `seq[i+1]`: an `N` is skipped as a
/// counted base but still separates its neighbours.
fn count_bytes(bytes: &[u8], prev: &mut Option<u8>, mono: &mut [u64; 256], di: &mut [u64]) {
    for &b in bytes {
        // `to_ascii_uppercase` matches Perl's default `uc` on a byte string
        // (no `use feature 'unicode_strings'`/locale → only ASCII a–z fold;
        // bytes ≥ 0x80 are left unchanged by both).
        let u = b.to_ascii_uppercase();
        if u != b'N' {
            mono[u as usize] += 1;
        }
        if let Some(p) = *prev
            && p != b'N'
            && u != b'N'
        {
            di[p as usize * 256 + u as usize] += 1;
        }
        *prev = Some(u);
    }
}

/// Write the frequency table in Perl's `sort keys %freqs` order. **Non-fatal:**
/// on any open/write/flush error, `warn` and skip the table (Perl's
/// warn-and-continue), never propagating an error from this step.
fn write_table(genome_folder: &Path, mono: &[u64; 256], di: &[u64], logger: &Logger) {
    let path = genome_folder.join(FREQ_FILENAME);
    let file = match File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            logger.note(&format!(
                "Failed to write out file {} because of: {e}. \
                 Skipping writing out genomic frequency table",
                path.display()
            ));
            return;
        }
    };
    let mut w = BufWriter::new(file);
    if let Err(e) = write_counts(&mut w, mono, di).and_then(|()| w.flush()) {
        logger.note(&format!(
            "Failed to write out file {} because of: {e}. \
             Skipping writing out genomic frequency table",
            path.display()
        ));
    }
}

/// Emit `"<key>\t<count>\n"` for every non-zero counter in **byte-lexical key
/// order**. Iterating each leading byte `b` ascending — its 1-byte mono key
/// first, then its 2-byte di keys `[b, c]` for `c` ascending — reproduces Perl's
/// global `sort` of the mixed mono+di string keys exactly: a 1-byte key is a
/// prefix of (so sorts before) its 2-byte extensions, and all keys with a
/// smaller leading byte sort first. **Plain byte order — NOT the case-folding
/// `fasta_name_cmp` used for the glob.**
fn write_counts(w: &mut impl Write, mono: &[u64; 256], di: &[u64]) -> std::io::Result<()> {
    let mut buf = Vec::new();
    for (b, &mono_count) in mono.iter().enumerate() {
        if mono_count > 0 {
            emit(&mut buf, &[b as u8], mono_count);
        }
        let base = b * 256;
        for (c, &di_count) in di[base..base + 256].iter().enumerate() {
            if di_count > 0 {
                emit(&mut buf, &[b as u8, c as u8], di_count);
            }
        }
    }
    w.write_all(&buf)
}

/// Append one `"<key>\t<count>\n"` record (key bytes, TAB, decimal count, LF).
/// `count.to_string()` is Perl's default integer stringification.
fn emit(buf: &mut Vec<u8>, key: &[u8], count: u64) {
    buf.extend_from_slice(key);
    buf.push(b'\t');
    buf.extend_from_slice(count.to_string().as_bytes());
    buf.push(b'\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Write `files` into a fresh genome dir, run the composition pass, and
    /// return the produced table bytes (or `None` if no file was written).
    fn run(files: &[(&str, &[u8])]) -> Option<Vec<u8>> {
        let d = tempdir().unwrap();
        let gdir = d.path().join("genome");
        fs::create_dir_all(&gdir).unwrap();
        let mut paths = Vec::new();
        for (name, content) in files {
            let p = gdir.join(name);
            fs::write(&p, content).unwrap();
            paths.push(p);
        }
        let logger = Logger::new(false);
        write_genomic_composition(&paths, &gdir, &logger).unwrap();
        let out = gdir.join(FREQ_FILENAME);
        if out.exists() {
            Some(fs::read(out).unwrap())
        } else {
            None
        }
    }

    /// Run and assert the table file was created (panics on error / no file).
    fn table(files: &[(&str, &[u8])]) -> Vec<u8> {
        run(files).expect("table file should be written")
    }

    /// Run expecting an error; return `(error, table-file-exists?)`.
    fn run_err(files: &[(&str, &[u8])]) -> (GenomePrepError, bool) {
        let d = tempdir().unwrap();
        let gdir = d.path().join("genome");
        fs::create_dir_all(&gdir).unwrap();
        let mut paths = Vec::new();
        for (name, content) in files {
            let p = gdir.join(name);
            fs::write(&p, content).unwrap();
            paths.push(p);
        }
        let logger = Logger::new(false);
        let err = write_genomic_composition(&paths, &gdir, &logger).unwrap_err();
        let exists = gdir.join(FREQ_FILENAME).exists();
        (err, exists)
    }

    #[test]
    fn acgt_only_mono_and_di() {
        // "ACGT": mono A,C,G,T=1; di AC,CG,GT=1. Sorted byte-lexically with each
        // mono immediately before its di block.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nACGT\n")]),
            b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn lowercase_is_uppercased() {
        // `uc` → identical to the uppercase input.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nacgt\n")]),
            table(&[("chr1.fa", b">chr1\nACGT\n")])
        );
    }

    #[test]
    fn n_skipped_mono_and_di() {
        // "ACNGT": mono skips N; di CN and NG (contain N) skipped; AC, GT kept.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nACNGT\n")]),
            b"A\t1\nAC\t1\nC\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn ambiguity_code_counted_not_mapped_to_n() {
        // "ARC": R counted as its own mono key + in di (unlike the conversion
        // path, which would map R→N).
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nARC\n")]),
            b"A\t1\nAR\t1\nC\t1\nR\t1\nRC\t1\n".to_vec()
        );
    }

    #[test]
    fn di_spans_line_boundary_within_chromosome() {
        // Two lines, one record → "ACGT": CG spans the line break.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nAC\nGT\n")]),
            b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn di_does_not_span_chromosomes() {
        // chr1="AC", chr2="GT" in ONE file → no CG (prev reset at >chr2).
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nAC\n>chr2\nGT\n")]),
            b"A\t1\nAC\t1\nC\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn di_does_not_span_files() {
        // Two files; prev reset per file → no CG. (a.fa < b.fa by glob order.)
        assert_eq!(
            table(&[("a.fa", b">chr1\nAC\n"), ("b.fa", b">chr2\nGT\n")]),
            b"A\t1\nAC\t1\nC\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn blank_line_preserves_di_carry() {
        // "AC" <blank> "GT" → sequence "ACGT"; CG spans the (empty) blank line.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nAC\n\nGT\n")]),
            b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn final_line_without_newline_counted() {
        // No trailing \n on the sequence line → still counted fully.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nACGT")]),
            b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn carriage_return_first_only_removed() {
        // "A\r\rC": chomp \n, s/\r// removes the FIRST \r → "A\rC". The SECOND
        // \r survives and is counted as its own byte (0x0D < 'A' < 'C').
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nA\r\rC\n")]),
            b"\r\t1\n\rC\t1\nA\t1\nA\r\t1\nC\t1\n".to_vec()
        );
    }

    #[test]
    fn crlf_line_terminator_stripped() {
        // "ACGT\r\n": chomp \n → "ACGT\r", s/\r// → "ACGT". Same as the LF case.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\r\nACGT\r\n")]),
            b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn stray_space_counted_as_own_key() {
        // A space inside the sequence is counted (no N-mapping in this path).
        // "A C": bytes A, ' ', C → mono A,' ',C; di "A " (A→space) and " C"
        // (space→C). Space (0x20) sorts before 'A' (0x41), so its keys lead.
        assert_eq!(
            table(&[("chr1.fa", b">chr1\nA C\n")]),
            b" \t1\n C\t1\nA\t1\nA \t1\nC\t1\n".to_vec()
        );
    }

    #[test]
    fn n_only_genome_is_zero_byte_file() {
        // All N → empty counters → an empty (0-byte) table file IS created.
        assert_eq!(table(&[("chr1.fa", b">chr1\nNNNN\n")]), Vec::<u8>::new());
    }

    #[test]
    fn header_only_record_is_zero_byte_file() {
        // No sequence at all → 0-byte file.
        assert_eq!(table(&[("chr1.fa", b">chr1\n")]), Vec::<u8>::new());
    }

    #[test]
    fn bare_gt_first_line_is_not_counted() {
        // Bare `>` → empty chromosome name (not an error); first line not counted.
        assert_eq!(
            table(&[("chr1.fa", b">\nACGT\n")]),
            b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn mus_musculus_file_excluded_from_counting() {
        // The legacy mouse file is skipped in the freq pass; only chr1 counts.
        assert_eq!(
            table(&[
                ("chr1.fa", b">chr1\nACGT\n"),
                ("Mus_musculus.NCBIM37.fa", b">mouse\nGGGGCCCC\n"),
            ]),
            b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
        );
    }

    #[test]
    fn first_line_not_header_errors_and_no_file() {
        let (err, exists) = run_err(&[("chr1.fa", b"ACGT\n")]);
        assert!(matches!(err, GenomePrepError::NotFasta(_)));
        assert!(!exists, "no table file when the first line isn't a header");
    }

    #[test]
    fn empty_file_errors_and_no_file() {
        let (err, exists) = run_err(&[("chr1.fa", b"")]);
        assert!(matches!(err, GenomePrepError::NotFasta(_)));
        assert!(!exists);
    }

    #[test]
    fn duplicate_chromosome_errors_and_no_orphan_file() {
        // Perl `die`s in read_genome_into_memory BEFORE writing the table.
        let (err, exists) = run_err(&[("chr1.fa", b">chr1\nAC\n>chr1\nGT\n")]);
        assert!(matches!(err, GenomePrepError::DuplicateChromosome(_)));
        assert!(!exists, "a dup name must leave NO orphan table file");
    }

    #[test]
    fn duplicate_across_files_errors() {
        let (err, _) = run_err(&[("a.fa", b">chr1\nAC\n"), ("b.fa", b">chr1\nGT\n")]);
        assert!(matches!(err, GenomePrepError::DuplicateChromosome(_)));
    }
}
