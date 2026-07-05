//! Mode-aware output-file key + filename generation (Phase E).
//!
//! Per `PHASE_E_PLAN.md` §5.1: an [`OutputKey`] enum carries the per-mode
//! key shape, and [`mode_keys`] returns the `(key, filename)` list for
//! `OutputFileMap::new`. Ordering is load-bearing — see [`mode_keys`].
//!
//! Phase E adds the five non-`Default` output modes (Phase B implemented
//! `Default`; `MbiasOnly` returns the empty `Vec` because no per-context
//! files are emitted).

use bismark_io::BismarkStrand;

use crate::call::{CytosineContext, MethCall};
use crate::cli::OutputMode;

/// CpG vs Non-CpG categorisation used by `MergeNonCpG` modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CpGOrNonCpG {
    /// CpG context.
    CpG,
    /// CHG or CHH context — collapsed into one "non-CpG" output by
    /// `--merge_non_CpG`.
    NonCpG,
}

impl CytosineContext {
    /// Returns `CpG` for [`CytosineContext::CpG`], `NonCpG` for `CHG`/`CHH`.
    /// Used by `MergeNonCpG` routing.
    pub fn cpg_or_non_cpg(self) -> CpGOrNonCpG {
        match self {
            CytosineContext::CpG => CpGOrNonCpG::CpG,
            CytosineContext::CHG | CytosineContext::CHH => CpGOrNonCpG::NonCpG,
        }
    }
}

/// One output-file key.
///
/// The enum discriminant is the mode; payload is the mode's per-key
/// shape (see [`mode_keys`] doc). Two distinct modes never collide in a
/// `HashMap<OutputKey, _>` because the discriminant always differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputKey {
    /// `Default` mode: 12 `(context, strand)` pairs.
    Default(CytosineContext, BismarkStrand),
    /// `Comprehensive` mode: 3 keys (one per context, strand merged).
    Comprehensive(CytosineContext),
    /// `MergeNonCpG` mode: 8 `(CpG/Non_CpG, strand)` pairs.
    MergeNonCpG(CpGOrNonCpG, BismarkStrand),
    /// `Comprehensive + MergeNonCpG`: 2 keys (CpG + Non_CpG, strand merged).
    ComprehensiveMergeNonCpG(CpGOrNonCpG),
    /// `Yacht`: 1 key (single `any_C_context_*` file).
    Yacht,
}

/// Build the per-mode `(key, filename)` list for `OutputFileMap::new`.
///
/// **ORDERING IS LOAD-BEARING.** Files are opened by `OutputFileMap::new`
/// in the order returned here, and `cleanup_partial_outputs` iterates the
/// resulting HashMap. The order mirrors Perl's per-mode `open(...)`
/// reading order (`bismark_methylation_extractor:5082-5403`) so:
///   - Eager-open error messages report the same "failed at file N" as Perl.
///   - Phase H byte-identity diagnostics line up file-by-file.
///   - Cleanup-on-error deletes in a deterministic order that's stable
///     across cargo test invocations.
///
/// Documented order (matches Perl source-code order):
///   - `Default`: CpG_OT, CpG_CTOT, CpG_CTOB, CpG_OB, CHG_OT, CHG_CTOT,
///     CHG_CTOB, CHG_OB, CHH_OT, CHH_CTOT, CHH_CTOB, CHH_OB.
///   - `Comprehensive`: CpG_context, CHG_context, CHH_context.
///   - `MergeNonCpG`: CpG_OT, CpG_CTOT, CpG_CTOB, CpG_OB, Non_CpG_OT,
///     Non_CpG_CTOT, Non_CpG_CTOB, Non_CpG_OB.
///   - `ComprehensiveMergeNonCpG`: CpG_context, Non_CpG_context.
///   - `Yacht`: any_C_context.
///
/// Returns the empty `Vec` for `MbiasOnly` — no split files are emitted
/// (Perl `:5148-5151 unless($mbias_only)`).
///
/// When `gzip` is true, every filename gets a `.gz` suffix (Perl
/// `:5066 $cytosine_output .= '.gz'` etc.).
pub fn mode_keys(mode: OutputMode, basename: &str, gzip: bool) -> Vec<(OutputKey, String)> {
    const CONTEXTS: [CytosineContext; 3] = [
        CytosineContext::CpG,
        CytosineContext::CHG,
        CytosineContext::CHH,
    ];
    const STRANDS: [BismarkStrand; 4] = [
        BismarkStrand::OT,
        BismarkStrand::CTOT,
        BismarkStrand::CTOB,
        BismarkStrand::OB,
    ];
    const CLASSES: [CpGOrNonCpG; 2] = [CpGOrNonCpG::CpG, CpGOrNonCpG::NonCpG];
    let suffix = if gzip { ".txt.gz" } else { ".txt" };

    match mode {
        OutputMode::Default => {
            let mut out = Vec::with_capacity(12);
            for context in CONTEXTS {
                for strand in STRANDS {
                    let filename = format!(
                        "{ctx}_{st}_{basename}{suffix}",
                        ctx = context_prefix(context),
                        st = strand_label(strand),
                    );
                    out.push((OutputKey::Default(context, strand), filename));
                }
            }
            out
        }
        OutputMode::Comprehensive => {
            let mut out = Vec::with_capacity(3);
            for context in CONTEXTS {
                let filename = format!(
                    "{ctx}_context_{basename}{suffix}",
                    ctx = context_prefix(context),
                );
                out.push((OutputKey::Comprehensive(context), filename));
            }
            out
        }
        OutputMode::MergeNonCpG => {
            let mut out = Vec::with_capacity(8);
            for class in CLASSES {
                for strand in STRANDS {
                    let filename = format!(
                        "{cls}_{st}_{basename}{suffix}",
                        cls = class_prefix(class),
                        st = strand_label(strand),
                    );
                    out.push((OutputKey::MergeNonCpG(class, strand), filename));
                }
            }
            out
        }
        OutputMode::ComprehensiveMergeNonCpG => {
            let mut out = Vec::with_capacity(2);
            for class in CLASSES {
                let filename = format!(
                    "{cls}_context_{basename}{suffix}",
                    cls = class_prefix(class),
                );
                out.push((OutputKey::ComprehensiveMergeNonCpG(class), filename));
            }
            out
        }
        OutputMode::Yacht => vec![(
            OutputKey::Yacht,
            format!("any_C_context_{basename}{suffix}"),
        )],
        OutputMode::MbiasOnly => Vec::new(),
    }
}

