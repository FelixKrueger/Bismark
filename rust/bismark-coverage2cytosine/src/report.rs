//! The core genome-wide cytosine report — the byte-identity crux.
//!
//! Mirrors Perl `generate_genome_wide_cytosine_report:168-745` +
//! `process_unprocessed_chromosomes:1388-1565`: stream the coverage file one
//! chromosome at a time, walk every `C`/`G` in that chromosome's genome
//! sequence with exact `pos = i+1` coordinate arithmetic, classify cytosine
//! context, and emit the per-cytosine report (CpG-only default; all contexts
//! with `--CX`), plus the always-on cytosine-context summary.
//!
//! A **single per-position kernel** ([`emit_position`]) serves all three Perl
//! blocks (first-chromosomes, last-chromosome, uncovered-chromosomes). They
//! differ only in guard *order*, which is byte-identical because every guard
//! is a skip-guard and the threshold check precedes context classification in
//! all of them (verified against the Perl, Phase-B plan §3.2).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;

use crate::cli::ResolvedConfig;
use crate::cov;
use crate::error::BismarkC2cError;
use crate::genome::Genome;
use crate::summary::ContextSummary;

/// Output sink for a report file — plain or gzip, with an explicit `finish()`
/// (Phase C). The context summary is never routed through this (always plain).
pub(crate) enum ReportWriter {
    Plain(BufWriter<File>),
    Gz(GzEncoder<BufWriter<File>>),
}

impl ReportWriter {
    /// Create (truncating) the file at `path`; wrap in a gzip encoder when
    /// `gzip`. Truncation is load-bearing for `--split_by_chromosome`'s
    /// reopen-on-every-transition semantics.
    pub(crate) fn create(path: &Path, gzip: bool) -> Result<Self, BismarkC2cError> {
        let bw = BufWriter::new(File::create(path)?);
        Ok(if gzip {
            ReportWriter::Gz(GzEncoder::new(bw, Compression::default()))
        } else {
            ReportWriter::Plain(bw)
        })
    }

    pub(crate) fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            ReportWriter::Plain(w) => w.write_all(buf),
            ReportWriter::Gz(w) => w.write_all(buf),
        }
    }

    /// Flush/finish the underlying stream. For gzip this writes the trailer —
    /// called even on a zero-write encoder (→ a valid empty-gzip stream).
    pub(crate) fn finish(self) -> Result<(), BismarkC2cError> {
        match self {
            ReportWriter::Plain(mut w) => w.flush()?,
            ReportWriter::Gz(w) => {
                w.finish()?;
            }
        }
        Ok(())
    }
}

/// Cytosine context.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Context {
    Cg,
    Chg,
    Chh,
}

impl Context {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Context::Cg => b"CG",
            Context::Chg => b"CHG",
            Context::Chh => b"CHH",
        }
    }
}

/// Faithful model of Perl `substr(seq, offset, want)`: a negative `offset`
/// counts from the end (clamped to 0 if `|offset| > len`); the result is
/// truncated at the string end; empty if `start >= len`.
pub(crate) fn perl_substr(seq: &[u8], offset: isize, want: usize) -> &[u8] {
    let start = if offset < 0 {
        let from_end = (-offset) as usize;
        seq.len().saturating_sub(from_end)
    } else {
        offset as usize
    };
    if start >= seq.len() {
        return &[];
    }
    let end = (start + want).min(seq.len());
    &seq[start..end]
}

/// Reverse-complement via `tr/ACTG/TGAC/`: A↔T, C↔G; **every other byte
/// (including `N`) passes through unchanged**.
pub(crate) fn revcomp(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .rev()
        .map(|&b| match b {
            b'A' => b'T',
            b'T' => b'A',
            b'C' => b'G',
            b'G' => b'C',
            other => other,
        })
        .collect()
}

