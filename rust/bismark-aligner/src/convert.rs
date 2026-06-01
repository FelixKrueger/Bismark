//! Bisulfite read conversion — the C→T-converted temp FastQ that Bowtie 2 reads.
//!
//! Mirrors Perl `biTransformFastQFiles` (5489–5651) + `fix_IDs` (6235–6246) for
//! the v1 spine (FastQ, single-end, directional). The output temp file must be
//! **byte-identical** to Perl's so Bowtie 2 receives identical input. The
//! *original* (unconverted) read is deliberately NOT retained here — it is
//! re-read in lockstep during the later methylation-call loop.
//!
//! Per record (Perl order, 5577–5634): `count++` → chomp+`fix_id`+re-append `\n`
//! on the ID → skip/upto → uppercase → max-length guard (mm2-only, inert here)
//! → tab-detect → record-1 FastQ sanity (bypassed when skipping) → `C→T` +
//! write (`id2`/`qual` verbatim).

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;

use crate::config::RunConfig;
use crate::error::{AlignerError, Result};

/// Options shaping the conversion (read from the [`RunConfig`] seam).
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    /// `--prefix` (prepended as `<prefix>.<basename>`).
    pub prefix: Option<String>,
    /// `--gzip` (gzip the temp file; only its *decompressed* content is gated).
    pub gzip: bool,
    /// `--skip` (skip first N; `0`/None = no skip, Perl falsy).
    pub skip: Option<u64>,
    /// `--upto` (stop after N; `0`/None = no limit, Perl falsy).
    pub upto: Option<u64>,
    /// `--icpc` (truncate IDs at first space/tab instead of underscoring).
    pub icpc: bool,
    /// minimap2-only max read length; inert for Bowtie 2.
    pub maximum_length_cutoff: Option<u32>,
}

impl ConvertOptions {
    /// Build from the resolved config: `gzip`/`prefix` come from `output`
    /// (single source of truth), the rest from `read_processing`.
    pub fn from_config(cfg: &RunConfig) -> Self {
        ConvertOptions {
            prefix: cfg.output.prefix.clone(),
            gzip: cfg.output.gzip,
            skip: cfg.read_processing.skip,
            upto: cfg.read_processing.upto,
            icpc: cfg.read_processing.icpc,
            maximum_length_cutoff: cfg.read_processing.maximum_length_cutoff,
        }
    }
}

/// The created C→T temp file.
#[derive(Debug, Clone)]
pub struct ConvertedReads {
    /// Relative file name (`<prefix.>?<basename>_C_to_T.fastq[.gz]`).
    pub name: String,
    /// Full path the file was written to (temp_dir + name).
    pub path: PathBuf,
    /// Number of records read (running count, incl. skipped — Perl `$count`).
    pub count: u64,
    /// Reads whose (post-`fix_id`) ID still contains a tab (Perl
    /// `$seqID_contains_tabs`, 5608). Surfaced for the Phase-6 report. NB: this
    /// is **effectively always 0** because `fix_id` removes tabs *before* the
    /// check — a faithful replica of Perl's likewise-dead detection.
    pub seqid_tab_count: u64,
}

/// `fix_IDs` (Perl 6235): default replaces every run of spaces/tabs with a
/// single `_`; `--icpc` truncates at the first space/tab. Byte-level so non-UTF-8
/// IDs are preserved; a `\r` (CRLF) is untouched (not space/tab).
pub fn fix_id(id: &[u8], icpc: bool) -> Vec<u8> {
    if icpc {
        match id.iter().position(|&b| b == b' ' || b == b'\t') {
            Some(pos) => id[..pos].to_vec(),
            None => id.to_vec(),
        }
    } else {
        let mut out = Vec::with_capacity(id.len());
        let mut in_ws = false;
        for &b in id {
            if b == b' ' || b == b'\t' {
                if !in_ws {
                    out.push(b'_');
                    in_ws = true;
                }
            } else {
                out.push(b);
                in_ws = false;
            }
        }
        out
    }
}

/// `uc` then `tr/C/T/` (Perl 5597 + 5625): ASCII-uppercase each byte, then map
/// `C`→`T`. Line endings (`\n`/`\r`) and non-bases are preserved. Net effect
/// incl. lowercase: `a→A, c→T, g→G, t→T, n→N, …`.
pub fn convert_seq_c_to_t(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .map(|&b| {
            let up = b.to_ascii_uppercase();
            if up == b'C' { b'T' } else { up }
        })
        .collect()
}

/// Strip a single trailing `\n` (Perl `chomp`, `$/ = "\n"`); a `\r` is kept.
fn chomp_newline(line: &[u8]) -> &[u8] {
    if line.last() == Some(&b'\n') {
        &line[..line.len() - 1]
    } else {
        line
    }
}

