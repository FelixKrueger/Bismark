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
    validated_bwamem3_versions,
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
/// The first bwa-mem3 release with `index --meth-only`.
const MIN_METH_ONLY_VERSION: (u32, u32, u32) = (0, 18, 0);

/// `true` if bwa-mem3 `version` (`x.y.z`) can skip the plain FM-index with `--meth-only`.
pub(crate) fn bwamem3_has_meth_only(version: &str) -> bool {
    let parts: Result<Vec<u32>, _> = version.split('.').map(str::parse).collect();
    matches!(parts.as_deref(), Ok(&[a, b, c]) if (a, b, c) >= MIN_METH_ONLY_VERSION)
}

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
            validated_bwamem3_versions()
        ));
    }
    Ok(version)
}

/// Build the index with bwa-mem3 `version`; returns its prefix `<genome>/Bwamem3_Index/genome.fa`.
pub fn build(
    files: &[PathBuf],
    genome_folder: &Path,
    bin: &Path,
    version: &str,
    threads: Option<u32>,
    max_memory: Option<&str>,
    logger: &Logger,
) -> Result<PathBuf, GenomePrepError> {
    for note in leftover_notes(genome_folder)? {
        logger.note(&note);
    }
    let partial = create_partial(genome_folder)?;

    let run = IndexRun {
        bin,
        meth_only: bwamem3_has_meth_only(version),
        threads,
        max_memory,
    };
    if let Err(e) = stage_and_index(files, genome_folder, &partial, &run, logger) {
        let _ = fs::remove_dir_all(&partial);
        return Err(e);
    }
    let dest = genome_folder.join(INDEX_DIR);
    publish(&partial, &dest, logger).map_err(|detail| GenomePrepError::Bwamem3NotPublished {
        partial: partial.clone(),
        dest,
        detail,
    })
}

/// Create `<genome>/Bwamem3_Index.partial.<random>` (unique even when PIDs repeat, as in containers).
fn create_partial(genome_folder: &Path) -> Result<PathBuf, GenomePrepError> {
    use std::hash::{BuildHasher, Hasher};
    let mut last = None;
    for attempt in 0..16u32 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u32(std::process::id());
        h.write_u32(attempt);
        h.write_u128(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        let path = genome_folder.join(format!("{PARTIAL_PREFIX}{:016x}", h.finish()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some((path, e)),
            Err(e) => return Err(at(&path)(e)),
        }
    }
    let (path, e) = last.expect("at least one attempt");
    Err(at(&path)(e))
}

/// `true` if `dir` holds a finished, pruned index (what `publish` moves into place).
fn is_complete_build(dir: &Path) -> bool {
    fs::symlink_metadata(dir.join("tmp")).is_err()
        && fs::symlink_metadata(dir.join(INDEX_PREFIX_NAME)).is_err()
        && BWAMEM3_INDEX_SUFFIXES
            .iter()
            .all(|s| dir.join(format!("{INDEX_PREFIX_NAME}.{s}")).is_file())
}

/// Notes on earlier runs' `.partial.*` and interrupted rebuilds' `.old.*` folders (never touched).
fn leftover_notes(genome_folder: &Path) -> Result<Vec<String>, GenomePrepError> {
    let mut notes = Vec::new();
    for entry in fs::read_dir(genome_folder)
        .map_err(at(genome_folder))?
        .filter_map(|e| e.ok())
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let partial = name.starts_with(PARTIAL_PREFIX);
        if !partial && !name.starts_with(OLD_PREFIX) {
            continue;
        }
        let path = entry.path();
        let age = newest_mtime(&path, 3)
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(|d| format!(" (last written {} min ago)", d.as_secs() / 60))
            .unwrap_or_default();
        notes.push(if partial && is_complete_build(&path) {
            format!(
                "Note: {}{age} is a complete bwa-mem3 index that an earlier run could not move \
                 into place. It is left alone; replace {INDEX_DIR}/ with it, or remove it.",
                path.display()
            )
        } else if partial {
            format!(
                "Note: {}{age} exists: another bwa-mem3 index build may be running here, or one \
                 was killed. It is left alone; remove it if no build is running.",
                path.display()
            )
        } else {
            format!(
                "Note: {}{age} is a previous bwa-mem3 index left by an interrupted rebuild. It is \
                 left alone; remove it once {INDEX_DIR}/ is complete.",
                path.display()
            )
        });
    }
    Ok(notes)
}

