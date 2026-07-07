//! Step III: locate and run the external indexer (`bowtie2-build` /
//! `hisat2-build` / `minimap2 -d`) on the two converted references,
//! concurrently (mirroring Perl's `fork`).
//!
//! Discovery tier mirrors the extractor's subprocess discovery:
//! `BISMARK_BIN` (strict, no fallback) → `PATH` (via `which`) → `current_exe`'s
//! directory. When `--path_to_aligner` is given, the binary is taken as
//! exactly `{dir}/{binary}` with **no `which`-fallback** (validated early in
//! Step I).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::genome_prep::cli::Aligner;
use crate::genome_prep::error::GenomePrepError;

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) => m.is_file() && (m.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

/// Resolve an explicit `--path_to_aligner` directory to `{dir}/{binary}`,
/// validating it exists + is executable. **No `which`-fallback** — a wrong
/// explicit path must fail (mirrors Perl's `chdir`-validate in Step I).
pub fn resolve_explicit(dir: &Path, aligner: Aligner) -> Result<PathBuf, GenomePrepError> {
    let cand = dir.join(aligner.binary_name());
    if is_executable_file(&cand) {
        Ok(cand)
    } else {
        Err(GenomePrepError::IndexerNotFound {
            tool: aligner.binary_name().to_string(),
            searched: vec![cand],
        })
    }
}

/// Discover the indexer binary: `BISMARK_BIN` (strict) → `PATH` → `current_exe`
/// dir. Used when `--path_to_aligner` is not given.
pub fn discover(aligner: Aligner) -> Result<PathBuf, GenomePrepError> {
    let tool = aligner.binary_name();
    let mut searched: Vec<PathBuf> = Vec::new();

    // 1. BISMARK_BIN strict (empty = unset).
    if let Ok(bin_dir) = std::env::var("BISMARK_BIN")
        && !bin_dir.is_empty()
    {
        let cand = PathBuf::from(&bin_dir).join(tool);
        searched.push(cand.clone());
        if is_executable_file(&cand) {
            return Ok(cand);
        }
        return Err(GenomePrepError::IndexerNotFound {
            tool: tool.to_string(),
            searched,
        });
    }

    // 2. PATH via `which`.
    match which::which(tool) {
        Ok(p) => return Ok(p),
        Err(_) => searched.push(PathBuf::from(format!("$PATH/{tool}"))),
    }

    // 3. current_exe() parent.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let cand = dir.join(tool);
        searched.push(cand.clone());
        if is_executable_file(&cand) {
            return Ok(cand);
        }
    }

    Err(GenomePrepError::IndexerNotFound {
        tool: tool.to_string(),
        searched,
    })
}

/// Build the indexer `Command` for one conversion directory. Re-globs `*.fa` in
/// `dir` (sorted on file-name bytes), joins with commas, and constructs the
/// aligner-specific argv. Always emits the threads flag (N=1 default,
/// Perl-faithful). The command's working directory is `dir` (so output
/// `basename` and the relative `*.fa` names resolve there).
fn build_command(
    bin: &Path,
    aligner: Aligner,
    dir: &Path,
    basename: &str,
    threads: u32,
    large_index: bool,
) -> Result<Command, GenomePrepError> {
    let mut fa: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let s = e.file_name().to_string_lossy().into_owned();
            (s.ends_with(".fa")).then_some(s)
        })
        .collect();
    // Same case-insensitive ordering as the discovery glob (Perl File::Glob).
    fa.sort_by(|a, b| crate::genome_prep::discovery::fasta_name_cmp(a.as_bytes(), b.as_bytes()));
    let file_list = fa.join(",");

    let mut cmd = Command::new(bin);
    cmd.current_dir(dir);
    match aligner {
        Aligner::Minimap2 => {
            cmd.arg("-k")
                .arg("20")
                .arg("-t")
                .arg(threads.to_string())
                .arg("-d")
                .arg(format!("{basename}.mmi"))
                .arg(&file_list);
        }
        Aligner::Bowtie2 | Aligner::Hisat2 => {
            cmd.arg("--threads").arg(threads.to_string());
            if large_index {
                cmd.arg("--large-index");
            }
            cmd.arg("-f").arg(&file_list).arg(basename);
        }
    }
    Ok(cmd)
}

