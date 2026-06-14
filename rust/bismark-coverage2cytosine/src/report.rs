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
// The `Gz` variant is intentionally larger than `Plain` (it wraps a
// `GzEncoder`). This enum is constructed once per output file, so the size
// disparity is immaterial; boxing would only add an allocation + indirection on
// the hot write path. Suppress `large_enum_variant` (surfaced by a clippy
// toolchain bump on otherwise-unchanged code).
#[allow(clippy::large_enum_variant)]
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
    pub(crate) fn as_bytes(self) -> &'static [u8] {
        match self {
            Context::Cg => b"CG",
            Context::Chg => b"CHG",
            Context::Chh => b"CHH",
        }
    }
}

/// `sprintf "%.6f"` of `m/(m+u)*100` — the percentage string Perl writes into a
/// `.cov` line (Perl `:403`/`:418`, GpC `:918`/`:930`, merge `:1934`). Caller
/// guarantees `m + u > 0` (the threshold guard ensures coverage ≥ 1 on every
/// path that writes a `.cov` line, so there is no division by zero).
pub(crate) fn pct6(m: u32, u: u32) -> String {
    format!("{:.6}", f64::from(m) / f64::from(m + u) * 100.0)
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

/// The three `--ffs` nucleotide-context fields `(tetra_nt, penta_nt, hexa_nt)`
/// for the `C`/`G` at index `i` (Perl `:262-330`/`:507-585`/`:1421-1493`). Each
/// is the empty string when its window runs off a chromosome edge (Perl prints a
/// blank field). On the reverse strand all three are reverse-complemented.
///
/// ⚠️ Forward `hexa_nt` uses the SIGNED offset `i-2`, which is negative at `i=0,1`
/// while its guard `len ≥ i+4` can still pass → Perl wraps `substr` from the
/// string end. So the empties are gated by the **numeric `len` guards**, NOT by
/// "`perl_substr` returned empty" (the wrap returns a non-empty short slice). The
/// reverse fields never negative-wrap (the `i ≥ 3`/`i ≥ 4` guards prevent it).
/// N bytes are passed through verbatim (Perl does NOT filter N-windows, despite
/// the `--help` text — verified against live Perl v0.25.1).
fn ffs_fields(seq: &[u8], i: usize, strand: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let len = seq.len();
    if strand == b'+' {
        // Forward: tetra=substr(i,4) [len≥i+4], penta=substr(i,5) [len≥i+5],
        // hexa=substr(i-2,6) [len≥i+4; offset signed → negative-wrap at i=0,1].
        let tetra = if len >= i + 4 {
            perl_substr(seq, i as isize, 4).to_vec()
        } else {
            Vec::new()
        };
        let penta = if len >= i + 5 {
            perl_substr(seq, i as isize, 5).to_vec()
        } else {
            Vec::new()
        };
        let hexa = if len >= i + 4 {
            perl_substr(seq, i as isize - 2, 6).to_vec()
        } else {
            Vec::new()
        };
        (tetra, penta, hexa)
    } else {
        // Reverse: revcomp of substr(i-3,4) [i≥3], substr(i-4,5) [i≥4],
        // substr(i-3,6) [i≥3]. The ≥0 guards mean the offset is never negative.
        let tetra = if i >= 3 {
            revcomp(perl_substr(seq, i as isize - 3, 4))
        } else {
            Vec::new()
        };
        let penta = if i >= 4 {
            revcomp(perl_substr(seq, i as isize - 4, 5))
        } else {
            Vec::new()
        };
        let hexa = if i >= 3 {
            revcomp(perl_substr(seq, i as isize - 3, 6))
        } else {
            Vec::new()
        };
        (tetra, penta, hexa)
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
    nome: bool,
    ffs: bool,
    accumulate_summary: bool,
    summary: &mut ContextSummary,
    out: &mut Vec<u8>,
    cov_out: &mut Vec<u8>,
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
    // NOMe ACG/TCG upstream filter (Perl :387-394): inside the CpG-context
    // branch, drop any CpG whose upstream trinucleotide is neither ACG nor TCG
    // (the `-` strand `upstream` is already revcomp'd by `extract`). Runs AFTER
    // `context_reporting` (summary accumulation above) — exactly as Perl's
    // `next` follows `:381` — so the summary still counts these positions.
    if nome && context == Context::Cg && upstream != b"ACG" && upstream != b"TCG" {
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
    // --ffs (Perl :399/:414/:1524): append tetra/penta/hexamer context columns
    // (7 → 10 fields) BEFORE the newline. An edge window is an empty field
    // (nothing between tabs). Columns 1–7 are byte-unchanged. Computed only when
    // `ffs`, so the default hot path is untouched.
    if ffs {
        let (tetra, penta, hexa) = ffs_fields(seq, i, strand);
        out.push(b'\t');
        out.extend_from_slice(&tetra);
        out.push(b'\t');
        out.extend_from_slice(&penta);
        out.push(b'\t');
        out.extend_from_slice(&hexa);
    }
    out.push(b'\n');

    // NOMe `.cov` companion (Perl :402-406 / :417-421): `chr out_pos out_pos
    // %.6f m u` — a POINT coordinate (both columns = out_pos, honouring
    // --zero_based), NOT the merge cov's half-open interval. `meth + nonmeth >=
    // threshold >= 1` here (NOMe threshold is always ≥ 1), so `pct6` is safe.
    //
    // ⚠️ Gated on `nome && !ffs`: Perl nests the emit as
    //   if ($tetra) { print CYT (10-col) }            # --ffs
    //   else { if ($nome) { print CYT (7-col); print CYTCOV } else { print CYT } }
    // (`:398-425`). So under `--ffs --nome-seq` the `$tetra` branch short-circuits
    // BEFORE `print CYTCOV` → the `.NOMe.CpG.cov` is opened (because `$nome`) but
    // never written (a 0-byte file). The report line above is still the 10-col
    // ffs line; only the cov companion is suppressed. (Phase-3 dual-review
    // Critical; the Phase-1 plan flagged this as a sibling-branch interaction.)
    if nome && !ffs {
        let pct = pct6(meth, nonmeth);
        cov_out.extend_from_slice(name);
        cov_out.push(b'\t');
        cov_out.extend_from_slice(out_pos.to_string().as_bytes());
        cov_out.push(b'\t');
        cov_out.extend_from_slice(out_pos.to_string().as_bytes());
        cov_out.push(b'\t');
        cov_out.extend_from_slice(pct.as_bytes());
        cov_out.push(b'\t');
        cov_out.extend_from_slice(meth.to_string().as_bytes());
        cov_out.push(b'\t');
        cov_out.extend_from_slice(nonmeth.to_string().as_bytes());
        cov_out.push(b'\n');
    }
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
) -> (Vec<u8>, Vec<u8>) {
    let mut out: Vec<u8> = Vec::new();
    let mut cov_out: Vec<u8> = Vec::new(); // stays empty unless config.nome
    let Some(seq) = genome.get(name) else {
        return (out, cov_out);
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
                config.nome,
                config.ffs,
                accumulate_summary,
                summary,
                &mut out,
                &mut cov_out,
            );
        }
    }
    (out, cov_out)
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
    // NOMe writes a `.cov` companion alongside the core report (Perl opens
    // CYTCOV only when $nome — handle_filehandles:141-148). Non-NOMe runs emit
    // no `.cov`.
    let mut cov_w = if config.nome {
        Some(ReportWriter::create(
            &nome_cov_path(config, None),
            config.gzip,
        )?)
    } else {
        None
    };
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
                let (bytes, cov_bytes) =
                    chromosome_report_bytes(&prev, genome, &buffer, config, true, summary);
                report_w.write_all(&bytes)?;
                if let Some(cw) = cov_w.as_mut() {
                    cw.write_all(&cov_bytes)?;
                }
            }
            buffer.clear();
            seen.insert(chr.clone());
            cur_chr = Some(chr);
        }
        buffer.insert(start, (meth, nonmeth));
    }
    // Plan 06142026_empty-sample-extractor-c2c — DELIBERATE divergence from
    // Perl's "No last chromosome was defined" die. A genuine read error
    // (corrupt gzip, missing file, I/O) has ALREADY propagated via `?` on
    // `read_until`/`parse_cov_line` above; reaching `None` here means a
    // cleanly-read but EMPTY coverage file — graceful on EVERY mode. Each mode
    // yields its own SEMANTICALLY-CORRECT empty output:
    //   - standard / `--gc` (threshold 0, non-NOMe): the uncovered pass below
    //     runs → all-zero genome-wide report (what methylseq needs).
    //   - `--nome-seq`: uncovered pass SKIPPED (NOMe = covered positions only) →
    //     a correct EMPTY report; unblocks methylseq's NOMe-seq c2c step.
    //   - `threshold>0`: uncovered pass skipped → correct empty report.
    // The `--gc`/`--nome-seq` GpC pass (gpc.rs, run after this in lib.rs) is
    // itself empty-graceful (its loop no-ops + finishes empty writers), so no
    // guard is needed here. Deliberate divergence from Perl (which dies
    // "No last chromosome was defined" on empty).
    match cur_chr.take() {
        None => { /* empty-but-valid: fall through; per-mode report path handles it */ }
        Some(prev) => {
            let (bytes, cov_bytes) =
                chromosome_report_bytes(&prev, genome, &buffer, config, true, summary);
            report_w.write_all(&bytes)?;
            if let Some(cw) = cov_w.as_mut() {
                cw.write_all(&cov_bytes)?;
            }
        }
    }
    // Uncovered-chromosome pass — runs ONLY at the default threshold 0 in
    // non-NOMe mode. Mirrors Perl's 3-way branch (:708-718):
    //   if ($nome)         → skip (NOMe reports covered positions only, :708-713)
    //   elsif ($threshold>0) → skip (e.g. `--gc --coverage_threshold N`, :714)
    //   else                 → process the uncovered chromosomes (:717).
    if config.threshold == 0 && !config.nome {
        let empty: HashMap<u32, (u32, u32)> = HashMap::new();
        for name in genome.names_sorted() {
            if !seen.contains(name) {
                let (bytes, _cov) =
                    chromosome_report_bytes(name, genome, &empty, config, false, summary);
                report_w.write_all(&bytes)?;
            }
        }
    }
    report_w.finish()?;
    if let Some(cw) = cov_w {
        cw.finish()?;
    }

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
    // The final cov chromosome is flushed here (never in the loop). On a
    // cleanly-read but EMPTY coverage file, `cur_chr` is `None`.
    //
    // Plan 06142026_empty-sample-extractor-c2c (+ NOMe follow-up) — DELIBERATE
    // divergence from Perl's "No last chromosome was defined" die (same as
    // `run_single`): an empty-but-valid cov falls through on EVERY mode (a
    // genuine read error surfaced earlier via `?`). standard/`--gc` → uncovered
    // pass → all-zero report; `--nome-seq`/`threshold>0` → uncovered pass skipped
    // → correct EMPTY report; the GpC pass (gpc.rs) is itself empty-graceful.
    // `last_summary_path` is `Option<PathBuf>` — `None` when no chromosome was
    // flushed (empty + NOMe/threshold, or the degenerate zero-chromosome genome);
    // the summary is then written to the default path (below) so it always exists.
    // (methylseq uses the standard non-split path; kept consistent with `run_single`.)
    let mut last_summary_path: Option<PathBuf> = match cur_chr.take() {
        None => None,
        Some(prev) => Some(flush_split_chromosome(
            &prev, genome, &buffer, config, true, summary,
        )?),
    };
    // Uncovered-chromosome pass — same gate as `run_single`: only at the default
    // threshold 0 in non-NOMe mode (Perl's 3-way branch :708-718).
    if config.threshold == 0 && !config.nome {
        let empty: HashMap<u32, (u32, u32)> = HashMap::new();
        for name in genome.names_sorted() {
            if !seen.contains(name) {
                last_summary_path = Some(flush_split_chromosome(
                    name, genome, &empty, config, false, summary,
                )?);
            }
        }
    }

    // Full summary → the LAST chromosome reopened (others stay empty), or the
    // default summary path when no chromosome was flushed (empty + NOMe/threshold,
    // or the degenerate zero-chromosome genome) so the file always exists —
    // matching `run_single` + methylseq's required `*cytosine_context_summary.txt`.
    let summary_dest = last_summary_path.unwrap_or_else(|| summary_path(config, None));
    let mut sw = BufWriter::new(File::create(&summary_dest)?);
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
    let (bytes, cov_bytes) =
        chromosome_report_bytes(name, genome, buffer, config, accumulate_summary, summary);
    let mut w = ReportWriter::create(&report_path(config, Some(name)), config.gzip)?;
    w.write_all(&bytes)?;
    w.finish()?; // zero-emit chr: empty file (plain) / valid empty-gzip stream
    // NOMe `.cov` companion — a fresh truncating per-chr writer (no caching), so
    // a non-contiguous chromosome re-appearance re-truncates its `.cov` exactly
    // as the report does (Perl reopens GCCOV/CYTCOV with '>').
    if config.nome {
        let mut cw = ReportWriter::create(&nome_cov_path(config, Some(name)), config.gzip)?;
        cw.write_all(&cov_bytes)?;
        cw.finish()?;
    }
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
    nome: bool,
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
    // NOMe inserts `.NOMe` before the suffix (Perl :121); NOMe is CpG-context
    // only, so the suffix is always `.CpG_report.txt` under `nome`.
    let nome_infix = if nome { ".NOMe" } else { "" };
    let gzs = if gz { ".gz" } else { "" };
    format!("{base}{nome_infix}{suffix}{gzs}")
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
            config.nome,
            config.gzip
        )
    ))
}

