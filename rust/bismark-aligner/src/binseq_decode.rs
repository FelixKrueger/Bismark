//! #1025 Phase 2 — Arc Institute BINSEQ (`.vbq`) read input.
//!
//! Bismark has no native BINSEQ input. This module transcodes a VBQ file into a
//! temporary FASTQ that byte-matches what `bqtools decode` would emit, so the
//! **existing, byte-frozen** bisulfite-convert → align → merge pipeline consumes
//! it unchanged. The contract is therefore "a BINSEQ run is identical to the
//! equivalent `bqtools decode` → FASTQ run" (the convert path's Perl byte-identity
//! is inherited transitively, exactly as for the uBAM backend in [`crate::ubam`]).
//!
//! ## Scope (v1): VBQ only
//! Only **VBQ** (`.vbq`) is decoded. `.cbq` and `.bq` are detected (so the input is
//! never silently mis-fed to the FASTQ parser) but **rejected fail-loud**:
//! - **BQ** is 2-bit fixed-length with **no quality and no names** — it cannot be
//!   faithfully aligned (Bismark needs real read names for output QNAMEs + real
//!   qualities), so it is rejected wholesale (D2).
//! - **CBQ** is the new (binseq 0.9.0) columnar variant whose serial, in-order
//!   reader is not cleanly exposed by the crate (only `process_parallel`); it is a
//!   documented fast-follow, rejected for now rather than read via a fragile path.
//!
//! ## Reader choice (load-bearing — plan R1)
//! Decodes via the **concrete, single-threaded `vbq::MmapReader`** block iterator
//! (`new_block` / `read_block_into` / `block.iter()`) — the deterministic, file-order
//! analog of [`crate::ubam`]'s noodles `record_bufs`. The unified `binseq::BinseqReader`
//! enum exposes ONLY the parallel `process_parallel` path, which fans blocks across
//! threads and flushes first-come-first-served → nondeterministic FASTQ order (gate
//! failure) and independent R1/R2 reordering (every paired mate mis-pairs). A serial
//! reader avoids both.
//!
//! ## Quality / header reject (file-level — plan R2)
//! D2 (reject quality-less / name-less BINSEQ) is enforced at the **file header**
//! level (`FileHeader.qual` / `.headers`), NOT per record. The crate **masks** the
//! absence of either: a missing quality column is back-filled with
//! `DEFAULT_QUALITY_SCORE` (`?`) so `squal()` is non-empty, and a missing header is
//! synthesized to the record's numeric index so `sheader()` is never empty. A
//! per-record emptiness check would therefore silently emit `?`-quality / index-named
//! FASTQ; the file-header flags are the only faithful signal.
//!
//! ## `bqtools decode` parity (bqtools 0.5.7, pinned from source)
//! `bqtools`'s `write_fastq_parts` emits exactly `@<header>\n<seq>\n+\n<qual>\n`.
//! The stored header carries **no** leading `@` (it is paraseq `record.id()`); bqtools
//! prepends one, so this decoder prepends `@` rather than storing it. Quality is the
//! stored bytes **verbatim** — raw ASCII, because binseq stores `record.qual()` as-is,
//! so there is NO phred offset to add (unlike the uBAM path). The separator is a bare
//! `+` line. Because both `bqtools decode` and this decoder call the SAME `binseq`
//! crate to decode the 2-bit/4-bit sequence, the decoded *values* are identical by
//! construction; only this line formatting must match. The contract is scoped to
//! quality+header-bearing files (we reject otherwise, where `bqtools decode` would
//! `?`-fill — a deliberate, never-silent divergence, plan R3).

use std::path::Path;

use crate::error::AlignerError;
use crate::error::Result;

/// Which BINSEQ variant an input path's extension names. Classified by extension
/// only (plan R4 — the crate has no magic-byte API; `.vbq`/`.cbq`/`.bq` are disjoint
/// from BAM magic and from FASTQ/FASTA, so the normal path is never misfired on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinseqExt {
    Vbq,
    Cbq,
    Bq,
}

/// Classify a path by its final extension. `None` for anything that is not BINSEQ.
fn binseq_ext(path: &Path) -> Option<BinseqExt> {
    match path.extension().and_then(|s| s.to_str()) {
        Some("vbq") => Some(BinseqExt::Vbq),
        Some("cbq") => Some(BinseqExt::Cbq),
        Some("bq") => Some(BinseqExt::Bq),
        _ => None,
    }
}

/// Is `path` a BINSEQ input? Extension-based (plan R4). Returns `true` for
/// `.vbq`/`.cbq`/`.bq`. This is **feature-independent** (always compiled) so a build
/// WITHOUT the `binseq-input` feature still routes a `.vbq` into this backend and
/// rejects it never-silently, rather than mis-feeding it to the FASTQ parser.
pub fn is_binseq_input(path: &Path) -> bool {
    binseq_ext(path).is_some()
}

