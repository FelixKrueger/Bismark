//! Assembly of the Bowtie 2 `aligner_options` string — **byte-identity-critical**.
//!
//! The push order below is verified against Perl `bismark` 7838–8142 and is the
//! literal option string each Bowtie 2 instance receives (the per-instance
//! `--norc`/`--nofw` is added later, NOT here). Order (rev-1, dual-review):
//! `-q`/`-f` → `--phred33`/`--phred64` → `-N` → `-L` → `-D` → `-R` →
//! `--score-min` → `--rdg` → `--rfg` → `-p`/`--reorder` → `--ignore-quals` →
//! (PE: `--no-mixed`/`--no-discordant`/`--dovetail`) → `--minins` →
//! `--maxins`/`--maxins 500` → `--quiet`.

use crate::aligner::cli::Cli;
use crate::aligner::config::{Aligner, GapPenalties, ReadFormat};
use crate::aligner::error::{AlignerError, Result};

/// Build the `aligner_options` string + the (read,ref) gap penalties used later
/// for MAPQ. `is_paired` gates the PE-only flags. The Bowtie 2 string is built
/// unchanged for every `aligner`; the HISAT2 delta is **appended to the finished
/// string** (mirrors Perl's last-push at 8314 — keeps Bowtie 2 structurally frozen).
pub fn build_aligner_options(
    cli: &Cli,
    aligner: Aligner,
    format: ReadFormat,
    is_paired: bool,
    // HISAT2 Approach B-faithful: when `--hisat2 --multicore N` is remapped to a single
    // instance with `-p N` (see `config::resolve`), this carries that `N`. The `-p` block
    // below falls back to the explicit `cli.bowtie_threads` when this is `None`. `None`
    // for every other path (Bowtie 2, minimap2, single-core HISAT2).
    hisat2_multicore_threads: Option<u32>,
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

    // 7. --local / --score-min (Perl 7896-7948). Branches:
    //  - Bowtie 2 --local: push `--local` + G-form `--score-min G,<i>,<s>` (default G,20,8).
    //  - HISAT2 --local OR end-to-end (any aligner): L-form `--score-min L,<i>,<s>` (default
    //    L,0,-0.2), NO `--local`. HISAT2-local uses the SAME L-form as end-to-end (Perl
    //    7912/7947) — its local-ness is the dropped `--no-softclip` in the HISAT2 tail + the
    //    ln() MAPQ scMin, NOT this option. minimap2-local is rejected in `config::resolve`.
    if cli.local && aligner == Aligner::Bowtie2 {
        opts.push("--local".into());
        let score_min = match &cli.score_min {
            Some(s) => {
                if !valid_score_min_g(s) {
                    return Err(AlignerError::Validation(
                        "In Bowtie 2 --local mode, the option '--score_min <func>' needs to be in the \
                         format <G,value,value>. Please consult \"setting up functions\" in the Bowtie 2 \
                         manual for further information"
                            .into(),
                    ));
                }
                s.clone()
            }
            None => "G,20,8".to_string(),
        };
        opts.push(format!("--score-min {score_min}"));
    } else {
        let score_min = match &cli.score_min {
            Some(s) => {
                if !valid_score_min_l(s) {
                    // HISAT2-local and end-to-end both take the L-form; name the mode so the
                    // message isn't mislabelled (Perl 7909 vs 7918 — error text, not gated).
                    let mode = if cli.local {
                        "HISAT2 --local"
                    } else {
                        "end-to-end (default)"
                    };
                    return Err(AlignerError::Validation(format!(
                        "In {mode} mode, the option '--score_min <func>' needs to be in the format \
                         <L,value,value>. Please consult \"setting up functions\" in the Bowtie 2 \
                         manual for further information"
                    )));
                }
                s.clone()
            }
            None => "L,0,-0.2".to_string(),
        };
        opts.push(format!("--score-min {score_min}"));
    }

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

    // 10. -p + --reorder (Bowtie 2 / HISAT2 intra-instance threads, Perl 7993-8007).
    // Explicit `-p` (`cli.bowtie_threads`) takes precedence; otherwise, for HISAT2 the
    // `--multicore N` remap (`config::resolve`) supplies `N` here. `--reorder` is
    // mandatory with `-p` (it restores input order — Perl 7999).
    if let Some(p) = cli.bowtie_threads.or(hisat2_multicore_threads) {
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
    // rammap is minimap-like: it reuses the identical clean-slate `map-ont`
    // option assembly + `--mm2_*` knobs (design#2/#3), so the same branch fires.
    if matches!(aligner, Aligner::Minimap2 | Aligner::Rammap) {
        return Ok((minimap2_options(cli)?, gp));
    }
    Ok((options, gp))
}

/// Assemble the minimap2 `aligner_options` from a clean slate (Perl 8358-8413).
/// Push order: `-a` → `--MD` → `--secondary=no` → `-t <N>` → `-x <preset>` →
/// `-K 250K`. `-t` follows Bismark's `-p` thread knob (`cli.bowtie_threads`),
/// defaulting to the Perl-faithful `-t 2` when `-p` is absent (#1074). minimap2
/// `-t` is thread-invariant (byte-identical + input-order-preserving across N —
/// spike `SPIKE_minimap2_thread_invariance.md`), so this is output-neutral: a bare
/// `--minimap2` still emits `-t 2` (unchanged vs Perl). `--reorder` is Bowtie-2-only
/// (pushed to the base string this clean slate discards). Preset (Perl 8374-8408):
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
    } else if cli.illumina_5base {
        // #787: Illumina 5-Base is short-read Illumina data → the `sr` preset by
        // default (no explicit `--mm2_*` given). An explicit preset above still wins.
        "sr"
    } else {
        // Default OR explicit `--mm2_nanopore` → ONT (Perl 8404-8408).
        "map-ont"
    };
    // `-t` = Bismark `-p` (threads-to-aligner) knob, default the Perl-faithful 2 when
    // absent (#1074; minimap2 `-t` is thread-invariant per the spike, so output-neutral).
    // `bowtie_threads` is ≥ 2 when set (guarded upstream). rammap-subprocess shares this.
    let t = cli.bowtie_threads.unwrap_or(2);
    Ok(format!("-a --MD --secondary=no -t {t} -x {preset} -K 250K"))
}

