//! `bismark run` — raw FastQ to coverage for one sample in one command:
//! trim (Trim Galore) → align → dedup → extract (bedGraph + `.cov` + CpG report) → HTML report.
//!
//! Align, dedup and extract run at the same time. Each BAM is still written to
//! disk, and its records also go to the next step in memory through an
//! [`crate::io::tap`], so no step re-reads the previous step's BAM. Each step runs
//! the same code as its subcommand. `--preset` applies the library-type table of
//! the docs (`docs/src/content/docs/usage/library-types.md`). Any option of
//! `bismark align`, `dedup` or `extract` can be given to `bismark run` as is; it
//! goes to the one step that has it ([`route`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, ValueEnum};

use crate::io::tap::{self, TapSource, Tapped};

/// Library type → what each step needs (the docs' "Library types" table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Preset {
    /// BS-Seq / WGBS: dedup; extract `--ignore_r2 2` for paired-end.
    Wgbs,
    /// RRBS: trim `--rrbs`; no dedup.
    Rrbs,
    /// PBAT: trim 8 bp (`--library pbat`); align `--pbat`; dedup.
    Pbat,
    /// Single-cell scBS-Seq: trim 6N (`--library scbs`); align `--non_directional`, single-end; dedup.
    Scbs,
    /// Accel-NGS (Swift): trim R1 10 / R2 15 bp (`--library accel`); dedup.
    Accel,
    /// Zymo Pico-Methyl: trim 10 bp (`--library zymo`); align `--non_directional`; dedup.
    Zymo,
    /// EM-seq (NEB): trim 10 bp (`--library emseq`); dedup.
    Emseq,
}

struct PresetSpec {
    trim: &'static [&'static str],
    align: &'static [&'static str],
    dedup: bool,
    /// Extractor options for paired-end data.
    extract_pe: &'static [&'static str],
}

impl Preset {
    fn spec(self) -> PresetSpec {
        let s = |trim, align, dedup, extract_pe| PresetSpec {
            trim,
            align,
            dedup,
            extract_pe,
        };
        match self {
            Preset::Wgbs => s(&[], &[], true, &["--ignore_r2", "2"]),
            Preset::Rrbs => s(&["--rrbs"], &[], false, &[]),
            Preset::Pbat => s(&["--library", "pbat"], &["--pbat"], true, &[]),
            Preset::Scbs => s(&["--library", "scbs"], &["--non_directional"], true, &[]),
            Preset::Accel => s(&["--library", "accel"], &[], true, &[]),
            Preset::Zymo => s(&["--library", "zymo"], &["--non_directional"], true, &[]),
            Preset::Emseq => s(&["--library", "emseq"], &[], true, &[]),
        }
    }
}

/// `bismark run` options.
#[derive(Parser, Debug)]
#[command(
    name = "bismark run",
    about = "Raw FastQ → trim → align → dedup → extract → report for one sample, in one command",
    after_help = "Any option of `trim_galore`, `bismark align`, `bismark dedup` or `bismark extract`\n\
can be given here under its own name: it goes to the step that has it (to each step that\n\
has it, for a name several steps share, such as --quiet). Options that bismark run sets\n\
itself (paired/single, output folder and names, threads) are refused.\n\n\
Align, dedup and extract run at the same time and pass their records in memory.\n\
Every step still writes its usual files to --output_dir.\n\n\
Example:\n  bismark run --genome GRCh38/ --preset emseq -1 s_R1.fq.gz -2 s_R2.fq.gz -o out/ \\\n      --parallel 4 --length 30 --barcode --CX"
)]
pub struct Cli {
    /// Genome folder prepared with `bismark prepare`.
    #[arg(long = "genome", visible_alias = "genome_folder", value_name = "PATH")]
    pub genome: PathBuf,
    /// Read 1 of a paired-end sample.
    #[arg(short = '1', value_name = "R1", requires = "mates2")]
    pub mates1: Option<String>,
    /// Read 2 of a paired-end sample.
    #[arg(short = '2', value_name = "R2", requires = "mates1")]
    pub mates2: Option<String>,
    /// Single-end read file.
    #[arg(
        value_name = "READS",
        required_unless_present = "mates1",
        conflicts_with = "mates1"
    )]
    pub reads: Option<String>,
    /// Library type; sets the trimming, alignment, dedup and extraction options from the docs.
    #[arg(long, value_enum, default_value_t = Preset::Wgbs)]
    pub preset: Preset,
    /// Output folder for every file of the run (created if missing).
    #[arg(
        short = 'o',
        long = "output_dir",
        default_value = ".",
        value_name = "PATH"
    )]
    pub output_dir: PathBuf,
    /// Sample name for the output files (default: the first read file without its FastQ suffix).
    #[arg(short = 'B', long = "basename", value_name = "NAME")]
    pub basename: Option<String>,
    /// Threads for every step (trim `--cores`, align `--multicore`, dedup and extract `--parallel`).
    #[arg(long = "parallel", visible_alias = "multicore", default_value_t = 1u32, value_parser = clap::value_parser!(u32).range(1..))]
    pub parallel: u32,
    /// The reads are already trimmed: skip Trim Galore.
    #[arg(long = "skip_trim")]
    pub skip_trim: bool,
    /// Skip deduplication even if the preset deduplicates.
    #[arg(long = "skip_dedup")]
    pub skip_dedup: bool,
}