/// CBQ-not-yet-supported reject (v1 scope; documented fast-follow).
fn reject_cbq() -> AlignerError {
    AlignerError::Validation(
        "CBQ (.cbq) BINSEQ input is not yet supported by bismark_rs (only VBQ is). \
         Convert it to VBQ (`bqtools`) or to FASTQ, then re-run."
            .into(),
    )
}

/// BQ reject (D2): BQ has no per-read quality and no names → cannot be aligned.
fn reject_bq() -> AlignerError {
    AlignerError::Validation(
        "BQ (.bq) BINSEQ input carries no per-read quality scores and no read names, so it \
         cannot be faithfully aligned by Bismark (which needs real qualities and output \
         QNAMEs). Re-encode as VBQ preserving quality + headers (`bqtools encode`)."
            .into(),
    )
}

/// Single- vs paired-end (peek the file header; a paired input is auto-split into
/// mates by the shared resolver). `.cbq`/`.bq` fail loud here (scope/D2); a `.vbq`
/// in a build without the `binseq-input` feature fails loud too.
pub fn is_paired(path: &Path) -> Result<bool> {
    match binseq_ext(path) {
        Some(BinseqExt::Vbq) => vbq_impl::vbq_is_paired(path),
        Some(BinseqExt::Cbq) => Err(reject_cbq()),
        Some(BinseqExt::Bq) => Err(reject_bq()),
        // Unreachable in practice: only called after `is_binseq_input` returned true.
        None => Err(AlignerError::Validation(format!(
            "'{}' is not a recognised BINSEQ file (.vbq/.cbq/.bq)",
            path.display()
        ))),
    }
}

/// Transcode a single-end BINSEQ input → one temp FASTQ named `<stem>.fastq`
/// (plan R6: `Path::file_stem` strips the `.vbq` so the downstream output stem is
/// what the equivalent `bqtools decode > <stem>.fastq` run would produce).
pub fn transcode_binseq_to_fastq_se(path: &Path, temp_dir: &Path) -> Result<std::path::PathBuf> {
    match binseq_ext(path) {
        Some(BinseqExt::Vbq) => vbq_impl::vbq_transcode_se(path, temp_dir),
        Some(BinseqExt::Cbq) => Err(reject_cbq()),
        Some(BinseqExt::Bq) => Err(reject_bq()),
        None => Err(AlignerError::Validation(format!(
            "'{}' is not a recognised BINSEQ file (.vbq/.cbq/.bq)",
            path.display()
        ))),
    }
}

/// Transcode a paired-end BINSEQ input → (R1, R2) temp FASTQs `<stem>_1/2.fastq`.
/// One VBQ record carries BOTH mates (plan R5), so there is no collation step: the
/// primary sequence → R1, the extended sequence → R2.
pub fn transcode_binseq_to_fastq_pe(
    path: &Path,
    temp_dir: &Path,
) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    match binseq_ext(path) {
        Some(BinseqExt::Vbq) => vbq_impl::vbq_transcode_pe(path, temp_dir),
        Some(BinseqExt::Cbq) => Err(reject_cbq()),
        Some(BinseqExt::Bq) => Err(reject_bq()),
        None => Err(AlignerError::Validation(format!(
            "'{}' is not a recognised BINSEQ file (.vbq/.cbq/.bq)",
            path.display()
        ))),
    }
}

// ===========================================================================
// VBQ decode — feature ON: real `binseq`-crate decode.
// ===========================================================================
#[cfg(feature = "binseq-input")]
mod vbq_impl {
    use std::fs::File;
    use std::io::{BufWriter, Write};
    use std::path::{Path, PathBuf};

    use binseq::BinseqRecord;
    use binseq::vbq::{FileHeader, MmapReader};

    use crate::error::{AlignerError, Result};

    /// Map a `binseq` crate error into an `AlignerError` (kept as a closure-friendly
    /// helper so `error.rs` never has to take a feature-gated `binseq` dependency).
    fn binseq_err(path: &Path, e: &binseq::Error) -> AlignerError {
        AlignerError::Validation(format!(
            "failed to read BINSEQ file '{}': {e}",
            path.display()
        ))
    }

