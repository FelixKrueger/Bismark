//! CRAM reference FASTA access, shared by the CRAM reader and writer.

use std::path::{Path, PathBuf};

use noodles_fasta::Repository;
use noodles_fasta::repository::adapters::IndexedReader as IndexedFastaAdapter;

use crate::io::error::BismarkIoError;

/// Build a noodles-fasta `Repository` from a FASTA path. Requires a `.fai`
/// sidecar alongside the FASTA; surfaces `MissingFastaIndex` with an
/// actionable hint if the sidecar is missing.
///
/// Shared helper used by both the CRAM reader (`crate::io::read::CramReader`)
/// and CRAM writer (`crate::io::write::CramWriter`).
pub(crate) fn build_fasta_repository(cram_ref: &Path) -> Result<Repository, BismarkIoError> {
    let mut fai_path = cram_ref.as_os_str().to_owned();
    fai_path.push(".fai");
    let fai_path = PathBuf::from(fai_path);
    if !fai_path.exists() {
        return Err(BismarkIoError::MissingFastaIndex(fai_path));
    }
    let indexed_reader =
        noodles_fasta::io::indexed_reader::Builder::default().build_from_path(cram_ref)?;
    Ok(Repository::new(IndexedFastaAdapter::new(indexed_reader)))
}
