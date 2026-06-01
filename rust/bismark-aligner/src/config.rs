//! Resolved run configuration — the typed seam (`RunConfig`) that Phase 1
//! produces and Phases 2–10 consume.
//!
//! [`resolve`] mirrors Perl `process_command_line`'s precedence: aligner →
//! library type → format → genome/reads → discovery → aligner detection →
//! `aligner_options` → output target. Only the **v1 spine** is wired; HISAT2/
//! minimap2 and SAM/CRAM output fail loudly (deferred), while non-directional/
//! pbat/PE/FastA resolve (no alignment runs in Phase 1).

use std::path::{Path, PathBuf};

use crate::aligner::{self, DetectedAligner};
use crate::cli::Cli;
use crate::discovery::{self, GenomeIndexes};
use crate::error::{AlignerError, Result};
use crate::options;

/// Which external aligner (v1 wires Bowtie 2 only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aligner {
    /// Bowtie 2 (default and only v1 aligner).
    Bowtie2,
}

/// Bisulfite library type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryType {
    /// Directional (default) — 2 instances for SE.
    Directional,
    /// Non-directional — 4 instances.
    NonDirectional,
    /// PBAT.
    Pbat,
}

/// Input read format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFormat {
    /// FASTQ (default).
    FastQ,
    /// FASTA.
    FastA,
}

/// Output container (v1 wires BAM only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// BAM (default).
    Bam,
}

/// Read layout + resolved file list.
#[derive(Debug, Clone)]
pub enum ReadLayout {
    /// Single-end reads.
    SingleEnd {
        /// One or more SE read files.
        reads: Vec<String>,
    },
    /// Paired-end reads (1:1 mate files).
    PairedEnd {
        /// Read-1 mate files.
        mates1: Vec<String>,
        /// Read-2 mate files.
        mates2: Vec<String>,
    },
}

impl ReadLayout {
    /// `true` for paired-end.
    pub fn is_paired(&self) -> bool {
        matches!(self, ReadLayout::PairedEnd { .. })
    }
}

/// Read/reference gap penalties (Bowtie 2 `--rdg`/`--rfg`; 5,3 defaults). Held
/// for the later MAPQ computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GapPenalties {
    /// Read-gap open (deletion in read).
    pub deletion_open: u32,
    /// Read-gap extend.
    pub deletion_extend: u32,
    /// Reference-gap open (insertion in read).
    pub insertion_open: u32,
    /// Reference-gap extend.
    pub insertion_extend: u32,
}

/// Resolved output target.
#[derive(Debug, Clone)]
pub struct OutputTarget {
    /// Output directory; empty = current directory (Perl default `''`).
    pub output_dir: PathBuf,
    /// Temp directory; empty = parent/CWD (Perl default `''`, independent of `output_dir`).
    pub temp_dir: PathBuf,
    /// `--basename` (fully overrides the derived output name).
    pub basename: Option<String>,
    /// `--prefix` prepended to the output name.
    pub prefix: Option<String>,
    /// Container format (BAM in v1).
    pub format: OutputFormat,
    /// Gzip text output.
    pub gzip: bool,
}

/// The fully-resolved Phase-1 configuration.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Verbatim argv (program name excluded) for the `@PG` `CL:` line (Phase 5).
    pub command_line: String,
    /// Aligner (Bowtie 2).
    pub aligner: Aligner,
    /// Library type.
    pub library: LibraryType,
    /// Read layout + files.
    pub layout: ReadLayout,
    /// Input format.
    pub format: ReadFormat,
    /// Discovered genome indexes + FASTA inventory.
    pub genome: GenomeIndexes,
    /// Detected Bowtie 2 binary + version.
    pub detected_aligner: DetectedAligner,
    /// Exact Bowtie 2 option string (per-instance `--norc`/`--nofw` added later).
    pub aligner_options: String,
    /// Gap penalties (for later MAPQ).
    pub gap_penalties: GapPenalties,
    /// Output target.
    pub output: OutputTarget,
}

/// Resolve a parsed [`Cli`] + the verbatim command line into a [`RunConfig`].
pub fn resolve(cli: &Cli, command_line: String) -> Result<RunConfig> {
    let aligner = resolve_aligner(cli)?;
    let library = resolve_library(cli)?;
    let format = resolve_format(cli)?;
    validate_multicore(cli)?;

    let (genome_arg, reads_positional) = resolve_genome_and_positional(cli)?;
    let layout = resolve_layout(cli, &reads_positional)?;

    let genome = discovery::discover_genome(&genome_arg)?;
    let detected_aligner = aligner::detect_bowtie2(cli.path_to_bowtie2.as_deref())?;
    let (aligner_options, gap_penalties) =
        options::build_aligner_options(cli, format, layout.is_paired())?;
    let output = resolve_output(cli)?;

    Ok(RunConfig {
        command_line,
        aligner,
        library,
        layout,
        format,
        genome,
        detected_aligner,
        aligner_options,
        gap_penalties,
        output,
    })
}

