//! `--drach` / `--m6A` — the standalone DRACH-motif (m6A) report.
//!
//! Mirrors Perl `generate_DRACH_report:1075-1383`: a **standalone early-exit
//! mode** (invoked from `lib::run` *instead of* the normal cytosine report —
//! Perl `:38-42`). It re-reads the coverage file, buffers one chromosome at a
//! time, scans each covered chromosome's genome sequence for the DRACH m6A
//! context (`D-R-A-C-H`: `D∈{A,G,T}`, `R∈{A,G}`, `A`, `C`, `H∈{A,C,T}`) on both
//! strands, looks up the measured C in the coverage map, and writes a
//! DRACH-filtered report + coverage file: `{raw-o}_DRACH_report.txt[.gz]` and
//! `{raw-o}_DRACH.cov[.gz]` (per-chromosome with `--split_by_chromosome`).
//!
//! Behaviour (all matching Perl v0.25.1, byte-identical):
//! - **always 1-based** (`--zero_based` is ignored — the DRACH subs have no
//!   `$zero` branch); the effective coverage threshold is auto-set to 1 in
//!   `validate()` (so an uncovered motif is dropped);
//! - **covered chromosomes only** (no uncovered-chromosome pass);
//! - filenames derive from the **raw `-o`** (no suffix strip);
//! - within a chromosome, **all top (`+`) lines precede all bottom (`-`) lines**
//!   (Perl runs `drach_filtering_top_strand` then `drach_filtering_bottom_strand`);
//! - the top-strand C is at 1-based `pos`; the bottom-strand C is at `pos-1`
//!   (the BS-seq cytosine anchor — Felix-confirmed correct);
//! - both the `tri_nt` and the DRACH 5-mer are extracted via `perl_substr`,
//!   because a chromosome-start motif (`pos<4`) produces a NEGATIVE offset that
//!   Perl wraps from the string end — and on the **top** strand it still emits
//!   (e.g. `ACAAA` cov@2 → `chrA 2 + 9 1 AA CAA`).
//!
//! `--drach` ignores (does not die on) `--CX` / `--merge_CpGs` — the early exit
//! drops them; `validate()` adds no DRACH-specific mutex.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use crate::cli::ResolvedConfig;
use crate::cov;
use crate::error::BismarkC2cError;
use crate::genome::Genome;
use crate::report::{self, ReportWriter, perl_substr, revcomp};

/// Standalone DRACH/m6A early-exit mode (Perl `generate_DRACH_report`).
pub fn run_drach(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError> {
    let mut reader = cov::open_cov(&config.cov_infile)?;
    if config.split_by_chromosome {
        run_drach_split(config, genome, reader.as_mut())
    } else {
        run_drach_single(config, genome, reader.as_mut())
    }
}

/// Single-file DRACH report (default / `--gzip`): one report writer + one cov
/// writer for the whole genome, **opened before the read loop** so an empty cov
/// still yields two 0-byte files (Perl opens the filehandles up front). Covered
/// chromosomes are flushed on each cov `chr`-transition (no uncovered pass).
fn run_drach_single(
    config: &ResolvedConfig,
    genome: &Genome,
    reader: &mut dyn BufRead,
) -> Result<(), BismarkC2cError> {
    let mut report_w = ReportWriter::create(&drach_report_path(config, None), config.gzip)?;
    let mut cov_w = ReportWriter::create(&drach_cov_path(config, None), config.gzip)?;
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
                let (rep, cov) = drach_chromosome_bytes(&prev, genome, &buffer, config.threshold);
                report_w.write_all(&rep)?;
                cov_w.write_all(&cov)?;
            }
            buffer.clear();
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth));
    }
    // Final flush — guarded by `Option`, so a zero-line cov writes nothing here
    // (no phantom `""`-chromosome walk; the two empty files were already opened).
    if let Some(prev) = cur_chr.take() {
        let (rep, cov) = drach_chromosome_bytes(&prev, genome, &buffer, config.threshold);
        report_w.write_all(&rep)?;
        cov_w.write_all(&cov)?;
    }
    report_w.finish()?;
    cov_w.finish()?;
    Ok(())
}