/// True if the user already gave one of `flags`, so `bismark run` must not add it
/// again (clap rejects a repeated option).
fn has(args: &[String], flags: &[&str]) -> bool {
    args.iter().any(|a| {
        flags
            .iter()
            .any(|f| a == f || a.strip_prefix(f).is_some_and(|r| r.starts_with('=')))
    })
}

/// The files each step writes, all under `output_dir`. Pure, so it is unit-tested.
#[derive(Debug, PartialEq)]
struct Plan {
    aligned_bam: PathBuf,
    alignment_report: PathBuf,
    /// `None` without dedup.
    dedup: Option<(PathBuf, PathBuf)>,
    splitting_report: PathBuf,
    mbias_report: PathBuf,
}

impl Plan {
    fn new(out: &Path, base: &str, paired: bool, dedup: bool) -> Self {
        // Aligner `--basename` naming (`derive_output_path` in aligner/mod.rs).
        let (bam_stem, layout) = if paired {
            (format!("{base}_pe"), "PE")
        } else {
            (base.to_string(), "SE")
        };
        let dedup_files = dedup.then(|| {
            (
                out.join(format!("{bam_stem}.deduplicated.bam")),
                out.join(format!("{bam_stem}.deduplication_report.txt")),
            )
        });
        // The extractor reads the last BAM and names its reports after it.
        let extract_stem = if dedup {
            format!("{bam_stem}.deduplicated")
        } else {
            bam_stem.clone()
        };
        Plan {
            aligned_bam: out.join(format!("{bam_stem}.bam")),
            alignment_report: out.join(format!("{base}_{layout}_report.txt")),
            dedup: dedup_files,
            splitting_report: out.join(format!("{extract_stem}_splitting_report.txt")),
            mbias_report: out.join(format!("{extract_stem}.M-bias.txt")),
        }
    }

    fn methylation_bam(&self) -> &Path {
        self.dedup
            .as_ref()
            .map_or(&self.aligned_bam, |(bam, _)| bam)
    }
}

/// Default sample name: the first read file's name without its FastQ suffix.
fn default_basename(read_file: &str) -> String {
    let name = Path::new(read_file).file_name().map_or_else(
        || read_file.to_string(),
        |f| f.to_string_lossy().into_owned(),
    );
    crate::aligner::strip_fastq_suffix(&name)
}

/// Read files of the sample: `(R1 or SE file, R2)`.
type Reads = (String, Option<String>);

/// The steps `bismark run` routes options to, in [`Routed`] order.
const STEPS: [&str; 4] = ["trim", "align", "dedup", "extract"];

/// Per step, the options `bismark run` sets itself (from the read files or its own
/// options), by long name. `-1`/`-2`, `-o`, `-B` and `--multicore` are `bismark run`'s
/// own options and never reach a step.
const SET_BY_RUN: [&[&str]; 4] = [
    &[
        "paired",
        "output_dir",
        "basename",
        "cores",
        "help",
        "version",
    ],
    &["version"],
    &[
        "single",
        "paired",
        "outfile",
        "output_dir",
        "bam",
        "sam",
        "cram_ref",
        "multiple",
        "parallel",
        "version",
    ],
    &[
        "single-end",
        "paired-end",
        "output_dir",
        "gzip",
        "bedGraph",
        "cytosine_report",
        "genome_folder",
        "parallel",
        "version",
    ],
];

