//! Output writers: the sorted bedGraph (`.gz`), the coverage file
//! (`.bismark.cov.gz`), and the optional plain `.zero.cov`. Mirrors Perl
//! `bismark2bedGraph` `generate_output` (`:590-618`) and the header/line
//! layouts (`:116`, `:406`/`:607`, `:409`/`:610`, `:413`/`:614`).
//!
//! Byte-identity is at the **decompressed-content** level (SPEC §1.1 D1):
//! we use pure-Rust `flate2`; the compressed bytes need not match GNU
//! `gzip`, but `zcat`-decompressed content must.

use std::fs::File;
use std::io::{BufWriter, Write};

use flate2::Compression;
use flate2::write::GzEncoder;

use crate::aggregate::Aggregator;
use crate::cli::ResolvedConfig;
use crate::error::BismarkBedgraphError;
use crate::fmt_g::format_g15;

/// Write the bedGraph + coverage (+ optional zero) outputs from a populated
/// aggregator. Positions whose total coverage is below `cfg.cutoff` are
/// dropped (Perl `:399`/`:601`).
pub fn write_outputs(cfg: &ResolvedConfig, agg: Aggregator) -> Result<(), BismarkBedgraphError> {
    let sorted = agg.into_sorted();

    let mut bedgraph = GzEncoder::new(
        File::create(cfg.output_dir.join(&cfg.bedgraph_name))?,
        Compression::default(),
    );
    // Header line — always written (Perl `:116`).
    writeln!(bedgraph, "track type=bedGraph")?;

    let mut coverage = GzEncoder::new(
        File::create(cfg.output_dir.join(&cfg.coverage_name))?,
        Compression::default(),
    );

    // Zero-based coverage is plain text (NOT gzipped) — Perl `:132`.
    let mut zero = if cfg.zero_based {
        Some(BufWriter::new(File::create(
            cfg.output_dir.join(&cfg.zero_name),
        )?))
    } else {
        None
    };

    for (chr, positions) in &sorted {
        for &(pos, meth, unmeth) in positions {
            let total = meth + unmeth;
            if total < cfg.cutoff {
                continue;
            }
            // Same string for every file (Perl reuses `$meth_percentage`).
            let pct = format_g15(meth as f64 / total as f64 * 100.0);
            let start = pos - 1; // bedGraph start is 0-based (pos ≥ 1 enforced at parse).

            // bedGraph: chr, 0-based start, 1-based end, pct.
            writeln!(bedgraph, "{chr}\t{start}\t{pos}\t{pct}")?;
            // coverage: chr, 1-based start = end, pct, meth, unmeth.
            writeln!(coverage, "{chr}\t{pos}\t{pos}\t{pct}\t{meth}\t{unmeth}")?;
            // zero: chr, 0-based start, 1-based end, pct, meth, unmeth.
            if let Some(z) = zero.as_mut() {
                writeln!(z, "{chr}\t{start}\t{pos}\t{pct}\t{meth}\t{unmeth}")?;
            }
        }
    }

    // Finish the gzip streams (writes the trailer) and flush the plain file.
    bedgraph.finish()?;
    coverage.finish()?;
    if let Some(mut z) = zero {
        z.flush()?;
    }
    Ok(())
}
