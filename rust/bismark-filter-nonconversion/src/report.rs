//! Byte-exact `*.non-conversion_filtering.txt` report formatting.
//!
//! Reproduces Perl `filter_non_conversion`'s SUMMARY (`process_file` lines
//! 311–353) plus the run-time line emitted after the `@ARGV` loop (line 664).
//! The report file content is part of the byte-identity gate, so every space,
//! tab, and newline here is load-bearing. Verified against live Perl by both
//! plan reviewers. The known SE/PE quirk: the PE count line has **two** spaces
//! before `in total` (line 314) while SE has **one** (line 318).

use std::fmt::Write as _;

use crate::filter::FilterMode;

/// The data needed to render a per-file filtering report (the SUMMARY block,
/// without the trailing run-time line — that is appended once, to the last
/// file's report, by the pipeline).
#[derive(Debug, Clone)]
pub struct FilterReport {
    /// The input path, echoed verbatim as supplied on the CLI (Perl `$infile`).
    pub infile: String,
    /// PE (`true`) vs SE (`false`).
    pub is_paired: bool,
    /// Reads analysed (SE) / read pairs analysed (PE).
    pub count: u64,
    /// Reads / pairs removed.
    pub kicked: u64,
    /// The decision mode (selects the "Sequences removed…" variant + values).
    pub mode: FilterMode,
}

impl FilterReport {
    /// Render the SUMMARY block (Line A + Line B). The trailing run-time line
    /// is NOT included here (see [`run_time_line`]).
    #[must_use]
    pub fn format(&self) -> String {
        let percent = if self.count == 0 {
            // Unreachable for `.bam`-named inputs (they die in the empty check
            // before process_file), but reachable for a header-only `*bam`
            // input that skips the dotted-gate empty check (SPEC §4.3 C1).
            "N/A".to_string()
        } else {
            format!("{:.1}", (self.kicked as f64) / (self.count as f64) * 100.0)
        };

        let mut s = String::with_capacity(256);

        // ── Line A: count ───────────────────────────────────────────────
        if self.is_paired {
            // NB: two spaces before "in total" (Perl line 314).
            writeln!(
                s,
                "Analysed read pairs (paired-end) in file >> {} <<  in total:\t{}",
                self.infile, self.count
            )
            .expect("write to String never fails");
        } else {
            // One space before "in total" (Perl line 318).
            writeln!(
                s,
                "Analysed sequences (single-end) in file >> {} << in total:\t{}",
                self.infile, self.count
            )
            .expect("write to String never fails");
        }

        // ── Line B: removed (four variants), trailing blank line ─────────
        let body = self.removed_line_body();
        // Each variant ends with `\n\n` (the removed line + a blank line).
        write!(s, "{body}\t{} ({percent}%)\n\n", self.kicked).expect("write to String never fails");

        s
    }

    /// The fixed prefix of the "Sequences removed…" line, up to (but not
    /// including) the `\t{kicked} ({percent}%)` tail. Picks one of the four
    /// Perl variants (lines 336/341/347/351).
    fn removed_line_body(&self) -> String {
        let head = "Sequences removed because of apparent non-bisulfite conversion";
        match self.mode {
            FilterMode::Percentage {
                cutoff,
                minimum_count,
            } => {
                if self.is_paired {
                    format!(
                        "{head} (at least {cutoff}% methylation and {minimum_count} non-CG calls \
                         in total in at least one of the reads):"
                    )
                } else {
                    format!(
                        "{head} (at least {cutoff}% methylation and {minimum_count} non-CG calls \
                         in total per read):"
                    )
                }
            }
            FilterMode::Threshold {
                threshold,
                consecutive,
            } => {
                // Perl `$insert = 'consecutive '` (trailing space) when --consecutive.
                let insert = if consecutive { "consecutive " } else { "" };
                if self.is_paired {
                    format!(
                        "{head} (at least {threshold} {insert}non-CG calls in one of the reads):"
                    )
                } else {
                    format!("{head} (at least {threshold} {insert}non-CG calls per read):")
                }
            }
        }
    }
}