    /// File stem for naming the temp FASTQ (basename minus the final `.vbq`).
    fn file_stem(path: &Path) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("reads")
            .to_string()
    }

    /// D2 (plan R2): reject a VBQ that lacks per-read quality or per-read headers, at
    /// the FILE-HEADER level (the crate masks per-record absence, so per-record checks
    /// would silently pass). Both are required for a faithful FASTQ reconstruction.
    fn require_quality_and_headers(header: &FileHeader, path: &Path) -> Result<()> {
        if !header.qual {
            return Err(AlignerError::Validation(format!(
                "BINSEQ input '{}' carries no per-read quality scores, so it cannot be \
                 faithfully aligned by Bismark. Re-encode it preserving quality (and headers) \
                 with `bqtools encode`.",
                path.display()
            )));
        }
        if !header.headers {
            return Err(AlignerError::Validation(format!(
                "BINSEQ input '{}' carries no per-read names/headers, so the aligned reads would \
                 have synthesized numeric QNAMEs rather than their original names. Re-encode it \
                 preserving headers (and quality) with `bqtools encode`.",
                path.display()
            )));
        }
        Ok(())
    }

    /// Append one record's 4 FASTQ lines, matching `bqtools decode`'s
    /// `write_fastq_parts`: `@<header>\n<seq>\n+\n<qual>\n`. The header is stored
    /// WITHOUT a leading `@` (we prepend it); quality is the stored bytes verbatim
    /// (raw ASCII — no phred offset). `bqtools` slices quality to `seq.len()`; with the
    /// file-level quality guarantee, `qual.len() == seq.len()`, so the slice is a no-op
    /// (kept defensively so a malformed equal-length-but-padded buffer can't overrun).
    fn write_fastq_record<W: Write>(
        w: &mut W,
        header: &[u8],
        seq: &[u8],
        qual: &[u8],
    ) -> Result<()> {
        w.write_all(b"@")?;
        w.write_all(header)?;
        w.write_all(b"\n")?;
        w.write_all(seq)?;
        w.write_all(b"\n+\n")?;
        w.write_all(&qual[..seq.len()])?;
        w.write_all(b"\n")?;
        Ok(())
    }

    /// `true` iff the VBQ file header marks records paired (`xlen > 0` per record).
    pub fn vbq_is_paired(path: &Path) -> Result<bool> {
        let reader = MmapReader::new(path).map_err(|e| binseq_err(path, &e))?;
        Ok(reader.is_paired())
    }

    /// Transcode a single-end VBQ into a temp FASTQ. Deterministic file order: blocks
    /// are read sequentially and each block iterates in stored order.
    pub fn vbq_transcode_se(path: &Path, temp_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(temp_dir)?;
        let out_path = temp_dir.join(format!("{}.fastq", file_stem(path)));
        let mut reader = MmapReader::new(path).map_err(|e| binseq_err(path, &e))?;
        require_quality_and_headers(&reader.header(), path)?;

        let mut w = BufWriter::new(File::create(&out_path)?);
        let mut block = reader.new_block();
        let mut seqbuf: Vec<u8> = Vec::new();
        while reader
            .read_block_into(&mut block)
            .map_err(|e| binseq_err(path, &e))?
        {
            for rec in block.iter() {
                // `decode_s` APPENDS the decoded bases; clear the reused buffer first.
                // (Serial reads never call `decode_all`, so `decode_s` decodes per
                // record — `sseq()` would panic here; `decode_s` is the correct call.)
                seqbuf.clear();
                rec.decode_s(&mut seqbuf)
                    .map_err(|e| binseq_err(path, &e))?;
                write_fastq_record(&mut w, rec.sheader(), &seqbuf, rec.squal())?;
            }
        }
        w.flush()?;
        Ok(out_path)
    }

    /// Transcode a paired-end VBQ into (R1, R2) temp FASTQs. One record carries both
    /// mates (plan R5): primary (`decode_s`/`sheader`/`squal`) → R1, extended
    /// (`decode_x`/`xheader`/`xqual`) → R2. No collation / desync logic is needed.
    pub fn vbq_transcode_pe(path: &Path, temp_dir: &Path) -> Result<(PathBuf, PathBuf)> {
        std::fs::create_dir_all(temp_dir)?;
        let stem = file_stem(path);
        let p1 = temp_dir.join(format!("{stem}_1.fastq"));
        let p2 = temp_dir.join(format!("{stem}_2.fastq"));
        let mut reader = MmapReader::new(path).map_err(|e| binseq_err(path, &e))?;
        require_quality_and_headers(&reader.header(), path)?;

        let mut w1 = BufWriter::new(File::create(&p1)?);
        let mut w2 = BufWriter::new(File::create(&p2)?);
        let mut block = reader.new_block();
        let mut sbuf: Vec<u8> = Vec::new();
        let mut xbuf: Vec<u8> = Vec::new();
        while reader
            .read_block_into(&mut block)
            .map_err(|e| binseq_err(path, &e))?
        {
            for rec in block.iter() {
                // Deliberate divergence from `bqtools decode`, which silently skips just the
                // R2 write for a mate-less record (`if !xbuf.is_empty()`). For an aligner that
                // would desync R1/R2 — every following read mis-pairs — so we fail loud
                // instead (never-silent). A well-formed paired VBQ never hits this (the file
                // header guarantees `xlen > 0` per record).
                if !rec.is_paired() {
                    return Err(AlignerError::Validation(format!(
                        "paired BINSEQ input '{}' has a record with no second mate (xlen == 0); \
                         the file is malformed for paired-end use.",
                        path.display()
                    )));
                }
                sbuf.clear();
                xbuf.clear();
                rec.decode_s(&mut sbuf).map_err(|e| binseq_err(path, &e))?;
                rec.decode_x(&mut xbuf).map_err(|e| binseq_err(path, &e))?;
                write_fastq_record(&mut w1, rec.sheader(), &sbuf, rec.squal())?;
                write_fastq_record(&mut w2, rec.xheader(), &xbuf, rec.xqual())?;
            }
        }
        w1.flush()?;
        w2.flush()?;
        Ok((p1, p2))
    }

    #[cfg(test)]
    mod tests {
        //! Format-level unit tests (feature-on). The end-to-end decode of real `.vbq`
        //! fixtures built with the `binseq` writer lives in `tests/binseq_transcode.rs`.
        use super::*;

        #[test]
        fn write_fastq_record_matches_bqtools_layout() {
            // `@<header>\n<seq>\n+\n<qual>\n`; header WITHOUT a stored `@`; qual verbatim.
            let mut out = Vec::new();
            write_fastq_record(&mut out, b"read1 comment", b"ACGTN", b"IIIII").unwrap();
            assert_eq!(out, b"@read1 comment\nACGTN\n+\nIIIII\n");
        }

        #[test]
        fn write_fastq_record_quality_is_verbatim_not_phred_shifted() {
            // Unlike the uBAM path (phred +33), VBQ stores raw ASCII quality — emitted as-is.
            let mut out = Vec::new();
            write_fastq_record(&mut out, b"r", b"AC", b"#%").unwrap();
            assert_eq!(out, b"@r\nAC\n+\n#%\n");
        }
    }
}

