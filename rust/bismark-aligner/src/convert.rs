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
pub(crate) fn chomp_newline(line: &[u8]) -> &[u8] {
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

/// Which bisulfite substitution a converted temp file applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConvKind {
    /// C→T (read 1 directional / SE).
    Ct,
    /// G→A (read 2 directional — Perl 5982).
    Ga,
}

/// `uc` then `tr/G/A/` (Perl 5982, the read-2 directional transform). Net incl.
/// lowercase: `a→A, c→C, g→A, t→T, n→N`.
pub fn convert_seq_g_to_a(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .map(|&b| {
            let up = b.to_ascii_uppercase();
            if up == b'G' { b'A' } else { up }
        })
        .collect()
}

fn convert_one(seq: &[u8], kind: ConvKind) -> Vec<u8> {
    match kind {
        ConvKind::Ct => convert_seq_c_to_t(seq),
        ConvKind::Ga => convert_seq_g_to_a(seq),
    }
}

/// Write the C→T-converted FastQ temp file for one single-end input (directional
/// + the C→T half of non-directional). Perl `biTransformFastQFiles` 5540–5573.
pub fn bisulfite_convert_fastq_se(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
) -> Result<ConvertedReads> {
    convert_fastq_impl(input, temp_dir, opts, ConvKind::Ct, b"", "_C_to_T")
}

/// Write the **G→A**-converted FastQ temp file for one single-end input — pbat
/// (the sole converted file, Perl 5523–5539) and the G→A half of non-directional
/// (Perl 5550–5573). No read-number ID suffix (SE), `_G_to_A` filename stem.
pub fn bisulfite_convert_fastq_se_ga(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
) -> Result<ConvertedReads> {
    convert_fastq_impl(input, temp_dir, opts, ConvKind::Ga, b"", "_G_to_A")
}

/// The temp-file filename stem for a conversion kind (Perl `_C_to_T` / `_G_to_A`).
fn file_base_for(kind: ConvKind) -> &'static str {
    match kind {
        ConvKind::Ct => "_C_to_T",
        ConvKind::Ga => "_G_to_A",
    }
}

/// The `/1/1` (R1) or `/2/2` (R2) read-number ID tag inserted before the ID's
/// trailing `\n` (Perl 5945–5960) — Bowtie 2 strips the outer `/1`,`/2`, leaving
/// `/1`,`/2`. Mode-independent: only the `tr` direction (`kind`) flips per library.
fn pe_id_suffix(read_number: u8) -> Result<&'static [u8]> {
    match read_number {
        1 => Ok(b"/1/1"),
        2 => Ok(b"/2/2"),
        _ => Err(AlignerError::Validation(format!(
            "invalid paired-end read number {read_number} (expected 1 or 2)"
        ))),
    }
}

/// Write the converted FastQ temp file for one paired-end **directional** mate
/// (Perl `biTransformFastQFiles_paired_end`, 5810–6025). Read 1 → **C→T**
/// (`_C_to_T`), read 2 → **forward G→A** (`_G_to_A`, NOT revcomp+C→T). Delegates
/// to [`bisulfite_convert_fastq_pe_kind`] with the directional read#→kind mapping.
pub fn bisulfite_convert_fastq_pe(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
    read_number: u8,
) -> Result<ConvertedReads> {
    // Directional: R1 = C→T, R2 = G→A (the mirror of pbat).
    let kind = match read_number {
        1 => ConvKind::Ct,
        2 => ConvKind::Ga,
        _ => {
            return Err(AlignerError::Validation(format!(
                "invalid paired-end read number {read_number} (expected 1 or 2)"
            )));
        }
    };
    bisulfite_convert_fastq_pe_kind(input, temp_dir, opts, read_number, kind)
}