/// Per-chromosome DRACH report (`--split_by_chromosome`): a fresh truncating
/// report + cov writer per covered chromosome (an empty cov produces no files —
/// there is no chromosome to open a writer for). No uncovered pass.
fn run_drach_split(
    config: &ResolvedConfig,
    genome: &Genome,
    reader: &mut dyn BufRead,
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
                flush_drach_split_chromosome(&prev, genome, &buffer, config)?;
            }
            buffer.clear();
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth));
    }
    if let Some(prev) = cur_chr.take() {
        flush_drach_split_chromosome(&prev, genome, &buffer, config)?;
    }
    Ok(())
}

fn flush_drach_split_chromosome(
    name: &[u8],
    genome: &Genome,
    buffer: &HashMap<u32, (u32, u32)>,
    config: &ResolvedConfig,
) -> Result<(), BismarkC2cError> {
    let (rep, cov) = drach_chromosome_bytes(name, genome, buffer, config.threshold);
    let mut rw = ReportWriter::create(&drach_report_path(config, Some(name)), config.gzip)?;
    rw.write_all(&rep)?;
    rw.finish()?;
    let mut cw = ReportWriter::create(&drach_cov_path(config, Some(name)), config.gzip)?;
    cw.write_all(&cov)?;
    cw.finish()?;
    Ok(())
}

/// Build one chromosome's DRACH `(report_bytes, cov_bytes)` — top strand fully,
/// then bottom strand fully (Perl `:1166-1167`). A cov chromosome absent from
/// the genome yields no bytes (Perl's empty `while`-walk over an undef sequence).
fn drach_chromosome_bytes(
    name: &[u8],
    genome: &Genome,
    buffer: &HashMap<u32, (u32, u32)>,
    threshold: u32,
) -> (Vec<u8>, Vec<u8>) {
    let mut report_out: Vec<u8> = Vec::new();
    let mut cov_out: Vec<u8> = Vec::new();
    let Some(seq) = genome.get(name) else {
        return (report_out, cov_out);
    };
    drach_top(name, seq, buffer, threshold, &mut report_out, &mut cov_out);
    drach_bottom(name, seq, buffer, threshold, &mut report_out, &mut cov_out);
    (report_out, cov_out)
}

/// Top strand (Perl `drach_filtering_top_strand:1207-1289`): scan every `AC`,
/// DRACH-filter the forward 5-mer, look up coverage at the C (1-based `pos`),
/// emit `+` lines. The `AC` scan advances by `+1` (distinct-byte 2-mers cannot
/// self-overlap → identical match set to Perl's `/(AC)/g`).
fn drach_top(
    name: &[u8],
    seq: &[u8],
    buffer: &HashMap<u32, (u32, u32)>,
    threshold: u32,
    report_out: &mut Vec<u8>,
    cov_out: &mut Vec<u8>,
) {
    let mut i = 0usize;
    while i + 1 < seq.len() {
        if seq[i] == b'A' && seq[i + 1] == b'C' {
            let pos = (i + 2) as u32; // 1-based; the measured C is at `pos`
            // Both via perl_substr: `drach`'s offset pos-4 is NEGATIVE at the
            // chromosome start (pos<4) and Perl wraps it from the string end —
            // and the top strand still emits (`ACAAA` cov@2 → `+ AA CAA`).
            let tri = perl_substr(seq, pos as isize - 1, 3);
            let drach = perl_substr(seq, pos as isize - 4, 5);
            if is_drach_motif(drach) && tri.len() >= 3 {
                let (meth, nonmeth) = buffer.get(&pos).copied().unwrap_or((0, 0));
                if meth + nonmeth >= threshold {
                    let pct = report::pct6(meth, nonmeth);
                    push_drach_cov(cov_out, name, pos, pos, &pct, meth, nonmeth);
                    push_drach_report(report_out, name, pos, b'+', meth, nonmeth, drach, tri);
                }
            }
        }
        i += 1;
    }
}

