//! The GpC-context report (`--gc`/`--gc_context`) + NOMe-Seq GpC filtering.
//!
//! Mirrors Perl `generate_GC_context_report:751-1073`: a SECOND genome walk —
//! run after the core report + context summary — that re-reads the same
//! coverage file and, for every `GC` dinucleotide of each *covered* chromosome,
//! emits a GpC-context per-cytosine report (`{raw}[.NOMe].GpC_report.txt[.gz]`)
//! and a companion coverage file (`{raw}[.NOMe].GpC.cov[.gz]`). Both strands of
//! a `GC` are processed together; the bottom strand (`-`) is printed before the
//! top strand (`+`).
//!
//! Differences from the core report (by design, all matching Perl):
//! - the effective threshold is `max(config.threshold, 1)` — Perl bumps it
//!   *inside* this function (`:758-761`), leaving the core report's threshold
//!   untouched, so uncovered `(0,0)` positions are never emitted;
//! - **no** uncovered-chromosome pass (covered chromosomes only — Perl's
//!   `generate_GC_context_report` never iterates `sort keys %processed`);
//! - **no** context summary (written once by the core report);
//! - **no** `--zero_based` adjustment — the GpC report/cov coordinates are
//!   always the 1-based `pos` / `pos-1` (Perl has no `$zero` branch here);
//! - filenames derive from the **raw `-o`** (`output_raw`), not the stem.
//!
//! With `--nome-seq`, CG-context GpC entries are dropped (only the non-CG
//! G-C-A / G-C-C / G-C-T sites survive — Perl `:919-922` / `:931-934`).

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use crate::cli::ResolvedConfig;
use crate::cov;
use crate::error::BismarkC2cError;
use crate::genome::Genome;
use crate::report::{self, Context, ReportWriter, classify_context, perl_substr, revcomp};

/// Generate the GpC-context report + cov (Perl `generate_GC_context_report`).
///
/// Re-reads the coverage file and walks every `GC` dinucleotide of each covered
/// chromosome's genome sequence. Always runs *after* the core report, so an
/// empty coverage file has already produced [`BismarkC2cError::EmptyCoverageInput`]
/// before this point — the GpC walk never sees one.
pub fn run_gpc(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError> {
    // Perl bumps the threshold to 1 inside this function (`:758-761`); the core
    // report already ran at the user's threshold. A LOCAL value — never mutate
    // `config.threshold` (the immutable user value).
    let gpc_threshold = config.threshold.max(1);
    let mut reader = cov::open_cov(&config.cov_infile)?;
    if config.split_by_chromosome {
        run_gpc_split(config, genome, reader.as_mut(), gpc_threshold)
    } else {
        run_gpc_single(config, genome, reader.as_mut(), gpc_threshold)
    }
}

/// Single-file GpC report (default / `--gzip`): one report writer + one cov
/// writer for the whole genome. Covered chromosomes are flushed on each cov
/// `chr`-transition; a non-contiguous re-appearance re-emits its block (the
/// same contract as the core [`crate::report`]'s `run_single`). No uncovered
/// pass, no summary.
fn run_gpc_single(
    config: &ResolvedConfig,
    genome: &Genome,
    reader: &mut dyn BufRead,
    gpc_threshold: u32,
) -> Result<(), BismarkC2cError> {
    let mut report_w = ReportWriter::create(&gpc_report_path(config, None), config.gzip)?;
    let mut cov_w = ReportWriter::create(&gpc_cov_path(config, None), config.gzip)?;
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
                let (rep, cov) =
                    gpc_chromosome_bytes(&prev, genome, &buffer, config.nome, gpc_threshold);
                report_w.write_all(&rep)?;
                cov_w.write_all(&cov)?;
            }
            buffer.clear();
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth));
    }
    if let Some(prev) = cur_chr.take() {
        let (rep, cov) = gpc_chromosome_bytes(&prev, genome, &buffer, config.nome, gpc_threshold);
        report_w.write_all(&rep)?;
        cov_w.write_all(&cov)?;
    }
    report_w.finish()?;
    cov_w.finish()?;
    Ok(())
}

