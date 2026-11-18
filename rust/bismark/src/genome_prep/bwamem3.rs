//! `--bwamem3`: a bwa-mem3 methylation index of the unconverted genome in
//! `<genome>/Bwamem3_Index/`, pruned to the files `bwa-mem3 mem --meth` opens.
//!
//! bwa-mem3 converts the genome itself, so no CT/GA copies are written. Each run builds in its
//! own `Bwamem3_Index.partial.<pid>/` and swaps the result in only once it is complete.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::aligner::aligner::{
    MIN_BWAMEM3_VERSION, VALIDATED_BWAMEM3_VERSIONS, bwamem3_version_ok, parse_bwamem3_version,
};
use crate::aligner::config::BWAMEM3_INDEX_SUFFIXES;
use crate::genome_prep::convert::open_fasta;
use crate::genome_prep::discovery::extract_chromosome_name;
use crate::genome_prep::error::GenomePrepError;
use crate::genome_prep::logging::Logger;

/// The index folder inside the genome folder; the aligner looks for it by this name.
pub const INDEX_DIR: &str = "Bwamem3_Index";
/// The index prefix inside [`INDEX_DIR`] (no FASTA is kept under this name).
pub const INDEX_PREFIX_NAME: &str = "genome.fa";
const PARTIAL_PREFIX: &str = "Bwamem3_Index.partial.";
const OLD_PREFIX: &str = "Bwamem3_Index.old.";

/// What staging learned about the genome.
#[derive(Debug)]
struct Staged {
    /// Contig names in file order.
    names: Vec<String>,
    /// Total sequence length (bases).
    total_len: u64,
}

/// Reject bwa-mem3 index files that discovery picked up as genome FASTA.
pub fn reject_index_files(files: &[PathBuf]) -> Result<(), GenomePrepError> {
    if let Some(f) = files.iter().find(|f| {
        f.file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".meth.fa"))
    }) {
        return Err(GenomePrepError::Validation(format!(
            "{} is a bwa-mem3 index file, not genome; remove it from the genome folder",
            f.display()
        )));
    }
    Ok(())
}

/// Run `<bin> version`; fail below [`MIN_BWAMEM3_VERSION`], warn outside the validated set.
pub(crate) fn check_version(bin: &Path, logger: &Logger) -> Result<String, GenomePrepError> {
    let floor = format!(
        "{}.{}.{}",
        MIN_BWAMEM3_VERSION.0, MIN_BWAMEM3_VERSION.1, MIN_BWAMEM3_VERSION.2
    );
    let out = Command::new(bin).arg("version").output().map_err(|e| {
        GenomePrepError::Validation(format!("could not run `{} version`: {e}", bin.display()))
    })?;
    if !out.status.success() {
        return Err(GenomePrepError::Validation(format!(
            "`{} version` failed ({})",
            bin.display(),
            out.status
        )));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let Some(version) = parse_bwamem3_version(&stdout) else {
        return Err(GenomePrepError::Validation(format!(
            "`bwa-mem3 version` printed no version ({}); --bwamem3 needs bwa-mem3 >= {floor}",
            bin.display()
        )));
    };
    if !bwamem3_version_ok(&version) {
        return Err(GenomePrepError::Validation(format!(
            "bwa-mem3 {version} is too old: --bwamem3 needs bwa-mem3 >= {floor}"
        )));
    }
    if !VALIDATED_BWAMEM3_VERSIONS.contains(&version.as_str()) {
        logger.note(&format!(
            "Warning: detected bwa-mem3 {version}; Bismark is validated against {}.",
            VALIDATED_BWAMEM3_VERSIONS.join(" and ")
        ));
    }
    Ok(version)
}

/// Build the index; returns its prefix `<genome>/Bwamem3_Index/genome.fa`.
pub fn build(
    files: &[PathBuf],
    genome_folder: &Path,
    bin: &Path,
    threads: Option<u32>,
    max_memory: Option<&str>,
    logger: &Logger,
) -> Result<PathBuf, GenomePrepError> {
    let partial = genome_folder.join(format!("{PARTIAL_PREFIX}{}", std::process::id()));
    for entry in fs::read_dir(genome_folder)?.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(PARTIAL_PREFIX) && entry.path() != partial {
            logger.note(&format!(
                "Note: {} exists: another bwa-mem3 index build may be running here, or one was \
                 killed. It is left alone; remove it if it is stale.",
                entry.path().display()
            ));
        }
    }
    // Same PID as a killed earlier run (PIDs repeat in containers): that folder is ours to clear.
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    fs::create_dir(&partial)?;

    let built = stage_and_index(
        files,
        genome_folder,
        &partial,
        bin,
        threads,
        max_memory,
        logger,
    )
    .and_then(|()| publish(&partial, genome_folder, logger));
    if built.is_err() {
        let _ = fs::remove_dir_all(&partial);
    }
    built
}