/// Bottom strand (Perl `drach_filtering_bottom_strand:1291-1383`): scan every
/// `GT`, reverse-complement the window, DRACH-filter, look up coverage at the
/// bottom-strand C (1-based `pos-1`), emit `-` lines.
fn drach_bottom(
    name: &[u8],
    seq: &[u8],
    buffer: &HashMap<u32, (u32, u32)>,
    threshold: u32,
    report_out: &mut Vec<u8>,
    cov_out: &mut Vec<u8>,
) {
    let mut i = 0usize;
    while i + 1 < seq.len() {
        if seq[i] == b'G' && seq[i + 1] == b'T' {
            let pos = (i + 2) as u32;
            let tri = revcomp(perl_substr(seq, pos as isize - 4, 3));
            let drach = revcomp(perl_substr(seq, pos as isize - 3, 5));
            if is_drach_motif(&drach) && tri.len() >= 3 {
                let key = pos - 1; // the bottom-strand C's 1-based coordinate
                let (meth, nonmeth) = buffer.get(&key).copied().unwrap_or((0, 0));
                if meth + nonmeth >= threshold {
                    let pct = report::pct6(meth, nonmeth);
                    push_drach_cov(cov_out, name, key, key, &pct, meth, nonmeth);
                    push_drach_report(report_out, name, key, b'-', meth, nonmeth, &drach, &tri);
                }
            }
        }
        i += 1;
    }
}

/// True iff the 5-mer is a DRACH motif (positions 3 `A` / 4 `C` are guaranteed
/// by the `AC`/`GT` match, so only `D` / `R` / `H` are tested). Uses `.get()`
/// with Perl-`substr`-empty semantics — the chromosome-start wrap and
/// chromosome-end truncation can hand this a slice shorter than 5 bytes:
/// - pos-0 (`D`): missing → `"" ne 'C'` → **passes**; present → `!= b'C'`
/// - pos-1 (`R`): missing → `"" ∉ {A,G}` → **fails**; present → `b'A'`/`b'G'`
/// - pos-4 (`H`): missing → `"" ne 'G'` → **passes**; present → `!= b'G'`
fn is_drach_motif(five_mer: &[u8]) -> bool {
    let d = five_mer.first().is_none_or(|&b| b != b'C');
    let r = matches!(five_mer.get(1), Some(&b) if b == b'A' || b == b'G');
    let h = five_mer.get(4).is_none_or(|&b| b != b'G');
    d && r && h
}

