//! M-bias.txt writer (Phase D).
//!
//! Consumes the `[MbiasTable; 2]` accumulator populated by Phases B + C.
//! Byte-identity-targeted at Perl `bismark_methylation_extractor` lines
//! 628-836 (`produce_mbias_plots` — name is historical; the sub writes
//! both M-bias.txt and the optional PNG plots, which are deferred in the
//! Rust port).
//!
//! ## Output topology (SPEC §4.2, rev 3 correction)
//!
//! - 3 sections for SE (CpG/CHG/CHH × R1/SE only).
//! - 6 sections for PE (CpG/CHG/CHH × R1, then CpG/CHG/CHH × R2).
//! - Each section: section header line + equals-rule line + column header
//!   line + per-position rows (1-based, all positions 1..=max_position) +
//!   one trailing blank line.
//! - Per-position row: 5 tab-separated columns —
//!   `position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n`.
//! - Zero-coverage rows render `% methylation` as an empty string (literal
//!   `\t\t` between unmeth and coverage).
//!
//! ## Filename derivation
//!
//! See [`derive_mbias_basename`]. **Different** from
//! [`crate::pipeline::derive_basename`]: this one mirrors Perl's
//! `s/X$//` regex chain that preserves the trailing dot.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::call::CytosineContext;
use crate::mbias::MbiasTable;

/// Strip the leading path components and known Bismark suffixes from
/// `input` per Perl `bismark_methylation_extractor:632-642`.
///
/// **5 sequential strip attempts** (`gz`, `sam`, `bam`, `cram`, `txt`), in
/// that order; each attempt is run exactly once, replacing the running
/// string if it matches. **Preserves the trailing `.`** that's left after
/// stripping (Perl regex `s/X$//` removes the suffix without the dot).
///
/// **Differs from [`crate::pipeline::derive_basename`]**: that one strips
/// `.bam`/`.sam`/`.cram` (single suffix, INCLUDING the dot), yielding
/// `sample` for `sample.bam`. This one strips `bam`/`sam`/`cram`/`txt`/`gz`
/// (without the dot), yielding `sample.` for `sample.bam`. The divergence
/// is mandated by Perl's distinct regex chains for split-file naming
/// (`s/bam$/txt/` → `sample.txt`) vs M-bias-naming (`s/bam$//` + append
/// `M-bias.txt` → `sample.M-bias.txt`).
///
/// # Examples
///
/// | Input             | Strip chain                       | Output         |
/// |-------------------|-----------------------------------|----------------|
/// | `sample.bam`      | `bam` matches                     | `sample.`      |
/// | `sample.bam.gz`   | `gz` strips → `sample.bam.`       | `sample.bam.`  |
/// | `sample.sam.gz`   | `gz` strips → `sample.sam.`       | `sample.sam.`  |
/// | `sample.cram`     | `cram` strips                     | `sample.`      |
/// | `sample.txt`      | `txt` strips                      | `sample.`      |
/// | `sample`          | no strip                          | `sample`       |
/// | `foo.txt.bam`     | `bam` strips → `foo.txt.`         | `foo.txt.`     |
/// | `foo.bam.txt`     | `txt` strips → `foo.bam.`         | `foo.bam.`     |
///
/// **Trailing-dot stop semantic** (Reviewer B Low, Phase D rev 2): after a
/// strip lands a trailing `.`, no subsequent same-style strip in the chain
/// will match — Rust's `strip_suffix("X")` requires literal `X` at end of
/// string (matches Perl's `s/X$//`), and `.` ≠ `bam`/`sam`/`cram`/`txt`/`gz`.
/// So each input strips **at most one suffix per `.`-bounded segment**.
/// E.g. `foo.txt.bam` peels `bam` to `foo.txt.`; the subsequent `txt`
/// attempt sees `foo.txt.` (tail is `.`, not `txt`) and skips. This is the
/// Perl-faithful behaviour; do NOT "fix" the loop to peel until fixed-point.
pub fn derive_mbias_basename(input: &Path) -> String {
    let filename = input
        .file_name()
        .expect("input path must have a filename component")
        .to_string_lossy()
        .into_owned();
    // 5 sequential strip attempts, each exactly once, in Perl order.
    let mut s = filename;
    for suffix in ["gz", "sam", "bam", "cram", "txt"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            s = stripped.to_string();
        }
    }
    s
}

/// Full path to `M-bias.txt` for the given input + output directory.
///
/// Concatenation: `{output_dir}/{derive_mbias_basename(input)}M-bias.txt`.
/// E.g. for `output_dir = "out/"` and `input = "/abs/sample.bam"`, the
/// returned path is `out/sample.M-bias.txt` (the trailing `.` from the
/// basename derivation lands between `sample` and `M-bias.txt`).
pub fn mbias_txt_path(output_dir: &Path, input: &Path) -> PathBuf {
    let basename = derive_mbias_basename(input);
    output_dir.join(format!("{basename}M-bias.txt"))
}

