//! Step IV (opt, `--combined_genome`): the Bismark-Rust combined-reference
//! extension. Additive — runs after the standard CT/GA outputs + indices.
//!
//! Writes `Bisulfite_Genome/Combined/genome_mfa.combined.fa` (all CT-converted
//! records, then all GA-converted records — built directly from the converted
//! stream, so it is well-defined in both MFA and `--single_fasta` modes) and
//! builds one combined index. **Not** byte-gated vs Perl (no counterpart);
//! alignment-correctness validation is deferred to the future aligner rewrite.

use std::path::{Path, PathBuf};

use crate::cli::Aligner;
use crate::error::GenomePrepError;
use crate::{convert, indexer};

/// Build the combined reference + index under `combined_dir`.
#[allow(clippy::too_many_arguments)]
pub fn build(
    files: &[PathBuf],
    combined_dir: &Path,
    indexer_bin: &Path,
    aligner: Aligner,
    threads: u32,
    large_index: bool,
    slam: bool,
) -> Result<(), GenomePrepError> {
    std::fs::create_dir_all(combined_dir)?;
    let combined_fa = combined_dir.join("genome_mfa.combined.fa");
    convert::write_combined(files, &combined_fa, slam)?;
    indexer::run_one(
        indexer_bin,
        aligner,
        combined_dir,
        "BS_combined",
        threads,
        large_index,
    )?;
    Ok(())
}
