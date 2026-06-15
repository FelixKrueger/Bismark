//! External-aligner detection (Bowtie 2 + HISAT2).
//!
//! Mirrors Perl `ensure_the_aligner_is_working` (7060–7092) and the path setup
//! (7480-ish): if `--path_to_<aligner> <dir>` is given it must be a directory
//! and the binary name is appended; otherwise the binary is resolved from
//! `PATH`. We run `<binary> --version`, require success, and parse the version
//! triple from a line like `.../bowtie2-align-s version 2.5.5` or
//! `.../hisat2-align-s version 2.2.2`. HISAT2 is a thin wrapper — the detection
//! is identical modulo the binary name + the pinned version.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Aligner;
use crate::error::{AlignerError, Result};

/// The Bowtie 2 version this port pins for the byte-identity gate.
pub const PINNED_BOWTIE2_VERSION: &str = "2.5.5";

/// The HISAT2 version this port pins for the byte-identity gate (oxy `bismark-test`).
pub const PINNED_HISAT2_VERSION: &str = "2.2.2";

/// The minimap2 version this port pins for the byte-identity gate (oxy `bismark-test`).
pub const PINNED_MINIMAP2_VERSION: &str = "2.31-r1302";

/// The rammap version this port pins (concordance gate, NOT byte-identity). rammap
/// reports `rammap 1.1.1` (a `rammap ` banner prefix, unlike minimap2's bare number).
pub const PINNED_RAMMAP_VERSION: &str = "1.1.1";

/// A located, working aligner.
#[derive(Debug, Clone)]
pub struct DetectedAligner {
    /// Resolved path to the aligner executable.
    pub path: PathBuf,
    /// Parsed version string (e.g. `2.5.5`), or the raw first line if unparsable.
    pub version: String,
}

/// The executable name Perl invokes for `kind` (literal `bowtie2` / `hisat2` /
/// `minimap2`).
fn binary_name(kind: Aligner) -> &'static str {
    match kind {
        Aligner::Bowtie2 => "bowtie2",
        Aligner::Hisat2 => "hisat2",
        Aligner::Minimap2 => "minimap2",
        Aligner::Rammap => "rammap",
    }
}

/// The pinned version `kind` is byte-identity-validated against.
fn pinned_version(kind: Aligner) -> &'static str {
    match kind {
        Aligner::Bowtie2 => PINNED_BOWTIE2_VERSION,
        Aligner::Hisat2 => PINNED_HISAT2_VERSION,
        Aligner::Minimap2 => PINNED_MINIMAP2_VERSION,
        Aligner::Rammap => PINNED_RAMMAP_VERSION,
    }
}

/// The `--path_to_<aligner>` flag name (for the not-working diagnostic).
fn path_flag(kind: Aligner) -> &'static str {
    match kind {
        Aligner::Bowtie2 => "--path_to_bowtie2",
        Aligner::Hisat2 => "--path_to_hisat2",
        Aligner::Minimap2 => "--path_to_minimap2",
        Aligner::Rammap => "--path_to_rammap",
    }
}

/// Resolve the aligner executable path. If `path_to` is given it must be a
/// directory (Perl requires this) and the binary name is appended; otherwise the
/// binary is looked up on `PATH`.
fn resolve_aligner_path(kind: Aligner, path_to: Option<&Path>) -> Result<PathBuf> {
    let bin = binary_name(kind);
    match path_to {
        Some(dir) => {
            if !dir.is_dir() {
                return Err(AlignerError::Validation(format!(
                    "the path to {} provided ({dir:?}) is invalid (it MUST be a directory)!",
                    kind.name()
                )));
            }
            Ok(dir.join(bin))
        }
        // Perl uses the literal binary name and relies on PATH; we resolve it via
        // `which` so a missing binary fails here rather than at exec time.
        None => which::which(bin).or_else(|_| Ok(PathBuf::from(bin))),
    }
}