/// Per-chromosome GpC report (`--split_by_chromosome`): a fresh truncating
/// report + cov writer per covered chromosome (Perl reopens `GC`/`GCCOV` with
/// `>` on every transition — `:949-958`). A non-contiguous re-appearance
/// re-truncates its per-chr files (the same contract as the core
/// `flush_split_chromosome`). No uncovered pass, no summary.
fn run_gpc_split(
    config: &ResolvedConfig,
    genome: &Genome,
    reader: &mut dyn BufRead,
    gpc_threshold: u32,
) -> Result<(), BismarkC2cError> {
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
                flush_gpc_split_chromosome(&prev, genome, &buffer, config, gpc_threshold)?;
            }
            buffer.clear();
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth));
    }
    if let Some(prev) = cur_chr.take() {
        flush_gpc_split_chromosome(&prev, genome, &buffer, config, gpc_threshold)?;
    }
    Ok(())
}

/// Split-mode flush: open a **fresh, truncating** per-chr GpC report + cov
/// writer (no caching — a re-appearance truncates), write + finish. A GC-less
/// chromosome still gets an empty pair of files (Perl opens the writers at the
/// transition regardless of emitted lines).
fn flush_gpc_split_chromosome(
    name: &[u8],
    genome: &Genome,
    buffer: &HashMap<u32, (u32, u32)>,
    config: &ResolvedConfig,
    gpc_threshold: u32,
) -> Result<(), BismarkC2cError> {
    let (rep, cov) = gpc_chromosome_bytes(name, genome, buffer, config.nome, gpc_threshold);
    let mut rw = ReportWriter::create(&gpc_report_path(config, Some(name)), config.gzip)?;
    rw.write_all(&rep)?;
    rw.finish()?;
    let mut cw = ReportWriter::create(&gpc_cov_path(config, Some(name)), config.gzip)?;
    cw.write_all(&cov)?;
    cw.finish()?;
    Ok(())
}

/// Walk one chromosome's `GC` dinucleotides → `(report_bytes, cov_bytes)`. A cov
/// chromosome absent from the genome yields no bytes (Perl's empty `while`-walk
/// over an undef sequence). The `GC` scan is non-overlapping (`/(GC)/g`): after
/// a match at index `j`, resume at `j + 2`.
fn gpc_chromosome_bytes(
    name: &[u8],
    genome: &Genome,
    buffer: &HashMap<u32, (u32, u32)>,
    nome: bool,
    gpc_threshold: u32,
) -> (Vec<u8>, Vec<u8>) {
    let mut report_out: Vec<u8> = Vec::new();
    let mut cov_out: Vec<u8> = Vec::new();
    let Some(seq) = genome.get(name) else {
        return (report_out, cov_out);
    };
    let mut j = 0usize;
    while j + 1 < seq.len() {
        if seq[j] == b'G' && seq[j + 1] == b'C' {
            emit_gpc_dinucleotide(
                name,
                seq,
                j,
                buffer,
                nome,
                gpc_threshold,
                &mut report_out,
                &mut cov_out,
            );
            j += 2; // non-overlapping: Perl `pos()` resumes one past the GC
        } else {
            j += 1;
        }
    }
    (report_out, cov_out)
}

