//! `--merge_CpGs` post-pass — pool top/bottom CpG strands into a single
//! dinucleotide entity. Mirrors Perl `combine_CpGs_to_single_CG_entity`
//! (`coverage2cytosine:1753-1958`).
//!
//! Runs **after** the genome-wide CpG report is written (Phase B/C). Re-reads
//! that report (gz-aware), pairs consecutive `+`/`-` strand lines (with the
//! chromosome-start resync — the historical source of bugs #98/#229), and
//! writes `*.merged_CpG_evidence.cov[.gz]`. With `--discordance_filter N`,
//! strand-discordant CpGs (Δ% > N on the `%.6f`-rounded values) go to
//! `*.discordant_CpG_evidence.cov[.gz]` instead of being merged.
//!
//! Phase A guarantees the precondition (CpG-context, non-split, no threshold),
//! so this always operates on the single `{stem}.CpG_report.txt[.gz]`.

use std::io::BufRead;

use crate::coverage2cytosine::cli::ResolvedConfig;
use crate::coverage2cytosine::cov;
use crate::coverage2cytosine::error::BismarkC2cError;
use crate::coverage2cytosine::report::{self, ReportWriter};

/// One parsed cytosine-report row (the trinucleotide field is ignored).
struct ReportRow {
    chr: Vec<u8>,
    pos: u32,
    strand: u8,
    m: u32,
    u: u32,
    context: Vec<u8>,
}

/// The 6-dp-rounded percentage Perl compares for discordance (it numifies the
/// `sprintf "%.6f"` string). A raw-f64 compare byte-diverges at the boundary.
/// Reuses [`report::pct6`] (the shared `%.6f` formatter).
fn round6(m: u32, u: u32) -> f64 {
    report::pct6(m, u).parse().expect("formatted f64 parses")
}

fn parse_u32(field: &[u8], line_no: usize) -> Result<u32, BismarkC2cError> {
    std::str::from_utf8(field)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or(BismarkC2cError::MalformedCovLine { line_no })
}

/// Parse a report line (`chr\tpos\tstrand\tm\tu\tcontext\ttri`). Blank → `None`.
fn parse_report_row(line: &[u8], line_no: usize) -> Result<Option<ReportRow>, BismarkC2cError> {
    let mut line = line;
    if line.last() == Some(&b'\n') {
        line = &line[..line.len() - 1];
    }
    if line.last() == Some(&b'\r') {
        line = &line[..line.len() - 1];
    }
    if line.is_empty() {
        return Ok(None);
    }
    let f: Vec<&[u8]> = line.split(|&b| b == b'\t').collect();
    if f.len() < 6 {
        return Err(BismarkC2cError::MalformedCovLine { line_no });
    }
    Ok(Some(ReportRow {
        chr: f[0].to_vec(),
        pos: parse_u32(f[1], line_no)?,
        strand: *f[2]
            .first()
            .ok_or(BismarkC2cError::MalformedCovLine { line_no })?,
        m: parse_u32(f[3], line_no)?,
        u: parse_u32(f[4], line_no)?,
        context: f[5].to_vec(),
    }))
}

