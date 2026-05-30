//! STDERR diagnostics — a logger gated by `--quiet`.
//!
//! All diagnostic output goes to STDERR, matching Perl `methylation_consistency`'s
//! `warn` model (stdout stays clean; only `--version` writes to stdout, in
//! `main.rs`). These messages are NOT part of the byte-identity gate (SPEC §7)
//! — they mirror Perl's `warn` text in spirit. The `sleep(3)` after the CHH
//! warning is intentionally dropped (UX artifact, not output; SPEC §4.7).

use std::io::Write;

/// STDERR diagnostics logger. `Copy` so it can be passed around cheaply.
#[derive(Debug, Clone, Copy)]
pub struct Logger {
    quiet: bool,
}

impl Logger {
    /// Construct a logger. `quiet` suppresses all diagnostic output.
    #[must_use]
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }

    /// Write `s` to `w` unless `--quiet`. Returns whether it wrote — the
    /// seam the unit tests use to assert the quiet gate without a real stderr.
    pub fn info_to<W: Write>(&self, w: &mut W, s: &str) -> bool {
        if self.quiet {
            return false;
        }
        let _ = w.write_all(s.as_bytes());
        true
    }

    /// Write `s` to STDERR unless `--quiet`.
    pub fn info(&self, s: &str) {
        if self.quiet {
            return;
        }
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(s.as_bytes());
    }

    /// The upper/lower threshold banner (Perl line 91).
    pub fn thresholds(&self, lower: i64, upper: i64) {
        self.info(&format!(
            "Upper and lower methylation thresholds given as:\nUpper: {upper}\nLower: {lower}\n\n"
        ));
    }

    /// The experimental-CHH warning (Perl lines 10–11; the `sleep(3)` is
    /// dropped).
    pub fn chh_experimental(&self) {
        self.info(
            "     ~~~~~     \nTHIS IS AN EXPERIMENTAL VERSION that works on CHH context and \
             **NOT** on the usual CpG context. You have been warned!\n     ~~~~\n\n",
        );
    }

    /// The per-file "Now processing file" line (Perl line 140).
    pub fn processing_file(&self, file: &str) {
        self.info(&format!("Now processing file: {file}\n     ~~~~~\n"));
    }

    /// The per-file STDERR summary echo (Perl lines 319–333): a
    /// `Summary for <file>:` header followed by the rendered report body.
    pub fn summary(&self, file: &str, report_body: &str) {
        self.info(&format!("Summary for {file}:\n\n"));
        self.info(report_body);
    }

    /// Note that an empty input file is being skipped (Perl line 145).
    pub fn skipping_empty(&self) {
        self.info("Skipping this file altogether\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_gate_suppresses_info_and_returns_false() {
        let quiet = Logger::new(true);
        let loud = Logger::new(false);
        let mut bq: Vec<u8> = Vec::new();
        let mut bl: Vec<u8> = Vec::new();
        assert!(!quiet.info_to(&mut bq, "hello\n"));
        assert!(bq.is_empty());
        assert!(loud.info_to(&mut bl, "hello\n"));
        assert_eq!(bl, b"hello\n");
    }
}