/// Classify a 5'→3' trinucleotide (Perl `:365-377`): `^CG` → CG (any length);
/// `^C.G$` (len 3) → CHG; `^C..$` (len 3) → CHH; else unclassifiable.
pub(crate) fn classify_context(tri: &[u8]) -> Option<Context> {
    if tri.len() >= 2 && tri[0] == b'C' && tri[1] == b'G' {
        return Some(Context::Cg);
    }
    if tri.len() == 3 && tri[0] == b'C' {
        if tri[2] == b'G' {
            return Some(Context::Chg);
        }
        return Some(Context::Chh);
    }
    None
}

/// Extract the 5'→3' `(tri_nt, upstream, strand)` for the `C`/`G` at index `i`.
/// `pos` (1-based) = `i + 1`; arithmetic per Perl `:262-341`.
fn extract(seq: &[u8], i: usize) -> (Vec<u8>, Vec<u8>, u8) {
    if seq[i] == b'C' {
        // forward strand
        let tri = perl_substr(seq, i as isize, 3).to_vec(); // substr(seq, pos-1, 3)
        let upstream = perl_substr(seq, i as isize - 1, 3).to_vec(); // substr(seq, pos-2, 3)
        (tri, upstream, b'+')
    } else {
        // 'G' — reverse strand
        let tri = if i < 2 {
            // Perl pos-3 < 0: substr(seq, 0, pos) → 1 or 2 bytes (dropped by len<3)
            seq[0..=i].to_vec()
        } else {
            revcomp(&seq[i - 2..=i]) // revcomp(substr(seq, pos-3, 3))
        };
        let upstream = revcomp(perl_substr(seq, i as isize - 1, 3));
        (tri, upstream, b'-')
    }
}

/// The per-position kernel: extract → guards → classify → accumulate summary
/// → emit. Appends the report line to `out` (when emitted) and accumulates the
/// context summary (when `accumulate_summary`). Guard order per Perl's
/// covered-chromosome block (`:343-447`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_position(
    name: &[u8],
    seq: &[u8],
    i: usize,
    buffer: &HashMap<u32, (u32, u32)>,
    cpg_only: bool,
    zero_based: bool,
    threshold: u32,
    accumulate_summary: bool,
    summary: &mut ContextSummary,
    out: &mut Vec<u8>,
) {
    let pos = (i + 1) as u32;
    let (tri, upstream, strand) = extract(seq, i);

    // Guard 1: trinucleotide could not be extracted (chromosome edge).
    if tri.len() < 3 {
        return;
    }
    // Guard 2: the very last genome base — its bottom-strand partner would
    // need the following base (Perl :347).
    if seq.len() as u32 - pos == 0 {
        return;
    }
    // Coverage lookup (uncovered ⇒ 0,0).
    let (meth, nonmeth) = buffer.get(&pos).copied().unwrap_or((0, 0));
    // Guard 3: coverage threshold (default 0 never skips).
    if meth + nonmeth < threshold {
        return;
    }
    // Classify context; unclassifiable → skip (Perl warns to stderr — exempt).
    let Some(context) = classify_context(&tri) else {
        return;
    };
    // Accumulate the context summary BEFORE the CpG-only emit filter, and only
    // for covered chromosomes (Perl's uncovered pass does not call this).
    if accumulate_summary && let Some(&ubase) = upstream.first() {
        summary.accumulate(&tri, ubase, meth, nonmeth);
    }
    // Emit filter: CpG-only emits CG only; --CX emits every classified context.
    if cpg_only && context != Context::Cg {
        return;
    }
    let out_pos = if zero_based { pos - 1 } else { pos };

    out.extend_from_slice(name);
    out.push(b'\t');
    out.extend_from_slice(out_pos.to_string().as_bytes());
    out.push(b'\t');
    out.push(strand);
    out.push(b'\t');
    out.extend_from_slice(meth.to_string().as_bytes());
    out.push(b'\t');
    out.extend_from_slice(nonmeth.to_string().as_bytes());
    out.push(b'\t');
    out.extend_from_slice(context.as_bytes());
    out.push(b'\t');
    out.extend_from_slice(&tri);
    out.push(b'\n');
}