/// Run the `--merge_CpGs` post-pass.
pub fn run_merge(config: &ResolvedConfig) -> Result<(), BismarkC2cError> {
    let report_path = report::report_path(config, None);
    let mut reader = cov::open_cov(&report_path)?;
    let zero = config.zero_based;
    let thr: u32 = if zero { 1 } else { 2 }; // pos1 < thr ⇒ chromosome-start CpG

    // Streaming row reader (skips blank lines; None at EOF).
    let mut line_no = 0usize;
    let mut next_row =
        move |reader: &mut dyn BufRead| -> Result<Option<ReportRow>, BismarkC2cError> {
            loop {
                let mut buf = Vec::new();
                if reader.read_until(b'\n', &mut buf)? == 0 {
                    return Ok(None);
                }
                line_no += 1;
                if let Some(row) = parse_report_row(&buf, line_no)? {
                    return Ok(Some(row));
                }
            }
        };

    let mut merged_w = ReportWriter::create(&report::merged_cov_path(config), config.gzip)?;
    let mut discordant_w = match config.discordance {
        Some(_) => Some(ReportWriter::create(
            &report::discordant_cov_path(config),
            config.gzip,
        )?),
        None => None,
    };

    // Perl: `while(1){ line1=<IN>; line2=<IN>; last unless ($line1 and $line2); … }`.
    while let (Some(mut r1), Some(mut r2)) =
        (next_row(reader.as_mut())?, next_row(reader.as_mut())?)
    {
        // Chromosome-start resync (Perl :1843-1883 default / :1809-1842 zero).
        // r1/r2 become Options because a resync advance may hit EOF (→ a later
        // sanity violation, matching Perl's die on undef line2).
        let mut o1 = Some(r1);
        let mut o2 = Some(r2);
        if o1.as_ref().is_some_and(|x| x.pos < thr) {
            let same_chr = o1.as_ref().unwrap().chr == o2.as_ref().unwrap().chr;
            if !same_chr {
                // Slide forward until chr1 == chr2 (or EOF).
                while let Some(r) = next_row(reader.as_mut())? {
                    o1 = o2.take();
                    o2 = Some(r);
                    if o1.as_ref().unwrap().chr == o2.as_ref().unwrap().chr {
                        break;
                    }
                }
                // If still at a chromosome-start, advance once more (Perl :1867).
                if o1.as_ref().is_none_or(|x| x.pos < thr) {
                    o1 = o2.take();
                    o2 = next_row(reader.as_mut())?;
                }
            } else {
                // Same chr but pos1 < thr: skip the orphan, re-pair (Perl :1875).
                o1 = o2.take();
                o2 = next_row(reader.as_mut())?;
            }
        }

        // Sanity asserts (Perl :1886-1897) → typed error (no panic). A `None`
        // here = report ended mid-pair during resync (Perl dies on undef).
        let (Some(a), Some(b)) = (o1, o2) else {
            return Err(BismarkC2cError::MergeCpgSanityViolation {
                detail: "report ended mid-CpG-pair (EOF during chromosome-start resync)".into(),
            });
        };
        r1 = a;
        r2 = b;
        sanity_check(&r1, &r2)?;

        // Discordance routing (Perl :1902-1932) — only when both strands measured.
        if let Some(n) = config.discordance
            && r1.m + r1.u > 0
            && r2.m + r2.u > 0
        {
            let top = round6(r1.m, r1.u);
            let bottom = round6(r2.m, r2.u);
            if (top - bottom).abs() > f64::from(n) {
                let dw = discordant_w
                    .as_mut()
                    .expect("discordant writer present when discordance set");
                let end1 = if zero { r1.pos + 1 } else { r1.pos };
                let end2 = if zero { r2.pos + 1 } else { r2.pos };
                write_cov_line(
                    dw,
                    &r1.chr,
                    r1.pos,
                    end1,
                    &report::pct6(r1.m, r1.u),
                    r1.m,
                    r1.u,
                )?;
                write_cov_line(
                    dw,
                    &r2.chr,
                    r2.pos,
                    end2,
                    &report::pct6(r2.m, r2.u),
                    r2.m,
                    r2.u,
                )?;
                continue;
            }
        }

        // Pool (Perl :1934-1952). Skip if no coverage.
        let pooled_m = r1.m + r2.m;
        let pooled_u = r1.u + r2.u;
        if pooled_m + pooled_u == 0 {
            continue;
        }
        let end = if zero { r2.pos + 1 } else { r2.pos };
        write_cov_line(
            &mut merged_w,
            &r1.chr,
            r1.pos,
            end,
            &report::pct6(pooled_m, pooled_u),
            pooled_m,
            pooled_u,
        )?;
    }

    merged_w.finish()?;
    if let Some(dw) = discordant_w {
        dw.finish()?;
    }
    Ok(())
}

/// Sanity checks mirroring Perl `:1886-1897` (die → typed error).
fn sanity_check(r1: &ReportRow, r2: &ReportRow) -> Result<(), BismarkC2cError> {
    let err = |detail: String| Err(BismarkC2cError::MergeCpgSanityViolation { detail });
    if r1.context != b"CG" {
        return err(format!(
            "line 1 context not CG: {:?}",
            String::from_utf8_lossy(&r1.context)
        ));
    }
    if r2.context != b"CG" {
        return err(format!(
            "line 2 context not CG: {:?}",
            String::from_utf8_lossy(&r2.context)
        ));
    }
    if r1.strand != b'+' || r2.strand != b'-' {
        return err("strands were not + and -".into());
    }
    if r2.pos != r1.pos + 1 {
        return err(format!(
            "positions not 1 bp apart: {} and {}",
            r1.pos, r2.pos
        ));
    }
    if r1.chr != r2.chr {
        return err("chromosome mismatch between line 1 and 2".into());
    }
    Ok(())
}