fn stage_and_index(
    files: &[PathBuf],
    genome_folder: &Path,
    partial: &Path,
    bin: &Path,
    threads: Option<u32>,
    max_memory: Option<&str>,
    logger: &Logger,
) -> Result<(), GenomePrepError> {
    let staged = stage_input(files, partial)?;
    logger.note(&format!(
        "Indexing {} bp in {} contigs with `bwa-mem3 index --meth`. A human genome needs about \
         104 GB of RAM and up to 70 GB of free disk; bwa-mem3 prints its exact memory requirement.",
        staged.total_len,
        staged.names.len()
    ));
    run_index(bin, partial, threads, max_memory, genome_folder)?;
    prune_and_check(partial, &staged.names, genome_folder)
}

/// Put the genome at `<dir>/genome.fa` (a link to a single file, else a decompressed
/// concatenation) and check its contig names.
fn stage_input(files: &[PathBuf], dir: &Path) -> Result<Staged, GenomePrepError> {
    let target = dir.join(INDEX_PREFIX_NAME);
    let mut out = if files.len() == 1 && cfg!(unix) {
        link(&fs::canonicalize(&files[0])?, &target)?;
        None
    } else {
        Some(BufWriter::new(File::create(&target)?))
    };

    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut total_len = 0u64;
    let mut line = Vec::new();
    for file in files {
        let mut reader = open_fasta(file)?;
        let mut first = true;
        let mut last_newline = true;
        loop {
            line.clear();
            if reader.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            if first {
                extract_chromosome_name(&line, file)?;
                first = false;
            }
            if line[0] == b'>' {
                let name = extract_chromosome_name(&line, file)?;
                if name.is_empty() {
                    return Err(GenomePrepError::Validation(format!(
                        "{}: header `{}` has no sequence name right after `>`; bwa-mem3 and \
                         Bismark would name this contig differently",
                        file.display(),
                        String::from_utf8_lossy(&line).trim_end()
                    )));
                }
                let name = String::from_utf8_lossy(name).into_owned();
                if !seen.insert(name.clone()) {
                    return Err(GenomePrepError::DuplicateChromosome(name));
                }
                names.push(name);
            } else {
                total_len += line
                    .iter()
                    .filter(|&&b| !matches!(b, b'\n' | b'\r'))
                    .count() as u64;
            }
            if let Some(w) = out.as_mut() {
                w.write_all(&line)?;
            }
            last_newline = line.last() == Some(&b'\n');
        }
        if first {
            return Err(GenomePrepError::NotFasta(file.clone()));
        }
        if let Some(w) = out.as_mut()
            && !last_newline
        {
            w.write_all(b"\n")?;
        }
    }
    if let Some(mut w) = out {
        w.flush()?;
    }
    Ok(Staged { names, total_len })
}

#[cfg(unix)]
fn link(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(not(unix))]
fn link(_src: &Path, _dst: &Path) -> std::io::Result<()> {
    unreachable!("single-file genomes are copied off unix")
}

/// `bwa-mem3 index --meth` with its scratch inside `dir`.
fn run_index(
    bin: &Path,
    dir: &Path,
    threads: Option<u32>,
    max_memory: Option<&str>,
    genome_folder: &Path,
) -> Result<(), GenomePrepError> {
    let tmp = dir.join("tmp");
    fs::create_dir(&tmp)?;
    let mut cmd = Command::new(bin);
    cmd.current_dir(dir).arg("index").arg("--meth");
    if let Some(t) = threads {
        cmd.arg("-t").arg(t.to_string());
    }
    if let Some(m) = max_memory {
        cmd.arg("--max-memory").arg(m);
    }
    cmd.arg("--tmp-dir").arg(&tmp).arg(INDEX_PREFIX_NAME);
    let status = cmd.status().map_err(|e| GenomePrepError::Bwamem3Failed {
        genome: genome_folder.to_path_buf(),
        detail: format!("could not run {}: {e}", bin.display()),
    })?;
    match status.code() {
        Some(0) => Ok(()),
        Some(3) => Err(GenomePrepError::Bwamem3Memory {
            genome: genome_folder.to_path_buf(),
        }),
        _ => Err(GenomePrepError::Bwamem3Failed {
            genome: genome_folder.to_path_buf(),
            detail: format!("`bwa-mem3 index --meth` {status}"),
        }),
    }
}