/// Build one chromosome's report lines (the Phase-B kernel walk). A cov
/// chromosome absent from the genome yields no bytes (Perl's empty `while`-walk).
/// The caller routes the bytes to the single shared writer (non-split) or a
/// per-chromosome writer (split).
fn chromosome_report_bytes(
    name: &[u8],
    genome: &Genome,
    buffer: &HashMap<u32, (u32, u32)>,
    config: &ResolvedConfig,
    accumulate_summary: bool,
    summary: &mut ContextSummary,
) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let Some(seq) = genome.get(name) else {
        return out;
    };
    for i in 0..seq.len() {
        if seq[i] == b'C' || seq[i] == b'G' {
            emit_position(
                name,
                seq,
                i,
                buffer,
                config.cpg_only,
                config.zero_based,
                config.threshold,
                accumulate_summary,
                summary,
                &mut out,
            );
        }
    }
    out
}

/// Generate the genome-wide cytosine report(s) + context summary. Dispatches
/// to the single-file path (default; gz-wrapped under `--gzip`) or the
/// per-chromosome path (`--split_by_chromosome`).
pub fn run_report(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError> {
    // Create the output directory if a non-empty prefix was given (Perl mkdir).
    if !config.output_dir.is_empty() {
        std::fs::create_dir_all(&config.output_dir)?;
    }
    let mut reader = cov::open_cov(&config.cov_infile)?;
    let mut summary = ContextSummary::new();
    if config.split_by_chromosome {
        run_split(config, genome, reader.as_mut(), &mut summary)
    } else {
        run_single(config, genome, reader.as_mut(), &mut summary)
    }
}

/// Single-file report (default / `--gzip`): one writer for the whole genome.
fn run_single(
    config: &ResolvedConfig,
    genome: &Genome,
    reader: &mut dyn BufRead,
    summary: &mut ContextSummary,
) -> Result<(), BismarkC2cError> {
    let mut report_w = ReportWriter::create(&report_path(config, None), config.gzip)?;
    let mut seen: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
    let mut cur_chr: Option<Vec<u8>> = None;
    let mut buffer: HashMap<u32, (u32, u32)> = HashMap::new();
    let mut line: Vec<u8> = Vec::new();
    let mut line_no = 0usize;

    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        line_no += 1;
        let Some((chr, start, meth, nonmeth)) = cov::parse_cov_line(&line, line_no)? else {
            continue;
        };
        if cur_chr.as_deref() != Some(chr.as_slice()) {
            if let Some(prev) = cur_chr.take() {
                let bytes = chromosome_report_bytes(&prev, genome, &buffer, config, true, summary);
                report_w.write_all(&bytes)?;
            }
            buffer.clear();
            seen.insert(chr.clone());
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth));
    }
    match cur_chr.take() {
        None => return Err(BismarkC2cError::EmptyCoverageInput),
        Some(prev) => {
            let bytes = chromosome_report_bytes(&prev, genome, &buffer, config, true, summary);
            report_w.write_all(&bytes)?;
        }
    }
    if config.threshold == 0 {
        let empty: HashMap<u32, (u32, u32)> = HashMap::new();
        for name in genome.names_sorted() {
            if !seen.contains(name) {
                let bytes = chromosome_report_bytes(name, genome, &empty, config, false, summary);
                report_w.write_all(&bytes)?;
            }
        }
    }
    report_w.finish()?;

    // Context summary — always plain (never gzipped).
    let mut sw = BufWriter::new(File::create(summary_path(config, None))?);
    summary.write_to(&mut sw)?;
    sw.flush()?;
    Ok(())
}

