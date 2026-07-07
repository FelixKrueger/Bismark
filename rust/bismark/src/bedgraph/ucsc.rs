//! Optional `--ucsc` post-pass (Perl `bismark2bedGraph:514-552`).
//!
//! Re-reads the just-written bedGraph and emits a UCSC-compatible variant
//! (`{out}_UCSC.bedGraph.gz`): the header line is copied verbatim; for each
//! data line the chromosome name is transformed — `MT` → `chrM`, and any
//! name not already starting with `chr` is prefixed with `chr`. Start, end
//! and percentage columns pass through untouched.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::bedgraph::cli::ResolvedConfig;
use crate::bedgraph::error::BismarkBedgraphError;

/// Transform a chromosome name for UCSC output. `MT` → `chrM` (checked
/// first, so it does not become `chrMT`); otherwise prefix `chr` unless it
/// already starts with `chr`. Perl `:537-545`.
fn ucsc_chr(chr: &str) -> String {
    if chr == "MT" {
        "chrM".to_string()
    } else if chr.starts_with("chr") {
        chr.to_string()
    } else {
        format!("chr{chr}")
    }
}

/// Write the UCSC bedGraph by re-reading the bedGraph already on disk.
pub fn write_ucsc(cfg: &ResolvedConfig) -> Result<(), BismarkBedgraphError> {
    let in_file = File::open(cfg.output_dir.join(&cfg.bedgraph_name))?;
    let mut reader = BufReader::new(GzDecoder::new(in_file));

    let mut out = GzEncoder::new(
        File::create(cfg.output_dir.join(&cfg.ucsc_name))?,
        Compression::default(),
    );

    let mut buf = String::new();
    let mut first = true;
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        if first {
            first = false;
            // Header line ("track type=bedGraph\n") copied verbatim.
            out.write_all(buf.as_bytes())?;
            continue;
        }
        // Split off the chromosome (first column); the remainder
        // (start\tend\tpct[\n]) passes through unchanged.
        let line = buf.strip_suffix('\n').unwrap_or(buf.as_str());
        match line.split_once('\t') {
            Some((chr, rest)) => {
                writeln!(out, "{}\t{}", ucsc_chr(chr), rest)?;
            }
            None => {
                // No tab — emit unchanged (defensive; shouldn't happen).
                out.write_all(buf.as_bytes())?;
            }
        }
    }

    out.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mt_becomes_chr_m_not_chr_mt() {
        assert_eq!(ucsc_chr("MT"), "chrM");
    }

    #[test]
    fn bare_name_gets_chr_prefix() {
        assert_eq!(ucsc_chr("1"), "chr1");
        assert_eq!(ucsc_chr("X"), "chrX");
    }

    #[test]
    fn already_prefixed_unchanged() {
        assert_eq!(ucsc_chr("chr1"), "chr1");
        assert_eq!(ucsc_chr("chrM"), "chrM");
    }
}
