//! SAM header → ASCII chromosome-name lookup table.
//!
//! Built once per input file at `extract_se` entry. Mirrors `bismark-dedup`'s
//! refid-table pattern (`pipeline.rs:64-82`) but returns chr **names** rather
//! than interned u32 ids — the extractor's output lines embed the literal
//! chr string.
//!
//! Per Reviewer B Optional O1: non-ASCII chr names fail loudly via
//! [`BismarkExtractorError::NonAsciiChromosomeName`] rather than silently
//! UTF-8-substituting `\u{fffd}` (which would corrupt byte-identity).

use noodles_sam::Header;

use crate::extractor::error::BismarkExtractorError;

/// Build a `Vec<String>` indexed by `noodles_sam::Record::reference_sequence_id()`
/// (i.e. the 0-based insertion order of `header.reference_sequences()`).
///
/// # Errors
///
/// `BismarkExtractorError::NonAsciiChromosomeName` if any `@SQ SN:...` value
/// contains non-ASCII bytes. Bismark's downstream tools (bismark2bedGraph,
/// coverage2cytosine) cannot round-trip non-ASCII names safely.
pub fn build_chr_name_table(header: &Header) -> Result<Vec<String>, BismarkExtractorError> {
    let mut out = Vec::with_capacity(header.reference_sequences().len());
    for (name, _ref_seq) in header.reference_sequences() {
        let bytes: &[u8] = name.as_ref();
        if !bytes.is_ascii() {
            return Err(BismarkExtractorError::NonAsciiChromosomeName {
                name: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        // Safe: just ASCII-validated.
        out.push(
            String::from_utf8(bytes.to_vec())
                .expect("ASCII verified immediately above; from_utf8 cannot fail"),
        );
    }
    Ok(out)
}