/// Newest modification time of `path` and of everything up to `depth` levels below it.
fn newest_mtime(path: &Path, depth: u32) -> Option<SystemTime> {
    let mut newest = fs::symlink_metadata(path).and_then(|m| m.modified()).ok();
    if depth > 0
        && let Ok(entries) = fs::read_dir(path)
    {
        for entry in entries.filter_map(|e| e.ok()) {
            let t = if entry.file_type().is_ok_and(|t| t.is_dir()) {
                newest_mtime(&entry.path(), depth - 1)
            } else {
                entry.metadata().and_then(|m| m.modified()).ok()
            };
            newest = newest.max(t);
        }
    }
    newest
}

/// How `bwa-mem3 index` is run.
struct IndexRun<'a> {
    bin: &'a Path,
    meth_only: bool,
    threads: Option<u32>,
    max_memory: Option<&'a str>,
}

fn stage_and_index(
    files: &[PathBuf],
    genome_folder: &Path,
    partial: &Path,
    run: &IndexRun,
    logger: &Logger,
) -> Result<(), GenomePrepError> {
    let contigs = stage_input(files, partial)?;
    let (flags, disk) = if run.meth_only {
        ("--meth --meth-only", "about 35 GB")
    } else {
        ("--meth", "up to 70 GB")
    };
    logger.note(&format!(
        "Indexing {} bp in {} contigs with `bwa-mem3 index {flags}`. A human genome needs about \
         104 GB of RAM and {disk} of free disk; bwa-mem3 prints its exact memory requirement.",
        contigs.iter().map(|c| c.1).sum::<u64>(),
        contigs.len()
    ));
    run_index(run, partial, genome_folder)?;
    prune_and_check(partial, &contigs, genome_folder)
}

/// Bases on one sequence line (without its `\n` and one trailing `\r`), or why bwa-mem3 would read
/// it differently from Bismark. `contig_start`: no base of the current contig has been read yet.
fn seq_line_len(line: &[u8], contig_start: bool) -> Result<u64, String> {
    const UNLOADABLE: &str = "which bwa-mem3 writes into the index and then cannot load";
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    if contig_start && line == b"\r" {
        return Err(format!(
            "a line holding only a carriage return (\\r) before the first base, {UNLOADABLE}"
        ));
    }
    if let Some(&c @ (b'+' | b'@')) = line.first() {
        return Err(format!(
            "a line starting with `{}`, which bwa-mem3 reads as the start of a FASTQ record, \
             cutting the contig short",
            c as char
        ));
    }
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let what = match line
        .iter()
        .find(|&&b| matches!(b, b' ' | b'\t' | b'\r' | 0x0b | 0x0c) || b >= 0x80)
    {
        None => return Ok(line.len() as u64),
        Some(b' ') => "a space".to_string(),
        Some(b'\t') => "a tab".to_string(),
        Some(b'\r') => "a carriage return (\\r) before the end of the line".to_string(),
        Some(0x0b) => "a vertical tab".to_string(),
        Some(0x0c) => "a form feed".to_string(),
        Some(b) => {
            return Err(format!(
                "the sequence contains the non-ASCII byte 0x{b:02x}, which Bismark and bwa-mem3 \
                 count differently, so the aligner would reject the index"
            ));
        }
    };
    Err(format!("the sequence contains {what}, {UNLOADABLE}"))
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
        let mut reader = open_fasta(file).map_err(|e| match e {
            GenomePrepError::Io(source) => at(file)(source),
            other => other,
        })?;
        let mut first = true;
        let mut last_newline = true;
        let mut line_no = 0u64;
        loop {
            line.clear();
            if reader.read_until(b'\n', &mut line).map_err(at(file))? == 0 {
                break;
            }
            line_no += 1;
            if first {
                first = false;
                if line[0] != b'>' {
                    return Err(GenomePrepError::NotFasta(file.clone()));
                }
            }
            if line[0] == b'>' {
                let body = line.strip_suffix(b"\n").unwrap_or(&line);
                if body.strip_suffix(b"\r").unwrap_or(body).contains(&b'\r') {
                    return Err(GenomePrepError::Validation(format!(
                        "{}: line {line_no} holds more than one line separated by carriage returns \
                         (\\r) alone; convert the file to LF or CRLF line ends",
                        file.display()
                    )));
                }
                reject_empty_contig(contigs.last(), file)?;
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
                last.1 += seq_line_len(&line, last.1 == 0).map_err(|problem| {
                    GenomePrepError::Validation(format!(
                        "{}: contig {}, line {line_no}: {problem}.",
                        file.display(),
                        last.0
                    ))
                })?;
            }
            if let Some(w) = out.as_mut() {
                w.write_all(&line).map_err(at(&target))?;
            }
            last_newline = line.last() == Some(&b'\n');
        }
        if first {
            return Err(GenomePrepError::NotFasta(file.clone()));
        }
        reject_empty_contig(contigs.last(), file)?;
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

/// A contig without bases makes `bwa-mem3 mem --meth` fail at start-up.
fn reject_empty_contig(contig: Option<&(String, u64)>, file: &Path) -> Result<(), GenomePrepError> {
    match contig {
        Some((name, 0)) => Err(GenomePrepError::Validation(format!(
            "{}: contig {name} has no sequence; bwa-mem3 cannot align against an index with an \
             empty contig",
            file.display()
        ))),
        _ => Ok(()),
    }
}

/// `bwa-mem3 index --meth` (plus `--meth-only` if `run.meth_only`) with its scratch inside `dir`.
fn run_index(run: &IndexRun, dir: &Path, genome_folder: &Path) -> Result<(), GenomePrepError> {
    let (bin, max_memory) = (run.bin, run.max_memory);
    let tmp = dir.join("tmp");
    fs::create_dir(&tmp).map_err(at(&tmp))?;
    let mut cmd = Command::new(bin);
    cmd.current_dir(dir).arg("index").arg("--meth");
    if run.meth_only {
        cmd.arg("--meth-only");
    }
    if let Some(t) = run.threads {
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
            detail: format!(
                "`bwa-mem3 index --meth{}` {status}",
                if run.meth_only { " --meth-only" } else { "" }
            ),
        }),
    }
}