/// Detect `kind`: resolve the path, run `--version`, parse the version, and warn
/// if it is not the pinned version. Byte-identity is guaranteed only against the
/// pinned version (Bowtie 2 2.5.5 / HISAT2 2.2.2).
pub fn detect_aligner(kind: Aligner, path_to: Option<&Path>) -> Result<DetectedAligner> {
    let path = resolve_aligner_path(kind, path_to)?;

    let not_working = || AlignerError::AlignerNotWorking {
        aligner: kind.name().to_string(),
        cmd: path.display().to_string(),
        path_flag: path_flag(kind).to_string(),
    };

    let output = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|_| not_working())?;

    if !output.status.success() {
        return Err(not_working());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Bowtie 2 / HISAT2 print a `… version x.y.z` banner; minimap2 prints ONLY the
    // bare version number (Perl 7078-7088 — the `bowtie`/`hisat2` regex branches vs
    // the minimap2 no-op `elsif`). The fallback (first non-empty line, trimmed) is
    // byte-harmless since the version is warn-only — it never enters the gated BAM
    // or report.
    let parsed = match kind {
        Aligner::Minimap2 => parse_minimap2_version(&stdout),
        // rammap prints `rammap 1.1.1` (a banner prefix) — take the last token.
        Aligner::Rammap => parse_rammap_version(&stdout),
        Aligner::Bowtie2 | Aligner::Hisat2 => parse_bowtie2_version(&stdout),
    };
    let version = parsed.unwrap_or_else(|| {
        stdout
            .lines()
            .next()
            .unwrap_or("unknown")
            .trim()
            .to_string()
    });

    let pinned = pinned_version(kind);
    if version != pinned {
        eprintln!(
            "Warning: detected {} {version}, but byte-identity is only guaranteed against the \
             pinned version {pinned}.",
            kind.name()
        );
    } else {
        eprintln!(
            "{} seems to be working fine (tested '{}' [{version}])",
            kind.name(),
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

/// Parse minimap2's version output. Unlike Bowtie 2 / HISAT2, `minimap2 --version`
/// prints **only** the bare version number (e.g. `2.31-r1302`) with no banner, so
/// Perl (7081-7083) does no regex — the chomped output IS the version. Take the
/// first non-empty line, trimmed.
fn parse_minimap2_version(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Parse rammap's version output. Unlike minimap2, `rammap --version` prints a
/// `rammap 1.1.1` banner (a `rammap ` prefix), so the version is the **last
/// whitespace token** of the first non-empty line (design#4). Warn-only — it
/// never enters the gated BAM or report.
fn parse_rammap_version(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string) // "rammap 1.1.1" -> "1.1.1"
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
    fn parses_hisat2_version_line() {
        // The generalized detector reuses the Bowtie 2 parser verbatim; HISAT2's
        // banner has the same `… version x.y.z` shape (spike Q2).
        let out = "/opt/env/bin/hisat2-align-s version 2.2.2\n64-bit\nBuilt on host\n";
        assert_eq!(parse_bowtie2_version(out).as_deref(), Some("2.2.2"));
    }

    #[test]
    fn aligner_token_and_name() {
        assert_eq!(Aligner::Bowtie2.token(), "bt2");
        assert_eq!(Aligner::Hisat2.token(), "hisat2");
        assert_eq!(Aligner::Bowtie2.name(), "Bowtie 2");
        assert_eq!(Aligner::Hisat2.name(), "HISAT2");
        assert_eq!(binary_name(Aligner::Hisat2), "hisat2");
        assert_eq!(pinned_version(Aligner::Hisat2), "2.2.2");
        assert_eq!(path_flag(Aligner::Hisat2), "--path_to_hisat2");
    }

    /// V (Phase 4): the minimap2 detection metadata (binary / pin / path flag).
    #[test]
    fn minimap2_detection_metadata() {
        assert_eq!(binary_name(Aligner::Minimap2), "minimap2");
        assert_eq!(pinned_version(Aligner::Minimap2), "2.31-r1302");
        assert_eq!(path_flag(Aligner::Minimap2), "--path_to_minimap2");
    }

    /// V (Phase 4 / OQ-4a): minimap2 prints only the bare version number — the
    /// Bowtie 2 `… version x.y.z` parser would miss it, so a minimap2-specific
    /// parse takes the first non-empty line verbatim.
    #[test]
    fn parses_bare_minimap2_version() {
        assert_eq!(
            parse_minimap2_version("2.31-r1302\n").as_deref(),
            Some("2.31-r1302")
        );
        // leading blank line tolerated; trimmed.
        assert_eq!(
            parse_minimap2_version("\n  2.31-r1302  \n").as_deref(),
            Some("2.31-r1302")
        );
        // the Bowtie 2 banner parser must NOT match the bare number.
        assert_eq!(parse_bowtie2_version("2.31-r1302\n"), None);
    }

    #[test]
    fn rejects_non_triple() {
        assert!(!is_version_triple("2.5"));
        assert!(!is_version_triple("2.5.x"));
        assert!(is_version_triple("2.5.5"));
        assert!(is_version_triple("10.0.123"));
    }

    /// Phase 3 (T1): the rammap detection metadata (binary / pin / path flag).
    #[test]
    fn rammap_detection_metadata() {
        assert_eq!(binary_name(Aligner::Rammap), "rammap");
        assert_eq!(pinned_version(Aligner::Rammap), "1.1.1");
        assert_eq!(path_flag(Aligner::Rammap), "--path_to_rammap");
    }

    /// Phase 3 (T1, design#4): `rammap --version` prints `rammap 1.1.1` (a banner
    /// prefix, unlike minimap2's bare number) → take the LAST whitespace token.
    #[test]
    fn parses_rammap_version_strips_prefix() {
        assert_eq!(
            parse_rammap_version("rammap 1.1.1\n").as_deref(),
            Some("1.1.1")
        );
        assert_eq!(
            parse_rammap_version("  rammap 1.1.1  \n").as_deref(),
            Some("1.1.1")
        );
    }
}
