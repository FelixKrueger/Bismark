//! Minimal STDERR logger. Diagnostics are never part of the byte-identity gate;
//! `--verbose` gates the extra detail. Mirrors `bismark-genome-preparation`.

/// Tiny logger: `note` always prints (Perl `warn`-level); `info` only with
/// `--verbose` (Perl `$verbose and print`).
#[derive(Debug, Clone, Copy)]
pub struct Logger {
    verbose: bool,
}

impl Logger {
    /// Construct from the `--verbose` flag.
    pub fn new(verbose: bool) -> Self {
        Logger { verbose }
    }

    /// Always emitted to STDERR (Perl `warn`).
    pub fn note(&self, msg: &str) {
        eprintln!("{msg}");
    }

    /// Emitted only under `--verbose` (Perl `$verbose and print`).
    pub fn info(&self, msg: &str) {
        if self.verbose {
            eprintln!("{msg}");
        }
    }
}