/// Step options given straight to `bismark run`, per step of [`STEPS`].
type Routed = [Vec<String>; 4];

/// Trim Galore's options, read from its `--help` (clap layout: `  -q, --quality <QUALITY>`).
/// `None` without a `trim_galore` on PATH.
fn trim_galore_command() -> Option<clap::Command> {
    let out = std::process::Command::new(which::which("trim_galore").ok()?)
        .arg("--help")
        .output()
        .ok()?;
    Some(parse_clap_help(
        "trim_galore",
        &String::from_utf8_lossy(&out.stdout),
    ))
}

fn parse_clap_help(name: &'static str, help: &str) -> clap::Command {
    let mut cmd = clap::Command::new(name);
    for line in help.lines() {
        // Option lines are indented 2–6 spaces; their descriptions 10.
        let body = line.trim_start();
        if !(2..=6).contains(&(line.len() - body.len())) || !body.starts_with('-') {
            continue;
        }
        let (short, rest) = match body.split_once(", ") {
            Some((s, r)) if s.len() == 2 => (s.chars().nth(1), r),
            _ => (None, body),
        };
        let Some(long) = rest.strip_prefix("--") else {
            continue;
        };
        let end = long.find([' ', '[']).unwrap_or(long.len());
        let takes_value = long[end..].starts_with(" <");
        // clap names are `&'static str` here (no `string` feature); parsed once per run.
        let long: &'static str = Box::leak(long[..end].to_string().into_boxed_str());
        let mut arg = clap::Arg::new(long).long(long);
        if let Some(c) = short {
            arg = arg.short(c);
        }
        cmd = cmd.arg(arg.action(if takes_value {
            clap::ArgAction::Set
        } else {
            clap::ArgAction::SetTrue
        }));
    }
    cmd
}

/// The option `token` names in `cmd` (`--long`, `--long=value`, `-s`), if any.
fn find_arg<'a>(cmd: &'a clap::Command, token: &str) -> Option<&'a clap::Arg> {
    if let Some(long) = token.strip_prefix("--") {
        let long = long.split_once('=').map_or(long, |(name, _)| name);
        cmd.get_arguments().find(|a| {
            a.get_long() == Some(long) || a.get_all_aliases().is_some_and(|al| al.contains(&long))
        })
    } else {
        let mut chars = token.strip_prefix('-')?.chars();
        let (c, None) = (chars.next()?, chars.next()) else {
            return None;
        };
        cmd.get_arguments().find(|a| {
            a.get_short() == Some(c) || a.get_all_short_aliases().is_some_and(|al| al.contains(&c))
        })
    }
}

/// Split `bismark run` argv into its own args and the step options it was given.
/// An option `bismark run` does not have goes, under its own name, to each step
/// that has it. `trim` is `None` when Trim Galore does not run.
fn route(argv: Vec<OsString>, trim: Option<clap::Command>) -> Result<(Vec<OsString>, Routed)> {
    let own = Cli::command();
    let steps = [
        trim,
        Some(crate::aligner::cli::Cli::command()),
        Some(crate::dedup::cli::Cli::command()),
        Some(crate::extractor::Cli::command()),
    ];
    let mut keep = Vec::new();
    let mut routed = Routed::default();
    let mut it = argv.into_iter();
    while let Some(tok) = it.next() {
        let Some(s) = tok.to_str().filter(|s| s.starts_with('-') && s.len() > 1) else {
            keep.push(tok);
            continue;
        };
        let takes_value = |a: &clap::Arg| a.get_action().takes_values() && !s.contains('=');
        if matches!(s, "-h" | "--help") || find_arg(&own, s).is_some() {
            let value = find_arg(&own, s)
                .is_some_and(takes_value)
                .then(|| it.next());
            keep.push(tok);
            keep.extend(value.flatten());
            continue;
        }
        let found: Vec<(usize, &clap::Arg)> = steps
            .iter()
            .enumerate()
            .filter_map(|(i, cmd)| find_arg(cmd.as_ref()?, s).map(|a| (i, a)))
            .collect();
        let owners: Vec<(usize, &clap::Arg)> = found
            .iter()
            .copied()
            .filter(|(i, a)| !a.get_long().is_some_and(|l| SET_BY_RUN[*i].contains(&l)))
            .collect();
        let Some(&(_, first)) = owners.first() else {
            if let Some((i, _)) = found.first() {
                bail!("`{s}` is set by bismark run for {}; drop it", STEPS[*i]);
            }
            bail!(
                "unknown option `{s}` (not an option of bismark run, trim_galore, align, dedup or extract)"
            );
        };
        // A shared name must mean the same everywhere; a short one often does not (-q).
        if owners
            .iter()
            .any(|(_, a)| a.get_long() != first.get_long() || takes_value(a) != takes_value(first))
        {
            let meanings: Vec<String> = owners
                .iter()
                .map(|(i, a)| format!("--{} ({})", a.get_long().unwrap_or("?"), STEPS[*i]))
                .collect();
            bail!("`{s}` means {}; use the long name", meanings.join(", "));
        }
        let value = if takes_value(first) {
            let v = it.next().with_context(|| format!("`{s}` needs a value"))?;
            Some(v.to_string_lossy().into_owned())
        } else {
            None
        };
        for (i, _) in owners {
            routed[i].push(s.to_string());
            routed[i].extend(value.clone());
        }
    }
    Ok((keep, routed))
}