/// Compute the [`OutputKey`] for a single call under the given mode.
///
/// Returns `None` for [`OutputMode::MbiasOnly`] (which has no per-call
/// routing — `route_call` short-circuits before this is consulted).
pub fn route_to_key(
    mode: OutputMode,
    context: CytosineContext,
    strand: BismarkStrand,
) -> Option<OutputKey> {
    match mode {
        OutputMode::Default => Some(OutputKey::Default(context, strand)),
        OutputMode::Comprehensive => Some(OutputKey::Comprehensive(context)),
        OutputMode::MergeNonCpG => Some(OutputKey::MergeNonCpG(context.cpg_or_non_cpg(), strand)),
        OutputMode::ComprehensiveMergeNonCpG => Some(OutputKey::ComprehensiveMergeNonCpG(
            context.cpg_or_non_cpg(),
        )),
        OutputMode::Yacht => Some(OutputKey::Yacht),
        OutputMode::MbiasOnly => None,
    }
}

/// Write one yacht-mode row to `writer` (zero-alloc per-row).
///
/// Row format (8 columns, tab-separated, terminated by LF):
/// ```text
/// read_id  meth_char  chr  ref_pos  xm_byte  col6  col7  read_orientation
/// ```
/// where:
///   - `meth_char`: `+` for methylated XM (uppercase `Z`/`X`/`H`), `-` for
///     unmethylated (lowercase). Perl `:4472` hardcodes per branch.
///   - `col6` / `col7`: strand-conditional polarity (forward-class emits
///     `(start, end)`, reverse-class emits `(end, start)`). Perl `:4350,
///     4382, 4422-4447`. Computed by caller (`route_call`); this helper
///     just writes the bytes.
///   - `read_orientation`: `+` for OT/CTOB, `-` for OB/CTOT. Perl `:1604,
///     1610` vs `:1607, 1613`.
pub fn write_yacht_row<W: std::io::Write>(
    writer: &mut W,
    record_name: &[u8],
    chr: &str,
    call: &MethCall,
    yacht_col6: u32,
    yacht_col7: u32,
    pair_strand: BismarkStrand,
) -> std::io::Result<()> {
    let meth_char: u8 = if call.methylated { b'+' } else { b'-' };
    let orient_char: u8 = orient_byte(pair_strand);
    writer.write_all(record_name)?;
    writer.write_all(b"\t")?;
    writer.write_all(&[meth_char])?;
    writer.write_all(b"\t")?;
    writer.write_all(chr.as_bytes())?;
    writer.write_all(b"\t")?;
    writer.write_all(call.ref_pos.to_string().as_bytes())?;
    writer.write_all(b"\t")?;
    writer.write_all(&[call.xm_byte])?;
    writer.write_all(b"\t")?;
    writer.write_all(yacht_col6.to_string().as_bytes())?;
    writer.write_all(b"\t")?;
    writer.write_all(yacht_col7.to_string().as_bytes())?;
    writer.write_all(b"\t")?;
    writer.write_all(&[orient_char])?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Yacht col-8 orientation byte: `+` for forward-class pair_strand
/// (OT/CTOB), `-` for reverse-class (OB/CTOT). Exposed for caller use
/// (also used internally by [`write_yacht_row`]).
pub fn orient_byte(pair_strand: BismarkStrand) -> u8 {
    match pair_strand {
        BismarkStrand::OT | BismarkStrand::CTOB => b'+',
        BismarkStrand::OB | BismarkStrand::CTOT => b'-',
    }
}

/// Strand label used in output filenames (also reused by `output.rs`'s
/// 5-column write path).
pub(crate) fn strand_label(strand: BismarkStrand) -> &'static str {
    match strand {
        BismarkStrand::OT => "OT",
        BismarkStrand::CTOT => "CTOT",
        BismarkStrand::CTOB => "CTOB",
        BismarkStrand::OB => "OB",
    }
}

/// Context prefix used in output filenames.
pub(crate) fn context_prefix(context: CytosineContext) -> &'static str {
    match context {
        CytosineContext::CpG => "CpG",
        CytosineContext::CHG => "CHG",
        CytosineContext::CHH => "CHH",
    }
}

/// CpG-or-Non-CpG prefix used in `--merge_non_CpG` filenames.
pub(crate) fn class_prefix(class: CpGOrNonCpG) -> &'static str {
    match class {
        CpGOrNonCpG::CpG => "CpG",
        CpGOrNonCpG::NonCpG => "Non_CpG",
    }
}