/// The per-`GC`-dinucleotide kernel (mirrors Perl `:848-940` / `:966-1060`,
/// collapsed to one shared kernel — the two Perl blocks differ only in the
/// order of coverage-lookup vs context-classification, which is output-identical
/// because every guard is a side-effect-free skip-guard).
///
/// `j` is the 0-based index of the `G`; the C on the top strand sits at `j+1`
/// (1-based `pos = j+2`), the C on the bottom strand at the `G` (reported
/// 1-based as `pos-1`). Both `len < 3` guards and both context classifications
/// gate the WHOLE dinucleotide (a failure skips both strands); coverage and the
/// NOMe CG-skip are per-strand. Bottom emitted before top.
#[allow(clippy::too_many_arguments)]
fn emit_gpc_dinucleotide(
    name: &[u8],
    seq: &[u8],
    j: usize,
    buffer: &HashMap<u32, (u32, u32)>,
    nome: bool,
    gpc_threshold: u32,
    report_out: &mut Vec<u8>,
    cov_out: &mut Vec<u8>,
) {
    let pos = (j + 2) as u32; // 1-based, one past the C (Perl `pos $chromosomes{...}`)

    // Top strand C at 1-based `pos`: tri = substr(seq, pos-1, 3) = seq[j+1..j+4].
    let tri_top = perl_substr(seq, pos as isize - 1, 3);
    // Bottom strand C at the G (1-based `pos-1`): tri = revcomp(substr(seq, pos-4, 3)).
    // pos-4 = j-2; for j < 2 this is negative → `perl_substr`'s from-end wrap
    // yields a short slice that the `len < 3` guard drops (chromosome-start GC).
    let tri_bottom = revcomp(perl_substr(seq, pos as isize - 4, 3));

    // Both trinucleotides must be length 3, else skip the whole dinucleotide
    // (Perl `:871-872` `next`).
    if tri_top.len() < 3 || tri_bottom.len() < 3 {
        return;
    }
    // Classify both; if either is unclassifiable, skip the whole dinucleotide
    // (Perl `:896` / `:911` warn + `next`; the warn is STDERR — exempt).
    let (Some(ctx_top), Some(ctx_bottom)) =
        (classify_context(tri_top), classify_context(&tri_bottom))
    else {
        return;
    };

    // Coverage lookup (1-based keys; uncovered → 0,0). Top at `pos`, bottom at `pos-1`.
    let (m_top, u_top) = buffer.get(&pos).copied().unwrap_or((0, 0));
    let (m_bot, u_bot) = buffer.get(&(pos - 1)).copied().unwrap_or((0, 0));

    // Bottom strand first (Perl `:917-927`). `gpc_threshold >= 1`, so an
    // uncovered (0,0) position is always dropped — `pct6` never divides by zero.
    if m_bot + u_bot >= gpc_threshold && !(nome && ctx_bottom == Context::Cg) {
        let pct = report::pct6(m_bot, u_bot);
        push_gpc_cov(cov_out, name, pos - 1, pos - 1, &pct, m_bot, u_bot);
        push_gpc_report(
            report_out,
            name,
            pos - 1,
            b'-',
            m_bot,
            u_bot,
            ctx_bottom,
            &tri_bottom,
        );
    }
    // Top strand (Perl `:929-939`).
    if m_top + u_top >= gpc_threshold && !(nome && ctx_top == Context::Cg) {
        let pct = report::pct6(m_top, u_top);
        push_gpc_cov(cov_out, name, pos, pos, &pct, m_top, u_top);
        push_gpc_report(report_out, name, pos, b'+', m_top, u_top, ctx_top, tri_top);
    }
}

/// Append a GpC `.cov` line: `chr\tstart\tend\tpct\tm\tu\n`. A POINT coordinate
/// (`start == end`); **not** `--zero_based`-adjusted (Perl `:924`/`:936`).
fn push_gpc_cov(out: &mut Vec<u8>, chr: &[u8], start: u32, end: u32, pct: &str, m: u32, u: u32) {
    out.extend_from_slice(chr);
    out.push(b'\t');
    out.extend_from_slice(start.to_string().as_bytes());
    out.push(b'\t');
    out.extend_from_slice(end.to_string().as_bytes());
    out.push(b'\t');
    out.extend_from_slice(pct.as_bytes());
    out.push(b'\t');
    out.extend_from_slice(m.to_string().as_bytes());
    out.push(b'\t');
    out.extend_from_slice(u.to_string().as_bytes());
    out.push(b'\n');
}

/// Append a GpC report line: `chr\tpos\tstrand\tm\tu\tcontext\ttri\n`
/// (Perl `:925`/`:937`).
#[allow(clippy::too_many_arguments)]
fn push_gpc_report(
    out: &mut Vec<u8>,
    chr: &[u8],
    pos: u32,
    strand: u8,
    m: u32,
    u: u32,
    context: Context,
    tri: &[u8],
) {
    out.extend_from_slice(chr);
    out.push(b'\t');
    out.extend_from_slice(pos.to_string().as_bytes());
    out.push(b'\t');
    out.push(strand);
    out.push(b'\t');
    out.extend_from_slice(m.to_string().as_bytes());
    out.push(b'\t');
    out.extend_from_slice(u.to_string().as_bytes());
    out.push(b'\t');
    out.extend_from_slice(context.as_bytes());
    out.push(b'\t');
    out.extend_from_slice(tri);
    out.push(b'\n');
}

// ── Filename derivation (Perl `generate_GC_context_report` `$filehandles_func`
//    `:789-813`) ──────────────────────────────────────────────────────────────