/// Everything the steps share.
struct Ctx {
    cli: Cli,
    routed: Routed,
    base: String,
    paired: bool,
    spec: PresetSpec,
    threads: String,
}

/// Entry for `bismark run <args…>` (argv[0] = `bismark run`).
pub fn run_from_args<I>(argv: I) -> ExitCode
where
    I: IntoIterator,
    I::Item: Into<OsString> + Clone,
{
    let argv: Vec<OsString> = argv.into_iter().map(Into::into).collect();
    if let Some(code) = crate::cli::help_if_no_args(argv.len(), Cli::command()) {
        return code;
    }
    // Trim Galore's options are known only if it will run.
    let trims = !argv.iter().any(|a| a == "--skip_trim");
    let trim = trims.then(trim_galore_command).flatten();
    let result = route(argv, trim).and_then(|(argv, routed)| run(Cli::parse_from(argv), routed));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: bismark run: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli, routed: Routed) -> Result<()> {
    let paired = cli.mates1.is_some();
    if cli.preset == Preset::Scbs && paired {
        bail!("--preset scbs aligns single-end reads; pass one read file, not -1/-2");
    }
    let raw: Reads = match (&cli.mates1, &cli.mates2, &cli.reads) {
        (Some(r1), Some(r2), _) => (r1.clone(), Some(r2.clone())),
        (_, _, Some(r)) => (r.clone(), None),
        _ => unreachable!("clap requires READS or -1/-2"),
    };
    let base = cli
        .basename
        .clone()
        .unwrap_or_else(|| default_basename(&raw.0));
    let ctx = Ctx {
        spec: cli.preset.spec(),
        threads: cli.parallel.to_string(),
        base,
        paired,
        cli,
        routed,
    };
    std::fs::create_dir_all(&ctx.cli.output_dir)
        .with_context(|| format!("--output_dir {}", ctx.cli.output_dir.display()))?;

    let reads = if ctx.cli.skip_trim {
        raw
    } else {
        trim(&ctx, &raw)?
    };

    let plan = Plan::new(
        &ctx.cli.output_dir,
        &ctx.base,
        ctx.paired,
        ctx.spec.dedup && !ctx.cli.skip_dedup,
    );
    stream_align_dedup_extract(&ctx, &plan, &reads)?;
    report(&ctx, &plan)
}

/// Trim Galore writes `<base>_val_1/2.fq.gz` (paired) or `<base>_trimmed.fq.gz`.
fn trim(ctx: &Ctx, raw: &Reads) -> Result<Reads> {
    eprintln!("\n=== bismark run: trim (Trim Galore) ===\n");
    let exe = which::which("trim_galore")
        .context("trim_galore is not on PATH; install Trim Galore 2 or pass --skip_trim")?;
    let mut user = ctx.routed[0].clone();
    // Trim Galore's --non_directional means non-directional RRBS (it requires --rrbs).
    if ctx.cli.preset != Preset::Rrbs {
        user.retain(|a| a != "--non_directional");
    }
    let out = &ctx.cli.output_dir;
    let mut cmd = std::process::Command::new(exe);
    if ctx.paired {
        cmd.arg("--paired");
    }
    cmd.arg("-o").arg(out).args(["--basename", &ctx.base]);
    cmd.args(["--cores", &ctx.threads]);
    cmd.args(ctx.spec.trim).args(&user).arg(&raw.0).args(&raw.1);
    let status = cmd.status().context("could not start trim_galore")?;
    if !status.success() {
        bail!("trim_galore failed ({status})");
    }
    let path = |suffix: &str| {
        out.join(format!("{}{suffix}", ctx.base))
            .to_string_lossy()
            .into_owned()
    };
    Ok(if ctx.paired {
        (path("_val_1.fq.gz"), Some(path("_val_2.fq.gz")))
    } else {
        (path("_trimmed.fq.gz"), None)
    })
}