// ===========================================================================
// VBQ decode — feature OFF: fail loud (never-silent).
// ===========================================================================
#[cfg(not(feature = "binseq-input"))]
mod vbq_impl {
    use std::path::{Path, PathBuf};

    use crate::error::{AlignerError, Result};

    /// This build was compiled without the `binseq-input` feature, so the `binseq`
    /// crate (and its zstd decode) is absent. Detection still fired (extension), so we
    /// reject explicitly rather than mis-feed the FASTQ parser.
    fn unsupported() -> AlignerError {
        AlignerError::Validation(
            "this bismark_rs build was compiled without BINSEQ (.vbq) support; rebuild with \
             `--features binseq-input` (the released Linux binaries include it), or convert the \
             input to FASTQ first."
                .into(),
        )
    }

    pub fn vbq_is_paired(_path: &Path) -> Result<bool> {
        Err(unsupported())
    }
    pub fn vbq_transcode_se(_path: &Path, _temp_dir: &Path) -> Result<PathBuf> {
        Err(unsupported())
    }
    pub fn vbq_transcode_pe(_path: &Path, _temp_dir: &Path) -> Result<(PathBuf, PathBuf)> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    //! Feature-INDEPENDENT tests: extension detection + the CBQ/BQ rejects compile and
    //! run on ANY build (default feature-off CI exercises these).
    use super::*;
    use std::path::Path;

    #[test]
    fn detects_binseq_extensions_only() {
        assert!(is_binseq_input(Path::new("reads.vbq")));
        assert!(is_binseq_input(Path::new("reads.cbq")));
        assert!(is_binseq_input(Path::new("reads.bq")));
        // Never misfire on the normal read formats (plan R4).
        assert!(!is_binseq_input(Path::new("reads.fastq")));
        assert!(!is_binseq_input(Path::new("reads.fq.gz")));
        assert!(!is_binseq_input(Path::new("reads.fa")));
        assert!(!is_binseq_input(Path::new("reads.bam")));
        assert!(!is_binseq_input(Path::new("reads")));
    }

    #[test]
    fn cbq_is_rejected_fail_loud() {
        let err = transcode_binseq_to_fastq_se(Path::new("x.cbq"), Path::new("/tmp")).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("CBQ") && msg.contains("not yet supported"),
            "got: {msg}"
        );
        // The paired + is_paired entry points reject identically.
        assert!(format!("{}", is_paired(Path::new("x.cbq")).unwrap_err()).contains("CBQ"));
    }

    #[test]
    fn bq_is_rejected_fail_loud() {
        let err = transcode_binseq_to_fastq_se(Path::new("x.bq"), Path::new("/tmp")).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("BQ (.bq)") && msg.contains("cannot be faithfully aligned"),
            "got: {msg}"
        );
    }
}