/// The NOMe core `.cov` companion path (`{raw}[.chr{name}].NOMe.CpG.cov[.gz]`).
///
/// ⚠️ Derived from the **raw `-o`** (`output_raw`), NOT the stripped stem —
/// Perl `handle_filehandles` never suffix-strips `$cytosine_coverage_file`
/// (`:96-101,:122`), only `$cytosine_report_file` (`:104-112`). For a plain
/// `-o sample` the two bases coincide; for a `.CpG_report.txt`-suffixed `-o`
/// the cov keeps the suffix (`foo.CpG_report.txt.NOMe.CpG.cov`) while the
/// report drops it (`foo.NOMe.CpG_report.txt`). Verified against live Perl
/// v0.25.1. NOMe ✗ `--CX`, so the suffix is always `.CpG.cov`.
pub(crate) fn nome_cov_path(config: &ResolvedConfig, chr: Option<&[u8]>) -> PathBuf {
    let base = match chr {
        Some(name) => format!("{}.chr{}", config.output_raw, String::from_utf8_lossy(name)),
        None => config.output_raw.clone(),
    };
    let gzs = if config.gzip { ".gz" } else { "" };
    PathBuf::from(format!("{}{base}.NOMe.CpG.cov{gzs}", config.output_dir))
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
        false, // --merge_CpGs ✗ --nome-seq
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
        false, // --merge_CpGs ✗ --nome-seq
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
        run_nome(seq, cov, cpg_only, zero, thr, false).0
    }
    fn run(seq: &[u8], cov: &[(u32, u32, u32)], cpg_only: bool, zero: bool) -> String {
        run_t(seq, cov, cpg_only, zero, 0)
    }
    /// Drive the kernel returning `(report, cov)`. `cov` is empty unless `nome`.
    fn run_nome(
        seq: &[u8],
        cov: &[(u32, u32, u32)],
        cpg_only: bool,
        zero: bool,
        thr: u32,
        nome: bool,
    ) -> (String, String) {
        let mut buf = HashMap::new();
        for &(p, m, u) in cov {
            buf.insert(p, (m, u));
        }
        let mut out = Vec::new();
        let mut cov_out = Vec::new();
        let mut summ = ContextSummary::new();
        for i in 0..seq.len() {
            if seq[i] == b'C' || seq[i] == b'G' {
                emit_position(
                    b"chr1",
                    seq,
                    i,
                    &buf,
                    cpg_only,
                    zero,
                    thr,
                    nome,
                    false, // ffs — exercised separately via ffs_fields unit tests
                    true,
                    &mut summ,
                    &mut out,
                    &mut cov_out,
                );
            }
        }
        (
            String::from_utf8(out).unwrap(),
            String::from_utf8(cov_out).unwrap(),
        )
    }

    #[test]
    fn nome_filters_acg_tcg_upstream_and_writes_cov() {
        // TTACGTTAGCATCGTT, cov at 4 (+,upstream ACG) & 13 (+,upstream TCG):
        // both kept; the NOMe `.cov` companion is a point coord with %.6f.
        let (report, cov) = run_nome(
            b"TTACGTTAGCATCGTT",
            &[(4, 3, 1), (13, 9, 0)],
            true,
            false,
            1,
            true,
        );
        assert!(report.contains("chr1\t4\t+\t3\t1\tCG\tCGT\n"), "{report}");
        assert!(report.contains("chr1\t13\t+\t9\t0\tCG\tCGT\n"), "{report}");
        assert_eq!(
            cov,
            "chr1\t4\t4\t75.000000\t3\t1\nchr1\t13\t13\t100.000000\t9\t0\n"
        );
    }

    #[test]
    fn nome_drops_non_acg_tcg_cpg() {
        // A CpG whose upstream trinucleotide is GCG (neither ACG nor TCG) is
        // dropped under NOMe — no report line, no cov line.
        let (report, cov) = run_nome(b"GGCGTT", &[(3, 5, 0)], true, false, 1, true);
        assert_eq!(report, "");
        assert_eq!(cov, "");
    }

    // ── Phase 3: --ffs tetra/penta/hexamer offset table (live-Perl-pinned) ──

    fn ffs3(seq: &[u8], i: usize, strand: u8) -> (String, String, String) {
        let (t, p, h) = ffs_fields(seq, i, strand);
        (
            String::from_utf8(t).unwrap(),
            String::from_utf8(p).unwrap(),
            String::from_utf8(h).unwrap(),
        )
    }

    #[test]
    fn ffs_forward_interior() {
        // V1: chr1=GCCGTGAAACACGGCTTT, i=2 (pos3, +).
        let g = b"GCCGTGAAACACGGCTTT";
        assert_eq!(
            ffs3(g, 2, b'+'),
            ("CGTG".into(), "CGTGA".into(), "GCCGTG".into())
        );
    }

    #[test]
    fn ffs_forward_hexa_negative_wrap() {
        // V2: forward hexa offset i-2 is NEGATIVE at i=1,0 while its guard
        // (len≥i+4) passes → Perl wraps from the string end (NOT empty/clamped).
        let g = b"GCCGTGAAACACGGCTTT"; // len 18
        assert_eq!(
            ffs3(g, 1, b'+'),
            ("CCGT".into(), "CCGTG".into(), "T".into())
        );
        let c = b"CGTAAACCC"; // len 9
        assert_eq!(
            ffs3(c, 0, b'+'),
            ("CGTA".into(), "CGTAA".into(), "CC".into())
        );
    }

    #[test]
    fn ffs_forward_empty_windows_at_chr_end() {
        // V3: penta empty at chr-end (len < i+5); all three empty further in.
        let g = b"GCCGTGAAACACGGCTTT"; // len 18
        assert_eq!(
            ffs3(g, 14, b'+'),
            ("CTTT".into(), "".into(), "GGCTTT".into())
        );
        let c = b"CGTAAACCC"; // len 9
        assert_eq!(ffs3(c, 6, b'+'), ("".into(), "".into(), "".into()));
    }

    #[test]
    fn ffs_reverse_fields_and_empty_penta() {
        // V4: chr1 i=3 (pos4, -): revcomp'd; penta empty (guard i≥4 fails at i=3).
        let g = b"GCCGTGAAACACGGCTTT";
        assert_eq!(
            ffs3(g, 3, b'-'),
            ("CGGC".into(), "".into(), "CACGGC".into())
        );
    }

    #[test]
    fn ffs_passes_n_windows_verbatim() {
        // Perl does NOT filter N-windows (the --help "Ns ignored" claim is stale);
        // the fields carry N through (forward verbatim, reverse via revcomp's N→N).
        assert_eq!(ffs3(b"CNGTAA", 0, b'+').0, "CNGT"); // forward tetra keeps N
        // reverse: a G at i=3 (`ANCGTT`) whose tetra window spans the N → revcomp
        // leaves N (revcomp(ANCG) = CGNT), NOT filtered.
        assert_eq!(ffs_fields(b"ANCGTT", 3, b'-').0, revcomp(b"ANCG"));
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
            report_name("foo", "foo", None, false, false, false),
            "foo.CpG_report.txt"
        );
        assert_eq!(
            report_name("foo", "foo", None, true, false, true),
            "foo.CX_report.txt.gz"
        );
        assert_eq!(
            summary_name("foo", "foo", None),
            "foo.cytosine_context_summary.txt"
        );
        // split: RAW `-o` + literal `.chr` infix, NO strip.
        assert_eq!(
            report_name("split", "split", Some(b"chr1"), false, false, false),
            "split.chrchr1.CpG_report.txt"
        );
        assert_eq!(
            report_name("split", "split", Some(b"chr1"), true, false, true),
            "split.chrchr1.CX_report.txt.gz"
        );
        // suffixed `-o` split → doubled suffix (C1, the extractor path).
        assert_eq!(
            report_name(
                "foo.CpG_report.txt",
                "foo",
                Some(b"chr1"),
                false,
                false,
                false
            ),
            "foo.CpG_report.txt.chrchr1.CpG_report.txt"
        );
        assert_eq!(
            summary_name("split", "split", Some(b"chr1")),
            "split.chrchr1.cytosine_context_summary.txt"
        );
    }

    #[test]
    fn nome_report_filename_inserts_nome_infix() {
        // Phase 1: NOMe core report = `{base}.NOMe.CpG_report.txt` (stem
        // non-split, raw+.chr split). NOMe ✗ --CX, so the suffix is .CpG.
        assert_eq!(
            report_name("foo", "foo", None, false, true, false),
            "foo.NOMe.CpG_report.txt"
        );
        assert_eq!(
            report_name("foo", "foo", None, false, true, true),
            "foo.NOMe.CpG_report.txt.gz"
        );
        // suffixed `-o`: report drops the suffix to the stem, then re-adds .NOMe.CpG.
        assert_eq!(
            report_name("foo.CpG_report.txt", "foo", None, false, true, false),
            "foo.NOMe.CpG_report.txt"
        );
        // split: raw + .chr infix (no strip) + .NOMe.
        assert_eq!(
            report_name("s", "s", Some(b"chr1"), false, true, false),
            "s.chrchr1.NOMe.CpG_report.txt"
        );
    }

    #[test]
    fn nome_cov_path_uses_raw_base() {
        // The NOMe `.cov` derives from the RAW `-o` (NOT the stripped stem) —
        // the load-bearing divergence verified against live Perl v0.25.1.
        let mk = |raw: &str, stem: &str, chr: Option<&[u8]>, gz: bool| {
            let c = ResolvedConfig {
                cov_infile: PathBuf::from("in.cov"),
                output_raw: raw.to_string(),
                output_stem: stem.to_string(),
                output_dir: String::new(),
                parent_dir: PathBuf::from("."),
                genome_folder: PathBuf::from("g"),
                cpg_only: true,
                cx_context: false,
                gc_context: true,
                nome: true,
                zero_based: false,
                split_by_chromosome: chr.is_some(),
                threshold: 1,
                gzip: gz,
                merge_cpgs: false,
                discordance: None,
                drach: false,
                ffs: false,
            };
            nome_cov_path(&c, chr).to_string_lossy().into_owned()
        };
        assert_eq!(mk("sample", "sample", None, false), "sample.NOMe.CpG.cov");
        assert_eq!(mk("sample", "sample", None, true), "sample.NOMe.CpG.cov.gz");
        // suffixed `-o`: cov keeps the raw (un-stripped) base.
        assert_eq!(
            mk("foo.CpG_report.txt", "foo", None, false),
            "foo.CpG_report.txt.NOMe.CpG.cov"
        );
        // split: raw + .chr infix.
        assert_eq!(mk("s", "s", Some(b"chr1"), false), "s.chrchr1.NOMe.CpG.cov");
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
