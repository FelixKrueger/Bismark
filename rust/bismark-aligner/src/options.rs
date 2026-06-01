//! Assembly of the Bowtie 2 `aligner_options` string — **byte-identity-critical**.
//!
//! The push order below is verified against Perl `bismark` 7838–8142 and is the
//! literal option string each Bowtie 2 instance receives (the per-instance
//! `--norc`/`--nofw` is added later, NOT here). Order (rev-1, dual-review):
//! `-q`/`-f` → `--phred33`/`--phred64` → `-N` → `-L` → `-D` → `-R` →
//! `--score-min` → `--rdg` → `--rfg` → `-p`/`--reorder` → `--ignore-quals` →
//! (PE: `--no-mixed`/`--no-discordant`/`--dovetail`) → `--minins` →
//! `--maxins`/`--maxins 500` → `--quiet`.

use crate::cli::Cli;
use crate::config::{GapPenalties, ReadFormat};
use crate::error::{AlignerError, Result};

/// Build the `aligner_options` string + the (read,ref) gap penalties used later
/// for MAPQ. `is_paired` gates the PE-only flags.
pub fn build_aligner_options(
    cli: &Cli,
    format: ReadFormat,
    is_paired: bool,
) -> Result<(String, GapPenalties)> {
    let mut opts: Vec<String> = Vec::new();

    // 1. format
    match format {
        ReadFormat::FastQ => opts.push("-q".into()),
        ReadFormat::FastA => opts.push("-f".into()),
    }

    // 2. phred (mutually exclusive; each requires FASTQ)
    if cli.phred33 && cli.phred64 {
        return Err(AlignerError::Validation(
            "You can only specify one type of quality value at a time! (--phred33 or --phred64)"
                .into(),
        ));
    }
    if cli.phred33 {
        require_fastq(format)?;
        opts.push("--phred33".into());
    }
    if cli.phred64 {
        require_fastq(format)?;
        opts.push("--phred64".into());
    }

    // 3. -N (0 or 1)
    if let Some(n) = cli.seedmms {
        if n == 0 || n == 1 {
            opts.push(format!("-N {n}"));
        } else {
            return Err(AlignerError::Validation(
                "Please set the number of multiseed mismatches with '-N <int>' (where <int> can be 0 or 1)".into(),
            ));
        }
    }
    // 4. -L
    if let Some(l) = cli.seedlen {
        opts.push(format!("-L {l}"));
    }
    // 5. -D  6. -R
    if let Some(d) = cli.seed_extension_fails {
        opts.push(format!("-D {d}"));
    }
    if let Some(r) = cli.reseed_repetitive_seeds {
        opts.push(format!("-R {r}"));
    }

    // 7. --score-min (always). v1 is end-to-end only — reject --local.
    if cli.local {
        return Err(AlignerError::Unsupported(
            "Bowtie 2 --local mode is not supported in this version (off the byte-identity spine); \
             use end-to-end (default)."
                .into(),
        ));
    }
    let score_min = match &cli.score_min {
        Some(s) => {
            if !valid_score_min_l(s) {
                return Err(AlignerError::Validation(
                    "In end-to-end mode (default) the option '--score_min <func>' needs to be in the \
                     format <L,value,value>. Please consult \"setting up functions\" in the Bowtie 2 \
                     manual for further information"
                        .into(),
                ));
            }
            s.clone()
        }
        None => "L,0,-0.2".to_string(),
    };
    opts.push(format!("--score-min {score_min}"));

    // 8/9. rdg / rfg (validate int,int). Defaults 5,3 are internal scalars only.
    let mut gp = GapPenalties {
        deletion_open: 5,
        deletion_extend: 3,
        insertion_open: 5,
        insertion_extend: 3,
    };
    if let Some(rdg) = &cli.rdg {
        let (a, b) = parse_int_pair(rdg).ok_or_else(|| {
            AlignerError::Validation(
                "The option '--rdg <int1>,<int2>' needs to be in the format <integer,integer>. Please \
                 consult \"setting up functions\" in the Bowtie 2 manual for further information"
                    .into(),
            )
        })?;
        gp.deletion_open = a;
        gp.deletion_extend = b;
        opts.push(format!("--rdg {rdg}"));
    }
    if let Some(rfg) = &cli.rfg {
        let (a, b) = parse_int_pair(rfg).ok_or_else(|| {
            AlignerError::Validation(
                "The option '--rfg <int1>,<int2>' needs to be in the format <integer,integer>. Please \
                 consult \"setting up functions\" in the Bowtie 2 manual for further information"
                    .into(),
            )
        })?;
        gp.insertion_open = a;
        gp.insertion_extend = b;
        opts.push(format!("--rfg {rfg}"));
    }

    // 10. -p + --reorder (Bowtie 2 intra-instance threads)
    if let Some(p) = cli.bowtie_threads {
        if p < 2 {
            return Err(AlignerError::Validation(
                "Please select a value for -p of 2 or more!".into(),
            ));
        }
        opts.push(format!("-p {p}"));
        opts.push("--reorder".into());
    }

    // 11. --ignore-quals (ALWAYS) — NOT last; PE/insert/quiet follow.
    opts.push("--ignore-quals".into());

    // 12. PE flags
    if is_paired {
        opts.push("--no-mixed".into());
        opts.push("--no-discordant".into());
        // --dovetail is auto-set unless --no_dovetail; conflicts with --old_flag.
        if !cli.no_dovetail {
            if cli.old_flag {
                return Err(AlignerError::Validation(
                    "The option '--dovetail' may only be specified with the current SAM FLAG values. \
                     Please respecify..."
                        .into(),
                ));
            }
            opts.push("--dovetail".into());
        }
    }

    // 13/14. minins / maxins (PE-only; SE error if given)
    if let Some(i) = cli.minins {
        if !is_paired {
            return Err(AlignerError::Validation(
                "-I/--minins can only be used for paired-end mapping!".into(),
            ));
        }
        opts.push(format!("--minins {i}"));
    }
    if let Some(x) = cli.maxins {
        if !is_paired {
            return Err(AlignerError::Validation(
                "-X/--maxins can only be used for paired-end mapping!".into(),
            ));
        }
        opts.push(format!("--maxins {x}"));
    } else if is_paired {
        opts.push("--maxins 500".into());
    }

    // 15. --quiet
    if cli.quiet {
        opts.push("--quiet".into());
    }

    Ok((opts.join(" "), gp))
}