/// Drop what `mem --meth` never opens, then require exactly the 7 index files and matching contigs.
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
    // A sidecar such as `.alt` or `.hdr` would change how `mem` aligns, so it is never published.
    let expected: HashSet<String> = BWAMEM3_INDEX_SUFFIXES
        .iter()
        .map(|s| format!("{INDEX_PREFIX_NAME}.{s}"))
        .collect();
    let mut unexpected = Vec::new();
    for entry in fs::read_dir(dir).map_err(at(dir))? {
        let name = entry
            .map_err(at(dir))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if !expected.contains(&name) {
            unexpected.push(name);
        }
    }
    if !unexpected.is_empty() {
        unexpected.sort();
        return Err(failed(format!(
            "the new index folder holds files Bismark does not expect: {}",
            unexpected.join(", ")
        )));
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

/// `rename(from, to)`, naming both paths on failure.
fn move_to(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to)
        .map_err(|e| format!("could not move {} to {}: {e}", from.display(), to.display()))
}

/// Move `partial` to `dest`, swapping out any existing index.
fn publish(partial: &Path, dest: &Path, logger: &Logger) -> Result<PathBuf, String> {
    if fs::symlink_metadata(dest).is_ok() {
        logger.note(&format!(
            "Replacing the existing bwa-mem3 index in {}",
            dest.display()
        ));
        let suffix = partial
            .file_name()
            .map(|n| n.to_string_lossy().replacen(PARTIAL_PREFIX, "", 1))
            .unwrap_or_default();
        let old = dest.with_file_name(format!("{OLD_PREFIX}{suffix}"));
        move_to(dest, &old)?;
        if let Err(e) = move_to(partial, dest) {
            if let Err(back) = move_to(&old, dest) {
                return Err(format!(
                    "{e}; {back}; the previous index is in {}",
                    old.display()
                ));
            }
            return Err(e);
        }
        if let Err(e) = fs::remove_dir_all(&old) {
            logger.note(&format!(
                "Warning: could not remove the previous index {}: {e}",
                old.display()
            ));
        }
    } else {
        move_to(partial, dest)?;
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
    fn meth_only_from_bwamem3_0_18_0() {
        for v in ["0.18.0", "0.18.1", "0.19.0", "1.0.0"] {
            assert!(bwamem3_has_meth_only(v), "{v}");
        }
        for v in ["0.16.0", "0.17.0", "0.18", "0.18.x", "unknown", ""] {
            assert!(!bwamem3_has_meth_only(v), "{v}");
        }
    }

    #[test]
    fn prune_rejects_files_outside_the_seven() {
        let two = [("chr1", 4), ("chr2", 4)];
        for extra in [".alt", ".hdr"] {
            let p = tempdir().unwrap();
            all_index_files(p.path(), &two);
            fs::write(p.path().join(format!("{INDEX_PREFIX_NAME}{extra}")), b"x").unwrap();
            fs::write(p.path().join("genome.dict"), b"x").unwrap();
            let e = prune_and_check(p.path(), &contigs(&two), Path::new("/g"))
                .unwrap_err()
                .to_string();
            assert!(
                e.contains(&format!("genome.dict, {INDEX_PREFIX_NAME}{extra}")) && e.contains("/g"),
                "{e}"
            );
        }
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
    fn sequence_lines_count_or_name_the_bad_byte() {
        for (line, len) in [
            (&b"ACGT\n"[..], 4),
            (b"ACGT\r\n", 4),
            (b"ACGT", 4),
            (b"NNNN\n", 4),
            (b"acgtRY\n", 6),
            (b"\n", 0),
        ] {
            assert_eq!(seq_line_len(line, true), Ok(len), "{line:?}");
        }
        for (line, what) in [
            (&b"AC GT\n"[..], "a space"),
            (b"AC\tGT\n", "a tab"),
            (b"ACGT\r\r\n", "carriage return"),
            (b"AC\rGT\r\n", "carriage return"),
            (b"AC\x0bGT\n", "vertical tab"),
            (b"AC\x0cGT\n", "form feed"),
            (
                b"AC\xc3\xa9\n",
                "0xc3, which Bismark and bwa-mem3 count differently",
            ),
            (b"+ACGT\n", "starting with `+`"),
            (b"@ACGT\n", "starting with `@`"),
        ] {
            let e = seq_line_len(line, false).unwrap_err();
            assert!(e.contains(what), "{line:?}: {e}");
        }
        // A lone `\r` line is harmless inside a contig, but bwa-mem3 keeps it as a base at the start.
        assert_eq!(seq_line_len(b"\r\n", false), Ok(0));
        let e = seq_line_len(b"\r\n", true).unwrap_err();
        assert!(e.contains("only a carriage return"), "{e}");
        let g = tempdir().unwrap();
        let p = tempdir().unwrap();
        let bad = g.path().join("bad.fa");
        fs::write(&bad, b">chr7\nACGT\nAC GT\n").unwrap();
        let e = stage_input(std::slice::from_ref(&bad), p.path())
            .unwrap_err()
            .to_string();
        assert!(
            e.contains(&format!(
                "{}: contig chr7, line 3: the sequence contains a space",
                bad.display()
            )),
            "{e}"
        );
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
    fn genomes_bwamem3_reads_differently_are_rejected() {
        for (content, want) in [
            (
                &b">chr1\nACGT\n>chr2\n>chr3\nAC\n"[..],
                "contig chr2 has no sequence",
            ),
            (b">chr1\nACGT\n>chr2\n", "contig chr2 has no sequence"),
            (b">chr1\n\n", "contig chr1 has no sequence"),
            (b">chr1\r\n\r\nACGT\r\n", "only a carriage return"),
            (b">chr1\n\n\r\nACGT\n", "only a carriage return"),
            (b">chr1\rACGT\rACGT\r", "separated by carriage returns"),
            (b">chr1\nACGT\n+CGT\n", "starting with `+`"),
        ] {
            let g = tempdir().unwrap();
            let p = tempdir().unwrap();
            let fa = g.path().join("g.fa");
            fs::write(&fa, content).unwrap();
            let e = stage_input(std::slice::from_ref(&fa), p.path())
                .unwrap_err()
                .to_string();
            assert!(e.contains(want), "{content:?}: {e}");
        }
        // A blank CRLF line inside a contig is fine.
        let g = tempdir().unwrap();
        let p = tempdir().unwrap();
        let fa = g.path().join("g.fa");
        fs::write(&fa, b">chr1\r\nAC\r\n\r\nGT\r\n").unwrap();
        assert_eq!(
            stage_input(std::slice::from_ref(&fa), p.path()).unwrap(),
            contigs(&[("chr1", 4)])
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_fasta_is_named() {
        use std::os::unix::fs::PermissionsExt;
        let g = tempdir().unwrap();
        let p = tempdir().unwrap();
        let a = g.path().join("a.fa");
        let b = g.path().join("b.fa");
        fs::write(&a, b">chrA\nAC\n").unwrap();
        fs::write(&b, b">chrB\nGT\n").unwrap();
        fs::set_permissions(&b, fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&b).is_ok() {
            fs::set_permissions(&b, fs::Permissions::from_mode(0o644)).unwrap();
            eprintln!("skipping: permission bits do not stop this user (root)");
            return;
        }
        let r = stage_input(&[a, b.clone()], p.path());
        fs::set_permissions(&b, fs::Permissions::from_mode(0o644)).unwrap();
        let e = r.unwrap_err().to_string();
        assert!(e.starts_with(&b.display().to_string()), "{e}");
    }

    fn notes(g: &Path) -> String {
        leftover_notes(g).unwrap().join("\n")
    }

    /// Set the mtime of `path` to `mins` minutes ago.
    fn age(path: &Path, mins: u64) {
        let t = SystemTime::now() - std::time::Duration::from_secs(mins * 60);
        File::open(path).unwrap().set_modified(t).unwrap();
    }

    #[test]
    fn leftover_notes_describe_each_kind_of_folder() {
        let g = tempdir().unwrap();
        // A running build: old folder and tmp/, a fresh file deep in tmp/bwa-meth-*.
        let running = g.path().join("Bwamem3_Index.partial.aaa");
        fs::create_dir_all(running.join("tmp/bwa-meth-x")).unwrap();
        fs::write(running.join("tmp/bwa-meth-x/seed.fa"), b"").unwrap();
        for d in [
            &running,
            &running.join("tmp"),
            &running.join("tmp/bwa-meth-x"),
        ] {
            age(d, 120);
        }
        // A stale one: everything two hours old.
        let stale = g.path().join("Bwamem3_Index.partial.bbb");
        fs::create_dir(&stale).unwrap();
        age(&stale, 120);
        // A finished build an earlier run could not move into place.
        let kept = g.path().join("Bwamem3_Index.partial.ccc");
        fs::create_dir(&kept).unwrap();
        all_index_files(&kept, &[("chr1", 4)]);
        fs::create_dir(g.path().join("Bwamem3_Index.old.ddd")).unwrap();
        fs::write(g.path().join("chr1.fa"), b">c\nA\n").unwrap();

        let n = notes(g.path());
        assert!(
            n.contains("partial.aaa (last written 0 min ago) exists"),
            "{n}"
        );
        assert!(
            n.contains("partial.bbb (last written 120 min ago) exists"),
            "{n}"
        );
        assert!(
            n.contains("partial.ccc") && n.contains("is a complete bwa-mem3 index"),
            "{n}"
        );
        assert!(
            n.contains("old.ddd") && n.contains("interrupted rebuild"),
            "{n}"
        );
        assert!(!n.contains("chr1"), "{n}");
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
        publish(&partial, &dest, &Logger::new(false)).unwrap();
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
        let prefix = publish(&partial, &dest, &Logger::new(false)).unwrap();
        assert_eq!(prefix, dest.join(INDEX_PREFIX_NAME));
        assert!(dest.join("new").exists() && !dest.join("old").exists());
        let left: Vec<_> = fs::read_dir(g.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "only Bwamem3_Index/ remains");
    }

    #[test]
    fn publish_restores_the_old_index_when_the_new_one_cannot_be_moved() {
        let g = tempdir().unwrap();
        let dest = g.path().join(INDEX_DIR);
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("old"), b"").unwrap();
        let missing = g.path().join("Bwamem3_Index.partial.gone");
        let e = publish(&missing, &dest, &Logger::new(false)).unwrap_err();
        assert!(
            e.contains("could not move") && e.contains("partial.gone"),
            "{e}"
        );
        assert!(dest.join("old").is_file(), "previous index put back");
        assert_eq!(fs::read_dir(g.path()).unwrap().count(), 1, "no .old left");
    }
}