/// Append a DRACH `.cov` line: `chr\tstart\tend\tpct\tmeth\tnonmeth\n` (`pct`
/// recomputed `%.6f`; both position columns equal).
fn push_drach_cov(out: &mut Vec<u8>, chr: &[u8], start: u32, end: u32, pct: &str, m: u32, u: u32) {
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

/// Append a DRACH report line: `chr\tpos\tstrand\tmeth\tnonmeth\tdrach_5mer\ttri_nt\n`.
#[allow(clippy::too_many_arguments)]
fn push_drach_report(
    out: &mut Vec<u8>,
    chr: &[u8],
    pos: u32,
    strand: u8,
    m: u32,
    u: u32,
    drach_5mer: &[u8],
    tri_nt: &[u8],
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
    out.extend_from_slice(drach_5mer);
    out.push(b'\t');
    out.extend_from_slice(tri_nt);
    out.push(b'\n');
}

// ── Filename derivation (Perl `:789-813`-style `filehandles_func`, but with the
//    `_DRACH_report.txt` / `_DRACH.cov` suffixes). Built from the RAW `-o`
//    (`output_raw`) — NO suffix strip (Perl uses `$cytosine_out` verbatim). ──

fn drach_base(config: &ResolvedConfig, chr: Option<&[u8]>) -> String {
    match chr {
        Some(name) => format!("{}.chr{}", config.output_raw, String::from_utf8_lossy(name)),
        None => config.output_raw.clone(),
    }
}

fn drach_report_path(config: &ResolvedConfig, chr: Option<&[u8]>) -> PathBuf {
    let gz = if config.gzip { ".gz" } else { "" };
    PathBuf::from(format!(
        "{}{}_DRACH_report.txt{}",
        config.output_dir,
        drach_base(config, chr),
        gz
    ))
}

fn drach_cov_path(config: &ResolvedConfig, chr: Option<&[u8]>) -> PathBuf {
    let gz = if config.gzip { ".gz" } else { "" };
    PathBuf::from(format!(
        "{}{}_DRACH.cov{}",
        config.output_dir,
        drach_base(config, chr),
        gz
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walk one chromosome (top then bottom) → `(report, cov)` strings.
    fn walk(seq: &[u8], cov: &[(u32, u32, u32)], threshold: u32) -> (String, String) {
        let mut buf: HashMap<u32, (u32, u32)> = HashMap::new();
        for &(p, m, u) in cov {
            buf.insert(p, (m, u));
        }
        let mut rep = Vec::new();
        let mut cv = Vec::new();
        drach_top(b"chr1", seq, &buf, threshold, &mut rep, &mut cv);
        drach_bottom(b"chr1", seq, &buf, threshold, &mut rep, &mut cv);
        (
            String::from_utf8(rep).unwrap(),
            String::from_utf8(cv).unwrap(),
        )
    }

    fn cfg(raw: &str, gzip: bool, split: bool) -> ResolvedConfig {
        ResolvedConfig {
            cov_infile: PathBuf::from("in.cov"),
            output_raw: raw.to_string(),
            output_stem: raw.to_string(),
            output_dir: String::new(),
            parent_dir: PathBuf::from("."),
            genome_folder: PathBuf::from("g"),
            cpg_only: true,
            cx_context: false,
            gc_context: false,
            nome: false,
            zero_based: false,
            split_by_chromosome: split,
            threshold: 1,
            gzip,
            merge_cpgs: false,
            discordance: None,
            drach: true,
        }
    }

    // ── is_drach_motif (V3) ──

    #[test]
    fn is_drach_motif_filter_arms() {
        assert!(is_drach_motif(b"GAACA")); // D=G,R=A,H=A → DRACH
        assert!(is_drach_motif(b"AAACT")); // D=A,R=A,H=T
        assert!(!is_drach_motif(b"CAACA")); // pos-0 == C → fail
        assert!(!is_drach_motif(b"GTACA")); // pos-1 == T (∉ {A,G}) → fail
        assert!(!is_drach_motif(b"GAACG")); // pos-4 == G → fail
        // non-ACGT bytes: pos-0 N passes (≠C), pos-1 N fails (∉{A,G}), pos-5 N passes (≠G)
        assert!(is_drach_motif(b"NAACA"));
        assert!(!is_drach_motif(b"GNACA"));
        assert!(is_drach_motif(b"GAACN"));
    }

    #[test]
    fn is_drach_motif_short_slices_no_panic() {
        // Perl-substr-empty semantics on <5-byte slices (the wrap / truncation).
        assert!(!is_drach_motif(b"")); // 0-byte: pos-1 missing → fail
        assert!(!is_drach_motif(b"A")); // 1-byte: pos-1 missing → fail
        assert!(is_drach_motif(b"AA")); // 2-byte: D=A pass, R=A pass, H missing → pass
        assert!(is_drach_motif(b"GACT")); // 4-byte (truncated): D=G,R=A,H missing → pass
        assert!(!is_drach_motif(b"CA")); // pos-0 == C → fail
    }

    // ── kernel anchors (live-Perl-verified by the dual plan-review) ──

    #[test]
    fn top_strand_interior_emits() {
        // ...GAACA... : AC at the C of pos 7 → `+ GAACA CAT`.
        let (rep, cv) = walk(b"TTTGAACATTT", &[(7, 4, 2)], 1);
        assert_eq!(rep, "chr1\t7\t+\t4\t2\tGAACA\tCAT\n");
        assert_eq!(cv, "chr1\t7\t7\t66.666667\t4\t2\n");
    }

    #[test]
    fn top_strand_chromosome_start_wrap_emits() {
        // rev 2 A-F1 / V15: `ACAAA` cov@2 → the wrapped drach `substr(-2,5)="AA"`
        // passes, tri `CAA` len 3 → EMITS. A naive slice would panic/diverge.
        let (rep, cv) = walk(b"ACAAA", &[(2, 9, 1)], 1);
        assert_eq!(rep, "chr1\t2\t+\t9\t1\tAA\tCAA\n");
        assert_eq!(cv, "chr1\t2\t2\t90.000000\t9\t1\n");
    }

    #[test]
    fn bottom_strand_truncated_5mer_emits_at_pos_minus_1() {
        // V10: `AAAGTA` cov@4 → GT at idx3 (pos5), bottom C at pos-1=4; the
        // revcomp'd 4-byte drach `TACT` passes (pos-5 missing), tri `CTT`.
        let (rep, cv) = walk(b"AAAGTA", &[(4, 5, 0)], 1);
        assert_eq!(rep, "chr1\t4\t-\t5\t0\tTACT\tCTT\n");
        assert_eq!(cv, "chr1\t4\t4\t100.000000\t5\t0\n");
    }

    #[test]
    fn uncovered_motif_skipped_by_threshold() {
        // A DRACH motif with no cov entry is dropped (threshold ≥ 1).
        let (rep, cv) = walk(b"TTTGAACATTT", &[], 1);
        assert_eq!(rep, "");
        assert_eq!(cv, "");
    }

    #[test]
    fn top_before_bottom_within_chromosome() {
        // A top hit (pos 7) and a bottom hit must order all `+` before all `-`.
        let (rep, _cv) = walk(b"TTTGAACATTTGTACATTT", &[(7, 4, 2), (11, 3, 3)], 1);
        let plus = rep.find("\t+\t");
        let minus = rep.find("\t-\t");
        if let (Some(p), Some(m)) = (plus, minus) {
            assert!(p < m, "all + lines must precede all - lines:\n{rep}");
        }
    }

    // ── filenames (V4) ──

    #[test]
    fn drach_filenames() {
        let p = |c: &ResolvedConfig, chr: Option<&[u8]>| {
            (
                drach_report_path(c, chr).to_string_lossy().into_owned(),
                drach_cov_path(c, chr).to_string_lossy().into_owned(),
            )
        };
        assert_eq!(
            p(&cfg("samp", false, false), None),
            ("samp_DRACH_report.txt".into(), "samp_DRACH.cov".into())
        );
        assert_eq!(
            p(&cfg("samp", true, false), None),
            (
                "samp_DRACH_report.txt.gz".into(),
                "samp_DRACH.cov.gz".into()
            )
        );
        // split: raw `-o` + `.chr<NAME>` infix (the `.chrchr1` doubling).
        assert_eq!(
            p(&cfg("samp", false, true), Some(b"chr1")),
            (
                "samp.chrchr1_DRACH_report.txt".into(),
                "samp.chrchr1_DRACH.cov".into()
            )
        );
        // suffixed `-o` is NOT stripped (raw verbatim).
        assert_eq!(
            p(&cfg("foo.CpG_report.txt", false, false), None),
            (
                "foo.CpG_report.txt_DRACH_report.txt".into(),
                "foo.CpG_report.txt_DRACH.cov".into()
            )
        );
    }
}