/// Append the HISAT2-specific option tail (Perl `process_command_line` 8286-8326,
/// the `### ADDITIONAL ALIGNMENT OPTIONS WE NEED FOR HISAT2` block) to the
/// finished Bowtie 2 option string. For HISAT2 the tail, in Perl order, is
/// `[--no-spliced-alignment] [--known-splicesite-infile <f>] --no-softclip
/// --omit-sec-seq` (splice flags BEFORE the softclip delta). For any non-HISAT2
/// aligner the splice flags are a hard error (Perl 8319-8324 — closes a
/// pre-existing Bowtie 2 silent-no-op gap).
///
/// NB: the HISAT2 softclip tail is mode-dependent (Perl 8309-8315): end-to-end emits
/// `--no-softclip --omit-sec-seq`; HISAT2 `--local` emits `--omit-sec-seq` only (allowing
/// soft-clipping). minimap2-`--local` is rejected upstream (`config::resolve`); Bowtie 2-local
/// is handled in `build_aligner_options` (the `--local` + G-form push), not here.
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
    // Softclip delta (Perl 8309-8315): end-to-end forces `--no-softclip`; HISAT2 `--local`
    // OMITS `--no-softclip` (allowing soft-clipping) and emits `--omit-sec-seq` only.
    if cli.local {
        tail.push("--omit-sec-seq".into());
    } else {
        tail.push("--no-softclip --omit-sec-seq".into());
    }

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