/// Drop what `mem --meth` never opens, then require the 7 index files and matching contigs.
fn prune_and_check(
    dir: &Path,
    names: &[String],
    genome_folder: &Path,
) -> Result<(), GenomePrepError> {
    let failed = |detail: String| GenomePrepError::Bwamem3Failed {
        genome: genome_folder.to_path_buf(),
        detail,
    };
    for name in [
        INDEX_PREFIX_NAME.to_string(),
        format!("{INDEX_PREFIX_NAME}.meth.fa"),
        format!("{INDEX_PREFIX_NAME}.bwt.2bit.64"),
    ] {
        match fs::remove_file(dir.join(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    match fs::remove_dir_all(dir.join("tmp")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    for suffix in BWAMEM3_INDEX_SUFFIXES {
        if !dir.join(format!("{INDEX_PREFIX_NAME}.{suffix}")).is_file() {
            return Err(failed(format!(
                "bwa-mem3 did not write {INDEX_PREFIX_NAME}.{suffix}"
            )));
        }
    }
    let ann = crate::aligner::read_bwa_ann(&dir.join(INDEX_PREFIX_NAME))
        .map_err(|e| failed(e.to_string()))?;
    let ann_names: Vec<&str> = ann.iter().map(|(n, _)| n.as_str()).collect();
    if ann_names != names.iter().map(String::as_str).collect::<Vec<_>>() {
        let first_diff = ann_names
            .iter()
            .zip(names)
            .find(|(a, b)| **a != b.as_str())
            .map(|(a, b)| format!("; first difference: `{a}` in the index, `{b}` in the genome"))
            .unwrap_or_default();
        return Err(failed(format!(
            "the index lists {} contigs, the genome {}{first_diff}",
            ann_names.len(),
            names.len()
        )));
    }
    Ok(())
}

/// Move `partial` to `<genome>/Bwamem3_Index/`, swapping out any existing index.
fn publish(
    partial: &Path,
    genome_folder: &Path,
    logger: &Logger,
) -> Result<PathBuf, GenomePrepError> {
    let dest = genome_folder.join(INDEX_DIR);
    if dest.exists() {
        logger.note(&format!(
            "Replacing the existing bwa-mem3 index in {}",
            dest.display()
        ));
        let old = genome_folder.join(format!("{OLD_PREFIX}{}", std::process::id()));
        if old.exists() {
            fs::remove_dir_all(&old)?;
        }
        fs::rename(&dest, &old)?;
        if let Err(e) = fs::rename(partial, &dest) {
            let _ = fs::rename(&old, &dest);
            return Err(e.into());
        }
        if let Err(e) = fs::remove_dir_all(&old) {
            logger.note(&format!(
                "Warning: could not remove the previous index {}: {e}",
                old.display()
            ));
        }
    } else {
        fs::rename(partial, &dest)?;
    }
    Ok(dest.join(INDEX_PREFIX_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn gz(path: &Path, content: &[u8]) {
        let mut e = flate2::write::GzEncoder::new(
            File::create(path).unwrap(),
            flate2::Compression::default(),
        );
        e.write_all(content).unwrap();
        e.finish().unwrap();
    }

    fn ann(dir: &Path, contigs: &[&str]) {
        let mut s = format!("{} {} 11\n", contigs.len() * 4, contigs.len());
        for (i, c) in contigs.iter().enumerate() {
            s.push_str(&format!("0 {c} (null)\n{} 4 0\n", i * 4));
        }
        fs::write(dir.join(format!("{INDEX_PREFIX_NAME}.ann")), s).unwrap();
    }

    fn all_index_files(dir: &Path, contigs: &[&str]) {
        for s in BWAMEM3_INDEX_SUFFIXES {
            fs::write(dir.join(format!("{INDEX_PREFIX_NAME}.{s}")), b"").unwrap();
        }
        ann(dir, contigs);
    }

    #[cfg(unix)]
    #[test]
    fn single_plain_fasta_is_linked() {
        let g = tempdir().unwrap();
        let p = tempdir().unwrap();
        let fa = g.path().join("chr.fa");
        fs::write(&fa, b">chr1 desc\nACGT\nAC\n>chr2\nGG\n").unwrap();
        let s = stage_input(std::slice::from_ref(&fa), p.path()).unwrap();
        let t = p.path().join(INDEX_PREFIX_NAME);
        assert!(fs::symlink_metadata(&t).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&t).unwrap(), fs::canonicalize(&fa).unwrap());
        assert_eq!(s.names, ["chr1", "chr2"]);
        assert_eq!(s.total_len, 8);
    }

    #[cfg(unix)]
    #[test]
    fn single_gz_fasta_is_linked_not_decompressed() {
        let g = tempdir().unwrap();
        let p = tempdir().unwrap();
        let fa = g.path().join("chr.fa.gz");
        gz(&fa, b">chr1\nACGT\n");
        let s = stage_input(std::slice::from_ref(&fa), p.path()).unwrap();
        let t = p.path().join(INDEX_PREFIX_NAME);
        assert!(fs::symlink_metadata(&t).unwrap().file_type().is_symlink());
        assert_eq!(s.names, ["chr1"]);
    }

    #[test]
    fn several_files_are_concatenated_with_newline_guard() {
        let g = tempdir().unwrap();
        let p = tempdir().unwrap();
        let a = g.path().join("a.fa.gz");
        let b = g.path().join("b.fa.gz");
        gz(&a, b">chrA\nAC");
        gz(&b, b">chrB\nGT\n");
        let s = stage_input(&[a, b], p.path()).unwrap();
        let t = p.path().join(INDEX_PREFIX_NAME);
        assert!(!fs::symlink_metadata(&t).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&t).unwrap(), b">chrA\nAC\n>chrB\nGT\n");
        assert_eq!(s.names, ["chrA", "chrB"]);
        assert_eq!(s.total_len, 4);
    }

    #[test]
    fn empty_names_and_duplicates_and_non_fasta_are_rejected() {
        for (content, want) in [
            (&b">\nACGT\n"[..], "no sequence name"),
            (&b"> chr1\nACGT\n"[..], "no sequence name"),
            (&b">chr1\nAC\n>chr1\nGT\n"[..], "already exists"),
            (&b"ACGT\n"[..], "FASTA format"),
            (&b""[..], "FASTA format"),
        ] {
            let g = tempdir().unwrap();
            let p = tempdir().unwrap();
            let a = g.path().join("a.fa");
            let b = g.path().join("b.fa");
            fs::write(&a, content).unwrap();
            fs::write(&b, b">other\nAC\n").unwrap();
            let e = stage_input(&[a, b], p.path()).unwrap_err().to_string();
            assert!(e.contains(want), "{e}");
        }
    }

    #[test]
    fn meth_fa_in_the_genome_folder_is_rejected() {
        let files = [
            PathBuf::from("/g/chr1.fa"),
            PathBuf::from("/g/genome.fa.meth.fa"),
        ];
        let e = reject_index_files(&files).unwrap_err().to_string();
        assert!(
            e.contains("genome.fa.meth.fa") && e.contains("bwa-mem3 index file"),
            "{e}"
        );
        assert!(reject_index_files(&files[..1]).is_ok());
    }

    #[test]
    fn prune_keeps_exactly_the_seven_files() {
        let p = tempdir().unwrap();
        all_index_files(p.path(), &["chr1", "chr2"]);
        for extra in ["", ".meth.fa", ".bwt.2bit.64"] {
            fs::write(p.path().join(format!("{INDEX_PREFIX_NAME}{extra}")), b"x").unwrap();
        }
        fs::create_dir(p.path().join("tmp")).unwrap();
        prune_and_check(p.path(), &["chr1".into(), "chr2".into()], Path::new("/g")).unwrap();
        let mut left: Vec<String> = fs::read_dir(p.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        let mut want: Vec<String> = BWAMEM3_INDEX_SUFFIXES
            .iter()
            .map(|s| format!("{INDEX_PREFIX_NAME}.{s}"))
            .collect();
        want.sort();
        assert_eq!(left, want);
    }

    #[test]
    fn prune_rejects_missing_file_and_contig_mismatch() {
        let p = tempdir().unwrap();
        all_index_files(p.path(), &["chr1", "chr2"]);
        fs::remove_file(p.path().join(format!("{INDEX_PREFIX_NAME}.meth.pac"))).unwrap();
        let e = prune_and_check(p.path(), &["chr1".into(), "chr2".into()], Path::new("/g"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("genome.fa.meth.pac") && e.contains("/g"), "{e}");

        let p = tempdir().unwrap();
        all_index_files(p.path(), &["chr1", "chrX"]);
        let e = prune_and_check(p.path(), &["chr1".into(), "chr2".into()], Path::new("/g"))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("`chrX` in the index, `chr2` in the genome"),
            "{e}"
        );
    }

    #[test]
    fn publish_swaps_an_existing_index() {
        let g = tempdir().unwrap();
        let dest = g.path().join(INDEX_DIR);
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("old"), b"").unwrap();
        let partial = g.path().join("Bwamem3_Index.partial.1");
        fs::create_dir(&partial).unwrap();
        fs::write(partial.join("new"), b"").unwrap();
        let prefix = publish(&partial, g.path(), &Logger::new(false)).unwrap();
        assert_eq!(prefix, dest.join(INDEX_PREFIX_NAME));
        assert!(dest.join("new").exists() && !dest.join("old").exists());
        let left: Vec<_> = fs::read_dir(g.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "only Bwamem3_Index/ remains");
    }
}