/// Normalize the temp dir to Perl's form (8211–31): empty → `""` (CWD-relative);
/// otherwise create it, make it absolute, and ensure a trailing separator, so
/// the raw concatenation `<temp_dir><name>` (Perl `${temp_dir}${name}`) is well
/// formed. Returns the prefix string to concatenate before the file name.
fn temp_dir_prefix(temp_dir: &Path) -> Result<String> {
    if temp_dir.as_os_str().is_empty() {
        return Ok(String::new());
    }
    std::fs::create_dir_all(temp_dir)?;
    let abs = std::fs::canonicalize(temp_dir)?;
    let mut s = abs.to_string_lossy().into_owned();
    if !s.ends_with(std::path::MAIN_SEPARATOR) {
        s.push(std::path::MAIN_SEPARATOR);
    }
    Ok(s)
}

/// Write the C→T-converted FastQ temp file for one single-end directional input.
pub fn bisulfite_convert_fastq_se(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
) -> Result<ConvertedReads> {
    // ---- output name + path (raw concat, Perl ${temp_dir}${name}) -----------
    let basename = input.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        AlignerError::Validation(format!("could not derive a file name from input {input:?}"))
    })?;
    let mut name = match &opts.prefix {
        Some(p) => format!("{p}.{basename}"),
        None => basename.to_string(),
    };
    name.push_str(if opts.gzip {
        "_C_to_T.fastq.gz"
    } else {
        "_C_to_T.fastq"
    });
    let full = format!("{}{name}", temp_dir_prefix(temp_dir)?);
    let full_path = PathBuf::from(&full);

    // ---- reader (gz or plain) ----------------------------------------------
    let file = File::open(input)?;
    let mut reader: Box<dyn BufRead> = if input.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };

    // ---- writer (gz or plain) ----------------------------------------------
    let out = File::create(&full_path)?;
    let mut writer: BufWriter<Box<dyn Write>> = BufWriter::new(if opts.gzip {
        Box::new(GzEncoder::new(out, Compression::default()))
    } else {
        Box::new(out)
    });

    let (mut id, mut seq, mut id2, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    let mut seqid_tab_count: u64 = 0;

    loop {
        id.clear();
        seq.clear();
        id2.clear();
        qual.clear();
        let n1 = reader.read_until(b'\n', &mut id)?;
        let n2 = reader.read_until(b'\n', &mut seq)?;
        let n3 = reader.read_until(b'\n', &mut id2)?;
        let n4 = reader.read_until(b'\n', &mut qual)?;
        // Perl `last unless ($id and $seq and $id2 and $qual)` — any missing
        // line ends the loop; a truncated final record is dropped.
        if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
            break;
        }
        count += 1;

        // ID: chomp (\n only) → fix_id → re-append \n.
        let mut fixed_id = fix_id(chomp_newline(&id), opts.icpc);
        fixed_id.push(b'\n');

        // skip/upto (Perl falsy-0 semantics). The record-1 sanity below sits
        // after this, so a non-zero --skip bypasses it (Perl quirk).
        if let Some(s) = opts.skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = opts.upto
            && u > 0
            && count > u
        {
            break;
        }

        // max-length guard (mm2-only, 5598–5604; length incl. line terminator,
        // case-independent so measured on the raw seq line). Inert on the v1
        // Bowtie 2 spine — resolve() rejects --mm2_maximum_length there.
        if let Some(cutoff) = opts.maximum_length_cutoff
            && seq.len() as u64 > cutoff as u64
        {
            continue;
        }

        // tab-in-ID detection (5607; byte-neutral counter — effectively never
        // fires, since fix_id removed tabs above, matching Perl's dead check).
        if fixed_id.contains(&b'\t') {
            seqid_tab_count += 1;
        }

        // record-1-only FastQ sanity (5612–16).
        if count == 1 && (!fixed_id.starts_with(b"@") || !id2.starts_with(b"+")) {
            return Err(AlignerError::Validation(format!(
                "Input file doesn't seem to be in FastQ format at sequence {count}"
            )));
        }

        // uc + C→T + write (id2/qual verbatim). `convert_seq_c_to_t` uppercases
        // internally (Perl `uc` then `tr/C/T/`), so we pass the raw seq line.
        writer.write_all(&fixed_id)?;
        writer.write_all(&convert_seq_c_to_t(&seq))?;
        writer.write_all(&id2)?;
        writer.write_all(&qual)?;
    }

    writer.flush()?;
    drop(writer);
    Ok(ConvertedReads {
        name,
        path: full_path,
        count,
        seqid_tab_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_id_default_underscores_whitespace_runs() {
        assert_eq!(fix_id(b"@R 1:N:0", false), b"@R_1:N:0");
        assert_eq!(fix_id(b"@R\t1", false), b"@R_1");
        assert_eq!(fix_id(b"@R  \t 1", false), b"@R_1"); // run collapses to one _
        assert_eq!(fix_id(b"@R1", false), b"@R1");
        assert_eq!(fix_id(b"@R\r", false), b"@R\r"); // CR kept
    }

    #[test]
    fn fix_id_icpc_truncates_at_first_whitespace() {
        assert_eq!(fix_id(b"@R 1:N:0", true), b"@R");
        assert_eq!(fix_id(b"@R\t1", true), b"@R");
        assert_eq!(fix_id(b"@R1", true), b"@R1");
    }

    #[test]
    fn convert_seq_uc_then_c_to_t() {
        // uc('ACGTacgtN') = 'ACGTACGTN', then C->T => 'ATGTATGTN'
        assert_eq!(convert_seq_c_to_t(b"ACGTacgtN\n"), b"ATGTATGTN\n");
        assert_eq!(convert_seq_c_to_t(b"ccCC\r\n"), b"TTTT\r\n"); // CR survives
    }

    #[test]
    fn chomp_strips_only_newline() {
        assert_eq!(chomp_newline(b"id\n"), b"id");
        assert_eq!(chomp_newline(b"id\r\n"), b"id\r");
        assert_eq!(chomp_newline(b"id"), b"id");
    }

    // ---- file-level conversion (calls bisulfite_convert_fastq_se) -----------

    use std::io::{Read, Write};
    use tempfile::TempDir;

    /// Golden: 2 records exercising a space-ID, a tab-ID, lowercase bases, and a
    /// non-bare `+`-line. Computed from the verified Perl transform (fix_IDs:
    /// ws→`_`; `uc` then `tr/C/T/`; id2/qual verbatim). The authoritative
    /// *Perl-generated* end-to-end check lives in the Phase-10 oxy gate.
    const GOLDEN_IN: &[u8] =
        b"@read1 1:N:0:ATCG\nACGTacgtNN\n+\nIIIIIIIIII\n@read2\tlane2\nccCCggTT\n+read2\nJJJJJJJJ\n";
    const GOLDEN_OUT: &[u8] =
        b"@read1_1:N:0:ATCG\nATGTATGTNN\n+\nIIIIIIIIII\n@read2_lane2\nTTTTGGTT\n+read2\nJJJJJJJJ\n";

    fn opts(gzip: bool, skip: Option<u64>, upto: Option<u64>, icpc: bool) -> ConvertOptions {
        ConvertOptions {
            prefix: None,
            gzip,
            skip,
            upto,
            icpc,
            maximum_length_cutoff: None,
        }
    }

    /// Write `input` to a temp file named `name`, convert, return (ConvertedReads, output bytes).
    fn run(input: &[u8], name: &str, o: &ConvertOptions) -> (ConvertedReads, Vec<u8>) {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join(name);
        std::fs::write(&inp, input).unwrap();
        let td = tmp.path().join("t");
        let cr = bisulfite_convert_fastq_se(&inp, &td, o).unwrap();
        let out = std::fs::read(&cr.path).unwrap();
        (cr, out)
    }

    fn gzip_bytes(data: &[u8]) -> Vec<u8> {
        let mut e = GzEncoder::new(Vec::new(), Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }
    fn gunzip_bytes(data: &[u8]) -> Vec<u8> {
        let mut d = MultiGzDecoder::new(data);
        let mut out = Vec::new();
        d.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn golden_plain_fastq() {
        let (cr, out) = run(GOLDEN_IN, "reads.fq", &opts(false, None, None, false));
        assert_eq!(out, GOLDEN_OUT);
        assert_eq!(cr.name, "reads.fq_C_to_T.fastq"); // extensions kept + suffix
        assert_eq!(cr.count, 2);
    }

    #[test]
    fn gzip_input_matches_plain() {
        let (cr, out) = run(
            &gzip_bytes(GOLDEN_IN),
            "reads.fq.gz",
            &opts(false, None, None, false),
        );
        assert_eq!(out, GOLDEN_OUT);
        assert_eq!(cr.name, "reads.fq.gz_C_to_T.fastq");
    }

    #[test]
    fn multi_member_gzip_input() {
        // two concatenated gzip members — only MultiGzDecoder reads both.
        let half = GOLDEN_IN.len() / 2; // split on a record boundary (4 lines each)
        let boundary = GOLDEN_IN[..half].iter().rposition(|&b| b == b'\n').unwrap() + 1;
        let mut concat = gzip_bytes(&GOLDEN_IN[..boundary]);
        concat.extend(gzip_bytes(&GOLDEN_IN[boundary..]));
        let (_, out) = run(&concat, "reads.fq.gz", &opts(false, None, None, false));
        assert_eq!(out, GOLDEN_OUT);
    }

    #[test]
    fn gzip_output_decompresses_to_plain() {
        let (cr, raw) = run(GOLDEN_IN, "reads.fq", &opts(true, None, None, false));
        assert_eq!(cr.name, "reads.fq_C_to_T.fastq.gz");
        assert_eq!(gunzip_bytes(&raw), GOLDEN_OUT); // raw .gz bytes NOT gated; content is
    }

    #[test]
    fn skip_and_upto_select_records() {
        let input =
            b"@r1\nAA\n+\nII\n@r2\nAA\n+\nII\n@r3\nAA\n+\nII\n@r4\nAA\n+\nII\n@r5\nAA\n+\nII\n";
        let (cr, out) = run(input, "r.fq", &opts(false, Some(2), Some(4), false));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("@r3") && s.contains("@r4"));
        assert!(!s.contains("@r1") && !s.contains("@r2") && !s.contains("@r5"));
        // count runs over the UNSKIPPED numbering and stops once count > upto:
        // r5 increments count to 5, then the upto check breaks (Perl `last`).
        assert_eq!(cr.count, 5);
    }

    #[test]
    fn falsy_zero_disables_skip_and_upto() {
        let (_, out) = run(GOLDEN_IN, "r.fq", &opts(false, Some(0), Some(0), false));
        assert_eq!(out, GOLDEN_OUT); // Some(0) is Perl-falsy → no skip / no limit
    }

    #[test]
    fn skip_bypasses_record1_sanity() {
        // record 1 is malformed (id not '@'); --skip 1 must skip it → no error.
        let input = b"BAD\nACGT\n+\nIIII\n@r2\nACGT\n+\nIIII\n";
        let (_, out) = run(input, "r.fq", &opts(false, Some(1), None, false));
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("@r2\n"));
    }

    #[test]
    fn record1_malformed_errors_but_record_n_passes() {
        // record 1 malformed → die.
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join("bad.fq");
        std::fs::write(&inp, b"BAD\nACGT\n+\nIIII\n").unwrap();
        assert!(
            bisulfite_convert_fastq_se(
                &inp,
                &tmp.path().join("t"),
                &opts(false, None, None, false)
            )
            .is_err()
        );
        // a malformed record 2 (N>1) passes VERBATIM (sanity is count==1 only).
        let input = b"@r1\nACGT\n+\nIIII\nGARBAGE\nACGT\n+\nIIII\n";
        let (_, out) = run(input, "r.fq", &opts(false, None, None, false));
        assert!(String::from_utf8(out).unwrap().contains("\nGARBAGE\n"));
    }

    #[test]
    fn icpc_truncates_ids_end_to_end() {
        let (_, out) = run(
            b"@r1 comment\nACGT\n+\nIIII\n",
            "r.fq",
            &opts(false, None, None, true),
        );
        assert!(String::from_utf8(out).unwrap().starts_with("@r1\n"));
    }

    #[test]
    fn truncated_tail_record_dropped() {
        // one full record + a 2-line fragment → exactly one record out.
        let (cr, out) = run(
            b"@r1\nACGT\n+\nIIII\n@r2\nACGT\n",
            "r.fq",
            &opts(false, None, None, false),
        );
        assert_eq!(cr.count, 1);
        assert_eq!(out, b"@r1\nATGT\n+\nIIII\n");
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let (cr, out) = run(b"", "r.fq", &opts(false, None, None, false));
        assert_eq!(cr.count, 0);
        assert!(out.is_empty());
    }

    #[test]
    fn crlf_line_endings_preserved_file_level() {
        // chomp strips only \n (keeps \r); seq C→T keeps \r\n; id2/qual verbatim.
        let (_, out) = run(
            b"@r1\r\nACGT\r\n+\r\nIIII\r\n",
            "r.fq",
            &opts(false, None, None, false),
        );
        assert_eq!(out, b"@r1\r\nATGT\r\n+\r\nIIII\r\n");
    }

    #[test]
    fn final_record_without_trailing_newline_written_verbatim() {
        // last qual line has no trailing \n — still a complete 4-line record;
        // written verbatim (no \n appended to qual).
        let (cr, out) = run(
            b"@r1\nACGT\n+\nIIII",
            "r.fq",
            &opts(false, None, None, false),
        );
        assert_eq!(cr.count, 1);
        assert_eq!(out, b"@r1\nATGT\n+\nIIII");
    }

    #[test]
    fn prefix_prepended_to_name() {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join("reads.fq");
        std::fs::write(&inp, GOLDEN_IN).unwrap();
        let o = ConvertOptions {
            prefix: Some("pre".into()),
            ..opts(false, None, None, false)
        };
        let cr = bisulfite_convert_fastq_se(&inp, &tmp.path().join("t"), &o).unwrap();
        assert_eq!(cr.name, "pre.reads.fq_C_to_T.fastq");
    }
}