/// Which read-identity slot a section corresponds to. Drives the header
/// text for that section.
#[derive(Debug, Clone, Copy)]
enum ReadIdentitySection {
    /// R1-or-SE slot. When `is_paired=true`, headers read
    /// `"{ctx} context (R1)\n================\n"` (16 equals); when
    /// `is_paired=false`, headers read `"{ctx} context\n===========\n"`
    /// (11 equals).
    R1OrSe { is_paired: bool },
    /// R2 slot (PE only). Headers read
    /// `"{ctx} context (R2)\n================\n"` (16 equals).
    R2,
}

/// Write the full `M-bias.txt` file at `path`.
///
/// Emits 3 sections for SE (`is_paired = false`) or 6 sections for PE
/// (`is_paired = true`). Section iteration: `[CpG, CHG, CHH]` for R1/SE,
/// then (PE only) `[CpG, CHG, CHH]` for R2.
///
/// # Errors
///
/// Propagates `std::io::Error` from disk operations. The caller (`state
/// .finalize`) wraps this as `BismarkExtractorError::IoWrite` via `?`.
pub fn write_mbias_txt(
    path: &Path,
    mbias: &[MbiasTable; 2],
    is_paired: bool,
) -> Result<(), std::io::Error> {
    let mut w = BufWriter::with_capacity(8 * 1024, File::create(path)?);

    // R1 (SE) or R1 (PE) sections.
    let max_1 = mbias[0].max_position();
    write_three_sections(
        &mut w,
        &mbias[0],
        max_1,
        ReadIdentitySection::R1OrSe { is_paired },
    )?;

    // R2 sections (PE only).
    if is_paired {
        let max_2 = mbias[1].max_position();
        write_three_sections(&mut w, &mbias[1], max_2, ReadIdentitySection::R2)?;
    }

    w.flush()
}

/// Emit the 3 sections (CpG, CHG, CHH) for one read-identity slot.
fn write_three_sections<W: Write>(
    w: &mut W,
    table: &MbiasTable,
    max_position: u32,
    identity: ReadIdentitySection,
) -> Result<(), std::io::Error> {
    for &context in &[
        CytosineContext::CpG,
        CytosineContext::CHG,
        CytosineContext::CHH,
    ] {
        write_one_section(w, table, context, max_position, identity)?;
    }
    Ok(())
}

/// Emit one section for `(context, identity)`. Section structure:
/// header line + equals-rule line + column header + N rows + blank line.
fn write_one_section<W: Write>(
    w: &mut W,
    table: &MbiasTable,
    context: CytosineContext,
    max_position: u32,
    identity: ReadIdentitySection,
) -> Result<(), std::io::Error> {
    // Section header.
    let context_str = context_name(context);
    match identity {
        ReadIdentitySection::R1OrSe { is_paired: false } => {
            // 11 equals (matches "CpG context" / "CHG context" / "CHH context" — all 11 chars).
            writeln!(w, "{context_str} context")?;
            writeln!(w, "===========")?;
        }
        ReadIdentitySection::R1OrSe { is_paired: true } => {
            // 16 equals (matches "CpG context (R1)" / "CHG context (R1)" / "CHH context (R1)" — all 16 chars).
            writeln!(w, "{context_str} context (R1)")?;
            writeln!(w, "================")?;
        }
        ReadIdentitySection::R2 => {
            // 16 equals (matches "CpG context (R2)" — 16 chars).
            writeln!(w, "{context_str} context (R2)")?;
            writeln!(w, "================")?;
        }
    }

    // Column header (5 columns per SPEC §4.2 rev 3 / Perl :729).
    writeln!(
        w,
        "position\tcount methylated\tcount unmethylated\t% methylation\tcoverage"
    )?;

    // Per-position rows: 1..=max_position, all positions emitted including
    // zero-coverage ones. If max_position == 0, this loop is empty —
    // matches Perl's `foreach my $pos (1..0)` empty-range semantic.
    let vec = match context {
        CytosineContext::CpG => &table.cpg,
        CytosineContext::CHG => &table.chg,
        CytosineContext::CHH => &table.chh,
    };
    for pos in 1..=max_position {
        let cell = vec.get(pos as usize).copied().unwrap_or_default();
        let meth = cell.meth;
        let un = cell.unmeth;
        let coverage = meth.saturating_add(un);
        // % methylation: `%.2f` when coverage > 0, empty string otherwise.
        // Perl `:740-743`: `$percent = ''` initially; only overwritten if
        // (meth+un > 0). Zero-coverage rows render `\t\t` between unmeth
        // and coverage.
        if coverage > 0 {
            let percent = (meth as f64) * 100.0 / (coverage as f64);
            writeln!(w, "{pos}\t{meth}\t{un}\t{percent:.2}\t{coverage}")?;
        } else {
            writeln!(w, "{pos}\t{meth}\t{un}\t\t{coverage}")?;
        }
    }

    // Trailing blank line after the section (Perl :762: `print MBIAS "\n";`).
    writeln!(w)?;

    Ok(())
}

/// Context label as it appears in M-bias.txt section headers ("CpG", "CHG", "CHH").
fn context_name(context: CytosineContext) -> &'static str {
    match context {
        CytosineContext::CpG => "CpG",
        CytosineContext::CHG => "CHG",
        CytosineContext::CHH => "CHH",
    }
}