/// GpC filename base: the **raw `-o`** (`output_raw`), + `.chr{name}` in split
/// mode, + `.NOMe` under `--nome-seq` (Perl `:795-797`). Note `output_raw` is
/// used verbatim — a `.CpG_report.txt`-suffixed `-o` is NOT stripped here
/// (`-o foo.CpG_report.txt --gc` → `foo.CpG_report.txt.GpC_report.txt`).
fn gpc_base(config: &ResolvedConfig, chr: Option<&[u8]>) -> String {
    let mut base = match chr {
        Some(name) => format!("{}.chr{}", config.output_raw, String::from_utf8_lossy(name)),
        None => config.output_raw.clone(),
    };
    if config.nome {
        base.push_str(".NOMe");
    }
    base
}

fn gpc_report_path(config: &ResolvedConfig, chr: Option<&[u8]>) -> PathBuf {
    let gz = if config.gzip { ".gz" } else { "" };
    PathBuf::from(format!(
        "{}{}.GpC_report.txt{}",
        config.output_dir,
        gpc_base(config, chr),
        gz
    ))
}

fn gpc_cov_path(config: &ResolvedConfig, chr: Option<&[u8]>) -> PathBuf {
    let gz = if config.gzip { ".gz" } else { "" };
    PathBuf::from(format!(
        "{}{}.GpC.cov{}",
        config.output_dir,
        gpc_base(config, chr),
        gz
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walk one chromosome of `seq` with the given 1-based coverage, returning
    /// `(report, cov)` as strings (mirrors `gpc_chromosome_bytes`).
    fn walk(seq: &[u8], cov: &[(u32, u32, u32)], nome: bool, thr: u32) -> (String, String) {
        let mut buf: HashMap<u32, (u32, u32)> = HashMap::new();
        for &(p, m, u) in cov {
            buf.insert(p, (m, u));
        }
        let mut report_out = Vec::new();
        let mut cov_out = Vec::new();
        let mut j = 0usize;
        while j + 1 < seq.len() {
            if seq[j] == b'G' && seq[j + 1] == b'C' {
                emit_gpc_dinucleotide(
                    b"chr1",
                    seq,
                    j,
                    &buf,
                    nome,
                    thr,
                    &mut report_out,
                    &mut cov_out,
                );
                j += 2;
            } else {
                j += 1;
            }
        }
        (
            String::from_utf8(report_out).unwrap(),
            String::from_utf8(cov_out).unwrap(),
        )
    }

    fn cfg(raw: &str, nome: bool, gzip: bool, split: bool) -> ResolvedConfig {
        let stem = raw
            .strip_suffix(".CpG_report.txt")
            .unwrap_or(raw)
            .to_string();
        ResolvedConfig {
            cov_infile: PathBuf::from("in.cov"),
            output_raw: raw.to_string(),
            output_stem: stem,
            output_dir: String::new(),
            parent_dir: PathBuf::from("."),
            genome_folder: PathBuf::from("g"),
            cpg_only: true,
            cx_context: false,
            gc_context: true,
            nome,
            zero_based: false,
            split_by_chromosome: split,
            threshold: if nome { 1 } else { 0 },
            gzip,
            merge_cpgs: false,
            discordance: None,
        }
    }

    // ── The primary live-Perl anchor (PLAN §3) ──

    #[test]
    fn gpc_primary_anchor_matches_perl() {
        // Genome AGCAGCGCATGCGGCATTAGCTAGC, cov at 6,7,8 → the documented report:
        //   chr1 6 + 10 0 CG CGC   (top of the GC at pos 8's neighbour)
        //   chr1 7 - 0  5 CG CGC   (bottom)
        //   chr1 8 + 3  1 CHH CAT  (top)
        // Bottom (7) precedes top (8) within that dinucleotide.
        let (report, cov) = walk(
            b"AGCAGCGCATGCGGCATTAGCTAGC",
            &[(6, 10, 0), (7, 0, 5), (8, 3, 1)],
            false,
            1,
        );
        assert_eq!(
            report,
            "chr1\t6\t+\t10\t0\tCG\tCGC\n\
             chr1\t7\t-\t0\t5\tCG\tCGC\n\
             chr1\t8\t+\t3\t1\tCHH\tCAT\n"
        );
        assert_eq!(
            cov,
            "chr1\t6\t6\t100.000000\t10\t0\n\
             chr1\t7\t7\t0.000000\t0\t5\n\
             chr1\t8\t8\t75.000000\t3\t1\n"
        );
    }

    #[test]
    fn gpc_nome_drops_cg_context_keeps_chh() {
        // Same fixture under NOMe: the two CG-context GpCs (pos 6, 7) are dropped;
        // only the CHH GpC at pos 8 survives (Perl :919-922 / :931-934).
        let (report, cov) = walk(
            b"AGCAGCGCATGCGGCATTAGCTAGC",
            &[(6, 10, 0), (7, 0, 5), (8, 3, 1)],
            true,
            1,
        );
        assert_eq!(report, "chr1\t8\t+\t3\t1\tCHH\tCAT\n");
        assert_eq!(cov, "chr1\t8\t8\t75.000000\t3\t1\n");
    }

    #[test]
    fn gpc_edge_guards_drop_first_and_last_gc() {
        // GCAGCTTAGC, cov at 4,5: the first GC (pos 2, bottom tri < 3 bp) and the
        // last GC (top tri < 3 bp) drop; only the interior GC emits (4 - CTG,
        // 5 + CTT).
        let (report, _cov) = walk(b"GCAGCTTAGC", &[(4, 2, 0), (5, 0, 3)], false, 1);
        assert_eq!(
            report,
            "chr1\t4\t-\t2\t0\tCHG\tCTG\n\
             chr1\t5\t+\t0\t3\tCHH\tCTT\n"
        );
    }

    #[test]
    fn gpc_gcgc_is_non_overlapping_and_consecutive() {
        // AAGCGCAA: two consecutive GCs at j=2 (pos 4) and j=4 (pos 6) — both
        // found, none double-counted. Cover all four C positions (3,4,5,6).
        let (report, _cov) = walk(
            b"AAGCGCAA",
            &[(3, 1, 0), (4, 1, 0), (5, 1, 0), (6, 1, 0)],
            false,
            1,
        );
        // pos-4 dinucleotide: bottom @3 (CTT/CHH), top @4 (CGC/CG).
        // pos-6 dinucleotide: bottom @5 (CGC/CG), top @6 (CAA/CHH).
        assert_eq!(
            report,
            "chr1\t3\t-\t1\t0\tCHH\tCTT\n\
             chr1\t4\t+\t1\t0\tCG\tCGC\n\
             chr1\t5\t-\t1\t0\tCG\tCGC\n\
             chr1\t6\t+\t1\t0\tCHH\tCAA\n"
        );
    }

    #[test]
    fn gpc_uncovered_positions_dropped_by_threshold() {
        // gpc_threshold >= 1 always drops uncovered (0,0) GpCs even at the core
        // default threshold 0 (which the --gc walk locally bumps to 1).
        let (report, cov) = walk(b"AGCAGCGCATGCGGCATTAGCTAGC", &[], false, 1);
        assert_eq!(report, "");
        assert_eq!(cov, "");
    }

    // ── Filename shapes (live-Perl-confirmed, PLAN §3.4) ──

    #[test]
    fn gpc_filenames_plain_gzip_split_nome_and_raw_suffix() {
        let p = |c: &ResolvedConfig, chr: Option<&[u8]>| {
            (
                gpc_report_path(c, chr).to_string_lossy().into_owned(),
                gpc_cov_path(c, chr).to_string_lossy().into_owned(),
            )
        };
        // -o sample --gc
        assert_eq!(
            p(&cfg("sample", false, false, false), None),
            ("sample.GpC_report.txt".into(), "sample.GpC.cov".into())
        );
        // --gc --gzip
        assert_eq!(
            p(&cfg("sample", false, true, false), None),
            (
                "sample.GpC_report.txt.gz".into(),
                "sample.GpC.cov.gz".into()
            )
        );
        // --gc --split_by_chromosome (the .chrchr1 doubling)
        assert_eq!(
            p(&cfg("sample", false, false, true), Some(b"chr1")),
            (
                "sample.chrchr1.GpC_report.txt".into(),
                "sample.chrchr1.GpC.cov".into()
            )
        );
        // --nome-seq (the .NOMe infix)
        assert_eq!(
            p(&cfg("sample", true, false, false), None),
            (
                "sample.NOMe.GpC_report.txt".into(),
                "sample.NOMe.GpC.cov".into()
            )
        );
        // -o sample.CpG_report.txt --gc → GpC uses the RAW (un-stripped) -o.
        assert_eq!(
            p(&cfg("sample.CpG_report.txt", false, false, false), None),
            (
                "sample.CpG_report.txt.GpC_report.txt".into(),
                "sample.CpG_report.txt.GpC.cov".into()
            )
        );
    }
}
