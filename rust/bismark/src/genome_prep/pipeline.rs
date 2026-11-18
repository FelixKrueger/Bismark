//! Top-level orchestration: Step I (folders + discovery + early aligner-path
//! validation) → Step II (conversion) → Step III (indexing) → Step IV (opt
//! combined genome).

use std::path::PathBuf;

use crate::genome_prep::cli::{Aligner, Cli, ResolvedConfig};
use crate::genome_prep::error::GenomePrepError;
use crate::genome_prep::logging::Logger;
use crate::genome_prep::{
    BISMARK_VERSION, bwamem3, combined, composition, convert, discovery, folders, indexer,
};

fn aligner_label(a: Aligner) -> &'static str {
    match a {
        Aligner::Bowtie2 => "Bowtie 2",
        Aligner::Hisat2 => "HISAT2",
        Aligner::Minimap2 => "Minimap2",
        Aligner::BwaMem3 => "bwa-mem3",
    }
}

/// Run the full genome-preparation pipeline from a parsed [`Cli`].
pub fn run(cli: Cli) -> Result<(), GenomePrepError> {
    let config = cli.validate()?;
    let logger = Logger::new(config.verbose);

    if config.slam {
        logger.note("`--slam` is deprecated and slated for removal in a future release.");
        logger.note(
            "Genome will be generated and indexed with in-silico T->C transitions, \
             and NOT in BISULFITE MODE",
        );
    }
    // ── Step I — discover FASTA, validate explicit aligner path, make folders ──
    logger.info("Bismark Genome Preparation - Step I: Preparing folders");
    let files = discovery::find_fasta_files(&config.genome_folder)?;
    logger.note(&format!(
        "Bisulfite Genome Indexer version {BISMARK_VERSION}"
    ));
    // Validate an explicit --path_to_aligner EARLY (before conversion), so a
    // bad path fails before any FASTA is written. No `which`-fallback here.
    let explicit_bin = match &config.path_to_aligner {
        Some(dir) => Some(indexer::resolve_explicit(dir, config.aligner)?),
        None => None,
    };
    if config.aligner == Aligner::BwaMem3 {
        return run_bwamem3(&config, &files, explicit_bin, &logger);
    }
    let (ct_dir, ga_dir) = folders::create_tree(&config.genome_folder, &logger)?;

    // ── Step I.5 — optional genomic composition (Perl runs `get_genomic_frequencies`
    // BEFORE `process_sequence_files`). Errors here (duplicate chromosome / not
    // FASTA) fire before any frequency table OR converted FASTA is written. ──
    if config.genomic_composition {
        logger.note(
            "Calculating genomic nucleotide frequencies (this may take several minutes \
             depending on genome size) ...",
        );
        composition::write_genomic_composition(&files, &config.genome_folder, &logger)?;
        logger.note("Finished processing genomic nucleotide frequencies\n");
    }

    // ── Step II — bisulfite conversion (the byte-identity core) ──
    logger.info("Bismark Genome Preparation - Step II: Bisulfite converting reference genome");
    let counts =
        convert::convert_split(&files, &ct_dir, &ga_dir, config.single_fasta, config.slam)?;
    println!("\nTotal number of conversions performed:");
    if config.slam {
        println!("T->C:\t{}", counts.ct);
        println!("A->G:\t{}", counts.ga);
    } else {
        println!("C->T:\t{}", counts.ct);
        println!("G->A:\t{}", counts.ga);
    }

    // ── Step III — external indexer (concurrent CT/GA) ──
    let bin = match explicit_bin {
        Some(p) => p,
        None => indexer::discover(config.aligner)?,
    };
    logger.note(&format!(
        "Bismark Genome Preparation - Step III: Launching the {} indexer",
        aligner_label(config.aligner)
    ));
    indexer::run_both(
        &bin,
        config.aligner,
        &ct_dir,
        &ga_dir,
        config.threads,
        config.large_index,
    )?;

    // ── Step IV — optional combined genome (Bismark-Rust extension) ──
    if config.combined_genome {
        let combined_dir = config
            .genome_folder
            .join("Bisulfite_Genome")
            .join("Combined");
        logger.note(
            "Bismark Genome Preparation - building the combined CT+GA reference (--combined_genome)",
        );
        combined::build(
            &files,
            &combined_dir,
            &bin,
            config.aligner,
            config.threads,
            config.large_index,
            config.slam,
        )?;
    }

    logger.note(
        "\n=========================================\n\nGenome preparation complete. Enjoy!\n",
    );
    Ok(())
}

/// `--bwamem3`: the bwa-mem3 methylation index, then the genomic composition (optional).
fn run_bwamem3(
    config: &ResolvedConfig,
    files: &[PathBuf],
    explicit_bin: Option<PathBuf>,
    logger: &Logger,
) -> Result<(), GenomePrepError> {
    bwamem3::reject_index_files(files)?;
    let bin = match explicit_bin {
        Some(p) => p,
        None => indexer::discover(config.aligner)?,
    };
    let version = bwamem3::check_version(&bin, logger)?;
    logger.note(&format!("Using bwa-mem3 {version} ({})", bin.display()));
    logger.note("Bismark Genome Preparation - Step II: Launching the bwa-mem3 indexer");
    let prefix = bwamem3::build(
        files,
        &config.genome_folder,
        &bin,
        config.parallel,
        config.bwamem3_max_memory.as_deref(),
        logger,
    )?;
    logger.note(&format!(
        "\nbwa-mem3 methylation index written to {}/ (prefix {}).\n\
         `bismark --taps` and `bismark --illumina_5base --bwamem3` find it with --genome {}",
        prefix.parent().unwrap_or(&prefix).display(),
        prefix.display(),
        config.genome_folder.display()
    ));
    // After the index, so bad contig names and a short memory budget fail before this pass.
    if config.genomic_composition {
        logger.note(
            "Calculating genomic nucleotide frequencies (this may take several minutes \
             depending on genome size) ...",
        );
        composition::write_genomic_composition(files, &config.genome_folder, logger)?;
        logger.note("Finished processing genomic nucleotide frequencies\n");
    }
    logger.note(
        "\n=========================================\n\nGenome preparation complete. Enjoy!\n",
    );
    Ok(())
}
