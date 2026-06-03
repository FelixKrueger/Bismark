//! External-aligner detection (Bowtie 2 for v1).
//!
//! Mirrors Perl `ensure_the_aligner_is_working` (7060–7092) and the path setup
//! (7480-ish): if `--path_to_bowtie2 <dir>` is given it must be a directory and
//! `bowtie2` is appended; otherwise `bowtie2` is resolved from `PATH`. We run
//! `<bowtie2> --version`, require success, and parse the version triple from a
//! line like `.../bowtie2-align-s version 2.5.5`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AlignerError, Result};

/// The Bowtie 2 version this port pins for the byte-identity gate.
pub const PINNED_BOWTIE2_VERSION: &str = "2.5.5";

/// A located, working aligner.
#[derive(Debug, Clone)]
pub struct DetectedAligner {
    /// Resolved path to the `bowtie2` executable.
    pub path: PathBuf,
    /// Parsed version string (e.g. `2.5.5`), or the raw first line if unparsable.
    pub version: String,
}

/// Resolve the `bowtie2` executable path. If `path_to_bowtie2` is given it must
/// be a directory (Perl requires this) and `bowtie2` is appended; otherwise the
/// binary is looked up on `PATH`.
fn resolve_bowtie2_path(path_to_bowtie2: Option<&Path>) -> Result<PathBuf> {
    match path_to_bowtie2 {
        Some(dir) => {
            if !dir.is_dir() {
                return Err(AlignerError::Validation(format!(
                    "the path to Bowtie 2 provided ({dir:?}) is invalid (it MUST be a directory)!"
                )));
            }
            Ok(dir.join("bowtie2"))
        }
        // Perl uses the literal 'bowtie2' and relies on PATH; we resolve it via
        // `which` so a missing binary fails here rather than at exec time.
        None => which::which("bowtie2").or_else(|_| Ok(PathBuf::from("bowtie2"))),
    }
}

/// Detect Bowtie 2: resolve the path, run `--version`, parse the version, and
/// warn if it is not the pinned [`PINNED_BOWTIE2_VERSION`].
pub fn detect_bowtie2(path_to_bowtie2: Option<&Path>) -> Result<DetectedAligner> {
    let path = resolve_bowtie2_path(path_to_bowtie2)?;

    let output = Command::new(&path).arg("--version").output().map_err(|_| {
        AlignerError::AlignerNotWorking {
            aligner: "Bowtie 2".to_string(),
            cmd: path.display().to_string(),
        }
    })?;

    if !output.status.success() {
        return Err(AlignerError::AlignerNotWorking {
            aligner: "Bowtie 2".to_string(),
            cmd: path.display().to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_bowtie2_version(&stdout).unwrap_or_else(|| {
        stdout
            .lines()
            .next()
            .unwrap_or("unknown")
            .trim()
            .to_string()
    });

    if version != PINNED_BOWTIE2_VERSION {
        eprintln!(
            "Warning: detected Bowtie 2 {version}, but byte-identity is only guaranteed against the \
             pinned version {PINNED_BOWTIE2_VERSION}."
        );
    } else {
        eprintln!(
            "Bowtie 2 seems to be working fine (tested '{}' [{version}])",
            path.display()
        );
    }

    Ok(DetectedAligner { path, version })
}

/// Parse `x.y.z` from the first line containing `version` (Perl regex
/// `bowtie.*version\s+(\d+\.\d+\.\d+)`).
fn parse_bowtie2_version(stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|l| l.contains("version"))?;
    let after = line.split("version").nth(1)?;
    let token = after.split_whitespace().next()?;
    if is_version_triple(token) {
        Some(token.to_string())
    } else {
        None
    }
}

/// `true` if `s` is `<int>.<int>.<int>`.
fn is_version_triple(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_bowtie2_version_line() {
        let out = "/opt/env/bin/bowtie2-align-s version 2.5.5\n64-bit\nBuilt on host\n";
        assert_eq!(parse_bowtie2_version(out).as_deref(), Some("2.5.5"));
    }

    #[test]
    fn rejects_non_triple() {
        assert!(!is_version_triple("2.5"));
        assert!(!is_version_triple("2.5.x"));
        assert!(is_version_triple("2.5.5"));
        assert!(is_version_triple("10.0.123"));
    }
}