/// Run the indexer for a single conversion directory, producing `basename`
/// (`BS_CT` / `BS_GA` / `BS_combined`, or `.mmi` for minimap2).
pub fn run_one(
    bin: &Path,
    aligner: Aligner,
    dir: &Path,
    basename: &str,
    threads: u32,
    large_index: bool,
) -> Result<(), GenomePrepError> {
    let mut cmd = build_command(bin, aligner, dir, basename, threads, large_index)?;
    let status = cmd.status().map_err(|_| GenomePrepError::IndexerFailed {
        tool: aligner.binary_name().to_string(),
        dir: dir.to_path_buf(),
    })?;
    if !status.success() {
        return Err(GenomePrepError::IndexerFailed {
            tool: aligner.binary_name().to_string(),
            dir: dir.to_path_buf(),
        });
    }
    Ok(())
}

/// Run the CT and GA index builds **concurrently** (CT on a spawned thread, GA
/// on the calling thread — mirrors Perl's `fork`). Joins both and returns the
/// first failure.
pub fn run_both(
    bin: &Path,
    aligner: Aligner,
    ct_dir: &Path,
    ga_dir: &Path,
    threads: u32,
    large_index: bool,
) -> Result<(), GenomePrepError> {
    let bin_ct = bin.to_path_buf();
    let ct_dir_owned = ct_dir.to_path_buf();
    let handle = std::thread::spawn(move || {
        run_one(
            &bin_ct,
            aligner,
            &ct_dir_owned,
            "BS_CT",
            threads,
            large_index,
        )
    });

    let ga_res = run_one(bin, aligner, ga_dir, "BS_GA", threads, large_index);

    let ct_res = handle.join().unwrap_or_else(|_| {
        Err(GenomePrepError::IndexerFailed {
            tool: aligner.binary_name().to_string(),
            dir: ct_dir.to_path_buf(),
        })
    });

    ct_res?;
    ga_res?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn bowtie2_command_has_threads_and_file_list() {
        let d = tempdir().unwrap();
        fs::write(d.path().join("genome_mfa.CT_conversion.fa"), b">x\nACGT\n").unwrap();
        let cmd = build_command(
            Path::new("bowtie2-build"),
            Aligner::Bowtie2,
            d.path(),
            "BS_CT",
            1,
            false,
        )
        .unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        // Always emits --threads (N=1 default, Perl-faithful).
        assert_eq!(
            args,
            vec![
                "--threads".to_string(),
                "1".to_string(),
                "-f".to_string(),
                "genome_mfa.CT_conversion.fa".to_string(),
                "BS_CT".to_string()
            ]
        );
    }

    #[test]
    fn bowtie2_large_index_flag() {
        let d = tempdir().unwrap();
        fs::write(d.path().join("x.fa"), b">x\nACGT\n").unwrap();
        let cmd = build_command(
            Path::new("bowtie2-build"),
            Aligner::Bowtie2,
            d.path(),
            "BS_CT",
            4,
            true,
        )
        .unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--large-index".to_string()));
        assert_eq!(args[0], "--threads");
        assert_eq!(args[1], "4");
    }

    #[test]
    fn minimap2_command_uses_k20_and_mmi() {
        let d = tempdir().unwrap();
        fs::write(d.path().join("genome_mfa.CT_conversion.fa"), b">x\nACGT\n").unwrap();
        let cmd = build_command(
            Path::new("minimap2"),
            Aligner::Minimap2,
            d.path(),
            "BS_CT",
            3,
            false,
        )
        .unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "-k".to_string(),
                "20".to_string(),
                "-t".to_string(),
                "3".to_string(),
                "-d".to_string(),
                "BS_CT.mmi".to_string(),
                "genome_mfa.CT_conversion.fa".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_explicit_missing_errors() {
        let d = tempdir().unwrap();
        let r = resolve_explicit(d.path(), Aligner::Bowtie2);
        assert!(matches!(r, Err(GenomePrepError::IndexerNotFound { .. })));
    }
}