fn resolve_aligner(cli: &Cli) -> Result<Aligner> {
    if cli.hisat2 && cli.bowtie2 {
        return Err(AlignerError::Validation(
            "You may not select both --hisat2 and --bowtie2. Make your pick! [default is Bowtie 2]"
                .into(),
        ));
    }
    if cli.hisat2 && cli.minimap2 {
        return Err(AlignerError::Validation(
            "You may not select both --hisat2 and --minimap2. Make your pick!".into(),
        ));
    }
    if cli.minimap2 && cli.bowtie2 {
        return Err(AlignerError::Validation(
            "You may not select both --minimap2 and --bowtie2. Make your pick! [default is Bowtie 2]".into(),
        ));
    }
    if cli.hisat2 {
        return Err(AlignerError::Unsupported(
            "HISAT2 alignment is deferred to a v1.x follow-up; use Perl Bismark or --bowtie2 (default).".into(),
        ));
    }
    if cli.minimap2 {
        return Err(AlignerError::Unsupported(
            "minimap2 alignment is deferred to a v1.x follow-up; use Perl Bismark or --bowtie2 (default).".into(),
        ));
    }
    Ok(Aligner::Bowtie2)
}

fn resolve_library(cli: &Cli) -> Result<LibraryType> {
    if cli.non_directional {
        if cli.pbat {
            return Err(AlignerError::Validation(
                "A library can only be specified to be either non-directional or a PBAT-Seq library. \
                 Please respecify!"
                    .into(),
            ));
        }
        return Ok(LibraryType::NonDirectional);
    }
    if cli.pbat {
        // Perl 8155–8156: --pbat is incompatible with --gzip and with -f (FastA).
        if cli.gzip {
            return Err(AlignerError::Validation(
                "The option --pbat is currently not compatible with --gzip. Please run alignments with \
                 uncompressed temporary files, i.e. lose the option --gzip"
                    .into(),
            ));
        }
        if cli.fasta {
            return Err(AlignerError::Validation(
                "The option --pbat is currently only working with FastQ files. Please respecify (i.e. \
                 lose the option -f)!"
                    .into(),
            ));
        }
        return Ok(LibraryType::Pbat);
    }
    Ok(LibraryType::Directional)
}

/// Validate `--multicore`/`--parallel` (file-level). Perl 8244: must be ≥ 1.
/// (The feature itself is wired in Phase 9; this guards the input value now.)
fn validate_multicore(cli: &Cli) -> Result<()> {
    if let Some(m) = cli.multicore
        && m < 1
    {
        return Err(AlignerError::Validation(format!(
            "Core usage needs to be set to 1 or more (currently selected {m}). Please respecify!"
        )));
    }
    Ok(())
}

/// Flags that are recognised (parsed) but not yet *active* in this build — they
/// take effect in a later phase. Returned so the caller can warn the user
/// rather than silently accepting and ignoring them (code-review finding).
pub fn deferred_flags(cli: &Cli) -> Vec<&'static str> {
    let mut v = Vec::new();
    let mut push = |cond: bool, name: &'static str| {
        if cond {
            v.push(name);
        }
    };
    push(cli.skip.is_some(), "--skip");
    push(cli.upto.is_some(), "--upto");
    push(cli.unmapped, "--unmapped");
    push(cli.ambiguous, "--ambiguous");
    push(cli.ambig_bam, "--ambig_bam");
    push(cli.nucleotide_coverage, "--nucleotide_coverage");
    push(cli.rg_tag, "--rg_tag");
    push(cli.slam, "--slam");
    push(cli.non_bs_mm, "--non_bs_mm");
    push(cli.multicore.is_some(), "--multicore");
    push(cli.gzip, "--gzip");
    push(cli.prefix.is_some(), "--prefix");
    push(cli.basename.is_some(), "--basename");
    push(cli.old_flag, "--old_flag");
    push(cli.sam_no_hd, "--sam-no-hd");
    v
}

fn resolve_format(cli: &Cli) -> Result<ReadFormat> {
    if cli.fasta && cli.fastq {
        return Err(AlignerError::Validation(
            "Please specify either -q/--fastq OR -f/--fasta, not both.".into(),
        ));
    }
    Ok(if cli.fasta {
        ReadFormat::FastA
    } else {
        ReadFormat::FastQ
    })
}

/// Resolve the genome argument and the positional read files. The genome is
/// `--genome` if given, otherwise the first positional (Perl `shift @ARGV`,
/// 7604–7612). Returns `(genome_path, remaining_positional_reads)`.
fn resolve_genome_and_positional(cli: &Cli) -> Result<(PathBuf, Vec<String>)> {
    match &cli.genome {
        Some(g) => Ok((g.clone(), cli.positional.clone())),
        None => {
            let mut it = cli.positional.iter();
            let genome = it.next().ok_or_else(|| {
                AlignerError::Validation(
                    "No genome folder specified! USAGE: bismark_rs [options] <genome_folder> \
                     {-1 <mates1> -2 <mates2> | <singles>}"
                        .into(),
                )
            })?;
            Ok((PathBuf::from(genome), it.cloned().collect()))
        }
    }
}