fn require_fastq(format: ReadFormat) -> Result<()> {
    if format == ReadFormat::FastQ {
        Ok(())
    } else {
        Err(AlignerError::Validation(
            "Phred quality values work only when -q (FASTQ) is specified".into(),
        ))
    }
}

/// Shape-only validation of `--score_min` for end-to-end mode: `L,<a>,<b>` with
/// non-empty `a`/`b` (mirrors Perl's permissive `^L,(.+),(.+)$` — content is not
/// numerically validated). Pushing the original string is byte-equivalent to
/// Perl's `L,$1,$2` reconstruction.
fn valid_score_min_l(s: &str) -> bool {
    match s.strip_prefix("L,").and_then(|rest| rest.split_once(',')) {
        Some((a, b)) => !a.is_empty() && !b.is_empty(),
        None => false,
    }
}

/// Parse `<uint>,<uint>` (Perl `^(\d+),(\d+)$`).
fn parse_int_pair(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once(',')?;
    if a.is_empty() || b.is_empty() {
        return None;
    }
    if !a.bytes().all(|c| c.is_ascii_digit()) || !b.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn cli_from(args: &[&str]) -> Cli {
        let mut v = vec!["bismark_rs"];
        v.extend_from_slice(args);
        Cli::parse_from(v)
    }

    #[test]
    fn default_se_options_match_phase0_spike() {
        let cli = cli_from(&[]);
        let (opts, _) = build_aligner_options(&cli, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-q --score-min L,0,-0.2 --ignore-quals");
    }

    #[test]
    fn seed_flags_precede_score_min_and_quiet_is_last() {
        // Bismark input flags `-n`/`-l` map to Bowtie 2 output flags `-N`/`-L`.
        let cli = cli_from(&["-n", "1", "-l", "20", "--quiet"]);
        let (opts, _) = build_aligner_options(&cli, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            opts,
            "-q -N 1 -L 20 --score-min L,0,-0.2 --ignore-quals --quiet"
        );
    }

    #[test]
    fn score_min_override_substituted() {
        let cli = cli_from(&["--score_min", "L,0,-0.4"]);
        let (opts, _) = build_aligner_options(&cli, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-q --score-min L,0,-0.4 --ignore-quals");
    }

    #[test]
    fn paired_end_tail_and_default_maxins() {
        let cli = cli_from(&[]);
        let (opts, _) = build_aligner_options(&cli, ReadFormat::FastQ, true).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500"
        );
    }

    #[test]
    fn fasta_uses_dash_f() {
        let cli = cli_from(&["-f"]);
        let (opts, _) = build_aligner_options(&cli, ReadFormat::FastA, false).unwrap();
        assert_eq!(opts, "-f --score-min L,0,-0.2 --ignore-quals");
    }

    #[test]
    fn rejects_bad_seedmms() {
        let cli = cli_from(&["-n", "2"]);
        assert!(build_aligner_options(&cli, ReadFormat::FastQ, false).is_err());
    }

    #[test]
    fn rejects_local_in_v1() {
        let cli = cli_from(&["--local"]);
        assert!(build_aligner_options(&cli, ReadFormat::FastQ, false).is_err());
    }

    #[test]
    fn phred_without_fastq_errors() {
        let cli = cli_from(&["--phred33-quals", "-f"]);
        assert!(build_aligner_options(&cli, ReadFormat::FastA, false).is_err());
    }

    #[test]
    fn rdg_rfg_appended_and_validated() {
        let cli = cli_from(&["--rdg", "5,3", "--rfg", "5,3"]);
        let (opts, gp) = build_aligner_options(&cli, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --rdg 5,3 --rfg 5,3 --ignore-quals"
        );
        assert_eq!((gp.deletion_open, gp.insertion_open), (5, 5));
        let bad = cli_from(&["--rdg", "5"]);
        assert!(build_aligner_options(&bad, ReadFormat::FastQ, false).is_err());
    }
}