/// Parse `--score_min` into the numeric `(intercept, slope)` for `calc_mapq`.
/// Form + defaults depend on mode AND aligner (Perl 7896-7948):
/// - end-to-end (any aligner): `L,<i>,<s>`, default `(0.0, -0.2)`.
/// - **Bowtie 2 `--local`**: `G,<i>,<s>`, default `(20.0, 8.0)` (Perl 7942).
/// - **HISAT2 `--local`**: `L,<i>,<s>`, default `(0.0, -0.2)` (Perl 7912/7947 — HISAT2 uses
///   the L-form even in local mode; the local-ness is the `ln()` scMin, not the form).
///
/// Splits on the LAST comma (Perl's greedy `^[LG],(.+),(.+)$`). The form is `G` iff
/// `cli.local && aligner == Bowtie2`.
pub fn score_min_params(cli: &Cli, aligner: Aligner) -> Result<(f64, f64)> {
    let (prefix, default) = if cli.local && aligner == Aligner::Bowtie2 {
        ("G,", (20.0, 8.0))
    } else {
        ("L,", (0.0, -0.2))
    };
    match &cli.score_min {
        None => Ok(default),
        Some(s) => {
            let rest = s.strip_prefix(prefix).ok_or_else(|| {
                AlignerError::Validation(format!(
                    "--score_min must be of the form {prefix}<intercept>,<slope>"
                ))
            })?;
            let (i, sl) = rest.rsplit_once(',').ok_or_else(|| {
                AlignerError::Validation(format!(
                    "--score_min must be of the form {prefix}<intercept>,<slope>"
                ))
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

/// Shape-only validation of `--score_min` for Bowtie 2 `--local` mode: `G,<a>,<b>`
/// with non-empty `a`/`b` (the G-form analog of [`valid_score_min_l`]; Perl
/// `^G,(.+),(.+)$`, `:7899`).
fn valid_score_min_g(s: &str) -> bool {
    match s.strip_prefix("G,").and_then(|rest| rest.split_once(',')) {
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
    use crate::aligner::cli::Cli;
    use clap::Parser;

    fn cli_from(args: &[&str]) -> Cli {
        let mut v = vec!["bismark"];
        v.extend_from_slice(args);
        Cli::parse_from(v)
    }

    #[test]
    fn default_se_options_match_phase0_spike() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(opts, "-q --score-min L,0,-0.2 --ignore-quals");
    }

    #[test]
    fn seed_flags_precede_score_min_and_quiet_is_last() {
        // Bismark input flags `-n`/`-l` map to Bowtie 2 output flags `-N`/`-L`.
        let cli = cli_from(&["-n", "1", "-l", "20", "--quiet"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(
            opts,
            "-q -N 1 -L 20 --score-min L,0,-0.2 --ignore-quals --quiet"
        );
    }

    #[test]
    fn score_min_override_substituted() {
        let cli = cli_from(&["--score_min", "L,0,-0.4"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(opts, "-q --score-min L,0,-0.4 --ignore-quals");
    }

    #[test]
    fn paired_end_tail_and_default_maxins() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, true, None).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500"
        );
    }

    #[test]
    fn fasta_uses_dash_f() {
        let cli = cli_from(&["-f"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastA, false, None).unwrap();
        assert_eq!(opts, "-f --score-min L,0,-0.2 --ignore-quals");
    }

    #[test]
    fn rejects_bad_seedmms() {
        let cli = cli_from(&["-n", "2"]);
        assert!(
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).is_err()
        );
    }

    #[test]
    fn accepts_local_for_bowtie2_emits_local_and_g_score_min() {
        // Bowtie 2 --local is now supported (the requires-Bowtie 2 reject lives in
        // config::resolve). Default emits `--local --score-min G,20,8` (Perl 7926/7942-44).
        let cli = cli_from(&["--local"]);
        let (opts, _gp) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None)
                .expect("Bowtie 2 --local is supported");
        assert!(opts.contains("--local"), "must emit --local: {opts}");
        assert!(
            opts.contains("--score-min G,20,8"),
            "must emit the local default G-form: {opts}"
        );
        assert!(
            !opts.contains("--score-min L,"),
            "must NOT emit the end-to-end L-form in local mode: {opts}"
        );
    }

    #[test]
    fn local_custom_g_score_min_accepted_l_rejected() {
        // A user G-form is accepted in local mode.
        let cli = cli_from(&["--local", "--score_min", "G,10,5"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert!(opts.contains("--score-min G,10,5"), "{opts}");
        // An L-form with --local is rejected (mirrors Perl :7900).
        let cli = cli_from(&["--local", "--score_min", "L,0,-0.2"]);
        assert!(
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).is_err()
        );
        // A G-form WITHOUT --local is rejected (end-to-end wants L; Perl :7908).
        let cli = cli_from(&["--score_min", "G,20,8"]);
        assert!(
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).is_err()
        );
    }

    #[test]
    fn score_min_params_aligner_and_mode_defaults() {
        // Bowtie 2 --local: G-form default (20, 8); custom G parses.
        let cli = cli_from(&["--local"]);
        assert_eq!(
            score_min_params(&cli, Aligner::Bowtie2).unwrap(),
            (20.0, 8.0)
        );
        let cli = cli_from(&["--local", "--score_min", "G,10,5"]);
        assert_eq!(
            score_min_params(&cli, Aligner::Bowtie2).unwrap(),
            (10.0, 5.0)
        );
        // HISAT2 --local: L-form default (0, -0.2) — NOT the Bowtie 2 (20,8) (Perl 7947);
        // accepts an L-form override, REJECTS a G-form.
        let cli = cli_from(&["--local"]);
        assert_eq!(
            score_min_params(&cli, Aligner::Hisat2).unwrap(),
            (0.0, -0.2)
        );
        let cli = cli_from(&["--local", "--score_min", "L,0,-0.6"]);
        assert_eq!(
            score_min_params(&cli, Aligner::Hisat2).unwrap(),
            (0.0, -0.6)
        );
        let cli = cli_from(&["--local", "--score_min", "G,20,8"]);
        assert!(score_min_params(&cli, Aligner::Hisat2).is_err());
        // end-to-end (any aligner): L-form default (0, -0.2).
        let cli = cli_from(&[]);
        assert_eq!(
            score_min_params(&cli, Aligner::Bowtie2).unwrap(),
            (0.0, -0.2)
        );
        assert_eq!(
            score_min_params(&cli, Aligner::Hisat2).unwrap(),
            (0.0, -0.2)
        );
    }

    #[test]
    fn phred_without_fastq_errors() {
        let cli = cli_from(&["--phred33-quals", "-f"]);
        assert!(
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastA, false, None).is_err()
        );
    }

    #[test]
    fn rdg_rfg_appended_and_validated() {
        let cli = cli_from(&["--rdg", "5,3", "--rfg", "5,3"]);
        let (opts, gp) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --rdg 5,3 --rfg 5,3 --ignore-quals"
        );
        assert_eq!((gp.deletion_open, gp.insertion_open), (5, 5));
        let bad = cli_from(&["--rdg", "5"]);
        assert!(
            build_aligner_options(&bad, Aligner::Bowtie2, ReadFormat::FastQ, false, None).is_err()
        );
    }

    // ---- HISAT2 option assembly (Phase 2a) ---------------------------------

    /// V2: the default SE HISAT2 string = the Bowtie 2 base + `--no-softclip
    /// --omit-sec-seq` appended last (spike Q2, Perl 8314).
    #[test]
    fn hisat2_se_option_string() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq"
        );
    }

    /// HISAT2 `--local`: drops `--no-softclip` (allows soft-clipping), keeps `--omit-sec-seq`,
    /// emits the SAME L-form `--score-min L,0,-0.2` as end-to-end, and does NOT push `--local`
    /// (Perl 7904 / 7946-48 / 8311). Bowtie 2-local stays byte-frozen (`--local` + G-form).
    #[test]
    fn hisat2_local_option_string() {
        // SE: no `--local`, no `--no-softclip`; L-form score-min; `--omit-sec-seq` only.
        let cli = cli_from(&["--local"]);
        let (se, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(se, "-q --score-min L,0,-0.2 --ignore-quals --omit-sec-seq");
        assert!(!se.contains("--local") && !se.contains("--no-softclip"));
        // PE: same delta (drop `--no-softclip`), PE flags retained.
        let (pe, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, true, None).unwrap();
        assert_eq!(
            pe,
            "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --omit-sec-seq"
        );
        // HISAT2-local accepts an L-form override (still no `--local`).
        let cli_ov = cli_from(&["--local", "--score_min", "L,0,-0.6"]);
        let (ov, _) =
            build_aligner_options(&cli_ov, Aligner::Hisat2, ReadFormat::FastQ, false, None)
                .unwrap();
        assert!(ov.contains("--score-min L,0,-0.6") && !ov.contains("--local"));
        // HISAT2-local REJECTS a G-form `--score_min`, with the HISAT2-local-specific message
        // (not the "end-to-end" text — Perl 7909).
        let cli_g = cli_from(&["--local", "--score_min", "G,20,8"]);
        let err = build_aligner_options(&cli_g, Aligner::Hisat2, ReadFormat::FastQ, false, None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("HISAT2 --local") && err.contains("<L,value,value>"),
            "got: {err}"
        );
        // REGRESSION: Bowtie 2-local stays byte-frozen (`--local` + G-form).
        let (bt2, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert!(bt2.contains("--local") && bt2.contains("--score-min G,20,8"));
    }

    /// HISAT2 Approach B-faithful (`--hisat2 --multicore N` → `-p N`): the remap
    /// injects `-p N --reorder` at the standard `-p` position (step 10, before
    /// `--ignore-quals`), byte-identical to Perl `--hisat2 -p N` (the softclip delta
    /// still lands last). `config::resolve` supplies the `N`.
    #[test]
    fn hisat2_multicore_remap_emits_p_reorder() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, Some(4))
                .unwrap();
        assert_eq!(
            opts,
            "-q --score-min L,0,-0.2 -p 4 --reorder --ignore-quals --no-softclip --omit-sec-seq"
        );
        // No remap (None) → no `-p`/`--reorder` from this param.
        let (sc, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).unwrap();
        assert!(!sc.contains("-p ") && !sc.contains("--reorder"));
        // An explicit `-p` takes precedence over the remap (`cli.bowtie_threads.or(..)`);
        // `config::resolve` rejects passing BOTH, so this only documents the fallback order.
        let cli_p = cli_from(&["-p", "8"]);
        let (opts_p, _) =
            build_aligner_options(&cli_p, Aligner::Hisat2, ReadFormat::FastQ, false, Some(4))
                .unwrap();
        assert!(opts_p.contains("-p 8 --reorder") && !opts_p.contains("-p 4"));
    }

    /// The remap param is HISAT2-specific in practice, but `build_aligner_options` is
    /// backend-agnostic: a Bowtie 2 caller always passes `None` (the fork model handles
    /// multicore), so no `-p` leaks in from this param for Bowtie 2.
    #[test]
    fn bowtie2_never_gets_p_from_the_remap_param() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert!(!opts.contains("-p ") && !opts.contains("--reorder"));
    }

    /// V3 (hard literal): the default PE HISAT2 string — the softclip delta lands
    /// AFTER the PE tail + `--maxins 500`, and there is **NO `--dovetail`** (Perl
    /// gates dovetail `if($bowtie2)`, 8051-8059).
    #[test]
    fn hisat2_pe_option_string_has_no_dovetail() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, true, None).unwrap();
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
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, true, None).unwrap();
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
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).unwrap();
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
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).unwrap();
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
        assert!(
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).is_err()
        );
    }

    /// V8: HISAT2 + a missing known-splicesite infile dies (Perl 8304).
    #[test]
    fn hisat2_known_splices_missing_file_dies() {
        let cli = cli_from(&["--known-splicesite-infile", "/no/such/splices.txt"]);
        assert!(
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).is_err()
        );
    }

    /// V8: the splice flags are HISAT2-only — they die in Bowtie 2 mode (Perl
    /// 8319-8324; closes a pre-existing Bowtie 2 silent-no-op gap).
    #[test]
    fn non_hisat2_splice_flags_die() {
        let cli = cli_from(&["--no-spliced-alignment"]);
        assert!(
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).is_err()
        );
        let cli = cli_from(&["--known-splicesite-infile", "/no/such/splices.txt"]);
        assert!(
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).is_err()
        );
    }

    // ---- minimap2 clean-slate option assembly (Phase 4) --------------------

    /// V2 (hard literal): the default minimap2 string — clean-slate, `-x map-ont`
    /// (NOT `-ax sr`), faithful `-t 2` when no `-p` is given (Perl 8358-8413; #1074).
    #[test]
    fn minimap2_default_option_string() {
        let cli = cli_from(&[]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-ont -K 250K");
    }

    /// #1074: `-p N` lifts minimap2 `-t` to N (spike-confirmed thread-invariant, so
    /// output-neutral); a bare `--minimap2` stays `-t 2` (covered above).
    #[test]
    fn minimap2_p_lifts_t() {
        let cli = cli_from(&["--minimap2", "-p", "8"]);
        let (opts, _) =
            build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 8 -x map-ont -K 250K");
    }

    /// #1074: rammap-subprocess shares the minimap2 clean-slate path → same `-p`→`-t`.
    #[test]
    fn rammap_subprocess_p_lifts_t() {
        let (opts, _) = build_aligner_options(
            &cli_from(&["--rammap", "--rammap_subprocess", "-p", "8"]),
            Aligner::Rammap,
            ReadFormat::FastQ,
            false,
            None,
        )
        .unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 8 -x map-ont -K 250K");
    }

    /// Phase 3 (T3, design#2): rammap is minimap-like — it reuses the IDENTICAL
    /// clean-slate `map-ont` option string (the same `minimap2_options` branch).
    #[test]
    fn rammap_default_option_string() {
        let (opts, _) = build_aligner_options(
            &cli_from(&["--rammap"]),
            Aligner::Rammap,
            ReadFormat::FastQ,
            false,
            None,
        )
        .unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-ont -K 250K");
    }

    /// V3: preset selection — `--mm2_short_reads`→`sr`, `--mm2_pacbio`→`map-pb`,
    /// explicit `--mm2_nanopore`→`map-ont` (same as default, Perl 8404-8408).
    #[test]
    fn minimap2_preset_selection() {
        let sr = cli_from(&["--mm2_short_reads"]);
        let (opts, _) =
            build_aligner_options(&sr, Aligner::Minimap2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x sr -K 250K");

        let pb = cli_from(&["--mm2_pacbio"]);
        let (opts, _) =
            build_aligner_options(&pb, Aligner::Minimap2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-pb -K 250K");

        let ont = cli_from(&["--mm2_nanopore"]);
        let (opts, _) =
            build_aligner_options(&ont, Aligner::Minimap2, ReadFormat::FastQ, false, None).unwrap();
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
                build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false, None)
                    .is_err(),
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
            build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(opts, "-a --MD --secondary=no -t 2 -x map-ont -K 250K");
        assert!(!opts.contains("-N") && !opts.contains("-L") && !opts.contains("--rdg"));
        assert!(!opts.contains("--score-min") && !opts.contains("--ignore-quals"));
    }

    /// Build-then-wipe parity: a Bowtie 2-base validation (`-N 2` out of range)
    /// STILL dies in minimap2 mode (Perl runs the base block before the wipe).
    #[test]
    fn minimap2_still_validates_bowtie2_base() {
        let cli = cli_from(&["-n", "2"]);
        assert!(
            build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false, None).is_err()
        );
    }

    /// V1 regression: the Minimap2 branch must not perturb the Bowtie 2 / HISAT2
    /// strings (they are unchanged from the dedicated tests above — re-pinned here
    /// alongside the new branch as a guard).
    #[test]
    fn bowtie2_hisat2_strings_byte_frozen_alongside_minimap2() {
        let cli = cli_from(&[]);
        let (bt2, _) =
            build_aligner_options(&cli, Aligner::Bowtie2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(bt2, "-q --score-min L,0,-0.2 --ignore-quals");
        let (ht2, _) =
            build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false, None).unwrap();
        assert_eq!(
            ht2,
            "-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq"
        );
    }
}