/// Library-aware paired-end per-mate conversion (Perl 5810–6025). The `/1/1`,
/// `/2/2` ID tag is per-mate regardless of mode; only the substitution `kind`
/// flips with the library — directional R1=C→T/R2=G→A, **pbat R1=G→A/R2=C→T**,
/// non-directional = BOTH per mate. The caller passes the explicit `kind` so the
/// pbat inversion / non-dir doubling is never a silent reuse of the directional
/// read#→kind hardcoding (rev1 plan-review B I-1). The filename stem follows the
/// kind (`_C_to_T` / `_G_to_A`). `read_number` ∈ {1, 2}.
pub(crate) fn bisulfite_convert_fastq_pe_kind(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
    read_number: u8,
    kind: ConvKind,
) -> Result<ConvertedReads> {
    let suffix = pe_id_suffix(read_number)?;
    convert_fastq_impl(input, temp_dir, opts, kind, suffix, file_base_for(kind))
}

/// Shared per-record conversion core for SE + PE. `kind` selects the substitution,
/// `id_suffix` the read-number tag (empty for SE), `file_base` the filename stem
/// (`_C_to_T` / `_G_to_A`). Everything else (gz, skip/upto, prefix, max-len guard,
/// record-1 sanity, verbatim id2/qual, truncated-tail drop) is shared.
fn convert_fastq_impl(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
    kind: ConvKind,
    id_suffix: &[u8],
    file_base: &str,
) -> Result<ConvertedReads> {
    // ---- output name + path (raw concat, Perl ${temp_dir}${name}) -----------
    let basename = input.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        AlignerError::Validation(format!("could not derive a file name from input {input:?}"))
    })?;
    let mut name = match &opts.prefix {
        Some(p) => format!("{p}.{basename}"),
        None => basename.to_string(),
    };
    name.push_str(file_base);
    name.push_str(if opts.gzip { ".fastq.gz" } else { ".fastq" });
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

        // ID: chomp (\n only) → fix_id → (PE: insert /1/1 or /2/2) → re-append \n.
        let mut fixed_id = fix_id(chomp_newline(&id), opts.icpc);
        fixed_id.extend_from_slice(id_suffix);
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
        writer.write_all(&convert_one(&seq, kind))?;
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

// ===========================================================================
// FastA input (Phase 9a) — 2-line records (`>id` / `seq`, no quality line).
// Mirrors Perl `biTransformFastAFiles` (5169–5306) / `_paired_end` (5308+).
//
// Kept as a SEPARATE core (not a `RecordShape`-parameterised merge with
// `convert_fastq_impl`): the 2-vs-4-line read/write, the PER-RECORD `^>` sanity
// (vs FastQ's record-1-only `@`/`+`), and the ABSENT max-length guard diverge
// enough that a merged core would be more branches than shared code — and
// leaving `convert_fastq_impl` UNMODIFIED guarantees the FastQ byte-freeze (its
// unit tests + the oxy gate). Shared logic is the existing helpers (`fix_id`,
// `convert_one`, `temp_dir_prefix`, `pe_id_suffix`, `file_base_for`). (rev1 A/B
// endorsed a shared core; deviation documented — same intent: FastA correct +
// FastQ frozen + helpers reused.)
// ===========================================================================

/// Write the **C→T**-converted FastA temp file for one single-end input
/// (directional + the C→T half of non-directional). Perl 5278–5287.
pub fn bisulfite_convert_fasta_se(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
) -> Result<ConvertedReads> {
    convert_fasta_impl(input, temp_dir, opts, ConvKind::Ct, b"", "_C_to_T")
}

/// Write the **G→A**-converted FastA temp file for one single-end input — pbat
/// (the sole file, Perl 5273–5276) + the G→A half of non-directional (5283–5286).
pub fn bisulfite_convert_fasta_se_ga(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
) -> Result<ConvertedReads> {
    convert_fasta_impl(input, temp_dir, opts, ConvKind::Ga, b"", "_G_to_A")
}