/// Align, dedup and extract at the same time, each step reading the previous
/// step's records from memory.
fn stream_align_dedup_extract(ctx: &Ctx, plan: &Plan, reads: &Reads) -> Result<()> {
    eprintln!("\n=== bismark run: align → dedup → extract (streaming) ===\n");
    let aligned_rx = tap::register(&plan.aligned_bam);
    let dedup_rx = plan.dedup.as_ref().map(|(bam, _)| tap::register(bam));

    let (aligned, deduped, extracted) = std::thread::scope(|s| {
        let (extract_rx, dedup) = match dedup_rx {
            Some(rx) => (rx, Some(s.spawn(move || dedup(ctx, plan, aligned_rx)))),
            None => (aligned_rx, None),
        };
        let extract = s.spawn(move || extract(ctx, plan, extract_rx));
        let aligned = align(ctx, plan, reads);
        // An aligner that stopped before opening its BAM never took its tap:
        // drop it so the next step stops waiting.
        drop(tap::take(&plan.aligned_bam));
        let join = |h: std::thread::ScopedJoinHandle<'_, Result<()>>| {
            h.join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("thread panicked")))
        };
        (aligned, dedup.map(join), join(extract))
    });

    // A step that failed only because its neighbour stopped is a symptom: name the cause.
    let failed: Vec<(&str, anyhow::Error)> = [
        ("align", Some(aligned)),
        ("dedup", deduped),
        ("extract", Some(extracted)),
    ]
    .into_iter()
    .filter_map(|(step, r)| Some((step, r?.err()?)))
    .collect();
    let symptom = |e: &anyhow::Error| format!("{e:#}").contains("`bismark run` step stopped");
    let causes: Vec<_> = failed.iter().filter(|(_, e)| !symptom(e)).collect();
    match (causes.first(), failed.first()) {
        (Some((step, e)), _) | (None, Some((step, e))) => bail!("{step} failed: {e}"),
        (None, None) => Ok(()),
    }
}

fn align(ctx: &Ctx, plan: &Plan, reads: &Reads) -> Result<()> {
    let user = ctx.routed[1].clone();
    let mut args: Vec<String> = vec![
        "--genome".into(),
        ctx.cli.genome.to_string_lossy().into_owned(),
        "-o".into(),
        ctx.cli.output_dir.to_string_lossy().into_owned(),
        "--basename".into(),
        ctx.base.clone(),
    ];
    args.extend(["--multicore".into(), ctx.threads.clone()]);
    args.extend(ctx.spec.align.iter().map(|s| s.to_string()));
    args.extend(user);
    match &reads.1 {
        Some(r2) => args.extend(["-1".into(), reads.0.clone(), "-2".into(), r2.clone()]),
        None => args.push(reads.0.clone()),
    }
    let command_line = args.join(" ");
    let argv = std::iter::once(String::from("bismark align"))
        .chain(args)
        .collect();
    if crate::aligner::run_dispatch(argv, command_line) != ExitCode::SUCCESS {
        bail!(
            "see the aligner error above ({} not finished)",
            plan.aligned_bam.display()
        );
    }
    Ok(())
}

