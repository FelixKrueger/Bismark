//! Step I output tree: `<genome>/Bisulfite_Genome/{CT,GA}_conversion/`.

use std::path::{Path, PathBuf};

use crate::genome_prep::error::GenomePrepError;
use crate::genome_prep::logging::Logger;

/// Create the `Bisulfite_Genome/{CT,GA}_conversion/` tree under `genome_folder`.
/// If `Bisulfite_Genome/` already exists, warn and proceed (overwrite) —
/// mirrors Perl lines 633–639. Returns `(ct_dir, ga_dir)`.
pub fn create_tree(
    genome_folder: &Path,
    logger: &Logger,
) -> Result<(PathBuf, PathBuf), GenomePrepError> {
    let bisulfite = genome_folder.join("Bisulfite_Genome");
    if bisulfite.exists() {
        logger.note(&format!(
            "\nA directory called {} already exists. Already existing converted sequences \
             and/or already existing Bowtie 2, HISAT2 or Minimap2 indices will be overwritten!\n",
            bisulfite.display()
        ));
    } else {
        std::fs::create_dir(&bisulfite)?;
        logger.info(&format!(
            "Created Bisulfite Genome folder {}",
            bisulfite.display()
        ));
    }

    let ct_dir = bisulfite.join("CT_conversion");
    let ga_dir = bisulfite.join("GA_conversion");
    for d in [&ct_dir, &ga_dir] {
        if !d.exists() {
            std::fs::create_dir(d)?;
            logger.info(&format!("Created Bisulfite Genome folder {}", d.display()));
        }
    }
    Ok((ct_dir, ga_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_tree_makes_subdirs() {
        let d = tempdir().unwrap();
        let (ct, ga) = create_tree(d.path(), &Logger::new(false)).unwrap();
        assert!(ct.is_dir() && ga.is_dir());
        assert!(d.path().join("Bisulfite_Genome").is_dir());
        assert!(ct.ends_with("Bisulfite_Genome/CT_conversion"));
        assert!(ga.ends_with("Bisulfite_Genome/GA_conversion"));
    }

    #[test]
    fn create_tree_existing_dir_is_overwrite_not_error() {
        let d = tempdir().unwrap();
        std::fs::create_dir(d.path().join("Bisulfite_Genome")).unwrap();
        // Pre-existing Bisulfite_Genome → warn + proceed (no error).
        assert!(create_tree(d.path(), &Logger::new(false)).is_ok());
    }
}
