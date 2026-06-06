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
use crate::config::{Aligner, GapPenalties, ReadFormat};
use crate::error::{AlignerError, Result};

/// Build the `aligner_options` string + the (read,ref) gap penalties used later
/// for MAPQ. `is_paired` gates the PE-only flags. The Bowtie 2 string is built
/// unchanged for every `aligner`; the HISAT2 delta is **appended to the finished
/// string** (mirrors Perl's last-push at 8314 — keeps Bowtie 2 structurally frozen).
pub fn build_aligner_options(
    cli: &Cli,
    aligner: Aligner,
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

    // 12. PE flags. --no-mixed/--no-discordant are pushed for BOTH Bowtie 2 and
    // HISAT2 (Perl 8044-8045, the `if($bowtie2)` is commented out). --dovetail is
    // Bowtie 2-only (Perl gates it `if($bowtie2)`, 8051-8059 — "HISAT2 doesn't
    // have the concept of --dovetail"); its --old_flag conflict is likewise
    // Bowtie 2-only.
    if is_paired {
        opts.push("--no-mixed".into());
        opts.push("--no-discordant".into());
        // --dovetail is auto-set unless --no_dovetail; conflicts with --old_flag.
        if aligner == Aligner::Bowtie2 && !cli.no_dovetail {
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

    // 16. HISAT2 tail (Perl 8286-8326), appended to the FINISHED string — after
    // the PE tail, --maxins, and --quiet — so the Bowtie 2 string is unchanged.
    // For minimap2 this still runs (the `else` branch dies on HISAT2-only splice
    // flags, Perl 8317-8324) but the result is discarded by the clean-slate below.
    let options = apply_aligner_specific_options(opts.join(" "), cli, aligner)?;

    // 17. minimap2 — CLEAN SLATE (Perl 8359 `@aligner_options = ()`). The Bowtie 2
    // base assembled above is thrown away; minimap2 needs a completely different
    // option set. Building (and thus validating: -N range, --score_min shape, the
    // splice-flag dies) the base first, then substituting, mirrors Perl's
    // build-then-wipe order — so e.g. `-N 2 --minimap2` still dies. Gap penalties
    // are vestigial (unused by `calc_mapq`), so the defaults are returned.
    if aligner == Aligner::Minimap2 {
        return Ok((minimap2_options(cli)?, gp));
    }
    Ok((options, gp))
}

/// Assemble the minimap2 `aligner_options` from a clean slate (Perl 8358-8413).
/// Push order: `-a` → `--MD` → `--secondary=no` → `-t 2` → `-x <preset>` →
/// `-K 250K`. The `-t 2` is hardcoded by Bismark (8372) — independent of any
/// `--multicore`/`-p` choice (`-p`/`--reorder` were pushed to the Bowtie 2 base,
/// which this discards). Preset (Perl 8374-8408):
/// - `--mm2_short_reads` → `sr`,
/// - `--mm2_pacbio` → `map-pb`,
/// - default **or** an explicit `--mm2_nanopore` → `map-ont` (the `else` serves
///   both — Perl sets `$mm2_nanopore=1` in the default case, 8405).
///
/// Preset-conflict dies (8375/8378/8391): short⊕nanopore, short⊕pacbio,
/// pacbio⊕nanopore. (The `--mm2_*`-without-`--minimap2` dies + the max-length
/// range/default live in `config::resolve_mm2_max_length`, mirroring Perl's
/// separate `unless($mm2)` / `if($mm2)` blocks.)
fn minimap2_options(cli: &Cli) -> Result<String> {
    let preset = if cli.mm2_short_read {
        if cli.mm2_nanopore {
            return Err(AlignerError::Validation(
                "Please select minimap2 in Short Read or Nanopore mode, but not both...".into(),
            ));
        }
        if cli.mm2_pacbio {
            return Err(AlignerError::Validation(
                "Please select minimap2 in Short Read or PacBio mode, but not both...".into(),
            ));
        }
        "sr"
    } else if cli.mm2_pacbio {
        if cli.mm2_nanopore {
            return Err(AlignerError::Validation(
                "Please select minimap2 in PacBio or Nanopore mode, but not both...".into(),
            ));
        }
        "map-pb"
    } else {
        // Default OR explicit `--mm2_nanopore` → ONT (Perl 8404-8408).
        "map-ont"
    };
    Ok(format!("-a --MD --secondary=no -t 2 -x {preset} -K 250K"))
}

/// Append the HISAT2-specific option tail (Perl `process_command_line` 8286-8326,
/// the `### ADDITIONAL ALIGNMENT OPTIONS WE NEED FOR HISAT2` block) to the
/// finished Bowtie 2 option string. For HISAT2 the tail, in Perl order, is
/// `[--no-spliced-alignment] [--known-splicesite-infile <f>] --no-softclip
/// --omit-sec-seq` (splice flags BEFORE the softclip delta). For any non-HISAT2
/// aligner the splice flags are a hard error (Perl 8319-8324 — closes a
/// pre-existing Bowtie 2 silent-no-op gap).
///
/// NB: `--local` is rejected upstream for every aligner (off the v1 byte-identity
/// spine), so Perl's experimental HISAT2+`--local` path (`--omit-sec-seq` only,
/// 8310-8312) is intentionally not reproduced — the default endToEnd tail
/// (`--no-softclip --omit-sec-seq`) is the only HISAT2 path v1 supports.
fn apply_aligner_specific_options(base: String, cli: &Cli, aligner: Aligner) -> Result<String> {
    if aligner != Aligner::Hisat2 {
        if cli.nosplice {
            return Err(AlignerError::Validation(
                "The option --no-spliced-alignment can only be selected in HISAT2 mode! Please \
                 re-specificy!"
                    .into(),
            ));
        }
        if cli.known_splices.is_some() {
            return Err(AlignerError::Validation(
                "The option --known-splicesite-infile can only be selected in HISAT2 mode! Please \
                 re-specificy!"
                    .into(),
            ));
        }
        return Ok(base);
    }

    let mut tail: Vec<String> = Vec::new();
    if cli.nosplice {
        if cli.known_splices.is_some() {
            return Err(AlignerError::Validation(
                "You cannot run Bismark in HISAT2 mode with known splice junctions but without \
                 spliced alignments! Please respecify!"
                    .into(),
            ));
        }
        tail.push("--no-spliced-alignment".into());
    }
    if let Some(infile) = &cli.known_splices {
        if !infile.exists() {
            return Err(AlignerError::Validation(format!(
                "Known splice site infile >{}< did not exist. Please check file name and try again!",
                infile.display()
            )));
        }
        tail.push(format!("--known-splicesite-infile {}", infile.display()));
    }
    // endToEnd alignments (default) — Perl 8314.
    tail.push("--no-softclip --omit-sec-seq".into());

    Ok(format!("{base} {}", tail.join(" ")))
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

/// Parse `--score_min` into the numeric `(intercept, slope)` for `calc_mapq`
/// (default `(0.0, -0.2)`). Splits on the LAST comma (Perl's greedy
/// `^L,(.+),(.+)$`). `--local` (G-form) is rejected in `build_aligner_options`,
/// so only the end-to-end `L` form reaches here.
pub fn score_min_params(cli: &Cli) -> Result<(f64, f64)> {
    match &cli.score_min {
        None => Ok((0.0, -0.2)),
        Some(s) => {
            let rest = s.strip_prefix("L,").ok_or_else(|| {
                AlignerError::Validation(
                    "--score_min must be of the form L,<intercept>,<slope>".into(),
                )
            })?;
            let (i, sl) = rest.rsplit_once(',').ok_or_else(|| {
                AlignerError::Validation(
                    "--score_min must be of the form L,<intercept>,<slope>".into(),
                )
            })?;
            let intercept = i.parse::<f64>().map_err(|_| {
                AlignerError::Validation(format!("bad --score_min intercept '{i}'"))
            })?;
            let slope = sl
                .parse::<f64>()
                .map_err(|_| AlignerError::Validation(format!("bad --score_min slope '{sl}'")))?;
            Ok((intercept, slope))
        }
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
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-q --score-min L,0,-0.2 --ignore-quals");
    }

    #[test]
    fn seed_flags_precede_score_min_and_quiet_is_last() {
        // Bismark input flags `-n`/`-l` map to Bowtie 2 output flags `-N`/`-L`.
        let cli = cli_from(&["-n", "1", "-l", "20", "--quiet"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            opts,
            "-q -N 1 -L 20 --score-min L,0,-0.2 --ignore-quals --quiet"
        );
    }

    #[test]
    fn score_min_override_substituted() {
        let cli = cli_from(&["--score_min", "L,0,-0.4"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-q --score-min L,0,-0.4 --ignore-quals");
    }

    #[test]
    fn paired_end_tail_and_default_maxins() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, true).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500"
        );
    }

    #[test]
    fn fasta_uses_dash_f() {
        let cli = cli_from(&["-f"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastA, false).unwrap();
        assert_eq!(opts, "-f --score-min L,0,-0.2 --ignore-quals");
    }

    #[test]
    fn rejects_bad_seedmms() {
        let cli = cli_from(&["-n", "2"]);
        assert!(build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).is_err());
    }

    #[test]
    fn rejects_local_in_v1() {
        let cli = cli_from(&["--local"]);
        assert!(build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).is_err());
    }

    #[test]
    fn phred_without_fastq_errors() {
        let cli = cli_from(&["--phred33-quals", "-f"]);
        assert!(build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastA, false).is_err());
    }

    #[test]
    fn rdg_rfg_appended_and_validated() {
        let cli = cli_from(&["--rdg", "5,3", "--rfg", "5,3"]);
        let (opts, gp) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --rdg 5,3 --rfg 5,3 --ignore-quals"
        );
        assert_eq!((gp.deletion_open, gp.insertion_open), (5, 5));
        let bad = cli_from(&["--rdg", "5"]);
        assert!(build_aligner_options(&bad, Aligner::Bowtie2, ReadFormat::FastQ, false).is_err());
    }

    // ---- HISAT2 option assembly (Phase 2a) ---------------------------------

    /// V2: the default SE HISAT2 string = the Bowtie 2 base + `--no-softclip
    /// --omit-sec-seq` appended last (spike Q2, Perl 8314).
    #[test]
    fn hisat2_se_option_string() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq"
        );
    }

    /// V3 (hard literal): the default PE HISAT2 string — the softclip delta lands
    /// AFTER the PE tail + `--maxins 500`, and there is **NO `--dovetail`** (Perl
    /// gates dovetail `if($bowtie2)`, 8051-8059).
    #[test]
    fn hisat2_pe_option_string_has_no_dovetail() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, true).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq"
        );
        assert!(!opts.contains("--dovetail"));
    }

    /// V1: passing the new `aligner` param does NOT change the Bowtie 2 PE string
    /// (the dovetail kind-gating must keep Bowtie 2 PE byte-frozen).
    #[test]
    fn bowtie2_pe_string_byte_frozen_with_aligner_param() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, true).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500"
        );
    }

    /// V8: `--no-spliced-alignment` appends before the softclip delta (Perl 8295).
    #[test]
    fn hisat2_nosplice_appends_before_softclip() {
        let cli = cli_from(&["--no-spliced-alignment"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-spliced-alignment --no-softclip --omit-sec-seq"
        );
    }

    /// V8: `--known-splicesite-infile <f>` (existing file) appends before the
    /// softclip delta (Perl 8298-8306).
    #[test]
    fn hisat2_known_splices_appends() {
        let tmp = tempfile::TempDir::new().unwrap();
        let infile = tmp.path().join("splices.txt");
        std::fs::write(&infile, b"x").unwrap();
        let cli = cli_from(&["--known-splicesite-infile", infile.to_str().unwrap()]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            opts,
            format!(
                "-q --score-min L,0,-0.2 --ignore-quals --known-splicesite-infile {} --no-softclip --omit-sec-seq",
                infile.display()
            )
        );
    }

    /// V8: HISAT2 + both splice flags dies (Perl 8290).
    #[test]
    fn hisat2_both_splice_flags_die() {
        let tmp = tempfile::TempDir::new().unwrap();
        let infile = tmp.path().join("splices.txt");
        std::fs::write(&infile, b"x").unwrap();
        let cli = cli_from(&[
            "--no-spliced-alignment",
            "--known-splicesite-infile",
            infile.to_str().unwrap(),
        ]);
        assert!(build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false).is_err());
    }

    /// V8: HISAT2 + a missing known-splicesite infile dies (Perl 8304).
    #[test]
    fn hisat2_known_splices_missing_file_dies() {
        let cli = cli_from(&["--known-splicesite-infile", "/no/such/splices.txt"]);
        assert!(build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false).is_err());
    }

    /// V8: the splice flags are HISAT2-only — they die in Bowtie 2 mode (Perl
    /// 8319-8324; closes a pre-existing Bowtie 2 silent-no-op gap).
    #[test]
    fn non_hisat2_splice_flags_die() {
        let cli = cli_from(&["--no-spliced-alignment"]);
        assert!(build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).is_err());
        let cli = cli_from(&["--known-splicesite-infile", "/no/such/splices.txt"]);
        assert!(build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).is_err());
    }

    // ---- minimap2 clean-slate option assembly (Phase 4) --------------------

    /// V2 (hard literal): the default minimap2 string — clean-slate, `-x map-ont`
    /// (NOT `-ax sr`), `-t 2` hardcoded (Perl 8358-8413; spike Q2).
    #[test]
    fn minimap2_default_option_string() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-ont -K 250K");
    }

    /// V3: preset selection — `--mm2_short_reads`→`sr`, `--mm2_pacbio`→`map-pb`,
    /// explicit `--mm2_nanopore`→`map-ont` (same as default, Perl 8404-8408).
    #[test]
    fn minimap2_preset_selection() {
        let sr = cli_from(&["--mm2_short_reads"]);
        let (opts, _) =
            build_aligner_options(&sr, Aligner::Minimap2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x sr -K 250K");

        let pb = cli_from(&["--mm2_pacbio"]);
        let (opts, _) =
            build_aligner_options(&pb, Aligner::Minimap2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-pb -K 250K");

        let ont = cli_from(&["--mm2_nanopore"]);
        let (opts, _) =
            build_aligner_options(&ont, Aligner::Minimap2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-ont -K 250K");
    }

    /// V3: preset-conflict dies (Perl 8375/8378/8391).
    #[test]
    fn minimap2_preset_conflicts_die() {
        for conflict in [
            ["--mm2_short_reads", "--mm2_nanopore"],
            ["--mm2_short_reads", "--mm2_pacbio"],
            ["--mm2_pacbio", "--mm2_nanopore"],
        ] {
            let cli = cli_from(&conflict);
            assert!(
                build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false).is_err(),
                "{conflict:?} should die"
            );
        }
    }

    /// The clean slate truly WIPES the Bowtie 2 base: `-N`/`-L`/`--rdg`/`-q` set on
    /// the command line do NOT appear in the minimap2 string (Perl `@aligner_options=()`).
    #[test]
    fn minimap2_clean_slate_discards_bowtie2_flags() {
        let cli = cli_from(&["-n", "1", "-l", "20", "--rdg", "5,3"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-ont -K 250K");
        assert!(!opts.contains("-N") && !opts.contains("-L") && !opts.contains("--rdg"));
        assert!(!opts.contains("--score-min") && !opts.contains("--ignore-quals"));
    }

    /// Build-then-wipe parity: a Bowtie 2-base validation (`-N 2` out of range)
    /// STILL dies in minimap2 mode (Perl runs the base block before the wipe).
    #[test]
    fn minimap2_still_validates_bowtie2_base() {
        let cli = cli_from(&["-n", "2"]);
        assert!(build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false).is_err());
    }

    /// V1 regression: the Minimap2 branch must not perturb the Bowtie 2 / HISAT2
    /// strings (they are unchanged from the dedicated tests above — re-pinned here
    /// alongside the new branch as a guard).
    #[test]
    fn bowtie2_hisat2_strings_byte_frozen_alongside_minimap2() {
        let cli = cli_from(&[]);
        let (bt2, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(bt2, "-q --score-min L,0,-0.2 --ignore-quals");
        let (ht2, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false).unwrap();
        assert_eq!(
            ht2,
            "-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq"
        );
    }
}
