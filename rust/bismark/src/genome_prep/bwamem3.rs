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
use std::time::SystemTime;

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

/// Contig names and lengths of the staged genome, in file order.
type Contigs = Vec<(String, u64)>;

/// Wrap an I/O error with the path it concerns.
fn at(path: &Path) -> impl FnOnce(std::io::Error) -> GenomePrepError + '_ {
    move |source| GenomePrepError::IoAt {
        path: path.to_path_buf(),
        source,
    }
}

/// `true` for the converted FASTA `bwa-mem3 index --meth` writes next to its input.
pub(crate) fn is_index_fasta(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".meth.fa"))
}

/// Reject bwa-mem3 index files that discovery picked up as genome FASTA.
pub fn reject_index_files(files: &[PathBuf]) -> Result<(), GenomePrepError> {
    if let Some(f) = files.iter().find(|f| is_index_fasta(f)) {
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
    note_leftovers(genome_folder, &partial, logger)?;
    // A folder with this run's PID is from a killed earlier run.
    if partial.exists() {
        fs::remove_dir_all(&partial).map_err(at(&partial))?;
    }
    fs::create_dir(&partial).map_err(at(&partial))?;

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

/// Mention other runs' `.partial.*` and interrupted rebuilds' `.old.*` folders; never touch them.
fn note_leftovers(
    genome_folder: &Path,
    own_partial: &Path,
    logger: &Logger,
) -> Result<(), GenomePrepError> {
    for entry in fs::read_dir(genome_folder)
        .map_err(at(genome_folder))?
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let age = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(|d| format!(" (last changed {} min ago)", d.as_secs() / 60))
            .unwrap_or_default();
        if name.starts_with(PARTIAL_PREFIX) && path != own_partial {
            logger.note(&format!(
                "Note: {}{age} exists: another bwa-mem3 index build may be running here, or one \
                 was killed. It is left alone; remove it if it is stale.",
                path.display()
            ));
        } else if name.starts_with(OLD_PREFIX) {
            logger.note(&format!(
                "Note: {}{age} is a previous bwa-mem3 index left by an interrupted rebuild. It is \
                 left alone; remove it once {INDEX_DIR}/ is complete.",
                path.display()
            ));
        }
    }
    Ok(())
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
    let contigs = stage_input(files, partial)?;
    logger.note(&format!(
        "Indexing {} bp in {} contigs with `bwa-mem3 index --meth`. A human genome needs about \
         104 GB of RAM and up to 70 GB of free disk; bwa-mem3 prints its exact memory requirement.",
        contigs.iter().map(|c| c.1).sum::<u64>(),
        contigs.len()
    ));
    run_index(bin, partial, threads, max_memory, genome_folder)?;
    prune_and_check(partial, &contigs, genome_folder)
}

/// Sequence bytes of one line as the aligner counts them: up to `\n`, then up to the first `\r`.
fn seq_len(line: &[u8]) -> u64 {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.iter().position(|&b| b == b'\r').unwrap_or(line.len()) as u64
}

/// Put the genome at `<dir>/genome.fa` (a link to a single file, else a decompressed
/// concatenation) and check its contig names.
fn stage_input(files: &[PathBuf], dir: &Path) -> Result<Contigs, GenomePrepError> {
    let target = dir.join(INDEX_PREFIX_NAME);
    #[cfg(unix)]
    let linked = files.len() == 1 && {
        let src = fs::canonicalize(&files[0]).map_err(at(&files[0]))?;
        std::os::unix::fs::symlink(&src, &target).map_err(at(&target))?;
        true
    };
    #[cfg(not(unix))]
    let linked = false;
    let mut out = if linked {
        None
    } else {
        Some(BufWriter::new(File::create(&target).map_err(at(&target))?))
    };

    let mut contigs: Contigs = Vec::new();
    let mut seen = HashSet::new();
    let mut line = Vec::new();
    for file in files {
        let mut reader = open_fasta(file)?;
        let mut first = true;
        let mut last_newline = true;
        loop {
            line.clear();
            if reader.read_until(b'\n', &mut line).map_err(at(file))? == 0 {
                break;
            }
            if std::mem::take(&mut first) && line[0] != b'>' {
                return Err(GenomePrepError::NotFasta(file.clone()));
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
                contigs.push((name, 0));
            } else if let Some(last) = contigs.last_mut() {
                last.1 += seq_len(&line);
            }
            if let Some(w) = out.as_mut() {
                w.write_all(&line).map_err(at(&target))?;
            }
            last_newline = line.last() == Some(&b'\n');
        }
        if first {
            return Err(GenomePrepError::NotFasta(file.clone()));
        }
        if let Some(w) = out.as_mut()
            && !last_newline
        {
            w.write_all(b"\n").map_err(at(&target))?;
        }
    }
    if let Some(mut w) = out {
        w.flush().map_err(at(&target))?;
    }
    Ok(contigs)
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
    fs::create_dir(&tmp).map_err(at(&tmp))?;
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
            hint: match max_memory {
                Some(m) => format!(
                    "The budget is the --bwamem3_max_memory you gave ({m}); raise it, or run on a \
                     machine with more memory."
                ),
                None => "Its budget is the detected RAM less a reserve; raise it with \
                         --bwamem3_max_memory, or run on a machine with more memory."
                    .to_string(),
            },
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
    contigs: &[(String, u64)],
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
        let path = dir.join(name);
        match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(at(&path)(e)),
            _ => {}
        }
    }
    let tmp = dir.join("tmp");
    match fs::remove_dir_all(&tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(at(&tmp)(e)),
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
    if ann != contigs {
        let first_diff = ann
            .iter()
            .zip(contigs)
            .find(|(a, b)| a != b)
            .map(|((an, al), (gn, gl))| {
                format!(
                    "; first difference: `{an}` ({al} bp) in the index, `{gn}` ({gl} bp) in the \
                     genome"
                )
            })
            .unwrap_or_default();
        return Err(failed(format!(
            "the index lists {} contigs, the genome {}{first_diff}",
            ann.len(),
            contigs.len()
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
    if fs::symlink_metadata(&dest).is_ok() {
        logger.note(&format!(
            "Replacing the existing bwa-mem3 index in {}",
            dest.display()
        ));
        let old = genome_folder.join(format!("{OLD_PREFIX}{}", std::process::id()));
        if fs::symlink_metadata(&old).is_ok() {
            fs::remove_dir_all(&old).map_err(at(&old))?;
        }
        fs::rename(&dest, &old).map_err(at(&dest))?;
        if let Err(e) = fs::rename(partial, &dest) {
            if let Err(back) = fs::rename(&old, &dest) {
                return Err(GenomePrepError::Bwamem3Failed {
                    genome: genome_folder.to_path_buf(),
                    detail: format!(
                        "could not move the new index into place ({e}) or the previous one back \
                         ({back}); the previous index is in {}",
                        old.display()
                    ),
                });
            }
            return Err(at(&dest)(e));
        }
        if let Err(e) = fs::remove_dir_all(&old) {
            logger.note(&format!(
                "Warning: could not remove the previous index {}: {e}",
                old.display()
            ));
        }
    } else {
        fs::rename(partial, &dest).map_err(at(&dest))?;
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

    fn ann(dir: &Path, contigs: &[(&str, u64)]) {
        let total: u64 = contigs.iter().map(|c| c.1).sum();
        let mut s = format!("{total} {} 11\n", contigs.len());
        let mut off = 0;
        for (c, len) in contigs {
            s.push_str(&format!("0 {c} (null)\n{off} {len} 0\n"));
            off += len;
        }
        fs::write(dir.join(format!("{INDEX_PREFIX_NAME}.ann")), s).unwrap();
    }

    fn contigs(c: &[(&str, u64)]) -> Contigs {
        c.iter().map(|(n, l)| (n.to_string(), *l)).collect()
    }

    fn all_index_files(dir: &Path, contigs: &[(&str, u64)]) {
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
        assert_eq!(s, [("chr1".to_string(), 6), ("chr2".to_string(), 2)]);
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
        assert_eq!(s, [("chr1".to_string(), 4)]);
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
        assert_eq!(s, [("chrA".to_string(), 2), ("chrB".to_string(), 2)]);
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
        let two = [("chr1", 4), ("chr2", 4)];
        all_index_files(p.path(), &two);
        for extra in ["", ".meth.fa", ".bwt.2bit.64"] {
            fs::write(p.path().join(format!("{INDEX_PREFIX_NAME}{extra}")), b"x").unwrap();
        }
        fs::create_dir(p.path().join("tmp")).unwrap();
        prune_and_check(p.path(), &contigs(&two), Path::new("/g")).unwrap();
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
        let two = contigs(&[("chr1", 4), ("chr2", 4)]);
        all_index_files(p.path(), &[("chr1", 4), ("chr2", 4)]);
        fs::remove_file(p.path().join(format!("{INDEX_PREFIX_NAME}.meth.pac"))).unwrap();
        let e = prune_and_check(p.path(), &two, Path::new("/g"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("genome.fa.meth.pac") && e.contains("/g"), "{e}");

        for (index, want) in [
            (
                [("chr1", 4), ("chrX", 4)],
                "`chrX` (4 bp) in the index, `chr2` (4 bp) in the genome",
            ),
            (
                [("chr1", 4), ("chr2", 5)],
                "`chr2` (5 bp) in the index, `chr2` (4 bp) in the genome",
            ),
        ] {
            let p = tempdir().unwrap();
            all_index_files(p.path(), &index);
            let e = prune_and_check(p.path(), &two, Path::new("/g"))
                .unwrap_err()
                .to_string();
            assert!(e.contains(want), "{e}");
        }
    }

    #[test]
    fn lengths_count_like_the_aligner() {
        assert_eq!(seq_len(b"ACGT\n"), 4);
        assert_eq!(seq_len(b"ACGT\r\n"), 4);
        assert_eq!(seq_len(b"AC\rGT\n"), 2);
        assert_eq!(seq_len(b"AC GT"), 5);
        let g = tempdir().unwrap();
        let p = tempdir().unwrap();
        let fa = g.path().join("a.fa");
        fs::write(&fa, b">chr1\r\nACGT\r\nAC\r\n").unwrap();
        let b = g.path().join("b.fa");
        fs::write(&b, b">chr2\nN\n").unwrap();
        assert_eq!(
            stage_input(&[fa, b], p.path()).unwrap(),
            contigs(&[("chr1", 6), ("chr2", 1)])
        );
    }

    #[test]
    fn index_fasta_names() {
        assert!(is_index_fasta(Path::new("/g/genome.fa.meth.fa")));
        assert!(!is_index_fasta(Path::new("/g/chr1.fa")));
        assert!(!is_index_fasta(Path::new("/g/meth.fasta")));
    }

    #[cfg(unix)]
    #[test]
    fn publish_replaces_a_dangling_index_link() {
        let g = tempdir().unwrap();
        let dest = g.path().join(INDEX_DIR);
        std::os::unix::fs::symlink(g.path().join("gone"), &dest).unwrap();
        let partial = g.path().join("Bwamem3_Index.partial.1");
        fs::create_dir(&partial).unwrap();
        fs::write(partial.join("new"), b"").unwrap();
        publish(&partial, g.path(), &Logger::new(false)).unwrap();
        assert!(dest.join("new").is_file());
        assert_eq!(fs::read_dir(g.path()).unwrap().count(), 1);
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