fn dedup(ctx: &Ctx, plan: &Plan, rx: crossbeam_channel::Receiver<Tapped>) -> Result<()> {
    let (bam, report_path) = plan.dedup.as_ref().expect("dedup step without dedup files");
    let result = (|| -> Result<()> {
        let source = TapSource::open(rx)?;
        let user = ctx.routed[2].clone();
        let mut argv: Vec<OsString> = vec![
            "bismark dedup".into(),
            "--parallel".into(),
            ctx.threads.clone().into(),
        ];
        argv.extend(user.into_iter().map(OsString::from));
        argv.push(plan.aligned_bam.clone().into());
        // Parsed for its options (UMI mode, threads); the records come from `source`.
        let config = crate::dedup::cli::Cli::try_parse_from(argv)?.validate()?;
        let parallel = std::num::NonZero::new(config.parallel).context("--parallel 0")?;
        eprintln!("Output file is: {}", bam.display());
        let report = crate::dedup::pipeline::run_tapped(
            source,
            bam,
            ctx.paired,
            plan.aligned_bam.to_string_lossy().into_owned(),
            parallel,
            config.umi_mode,
        )?;
        report.write_to(report_path)?;
        eprintln!("{}", report.format_stderr());
        Ok(())
    })();
    // A dedup that stopped before opening its BAM never took its tap.
    drop(tap::take(bam));
    result
}

fn extract(ctx: &Ctx, plan: &Plan, rx: crossbeam_channel::Receiver<Tapped>) -> Result<()> {
    // Wait for the BAM to exist (its header arrives once it is created): the
    // extractor's option check wants the input file on disk.
    let source = TapSource::open(rx)?;
    let user = ctx.routed[3].clone();
    let mut argv: Vec<OsString> = [
        "bismark extract",
        "--gzip",
        "--bedGraph",
        "--cytosine_report",
    ]
    .map(OsString::from)
    .into();
    argv.extend(["-o".into(), ctx.cli.output_dir.clone().into()]);
    argv.extend(["--genome_folder".into(), ctx.cli.genome.clone().into()]);
    argv.extend(["--parallel".into(), ctx.threads.clone().into()]);
    if ctx.paired {
        for pair in ctx.spec.extract_pe.chunks(2) {
            if !has(&user, &[pair[0]]) {
                argv.extend(pair.iter().map(OsString::from));
            }
        }
    }
    argv.extend(user.into_iter().map(OsString::from));
    argv.push(plan.methylation_bam().into());
    let config = crate::extractor::Cli::try_parse_from(argv)?.validate()?;
    crate::extractor::parallel::extract_tapped(
        plan.methylation_bam(),
        source,
        &config,
        ctx.paired,
    )?;
    Ok(())
}