/// Library-aware paired-end per-mate **FastA** conversion (Perl 5308+). Same
/// `(library, read_number) → kind` contract as the FastQ PE converter (caller
/// passes the explicit `kind`), `/1/1`,`/2/2` tag per mate, `_C_to_T`/`_G_to_A`
/// stem. 🔴 **PE FastA does NOT honor `--gzip`** — Perl warns and writes
/// uncompressed `.fa` (5311–5314); SE FastA gzips. gzip is forced off here
/// (byte-invisible: the converted temp is intermediate, fed straight to Bowtie 2).
pub(crate) fn bisulfite_convert_fasta_pe_kind(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
    read_number: u8,
    kind: ConvKind,
) -> Result<ConvertedReads> {
    let suffix = pe_id_suffix(read_number)?;
    let opts_no_gzip = ConvertOptions {
        gzip: false,
        ..opts.clone()
    };
    convert_fasta_impl(
        input,
        temp_dir,
        &opts_no_gzip,
        kind,
        suffix,
        file_base_for(kind),
    )
}

/// 2-line FastA conversion core (Perl 5169–5306). Per record: read `header` +
/// `sequence`; break if either is missing (truncated tail dropped, Perl
/// `last unless ($header and $sequence)`). `count++` → chomp+`fix_id`+suffix+`\n`
/// → skip/upto (falsy-0) → tab-detect → **PER-RECORD `^>` sanity** (die on every
/// non-skipped record, NOT record-1-only — Perl 5271) → write `header` +
/// `convert_one(seq, kind)` (2 lines; NO `+`/qual line, NO max-length guard).
/// Filename `<prefix.>?<basename><file_base>.fa[.gz]`.
fn convert_fasta_impl(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions,
    kind: ConvKind,
    id_suffix: &[u8],
    file_base: &str,
) -> Result<ConvertedReads> {
    // ---- output name + path (raw concat; `.fa` ext, not `.fastq`) ----------
    let basename = input.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        AlignerError::Validation(format!("could not derive a file name from input {input:?}"))
    })?;
    let mut name = match &opts.prefix {
        Some(p) => format!("{p}.{basename}"),
        None => basename.to_string(),
    };
    name.push_str(file_base);
    name.push_str(if opts.gzip { ".fa.gz" } else { ".fa" });
    let full = format!("{}{name}", temp_dir_prefix(temp_dir)?);
    let full_path = PathBuf::from(&full);

    // ---- reader / writer (gz or plain) — same as the FastQ core ------------
    let file = File::open(input)?;
    let mut reader: Box<dyn BufRead> = if input.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let out = File::create(&full_path)?;
    let mut writer: BufWriter<Box<dyn Write>> = BufWriter::new(if opts.gzip {
        Box::new(GzEncoder::new(out, Compression::default()))
    } else {
        Box::new(out)
    });

    let (mut id, mut seq) = (Vec::new(), Vec::new());
    let mut count: u64 = 0;
    let mut seqid_tab_count: u64 = 0;

    loop {
        id.clear();
        seq.clear();
        let n1 = reader.read_until(b'\n', &mut id)?;
        let n2 = reader.read_until(b'\n', &mut seq)?;
        // Perl `last unless ($header and $sequence)` — a truncated final record drops.
        if n1 == 0 || n2 == 0 {
            break;
        }
        count += 1;

        // header: chomp (\n only) → fix_id → (PE: insert /1/1 or /2/2) → re-append \n.
        let mut fixed_id = fix_id(chomp_newline(&id), opts.icpc);
        fixed_id.extend_from_slice(id_suffix);
        fixed_id.push(b'\n');

        // skip/upto (Perl falsy-0). The per-record sanity below sits AFTER skip,
        // so a skipped record is not sanity-checked (Perl `next` precedes 5271).
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

        // tab-in-ID detection (byte-neutral counter; dead like FastQ — fix_id
        // already removed tabs — matching Perl 5266).
        if fixed_id.contains(&b'\t') {
            seqid_tab_count += 1;
        }

        // PER-RECORD FastA sanity (Perl 5271): every non-skipped header must be `^>`.
        if !fixed_id.starts_with(b">") {
            return Err(AlignerError::Validation(format!(
                "Input file doesn't seem to be in FastA format at sequence {count}"
            )));
        }

        // Write 2 lines: header + converted seq (`convert_one` uppercases then
        // C→T/G→A, preserving the seq's own `\n`). No `+`/qual, no max-len guard.
        writer.write_all(&fixed_id)?;
        writer.write_all(&convert_one(&seq, kind))?;
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

    // ---- paired-end conversion (Phase 7) -----------------------------------

    #[test]
    fn convert_g_to_a_uc_then_substitute() {
        // uc('ACGTacgtN') = 'ACGTACGTN', then G->A => 'ACATACATN'
        assert_eq!(convert_seq_g_to_a(b"ACGTacgtN\n"), b"ACATACATN\n");
        assert_eq!(convert_seq_g_to_a(b"ggGG\r\n"), b"AAAA\r\n");
    }

    fn run_pe(
        input: &[u8],
        name: &str,
        o: &ConvertOptions,
        read_number: u8,
    ) -> (ConvertedReads, Vec<u8>) {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join(name);
        std::fs::write(&inp, input).unwrap();
        let td = tmp.path().join("t");
        let cr = bisulfite_convert_fastq_pe(&inp, &td, o, read_number).unwrap();
        let out = std::fs::read(&cr.path).unwrap();
        (cr, out)
    }

    #[test]
    fn pe_read1_c_to_t_with_slash_1_1_suffix() {
        let (cr, out) = run_pe(GOLDEN_IN, "r_1.fq", &opts(false, None, None, false), 1);
        assert_eq!(cr.name, "r_1.fq_C_to_T.fastq");
        assert_eq!(
            out,
            b"@read1_1:N:0:ATCG/1/1\nATGTATGTNN\n+\nIIIIIIIIII\n@read2_lane2/1/1\nTTTTGGTT\n+read2\nJJJJJJJJ\n".to_vec()
        );
    }

    #[test]
    fn pe_read2_g_to_a_with_slash_2_2_suffix() {
        let (cr, out) = run_pe(GOLDEN_IN, "r_2.fq", &opts(false, None, None, false), 2);
        assert_eq!(cr.name, "r_2.fq_G_to_A.fastq");
        assert_eq!(
            out,
            b"@read1_1:N:0:ATCG/2/2\nACATACATNN\n+\nIIIIIIIIII\n@read2_lane2/2/2\nCCCCAATT\n+read2\nJJJJJJJJ\n".to_vec()
        );
    }

    #[test]
    fn pe_suffix_inserted_before_newline_crlf() {
        // the /1/1 tag goes BEFORE the trailing \n; a CRLF id keeps its \r.
        let (_, out) = run_pe(
            b"@r1\r\nACGT\r\n+\r\nIIII\r\n",
            "r.fq",
            &opts(false, None, None, false),
            1,
        );
        assert_eq!(out, b"@r1\r/1/1\nATGT\r\n+\r\nIIII\r\n".to_vec());
    }

    #[test]
    fn pe_invalid_read_number_errors() {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join("r.fq");
        std::fs::write(&inp, GOLDEN_IN).unwrap();
        assert!(
            bisulfite_convert_fastq_pe(
                &inp,
                &tmp.path().join("t"),
                &opts(false, None, None, false),
                3
            )
            .is_err()
        );
    }

    // ---- Phase 8: non-directional + pbat conversion variants ----------------

    /// G→A of `GOLDEN_IN` (uc then `tr/G/A/`): read1 ACGTACGTNN→ACATACATNN,
    /// read2 CCCCGGTT→CCCCAATT. id2/qual verbatim; fix_IDs ws→`_`.
    const GOLDEN_OUT_GA: &[u8] =
        b"@read1_1:N:0:ATCG\nACATACATNN\n+\nIIIIIIIIII\n@read2_lane2\nCCCCAATT\n+read2\nJJJJJJJJ\n";

    #[test]
    fn se_ga_entry_point_g_to_a_no_suffix() {
        // pbat SE + the G→A half of non-dir SE: G→A bytes, `_G_to_A` stem, no tag.
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join("reads.fq");
        std::fs::write(&inp, GOLDEN_IN).unwrap();
        let cr = bisulfite_convert_fastq_se_ga(
            &inp,
            &tmp.path().join("t"),
            &opts(false, None, None, false),
        )
        .unwrap();
        let out = std::fs::read(&cr.path).unwrap();
        assert_eq!(out, GOLDEN_OUT_GA);
        assert_eq!(cr.name, "reads.fq_G_to_A.fastq");
        assert_eq!(cr.count, 2);
    }

    fn run_pe_kind(
        input: &[u8],
        name: &str,
        o: &ConvertOptions,
        read_number: u8,
        kind: ConvKind,
    ) -> (ConvertedReads, Vec<u8>) {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join(name);
        std::fs::write(&inp, input).unwrap();
        let td = tmp.path().join("t");
        let cr = bisulfite_convert_fastq_pe_kind(&inp, &td, o, read_number, kind).unwrap();
        let out = std::fs::read(&cr.path).unwrap();
        (cr, out)
    }

    #[test]
    fn pe_pbat_r1_is_g_to_a_with_slash_1_1() {
        // pbat INVERTS directional: R1 → G→A `/1/1` `_G_to_A` (not C→T).
        let (cr, out) = run_pe_kind(
            GOLDEN_IN,
            "r_1.fq",
            &opts(false, None, None, false),
            1,
            ConvKind::Ga,
        );
        assert_eq!(cr.name, "r_1.fq_G_to_A.fastq");
        assert_eq!(
            out,
            b"@read1_1:N:0:ATCG/1/1\nACATACATNN\n+\nIIIIIIIIII\n@read2_lane2/1/1\nCCCCAATT\n+read2\nJJJJJJJJ\n".to_vec()
        );
    }

    #[test]
    fn pe_pbat_r2_is_c_to_t_with_slash_2_2() {
        // pbat R2 → C→T `/2/2` `_C_to_T` (the mirror of directional R1).
        let (cr, out) = run_pe_kind(
            GOLDEN_IN,
            "r_2.fq",
            &opts(false, None, None, false),
            2,
            ConvKind::Ct,
        );
        assert_eq!(cr.name, "r_2.fq_C_to_T.fastq");
        assert_eq!(
            out,
            b"@read1_1:N:0:ATCG/2/2\nATGTATGTNN\n+\nIIIIIIIIII\n@read2_lane2/2/2\nTTTTGGTT\n+read2\nJJJJJJJJ\n".to_vec()
        );
    }

    #[test]
    fn pe_nondir_makes_both_kinds_per_mate() {
        // non-dir: each mate → BOTH C→T and G→A (4 temp files); the `/1/1`,`/2/2`
        // tag is per-mate regardless of kind, the stem follows the kind.
        let o = opts(false, None, None, false);
        let (ct1, _) = run_pe_kind(GOLDEN_IN, "r_1.fq", &o, 1, ConvKind::Ct);
        let (ga1, ga1_out) = run_pe_kind(GOLDEN_IN, "r_1.fq", &o, 1, ConvKind::Ga);
        let (ct2, ct2_out) = run_pe_kind(GOLDEN_IN, "r_2.fq", &o, 2, ConvKind::Ct);
        let (ga2, _) = run_pe_kind(GOLDEN_IN, "r_2.fq", &o, 2, ConvKind::Ga);
        assert_eq!(ct1.name, "r_1.fq_C_to_T.fastq");
        assert_eq!(ga1.name, "r_1.fq_G_to_A.fastq");
        assert_eq!(ct2.name, "r_2.fq_C_to_T.fastq");
        assert_eq!(ga2.name, "r_2.fq_G_to_A.fastq");
        // R1 G→A carries the `/1/1` tag; R2 C→T carries `/2/2`.
        assert!(ga1_out.starts_with(b"@read1_1:N:0:ATCG/1/1\nACATACATNN\n"));
        assert!(ct2_out.starts_with(b"@read1_1:N:0:ATCG/2/2\nATGTATGTNN\n"));
    }

    // ---- Phase 9a: FastA conversion (2-line records, `>` prefix, `.fa`) ------

    /// 2-line FastA: `>id` / seq, no `+`/qual. Same IDs/seqs as `GOLDEN_IN`.
    const GOLDEN_FA_IN: &[u8] = b">read1 1:N:0:ATCG\nACGTacgtNN\n>read2\tlane2\nccCCggTT\n";
    /// C→T (uc then tr/C/T/); `>` PRESERVED, fix_IDs ws→`_`; no qual lines.
    const GOLDEN_FA_OUT_CT: &[u8] = b">read1_1:N:0:ATCG\nATGTATGTNN\n>read2_lane2\nTTTTGGTT\n";
    /// G→A.
    const GOLDEN_FA_OUT_GA: &[u8] = b">read1_1:N:0:ATCG\nACATACATNN\n>read2_lane2\nCCCCAATT\n";

    fn run_fa(input: &[u8], name: &str, o: &ConvertOptions) -> (ConvertedReads, Vec<u8>) {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join(name);
        std::fs::write(&inp, input).unwrap();
        let cr = bisulfite_convert_fasta_se(&inp, &tmp.path().join("t"), o).unwrap();
        let out = std::fs::read(&cr.path).unwrap();
        (cr, out)
    }

    #[test]
    fn fasta_se_c_to_t_golden() {
        let (cr, out) = run_fa(GOLDEN_FA_IN, "reads.fa", &opts(false, None, None, false));
        assert_eq!(out, GOLDEN_FA_OUT_CT);
        assert_eq!(cr.name, "reads.fa_C_to_T.fa"); // `.fa`, not `.fastq`
        assert_eq!(cr.count, 2);
    }

    #[test]
    fn fasta_se_g_to_a_golden() {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join("reads.fa");
        std::fs::write(&inp, GOLDEN_FA_IN).unwrap();
        let cr = bisulfite_convert_fasta_se_ga(
            &inp,
            &tmp.path().join("t"),
            &opts(false, None, None, false),
        )
        .unwrap();
        assert_eq!(std::fs::read(&cr.path).unwrap(), GOLDEN_FA_OUT_GA);
        assert_eq!(cr.name, "reads.fa_G_to_A.fa");
    }

    fn run_fa_kind(
        input: &[u8],
        name: &str,
        o: &ConvertOptions,
        read_number: u8,
        kind: ConvKind,
    ) -> (ConvertedReads, Vec<u8>) {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join(name);
        std::fs::write(&inp, input).unwrap();
        let cr = bisulfite_convert_fasta_pe_kind(&inp, &tmp.path().join("t"), o, read_number, kind)
            .unwrap();
        let out = std::fs::read(&cr.path).unwrap();
        (cr, out)
    }

    #[test]
    fn fasta_pe_pbat_r1_ga_r2_ct() {
        // pbat PE FastA: R1 → G→A `/1/1` `_G_to_A.fa`; R2 → C→T `/2/2` `_C_to_T.fa`.
        let (cr1, out1) = run_fa_kind(
            GOLDEN_FA_IN,
            "r_1.fa",
            &opts(false, None, None, false),
            1,
            ConvKind::Ga,
        );
        assert_eq!(cr1.name, "r_1.fa_G_to_A.fa");
        assert_eq!(
            out1,
            b">read1_1:N:0:ATCG/1/1\nACATACATNN\n>read2_lane2/1/1\nCCCCAATT\n".to_vec()
        );
        let (cr2, out2) = run_fa_kind(
            GOLDEN_FA_IN,
            "r_2.fa",
            &opts(false, None, None, false),
            2,
            ConvKind::Ct,
        );
        assert_eq!(cr2.name, "r_2.fa_C_to_T.fa");
        assert_eq!(
            out2,
            b">read1_1:N:0:ATCG/2/2\nATGTATGTNN\n>read2_lane2/2/2\nTTTTGGTT\n".to_vec()
        );
    }

    #[test]
    fn fasta_per_record_sanity_record2_dies() {
        // 🔴 rev1 A/B: FastA dies on EVERY record whose header is not `^>` (Perl
        // 5271), NOT record-1-only like FastQ. A malformed record 2 must die.
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join("bad.fa");
        std::fs::write(&inp, b">r1\nACGT\nNOT_A_HEADER\nACGT\n").unwrap();
        assert!(
            bisulfite_convert_fasta_se(
                &inp,
                &tmp.path().join("t"),
                &opts(false, None, None, false)
            )
            .is_err(),
            "FastA record-2 with a non-`>` header must die (per-record sanity)"
        );
        // contrast: the SAME malformed record-2 PASSES under FastQ (record-1-only).
        let fq = tmp.path().join("ok.fq");
        std::fs::write(&fq, b"@r1\nACGT\n+\nIIII\nNOT_A_HEADER\nACGT\n+\nIIII\n").unwrap();
        assert!(
            bisulfite_convert_fastq_se(
                &fq,
                &tmp.path().join("t2"),
                &opts(false, None, None, false)
            )
            .is_ok()
        );
    }

    #[test]
    fn fasta_record1_malformed_dies() {
        let tmp = TempDir::new().unwrap();
        let inp = tmp.path().join("bad.fa");
        std::fs::write(&inp, b"@notfasta\nACGT\n").unwrap(); // `@` not `>`
        assert!(
            bisulfite_convert_fasta_se(
                &inp,
                &tmp.path().join("t"),
                &opts(false, None, None, false)
            )
            .is_err()
        );
    }

    #[test]
    fn fasta_se_gzip_decompresses_to_plain() {
        let (cr, raw) = run_fa(GOLDEN_FA_IN, "reads.fa", &opts(true, None, None, false));
        assert_eq!(cr.name, "reads.fa_C_to_T.fa.gz");
        assert_eq!(gunzip_bytes(&raw), GOLDEN_FA_OUT_CT); // SE FastA honors --gzip
    }

    #[test]
    fn fasta_pe_gzip_forced_off() {
        // PE FastA does NOT gzip even when --gzip is set (Perl warns + uncompressed).
        let (cr, out) = run_fa_kind(
            GOLDEN_FA_IN,
            "r_1.fa",
            &opts(true, None, None, false),
            1,
            ConvKind::Ct,
        );
        assert_eq!(cr.name, "r_1.fa_C_to_T.fa"); // NOT `.fa.gz`
        assert!(out.starts_with(b">read1_1:N:0:ATCG/1/1\n")); // plain text, not gzip
    }

    #[test]
    fn fasta_empty_and_crlf() {
        let (cr, out) = run_fa(b"", "r.fa", &opts(false, None, None, false));
        assert_eq!(cr.count, 0);
        assert!(out.is_empty());
        // CRLF: chomp strips \n (keeps \r) on the header; seq keeps \r\n.
        let (_, out2) = run_fa(b">r1\r\nACGT\r\n", "r.fa", &opts(false, None, None, false));
        assert_eq!(out2, b">r1\r\nATGT\r\n");
    }

    #[test]
    fn fasta_skip_and_upto() {
        let input = b">r1\nAA\n>r2\nAA\n>r3\nAA\n>r4\nAA\n>r5\nAA\n";
        let (cr, out) = run_fa(input, "r.fa", &opts(false, Some(2), Some(4), false));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(">r3") && s.contains(">r4"));
        assert!(!s.contains(">r1") && !s.contains(">r2") && !s.contains(">r5"));
        assert_eq!(cr.count, 5); // count runs over unskipped numbering; upto breaks at 5
    }
}