/// Write one coverage line `chr\tstart\tend\tpct\tm\tu\n` (raw chr bytes).
fn write_cov_line(
    w: &mut ReportWriter,
    chr: &[u8],
    start: u32,
    end: u32,
    pct: &str,
    m: u32,
    u: u32,
) -> Result<(), BismarkC2cError> {
    let mut line = Vec::with_capacity(chr.len() + 32);
    line.extend_from_slice(chr);
    line.push(b'\t');
    line.extend_from_slice(start.to_string().as_bytes());
    line.push(b'\t');
    line.extend_from_slice(end.to_string().as_bytes());
    line.push(b'\t');
    line.extend_from_slice(pct.as_bytes());
    line.push(b'\t');
    line.extend_from_slice(m.to_string().as_bytes());
    line.push(b'\t');
    line.extend_from_slice(u.to_string().as_bytes());
    line.push(b'\n');
    w.write_all(&line)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_report_row_fields() {
        let r = parse_report_row(b"chr1\t2\t+\t403\t400\tCG\tCGT", 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                r.chr.as_slice(),
                r.pos,
                r.strand,
                r.m,
                r.u,
                r.context.as_slice()
            ),
            (b"chr1".as_slice(), 2, b'+', 403, 400, b"CG".as_slice())
        );
    }

    #[test]
    fn parse_report_row_blank_is_none() {
        assert!(parse_report_row(b"", 1).unwrap().is_none());
        assert!(parse_report_row(b"\n", 1).unwrap().is_none());
    }

    #[test]
    fn round6_and_pct6_match_perl_sprintf() {
        assert_eq!(report::pct6(408, 400), "50.495050"); // 408/808*100
        assert!((round6(408, 400) - 50.495050).abs() < 1e-9);
        // boundary: 11/(11+9)=55% exactly; raw f64 is 55.000…007 but %.6f → 55.0
        assert_eq!(round6(11, 9), 55.0);
    }

    #[test]
    fn round6_discordance_boundary_matches_perl() {
        // 1/1 = 50%, 11/9 = 55% → rounded Δ = 5.0, NOT > 5 (Perl merges).
        let top = round6(1, 1);
        let bottom = round6(11, 9);
        assert!((top - bottom).abs() <= 5.0, "rounded Δ must not exceed 5");
        // raw f64 WOULD exceed (the trap): 55.00000000000001 - 50.0 > 5.
        let raw = (50.0_f64 - (11.0 / 20.0 * 100.0)).abs();
        assert!(
            raw > 5.0,
            "raw-f64 path diverges (this is the bug we avoid)"
        );
    }

    #[test]
    fn sanity_check_rejects_desynced_pairs() {
        // V10: each desync arm (Perl :1886-1897) must yield a typed
        // MergeCpgSanityViolation, never a panic.
        let row = |chr: &[u8], pos: u32, strand: u8, ctx: &[u8]| ReportRow {
            chr: chr.to_vec(),
            pos,
            strand,
            m: 1,
            u: 0,
            context: ctx.to_vec(),
        };
        let violation = |r: Result<(), BismarkC2cError>| {
            matches!(r, Err(BismarkC2cError::MergeCpgSanityViolation { .. }))
        };
        // A well-formed +/- adjacent CG pair passes.
        assert!(sanity_check(&row(b"chr1", 2, b'+', b"CG"), &row(b"chr1", 3, b'-', b"CG")).is_ok());
        // line-1 context not CG.
        assert!(violation(sanity_check(
            &row(b"chr1", 2, b'+', b"CHG"),
            &row(b"chr1", 3, b'-', b"CG"),
        )));
        // line-2 context not CG.
        assert!(violation(sanity_check(
            &row(b"chr1", 2, b'+', b"CG"),
            &row(b"chr1", 3, b'-', b"CHH"),
        )));
        // strands not + / -.
        assert!(violation(sanity_check(
            &row(b"chr1", 2, b'+', b"CG"),
            &row(b"chr1", 3, b'+', b"CG"),
        )));
        // positions not 1 bp apart.
        assert!(violation(sanity_check(
            &row(b"chr1", 2, b'+', b"CG"),
            &row(b"chr1", 5, b'-', b"CG"),
        )));
        // chromosome mismatch.
        assert!(violation(sanity_check(
            &row(b"chr1", 2, b'+', b"CG"),
            &row(b"chr2", 3, b'-', b"CG"),
        )));
    }
}