fn report(ctx: &Ctx, plan: &Plan) -> Result<()> {
    eprintln!("\n=== bismark run: report ===\n");
    let os = |p: &Path| OsString::from(p);
    let mut argv = vec![
        "bismark report".into(),
        "--dir".into(),
        os(&ctx.cli.output_dir),
        "--alignment_report".into(),
        os(&plan.alignment_report),
        "--splitting_report".into(),
        os(&plan.splitting_report),
        "--mbias_report".into(),
        os(&plan.mbias_report),
    ];
    if let Some((_, dedup_report)) = &plan.dedup {
        argv.extend(["--dedup_report".into(), os(dedup_report)]);
    }
    if crate::report::run_from_args(argv) != ExitCode::SUCCESS {
        bail!("see the report error above");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_names_match_each_steps_outputs() {
        let p = Plan::new(Path::new("out"), "s", true, true);
        assert_eq!(p.aligned_bam, Path::new("out/s_pe.bam"));
        assert_eq!(p.alignment_report, Path::new("out/s_PE_report.txt"));
        assert_eq!(
            p.dedup,
            Some((
                PathBuf::from("out/s_pe.deduplicated.bam"),
                PathBuf::from("out/s_pe.deduplication_report.txt")
            ))
        );
        assert_eq!(p.methylation_bam(), Path::new("out/s_pe.deduplicated.bam"));
        assert_eq!(
            p.splitting_report,
            Path::new("out/s_pe.deduplicated_splitting_report.txt")
        );
        assert_eq!(
            p.mbias_report,
            Path::new("out/s_pe.deduplicated.M-bias.txt")
        );

        let se = Plan::new(Path::new("out"), "s", false, false);
        assert_eq!(se.alignment_report, Path::new("out/s_SE_report.txt"));
        assert_eq!(se.dedup, None);
        assert_eq!(se.methylation_bam(), Path::new("out/s.bam"));
        assert_eq!(se.splitting_report, Path::new("out/s_splitting_report.txt"));
        assert_eq!(se.mbias_report, Path::new("out/s.M-bias.txt"));
    }

    #[test]
    fn default_basename_strips_fastq_suffix() {
        assert_eq!(default_basename("dir/sample_R1.fq.gz"), "sample_R1");
        assert_eq!(default_basename("sample.fastq"), "sample");
    }

    #[test]
    fn has_matches_flag_and_flag_equals_value() {
        let a: Vec<String> = ["--parallel=3", "--CX"].map(String::from).into();
        assert!(has(&a, &["--parallel"]));
        assert!(has(&a, &["--CX"]));
        assert!(!has(&a, &["--par"]));
    }

    #[test]
    fn presets_follow_the_library_types_table() {
        assert!(!Preset::Rrbs.spec().dedup);
        assert!(Preset::Wgbs.spec().extract_pe.contains(&"--ignore_r2"));
        assert_eq!(Preset::Pbat.spec().align, ["--pbat"]);
        assert_eq!(Preset::Zymo.spec().align, ["--non_directional"]);
        for p in Preset::value_variants() {
            assert_eq!(p.spec().extract_pe.len() % 2, 0, "{p:?}: flag/value pairs");
        }
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    const TRIM_HELP: &str = "Options:\n  -q, --quality <QUALITY>\n          Trim low-quality ends\n      --length <LENGTH>\n      --paired\n      --non_directional\n      --implicon[=<IMPLICON>]\n          - fastq: not an option\n";

    #[test]
    fn trim_galore_help_is_parsed_into_options() {
        let cmd = parse_clap_help("trim_galore", TRIM_HELP);
        let q = find_arg(&cmd, "-q").expect("-q");
        assert_eq!(q.get_long(), Some("quality"));
        assert!(q.get_action().takes_values());
        assert!(
            !find_arg(&cmd, "--paired")
                .unwrap()
                .get_action()
                .takes_values()
        );
        assert!(
            !find_arg(&cmd, "--implicon")
                .unwrap()
                .get_action()
                .takes_values()
        );
        assert_eq!(cmd.get_arguments().count(), 5);
    }

    #[test]
    fn step_options_go_to_the_steps_that_have_them() {
        let trim = Some(parse_clap_help("trim_galore", TRIM_HELP));
        let (keep, routed) = route(
            os(&[
                "bismark run",
                "--genome",
                "G",
                "-o",
                "out",
                "--length",
                "30",
                "--non_directional",
                "-N",
                "1",
                "--barcode",
                "--CX",
                "--ignore=2",
                "--quiet",
                "r.fq",
            ]),
            trim,
        )
        .unwrap();
        assert_eq!(
            keep,
            os(&["bismark run", "--genome", "G", "-o", "out", "r.fq"])
        );
        let [trim, align, dedup, extract] = routed;
        assert_eq!(trim, ["--length", "30", "--non_directional"]);
        assert_eq!(align, ["--non_directional", "-N", "1", "--quiet"]);
        assert_eq!(dedup, ["--barcode"]);
        assert_eq!(extract, ["--CX", "--ignore=2", "--quiet"]);
    }

    #[test]
    fn refused_options_say_why() {
        let trim = || Some(parse_clap_help("trim_galore", TRIM_HELP));
        let e = route(os(&["bismark run", "-q", "20"]), trim())
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("--quality (trim)") && e.contains("long name"),
            "{e}"
        );
        let e = route(os(&["bismark run", "--bedGraph"]), trim())
            .unwrap_err()
            .to_string();
        assert!(e.contains("set by bismark run"), "{e}");
        let e = route(os(&["bismark run", "--nope"]), trim())
            .unwrap_err()
            .to_string();
        assert!(e.contains("unknown option"), "{e}");
        // Without Trim Galore, its options are unknown.
        assert!(route(os(&["bismark run", "--length", "30"]), None).is_err());
    }

    #[test]
    fn cli_needs_reads_or_a_pair() {
        assert!(Cli::try_parse_from(["bismark run", "--genome", "G"]).is_err());
        assert!(Cli::try_parse_from(["bismark run", "--genome", "G", "-1", "a.fq"]).is_err());
        assert!(
            Cli::try_parse_from(["bismark run", "--genome", "G", "-1", "a", "-2", "b", "c"])
                .is_err()
        );
        assert!(Cli::try_parse_from(["bismark run", "--genome", "G", "r.fq"]).is_ok());
    }
}
