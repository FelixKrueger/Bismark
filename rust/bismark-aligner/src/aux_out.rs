//! `--unmapped` / `--ambiguous` FastQ output (Perl 1644–1709 + the record write
//! at 2452–2455 / 2461–2464).
//!
//! For the SE single-core spine these files are **gzipped** (`gzip -c`), so the
//! byte-identity gate compares the **decompressed** content (flate2 ≠ Perl gzip
//! bytes — the Phase-2 `--gzip` precedent). Two byte traps replicated here:
//! the filename uses the **un-stripped** read-file basename (NOT the BAM/report
//! stem), and the record's sequence is the **original, non-uppercased** read
//! with the `+` line passed through **verbatim**.

use std::io::Write;

use crate::error::Result;

/// Which auxiliary FastQ file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxKind {
    /// Reads with no alignment (`--unmapped`).
    Unmapped,
    /// Ambiguously-mapping reads (`--ambiguous`).
    Ambiguous,
}

impl AuxKind {
    fn stem(self) -> &'static str {
        match self {
            AuxKind::Unmapped => "unmapped_reads",
            AuxKind::Ambiguous => "ambiguous_reads",
        }
    }
}

/// Derive the gzipped `--unmapped`/`--ambiguous` filename (Perl 1644–1709).
///
/// `filename` is the read-file **basename**, used **un-stripped** (Perl
/// `$unmapped_file = $filename`; the FastQ suffix is NOT removed — unlike the
/// BAM/report stems). `--basename` overrides both prefix and filename. `fasta`
/// selects `.fa` vs `.fq` (FastA = Phase 9). The `.gz` is appended for the
/// SE single-core path.
pub fn aux_filename(
    filename: &str,
    prefix: Option<&str>,
    basename: Option<&str>,
    kind: AuxKind,
    fasta: bool,
    mate: Option<u8>,
) -> String {
    let ext = if fasta { "fa" } else { "fq" };
    // Paired-end inserts the mate number after the stem (Perl `_unmapped_reads_1.fq`).
    let stem = match mate {
        Some(m) => format!("{}_{m}", kind.stem()),
        None => kind.stem().to_string(),
    };
    let base = if let Some(b) = basename {
        // --basename overrides prefix + filename (Perl 1650 overwrites).
        format!("{b}_{stem}.{ext}")
    } else if let Some(p) = prefix {
        format!("{p}.{filename}_{stem}.{ext}")
    } else {
        format!("{filename}_{stem}.{ext}")
    };
    format!("{base}.gz")
}

/// Write one FastQ record to an aux file (Perl 2452–2455): `@<fixed_id>\n` +
/// `<seq>\n` + `<plus_line verbatim>` + `<qual>\n`.
///
/// - `fixed_id`: the `fix_id`'d, `@`-stripped identifier (a fresh `@` is prepended).
/// - `seq`: the original read, **chomped but NOT upper-cased**.
/// - `plus_line`: the FastQ 3rd line **verbatim, including its own terminator**
///   (no chomp, no appended `\n`).
/// - `qual`: the chomped quality line (an explicit `\n` is appended).
pub fn write_fastq_record<W: Write>(
    w: &mut W,
    fixed_id: &[u8],
    seq: &[u8],
    plus_line: &[u8],
    qual: &[u8],
) -> Result<()> {
    w.write_all(b"@")?;
    w.write_all(fixed_id)?;
    w.write_all(b"\n")?;
    w.write_all(seq)?;
    w.write_all(b"\n")?;
    w.write_all(plus_line)?; // verbatim — already carries its newline
    w.write_all(qual)?;
    w.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_unstripped_basename() {
        // The FastQ suffix is NOT stripped (contrast the BAM/report stems).
        assert_eq!(
            aux_filename("reads.fq.gz", None, None, AuxKind::Unmapped, false, None),
            "reads.fq.gz_unmapped_reads.fq.gz"
        );
        assert_eq!(
            aux_filename("reads.fq", None, None, AuxKind::Ambiguous, false, None),
            "reads.fq_ambiguous_reads.fq.gz"
        );
    }

    #[test]
    fn filename_prefix() {
        assert_eq!(
            aux_filename(
                "reads.fq",
                Some("expA"),
                None,
                AuxKind::Unmapped,
                false,
                None
            ),
            "expA.reads.fq_unmapped_reads.fq.gz"
        );
    }

    #[test]
    fn filename_basename_overrides_prefix() {
        assert_eq!(
            aux_filename(
                "reads.fq",
                Some("expA"),
                Some("sampleX"),
                AuxKind::Unmapped,
                false,
                None,
            ),
            "sampleX_unmapped_reads.fq.gz"
        );
    }

    #[test]
    fn filename_paired_mate_suffix() {
        // PE inserts the mate number after the stem, before the extension.
        assert_eq!(
            aux_filename("r_1.fq", None, None, AuxKind::Unmapped, false, Some(1)),
            "r_1.fq_unmapped_reads_1.fq.gz"
        );
        assert_eq!(
            aux_filename("r_2.fq", None, None, AuxKind::Ambiguous, false, Some(2)),
            "r_2.fq_ambiguous_reads_2.fq.gz"
        );
        // --basename + mate.
        assert_eq!(
            aux_filename(
                "r_1.fq",
                None,
                Some("samp"),
                AuxKind::Unmapped,
                false,
                Some(1)
            ),
            "samp_unmapped_reads_1.fq.gz"
        );
    }

    #[test]
    fn record_bytes_non_uc_seq_verbatim_plus() {
        // seq is NOT uppercased; the `+` line is verbatim (keeps its own \n).
        let mut v = Vec::new();
        write_fastq_record(&mut v, b"r1_1:N:0", b"acgtACGT", b"+\n", b"IIIIIIII").unwrap();
        assert_eq!(
            String::from_utf8(v).unwrap(),
            "@r1_1:N:0\nacgtACGT\n+\nIIIIIIII\n"
        );
    }

    #[test]
    fn record_bytes_crlf_plus_line_verbatim() {
        // A CRLF `+` line is passed through verbatim (retains \r\n); seq/qual were
        // chomped of \r upstream and get an explicit \n.
        let mut v = Vec::new();
        write_fastq_record(&mut v, b"r1", b"ACGT", b"+r1\r\n", b"IIII").unwrap();
        assert_eq!(String::from_utf8(v).unwrap(), "@r1\nACGT\n+r1\r\nIIII\n");
    }
}