/// The final run-time line appended to the **last** processed file's report
/// (Perl line 664). Single trailing `\n` (the STDERR `warn` at line 663 uses
/// `\n\n`; the report gets one). `elapsed_secs` is whole seconds, formatted
/// exactly as Perl's integer day/hour/min/sec breakdown.
///
/// Under SPEC D2 the byte-identity gate normalizes this line by
/// prefix/format, so the actual duration need not match Perl's (the Rust port
/// also omits Perl's two `sleep(1)` calls).
#[must_use]
pub fn run_time_line(elapsed_secs: u64) -> String {
    let days = elapsed_secs / (24 * 60 * 60);
    let hours = (elapsed_secs / (60 * 60)) % 24;
    let mins = (elapsed_secs / 60) % 60;
    let secs = elapsed_secs % 60;
    format!("filter_non_conversion completed in {days}d {hours}h {mins}m {secs}s\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thresh(consecutive: bool) -> FilterMode {
        FilterMode::Threshold {
            threshold: 3,
            consecutive,
        }
    }
    fn pct() -> FilterMode {
        FilterMode::Percentage {
            cutoff: 20,
            minimum_count: 5,
        }
    }

    #[test]
    fn se_threshold_default_byte_exact() {
        let r = FilterReport {
            infile: "foo.bam".into(),
            is_paired: false,
            count: 203,
            kicked: 2,
            mode: thresh(false),
        };
        // SE: one space before "in total"; 2/203 → 1.0%.
        let expected = "Analysed sequences (single-end) in file >> foo.bam << in total:\t203\n\
            Sequences removed because of apparent non-bisulfite conversion \
            (at least 3 non-CG calls per read):\t2 (1.0%)\n\n";
        assert_eq!(r.format(), expected);
    }

    #[test]
    fn pe_threshold_default_byte_exact_two_spaces() {
        let r = FilterReport {
            infile: "foo.bam".into(),
            is_paired: true,
            count: 2,
            kicked: 1,
            mode: thresh(false),
        };
        // PE: TWO spaces before "in total"; 1/2 → 50.0%.
        let expected = "Analysed read pairs (paired-end) in file >> foo.bam <<  in total:\t2\n\
            Sequences removed because of apparent non-bisulfite conversion \
            (at least 3 non-CG calls in one of the reads):\t1 (50.0%)\n\n";
        assert_eq!(r.format(), expected);
    }

    #[test]
    fn se_consecutive_insert() {
        let r = FilterReport {
            infile: "x.bam".into(),
            is_paired: false,
            count: 10,
            kicked: 3,
            mode: thresh(true),
        };
        assert!(
            r.format()
                .contains("(at least 3 consecutive non-CG calls per read):\t3 (30.0%)\n\n"),
            "got: {}",
            r.format()
        );
    }

    #[test]
    fn pe_consecutive_insert() {
        let r = FilterReport {
            infile: "x.bam".into(),
            is_paired: true,
            count: 10,
            kicked: 3,
            mode: thresh(true),
        };
        assert!(
            r.format().contains(
                "(at least 3 consecutive non-CG calls in one of the reads):\t3 (30.0%)\n\n"
            ),
            "got: {}",
            r.format()
        );
    }

    #[test]
    fn se_percentage_variant() {
        let r = FilterReport {
            infile: "x.bam".into(),
            is_paired: false,
            count: 100,
            kicked: 7,
            mode: pct(),
        };
        let expected = "Analysed sequences (single-end) in file >> x.bam << in total:\t100\n\
            Sequences removed because of apparent non-bisulfite conversion \
            (at least 20% methylation and 5 non-CG calls in total per read):\t7 (7.0%)\n\n";
        assert_eq!(r.format(), expected);
    }

    #[test]
    fn pe_percentage_variant() {
        let r = FilterReport {
            infile: "x.bam".into(),
            is_paired: true,
            count: 100,
            kicked: 7,
            mode: pct(),
        };
        let expected = "Analysed read pairs (paired-end) in file >> x.bam <<  in total:\t100\n\
            Sequences removed because of apparent non-bisulfite conversion \
            (at least 20% methylation and 5 non-CG calls in total in at least one of the reads):\t7 (7.0%)\n\n";
        assert_eq!(r.format(), expected);
    }

    #[test]
    fn na_branch_when_count_zero() {
        // Reachable via a header-only `*bam` (no dot) input (SPEC §4.3 C1).
        let r = FilterReport {
            infile: "emptyfoobam".into(),
            is_paired: false,
            count: 0,
            kicked: 0,
            mode: thresh(false),
        };
        let expected = "Analysed sequences (single-end) in file >> emptyfoobam << in total:\t0\n\
            Sequences removed because of apparent non-bisulfite conversion \
            (at least 3 non-CG calls per read):\t0 (N/A%)\n\n";
        assert_eq!(r.format(), expected);
    }

    #[test]
    fn report_line_rounding_one_third() {
        // 1/3 → 33.3% (report-line %.1f rounding, distinct from per-read %).
        let r = FilterReport {
            infile: "x.bam".into(),
            is_paired: false,
            count: 3,
            kicked: 1,
            mode: thresh(false),
        };
        assert!(
            r.format().contains("\t1 (33.3%)\n\n"),
            "got: {}",
            r.format()
        );
    }

    #[test]
    fn run_time_line_format() {
        assert_eq!(
            run_time_line(0),
            "filter_non_conversion completed in 0d 0h 0m 0s\n"
        );
        // 1d 2h 3m 4s = 93784 s.
        assert_eq!(
            run_time_line(93_784),
            "filter_non_conversion completed in 1d 2h 3m 4s\n"
        );
        // 25 h wraps to 1d 1h.
        assert_eq!(
            run_time_line(90_000),
            "filter_non_conversion completed in 1d 1h 0m 0s\n"
        );
    }
}