/// Per-chromosome report (`--split_by_chromosome`): a fresh truncating writer
/// per chromosome (incl. re-appearance), and the Perl context-summary quirk —
/// every chr gets an (empty) summary file, the full summary lands only in the
/// LAST chromosome reopened.
fn run_split(
    config: &ResolvedConfig,
    genome: &Genome,
    reader: &mut dyn BufRead,
    summary: &mut ContextSummary,
) -> Result<(), BismarkC2cError> {
    let mut seen: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
    let mut cur_chr: Option<Vec<u8>> = None;
    let mut buffer: HashMap<u32, (u32, u32)> = HashMap::new();
    let mut line: Vec<u8> = Vec::new();
    let mut line_no = 0usize;

    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        line_no += 1;
        let Some((chr, start, meth, nonmeth)) = cov::parse_cov_line(&line, line_no)? else {
            continue;
        };
        if cur_chr.as_deref() != Some(chr.as_slice()) {
            if let Some(prev) = cur_chr.take() {
                // A non-final covered chr: flush it (creates its report + empty
                // summary file). It is never the last chr reopened (the final
                // cur_chr is flushed after the loop), so its summary path is
                // discarded — the full summary goes to a later chr.
                flush_split_chromosome(&prev, genome, &buffer, config, true, summary)?;
            }
            buffer.clear();
            seen.insert(chr.clone());
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth));
    }
    // The final cov chromosome is flushed here (never in the loop); empty input
    // → error before the uncovered pass. This is the first definite value of
    // `last_summary_path` (a `PathBuf`, not an `Option` — there is always ≥1 chr).
    let mut last_summary_path = match cur_chr.take() {
        None => return Err(BismarkC2cError::EmptyCoverageInput),
        Some(prev) => flush_split_chromosome(&prev, genome, &buffer, config, true, summary)?,
    };
    if config.threshold == 0 {
        let empty: HashMap<u32, (u32, u32)> = HashMap::new();
        for name in genome.names_sorted() {
            if !seen.contains(name) {
                last_summary_path =
                    flush_split_chromosome(name, genome, &empty, config, false, summary)?;
            }
        }
    }

    // Full summary → only the LAST chromosome reopened (others stay empty).
    let mut sw = BufWriter::new(File::create(&last_summary_path)?);
    summary.write_to(&mut sw)?;
    sw.flush()?;
    Ok(())
}

/// Split-mode per-chromosome flush: walk the chromosome, open a **fresh,
/// truncating** per-chr report writer (no caching — a re-appearance truncates),
/// write + finish; then create/truncate an empty per-chr summary file. Returns
/// that summary path so the caller can route the full summary to the last one.
fn flush_split_chromosome(
    name: &[u8],
    genome: &Genome,
    buffer: &HashMap<u32, (u32, u32)>,
    config: &ResolvedConfig,
    accumulate_summary: bool,
    summary: &mut ContextSummary,
) -> Result<PathBuf, BismarkC2cError> {
    let bytes = chromosome_report_bytes(name, genome, buffer, config, accumulate_summary, summary);
    let mut w = ReportWriter::create(&report_path(config, Some(name)), config.gzip)?;
    w.write_all(&bytes)?;
    w.finish()?; // zero-emit chr: empty file (plain) / valid empty-gzip stream
    let summary_path = summary_path(config, Some(name));
    File::create(&summary_path)?; // empty (truncate) — Perl reopens '>' per chr
    Ok(summary_path)
}

// ── Filename derivation (Perl handle_filehandles:99-117) ────────────────────

/// Report filename. Split (`chr = Some`) uses the **raw `-o`** + literal `.chr`
/// infix with **no** suffix strip (Perl appends `.chr` before the strip, which
/// then no-ops — so a suffixed `-o` doubles the suffix). Non-split uses the
/// stripped stem. `.gz` appended under `--gzip`.
pub(crate) fn report_name(
    output_raw: &str,
    output_stem: &str,
    chr: Option<&[u8]>,
    cx: bool,
    gz: bool,
) -> String {
    let base = match chr {
        Some(name) => format!("{output_raw}.chr{}", String::from_utf8_lossy(name)),
        None => output_stem.to_string(),
    };
    let suffix = if cx {
        ".CX_report.txt"
    } else {
        ".CpG_report.txt"
    };
    let gzs = if gz { ".gz" } else { "" };
    format!("{base}{suffix}{gzs}")
}

