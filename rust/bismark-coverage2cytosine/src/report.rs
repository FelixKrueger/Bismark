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
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use crate::cli::ResolvedConfig;
use crate::cov;
use crate::error::BismarkC2cError;
use crate::genome::Genome;
use crate::summary::ContextSummary;

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

/// Walk one chromosome's genome sequence and write its report lines. A cov
/// chromosome absent from the genome emits nothing (Perl's empty `while`-walk).
fn flush_chromosome(
    name: &[u8],
    genome: &Genome,
    buffer: &HashMap<u32, (u32, u32)>,
    config: &ResolvedConfig,
    accumulate_summary: bool,
    summary: &mut ContextSummary,
    w: &mut dyn Write,
) -> Result<(), BismarkC2cError> {
    let Some(seq) = genome.get(name) else {
        return Ok(());
    };
    let mut out: Vec<u8> = Vec::new();
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
    w.write_all(&out)?;
    Ok(())
}

/// Generate the genome-wide cytosine report + context summary (Phase B, plain).
pub fn run_report(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError> {
    // Create the output directory if a non-empty prefix was given (Perl mkdir).
    if !config.output_dir.is_empty() {
        std::fs::create_dir_all(&config.output_dir)?;
    }

    let mut reader = cov::open_cov(&config.cov_infile)?;
    let mut report_w = open_report_writer(config)?;
    let mut summary = ContextSummary::new();

    let mut seen: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
    let mut cur_chr: Option<Vec<u8>> = None;
    let mut buffer: HashMap<u32, (u32, u32)> = HashMap::new();
    let mut line: Vec<u8> = Vec::new();
    let mut line_no = 0usize;

    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        line_no += 1;
        let Some((chr, start, meth, nonmeth)) = cov::parse_cov_line(&line, line_no)? else {
            continue; // blank line
        };
        // Flush on every chromosome transition (NOT suppressed by `seen` — a
        // non-contiguous chr re-flushes + re-emits, matching Perl :227).
        if cur_chr.as_deref() != Some(chr.as_slice()) {
            if let Some(prev) = cur_chr.take() {
                flush_chromosome(
                    &prev,
                    genome,
                    &buffer,
                    config,
                    true,
                    &mut summary,
                    &mut report_w,
                )?;
            }
            buffer.clear();
            // Seed the fresh buffer with the triggering line (Perl :453-455).
            seen.insert(chr.clone());
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth)); // last-write-wins on a dup pos
    }

    // Flush the final chromosome; empty input → error before the uncovered pass.
    match cur_chr.take() {
        None => return Err(BismarkC2cError::EmptyCoverageInput),
        Some(prev) => {
            flush_chromosome(
                &prev,
                genome,
                &buffer,
                config,
                true,
                &mut summary,
                &mut report_w,
            )?;
        }
    }

    // Uncovered chromosomes: only when threshold == 0 (Perl :714), bytewise
    // sorted, all 0,0 coverage, no summary accumulation.
    if config.threshold == 0 {
        let empty: HashMap<u32, (u32, u32)> = HashMap::new();
        for name in genome.names_sorted() {
            if !seen.contains(name) {
                flush_chromosome(
                    name,
                    genome,
                    &empty,
                    config,
                    false,
                    &mut summary,
                    &mut report_w,
                )?;
            }
        }
    }
    report_w.flush()?;

    // Context summary (always written, uncompressed).
    let mut summary_w = open_summary_writer(config)?;
    summary.write_to(&mut summary_w)?;
    summary_w.flush()?;

    Ok(())
}

// ── Filename derivation + writer seam (Phase C wraps these for gzip/per-chr) ──

fn report_filename(stem: &str, cx_context: bool) -> String {
    if cx_context {
        format!("{stem}.CX_report.txt")
    } else {
        format!("{stem}.CpG_report.txt")
    }
}

fn summary_filename(stem: &str) -> String {
    format!("{stem}.cytosine_context_summary.txt")
}

/// `{output_dir}{filename}` — `output_dir` is a path *prefix* (`""` = cwd, else
/// ends with `/`), matching Perl's `"${output_dir}${file}"` concatenation.
fn output_path(config: &ResolvedConfig, filename: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", config.output_dir, filename))
}

/// Open the report writer. Phase B: `BufWriter<File>`. **Seam for Phase C**
/// (gzip wrapping + per-chromosome multiplexing).
fn open_report_writer(config: &ResolvedConfig) -> Result<Box<dyn Write>, BismarkC2cError> {
    let fname = report_filename(&config.output_stem, config.cx_context);
    let file = File::create(output_path(config, &fname))?;
    Ok(Box::new(BufWriter::new(file)))
}

fn open_summary_writer(config: &ResolvedConfig) -> Result<Box<dyn Write>, BismarkC2cError> {
    let fname = summary_filename(&config.output_stem);
    let file = File::create(output_path(config, &fname))?;
    Ok(Box::new(BufWriter::new(file)))
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

    // ── Task 7: filename derivation ──

    #[test]
    fn filename_derivation() {
        assert_eq!(report_filename("foo", false), "foo.CpG_report.txt");
        assert_eq!(report_filename("foo", true), "foo.CX_report.txt");
        assert_eq!(summary_filename("foo"), "foo.cytosine_context_summary.txt");
    }
}