fn resolve_layout(cli: &Cli, reads_positional: &[String]) -> Result<ReadLayout> {
    if let Some(m1) = &cli.mates1 {
        // Paired-end.
        if cli.single_end.is_some() {
            return Err(AlignerError::Validation(
                "You cannot set --single_end and supply files in paired-end format (-1 <mates1> -2 <mates2>). Please respecify!".into(),
            ));
        }
        let m2 = cli.mates2.as_ref().ok_or_else(|| {
            AlignerError::Validation(
                "Paired-end mapping requires the format: -1 <mates1> -2 <mates2>, please respecify!".into(),
            )
        })?;
        let mates1: Vec<String> = m1.split(',').map(str::to_string).collect();
        let mates2: Vec<String> = m2.split(',').map(str::to_string).collect();
        if mates1.len() != mates2.len() {
            return Err(AlignerError::Validation(
                "Paired-end mapping requires the same amount of mate1 and mate2 files, please respecify! (format: -1 <mates1> -2 <mates2>)".into(),
            ));
        }
        for (a, b) in mates1.iter().zip(&mates2) {
            if a == b {
                return Err(AlignerError::Validation(format!(
                    "[FATAL ERROR]: Read 1 ({a}) and Read 2 ({b}) files were specified as the exact same file, which is almost certainly unintentional (and wrong). Please re-specify!"
                )));
            }
            check_exists(a)?;
            check_exists(b)?;
        }
        return Ok(ReadLayout::PairedEnd { mates1, mates2 });
    }

    if cli.mates2.is_some() {
        return Err(AlignerError::Validation(
            "Paired-end mapping requires the format: -1 <mates1> -2 <mates2>, please respecify!"
                .into(),
        ));
    }

    // Single-end: explicit --single_end (`:`→`,`), else the positional reads.
    let reads: Vec<String> = if let Some(se) = &cli.single_end {
        se.replace(':', ",")
            .split(',')
            .map(str::to_string)
            .collect()
    } else {
        reads_positional.to_vec()
    };
    if reads.is_empty() || reads.iter().all(String::is_empty) {
        return Err(AlignerError::Validation(
            "No filename supplied! Please specify one or more files for single-end Bismark mapping!".into(),
        ));
    }
    for r in &reads {
        check_exists(r)?;
    }
    Ok(ReadLayout::SingleEnd { reads })
}

fn check_exists(file: &str) -> Result<()> {
    if Path::new(file).exists() {
        Ok(())
    } else {
        Err(AlignerError::InputFileMissing(file.to_string()))
    }
}

fn resolve_output(cli: &Cli) -> Result<OutputTarget> {
    if cli.sam {
        return Err(AlignerError::Unsupported(
            "SAM output is not yet supported in v1 (BAM only).".into(),
        ));
    }
    if cli.cram {
        return Err(AlignerError::Unsupported(
            "CRAM output is not yet supported in v1 (BAM only).".into(),
        ));
    }
    Ok(OutputTarget {
        output_dir: cli.output_dir.clone().unwrap_or_default(),
        temp_dir: cli.temp_dir.clone().unwrap_or_default(),
        basename: cli.basename.clone(),
        prefix: cli.prefix.clone(),
        format: OutputFormat::Bam,
        gzip: cli.gzip,
    })
}

impl RunConfig {
    /// A human-readable resolved-config summary (STDERR; not byte-gated).
    pub fn summary(&self) -> String {
        let library = match self.library {
            LibraryType::Directional => "directional",
            LibraryType::NonDirectional => "non-directional",
            LibraryType::Pbat => "pbat",
        };
        let (layout, files) = match &self.layout {
            ReadLayout::SingleEnd { reads } => ("single-end", reads.join(", ")),
            ReadLayout::PairedEnd { mates1, mates2 } => {
                let pairs: Vec<String> = mates1
                    .iter()
                    .zip(mates2)
                    .map(|(a, b)| format!("{a}+{b}"))
                    .collect();
                ("paired-end", pairs.join(", "))
            }
        };
        let format = match self.format {
            ReadFormat::FastQ => "FASTQ",
            ReadFormat::FastA => "FASTA",
        };
        format!(
            "Bismark aligner (Rust) — resolved configuration\n\
               aligner:        Bowtie 2 {} ({})\n\
               library:        {library}\n\
               layout:         {layout} [{format}]\n\
               reads:          {files}\n\
               genome:         {}\n\
               CT index:       {}\n\
               GA index:       {}\n\
               large index:    {}\n\
               FASTA(s):       {} file(s) ({:?})\n\
               aligner_options: {}\n\
               output:         BAM, dir={:?}, basename={:?}\n\
             (Phase 1: parse + discover + detect only — no alignment performed.)",
            self.detected_aligner.version,
            self.detected_aligner.path.display(),
            self.genome.genome_dir.display(),
            self.genome.ct_index_basename.display(),
            self.genome.ga_index_basename.display(),
            self.genome.large_index,
            self.genome.fastas.len(),
            self.genome.fasta_kind,
            self.aligner_options,
            self.output.output_dir,
            self.output.basename,
        )
    }
}