/// Context-summary filename (never gzipped). Same base as the report.
fn summary_name(output_raw: &str, output_stem: &str, chr: Option<&[u8]>) -> String {
    let base = match chr {
        Some(name) => format!("{output_raw}.chr{}", String::from_utf8_lossy(name)),
        None => output_stem.to_string(),
    };
    format!("{base}.cytosine_context_summary.txt")
}

/// `{output_dir}{name}` — `output_dir` is a path prefix (`""` = cwd, else ends `/`).
pub(crate) fn report_path(config: &ResolvedConfig, chr: Option<&[u8]>) -> PathBuf {
    PathBuf::from(format!(
        "{}{}",
        config.output_dir,
        report_name(
            &config.output_raw,
            &config.output_stem,
            chr,
            config.cx_context,
            config.gzip
        )
    ))
}

fn summary_path(config: &ResolvedConfig, chr: Option<&[u8]>) -> PathBuf {
    PathBuf::from(format!(
        "{}{}",
        config.output_dir,
        summary_name(&config.output_raw, &config.output_stem, chr)
    ))
}

// ── Phase D: merged / discordant CpG cov filenames (Perl combine_CpGs:1766-1790) ──
// Derived from the REPORT filename: strip trailing `.gz` then `.txt`, append the
// suffix (+ `.gz` under `--gzip`). For `-o merge` → report `merge.CpG_report.txt`
// → `merge.CpG_report.merged_CpG_evidence.cov`.

fn cov_evidence_name(report_filename: &str, suffix: &str, gzip: bool) -> String {
    // Perl: s/\.gz$//; s/\.txt$//; (strip .gz then .txt, each at most once).
    let base = report_filename
        .strip_suffix(".gz")
        .unwrap_or(report_filename);
    let base = base.strip_suffix(".txt").unwrap_or(base);
    let gzs = if gzip { ".gz" } else { "" };
    format!("{base}{suffix}{gzs}")
}

pub(crate) fn merged_cov_name(report_filename: &str, gzip: bool) -> String {
    cov_evidence_name(report_filename, ".merged_CpG_evidence.cov", gzip)
}

pub(crate) fn discordant_cov_name(report_filename: &str, gzip: bool) -> String {
    cov_evidence_name(report_filename, ".discordant_CpG_evidence.cov", gzip)
}

/// `{output_dir}{merged_cov_name(report basename)}`. `--merge_CpGs` is non-split
/// + non-CX (Phase A), so the report is `{output_stem}.CpG_report.txt[.gz]`.
pub(crate) fn merged_cov_path(config: &ResolvedConfig) -> PathBuf {
    let report = report_name(
        &config.output_raw,
        &config.output_stem,
        None,
        false,
        config.gzip,
    );
    PathBuf::from(format!(
        "{}{}",
        config.output_dir,
        merged_cov_name(&report, config.gzip)
    ))
}

pub(crate) fn discordant_cov_path(config: &ResolvedConfig) -> PathBuf {
    let report = report_name(
        &config.output_raw,
        &config.output_stem,
        None,
        false,
        config.gzip,
    );
    PathBuf::from(format!(
        "{}{}",
        config.output_dir,
        discordant_cov_name(&report, config.gzip)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Task 1: primitives ──

    #[test]
    fn perl_substr_interior_and_truncation() {
        assert_eq!(perl_substr(b"ACGT", 1, 2), b"CG");
        assert_eq!(perl_substr(b"ACGT", 2, 9), b"GT");
        assert_eq!(perl_substr(b"ACGT", 9, 3), b"");
    }

    #[test]
    fn perl_substr_negative_wraps_from_end() {
        assert_eq!(perl_substr(b"ACGT", -1, 3), b"T");
        assert_eq!(perl_substr(b"ACGT", -2, 3), b"GT");
        assert_eq!(perl_substr(b"ACGT", -9, 3), b"ACG"); // |off|>len → clamp to 0
    }

    #[test]
    fn revcomp_complements_acgt_leaves_n() {
        assert_eq!(revcomp(b"ACG"), b"CGT");
        assert_eq!(revcomp(b"GCG"), b"CGC");
        assert_eq!(revcomp(b"ANG"), b"CNT");
    }

    #[test]
    fn classify_context_matches_perl_regex() {
        assert_eq!(classify_context(b"CGT"), Some(Context::Cg));
        assert_eq!(classify_context(b"CAG"), Some(Context::Chg));
        assert_eq!(classify_context(b"CAA"), Some(Context::Chh));
        assert_eq!(classify_context(b"CNG"), Some(Context::Chg));
        assert_eq!(classify_context(b"CNN"), Some(Context::Chh));
        assert_eq!(classify_context(b"CCG"), Some(Context::Chg));
        assert_eq!(classify_context(b"GTA"), None);
        assert_eq!(classify_context(b"CG"), Some(Context::Cg));
        assert_eq!(classify_context(b"CA"), None);
    }

    // ── Task 4: the kernel (verified against the Perl anchor) ──

    fn run_t(seq: &[u8], cov: &[(u32, u32, u32)], cpg_only: bool, zero: bool, thr: u32) -> String {
        let mut buf = HashMap::new();
        for &(p, m, u) in cov {
            buf.insert(p, (m, u));
        }
        let mut out = Vec::new();
        let mut summ = ContextSummary::new();
        for i in 0..seq.len() {
            if seq[i] == b'C' || seq[i] == b'G' {
                emit_position(
                    b"chr1", seq, i, &buf, cpg_only, zero, thr, true, &mut summ, &mut out,
                );
            }
        }
        String::from_utf8(out).unwrap()
    }
    fn run(seq: &[u8], cov: &[(u32, u32, u32)], cpg_only: bool, zero: bool) -> String {
        run_t(seq, cov, cpg_only, zero, 0)
    }

    #[test]
    fn cpg_report_matches_perl_anchor() {
        let got = run(b"ACGTACGCGT", &[(3, 5, 0)], true, false);
        assert_eq!(
            got,
            "chr1\t2\t+\t0\t0\tCG\tCGT\n\
             chr1\t3\t-\t5\t0\tCG\tCGT\n\
             chr1\t6\t+\t0\t0\tCG\tCGC\n\
             chr1\t7\t-\t0\t0\tCG\tCGT\n\
             chr1\t8\t+\t0\t0\tCG\tCGT\n\
             chr1\t9\t-\t0\t0\tCG\tCGC\n"
        );
    }

    #[test]
    fn zero_based_subtracts_one() {
        let got = run(b"ACGTACGCGT", &[(3, 5, 0)], true, true);
        assert!(got.contains("chr1\t2\t-\t5\t0\tCG\tCGT\n"));
    }

    #[test]
    fn cx_emits_chg_chh_too() {
        let cpg = run(b"ACCAAC", &[], true, false);
        let cx = run(b"ACCAAC", &[], false, false);
        assert!(!cpg.contains("CHH") && !cpg.contains("CHG"));
        assert!(cx.contains("CHH") || cx.contains("CHG"));
    }

    #[test]
    fn last_base_excluded_and_short_tri_skipped() {
        // trailing C at the final base: len<3 + last-base guard → no emit.
        let got = run(b"AAC", &[(3, 9, 0)], true, false);
        assert_eq!(got, "");
    }

    #[test]
    fn threshold_filters_below_cutoff() {
        let got = run_t(b"ACGTACGCGT", &[(3, 2, 0)], true, false, 5);
        assert_eq!(got, ""); // coverage 2 < 5, and uncovered 0,0 also dropped
    }

    #[test]
    fn exact_report_line_bytes() {
        let got = run(b"ACGTACGCGT", &[(3, 5, 0)], true, false);
        let first = got.lines().next().unwrap();
        assert_eq!(first.matches('\t').count(), 6); // 7 fields → 6 tabs
        assert!(!first.ends_with('\t'));
        assert!(got.ends_with('\n'));
    }

    // ── Phase C: filename derivation (raw-`-o` split + gz suffix) ──

    #[test]
    fn filename_derivation() {
        // non-split: stripped stem.
        assert_eq!(
            report_name("foo", "foo", None, false, false),
            "foo.CpG_report.txt"
        );
        assert_eq!(
            report_name("foo", "foo", None, true, true),
            "foo.CX_report.txt.gz"
        );
        assert_eq!(
            summary_name("foo", "foo", None),
            "foo.cytosine_context_summary.txt"
        );
        // split: RAW `-o` + literal `.chr` infix, NO strip.
        assert_eq!(
            report_name("split", "split", Some(b"chr1"), false, false),
            "split.chrchr1.CpG_report.txt"
        );
        assert_eq!(
            report_name("split", "split", Some(b"chr1"), true, true),
            "split.chrchr1.CX_report.txt.gz"
        );
        // suffixed `-o` split → doubled suffix (C1, the extractor path).
        assert_eq!(
            report_name("foo.CpG_report.txt", "foo", Some(b"chr1"), false, false),
            "foo.CpG_report.txt.chrchr1.CpG_report.txt"
        );
        assert_eq!(
            summary_name("split", "split", Some(b"chr1")),
            "split.chrchr1.cytosine_context_summary.txt"
        );
    }

    // ── Phase D: merged / discordant cov filename derivation (V2) ──

    #[test]
    fn merged_discordant_cov_name_derivation() {
        // From the REPORT filename: strip trailing `.gz` then `.txt`, append suffix.
        assert_eq!(
            merged_cov_name("merge.CpG_report.txt", false),
            "merge.CpG_report.merged_CpG_evidence.cov"
        );
        assert_eq!(
            merged_cov_name("merge.CpG_report.txt.gz", true),
            "merge.CpG_report.merged_CpG_evidence.cov.gz"
        );
        assert_eq!(
            discordant_cov_name("merge.CpG_report.txt", false),
            "merge.CpG_report.discordant_CpG_evidence.cov"
        );
        assert_eq!(
            discordant_cov_name("merge.CpG_report.txt.gz", true),
            "merge.CpG_report.discordant_CpG_evidence.cov.gz"
        );
    }

    // ── Phase C: ReportWriter (plain / gz / empty-gz stream) ──

    #[test]
    fn report_writer_plain_round_trip() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a.txt");
        let mut w = ReportWriter::create(&p, false).unwrap();
        w.write_all(b"hello\n").unwrap();
        w.finish().unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"hello\n");
    }

    #[test]
    fn report_writer_gz_round_trip() {
        use std::io::Read;
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a.gz");
        let mut w = ReportWriter::create(&p, true).unwrap();
        w.write_all(b"hello\n").unwrap();
        w.finish().unwrap();
        let mut d = flate2::read::MultiGzDecoder::new(std::fs::File::open(&p).unwrap());
        let mut s = Vec::new();
        d.read_to_end(&mut s).unwrap();
        assert_eq!(s, b"hello\n");
    }

    #[test]
    fn report_writer_gz_empty_is_valid_stream() {
        use std::io::Read;
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("e.gz");
        let w = ReportWriter::create(&p, true).unwrap();
        w.finish().unwrap(); // no writes → valid empty-gzip stream
        let bytes = std::fs::read(&p).unwrap();
        assert!(bytes.len() >= 18 && bytes[0] == 0x1f && bytes[1] == 0x8b);
        let mut d = flate2::read::MultiGzDecoder::new(&bytes[..]);
        let mut s = Vec::new();
        d.read_to_end(&mut s).unwrap();
        assert!(s.is_empty());
    }
}
