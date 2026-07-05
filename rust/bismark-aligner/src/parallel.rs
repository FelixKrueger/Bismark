//! Phase 9b — order-preserving file-level threading (`--multicore`/`--parallel`).
//!
//! `--parallel N` (N > 1) splits each read file (SE) or mate-pair (PE) into **N
//! contiguous chunks**, runs the full convert → 2–4 Bowtie 2 → lockstep-merge
//! pipeline ([`crate::process_se_chunk`]/[`crate::process_pe_chunk`]) on each chunk
//! in its own [`std::thread::scope`] worker, then **merges the per-chunk outputs in
//! chunk order** into the single Bismark BAM + report + `--unmapped`/`--ambiguous`/
//! `--ambig_bam`. This is **byte-identical to single-core** (`--parallel 1`) and to
//! Perl single-core — the worker-invariance gate (PLAN §1). It deliberately does NOT
//! reproduce Perl's own `--multicore N` byte layout (Perl fork+modulo striping
//! reorders the merged reads; PLAN §2.4).
//!
//! The three invariants (PLAN §2.5):
//! 1. **Contiguous partition** — each effective read goes to exactly one chunk, in
//!    order ([`split_contiguous`]). Concatenating chunks in order == the input.
//! 2. **In-order single-writer merge** — [`merge_bams`] copies per-chunk records in
//!    chunk order through one writer; [`merge_aux_gz`] re-emits per-chunk *plain* aux
//!    through ONE `GzEncoder` (a single-member gz raw-identical to `--parallel 1`).
//! 3. **Commutative counter sum** — [`crate::merge::Counters::merge`] is field-wise
//!    addition, so the report is identical regardless of worker count.
//!
//! `skip`/`upto` are applied **once, at the split**, with the converter's exact
//! `(skip, upto]` 1-based-ordinal arithmetic; the per-chunk pipeline runs with them
//! **cleared** via a `RunConfig` clone (so neither the converter nor `drive_merge`
//! re-applies them — PLAN §3.6, A-Imp2/B-I1).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;

use bismark_io::BamWriter;
use noodles_sam::Header;

use crate::aux_out::{self, AuxKind};
use crate::config::{LibraryType, ReadFormat, RunConfig};
use crate::convert::ConvertOptions;
use crate::error::{AlignerError, Result};
use crate::genome::{Genome, read_genome_into_memory};
use crate::merge::Counters;
use crate::output::{build_refid, generate_sam_header};
use crate::report::{self, ReportHeader};
use crate::{PeSinks, Sinks};

/// One SE chunk's outputs: the per-chunk temp BAM (+ optional ambig BAM) and the
/// plain per-chunk `--unmapped`/`--ambiguous` temps (merged by the orchestrator),
/// plus the chunk's `Counters`. Temp paths are deleted after the merge.
struct SeChunkOutcome {
    bam: PathBuf,
    ambig_bam: Option<PathBuf>,
    unmapped: Option<PathBuf>,
    ambiguous: Option<PathBuf>,
    counters: Counters,
}

/// One PE chunk's outputs (the PE aux is per-mate: `_1`/`_2`).
struct PeChunkOutcome {
    bam: PathBuf,
    ambig_bam: Option<PathBuf>,
    unmapped_1: Option<PathBuf>,
    unmapped_2: Option<PathBuf>,
    ambiguous_1: Option<PathBuf>,
    ambiguous_2: Option<PathBuf>,
    counters: Counters,
}

/// Join a temp-dir + name (empty temp-dir = CWD-relative, mirroring the converter).
fn temp_join(temp_dir: &Path, name: &str) -> PathBuf {
    if temp_dir.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        temp_dir.join(name)
    }
}

/// Read one record (`arity` lines) as raw bytes (newlines kept) into `buf`. Returns
/// `false` at clean EOF **or** on a truncated trailing record (mirrors the drivers'
/// break-on-incomplete: Perl `last unless ($header and $sequence …)`).
fn read_record(r: &mut dyn BufRead, arity: usize, buf: &mut Vec<u8>) -> io::Result<bool> {
    buf.clear();
    for _ in 0..arity {
        let n = r.read_until(b'\n', buf)?;
        if n == 0 {
            // EOF mid-record (or at a clean boundary): no further complete record.
            return Ok(false);
        }
    }
    Ok(true)
}

/// `true` if record `count` (1-based) is inside the `(skip, upto]` effective window,
/// with the converter's Perl-falsy-0 guard (`convert.rs:307–327` / `lib.rs:513–525`).
/// `None`/`Some(0)` both disable. The caller stops reading once `count > upto`.
#[inline]
fn in_window(count: u64, skip: Option<u64>, upto: Option<u64>) -> bool {
    if matches!(skip, Some(s) if s > 0 && count <= s) {
        return false;
    }
    if matches!(upto, Some(u) if u > 0 && count > u) {
        return false;
    }
    true
}

/// `true` if `count` is past the `upto` cutoff (stop reading).
#[inline]
fn past_upto(count: u64, upto: Option<u64>) -> bool {
    matches!(upto, Some(u) if u > 0 && count > u)
}

/// Per-chunk record quotas for `eff` effective records over `n` chunks: balanced
/// contiguous (`base = eff/n`, the first `eff % n` chunks get one extra). Any
/// contiguous partition is byte-identical after the ordered merge; this one
/// minimises chunk-size skew. Trailing chunks get `0` when `eff < n`.
fn quotas(eff: u64, n: u32) -> Vec<u64> {
    let n = u64::from(n);
    let base = eff / n;
    let rem = eff % n;
    (0..n).map(|i| base + u64::from(i < rem)).collect()
}

/// Count effective (`(skip, upto]`) records in a single SE input. Pass 1 of the
/// 2-pass split.
fn count_effective(
    input: &Path,
    arity: usize,
    skip: Option<u64>,
    upto: Option<u64>,
) -> Result<u64> {
    let mut reader = crate::open_reader(input)?;
    let mut buf = Vec::new();
    let mut count: u64 = 0;
    let mut eff: u64 = 0;
    while read_record(reader.as_mut(), arity, &mut buf)? {
        count += 1;
        if past_upto(count, upto) {
            break;
        }
        if in_window(count, skip, upto) {
            eff += 1;
        }
    }
    Ok(eff)
}

/// Split a single SE input into `n` contiguous plain subset files under `temp_dir`,
/// applying `skip`/`upto` once (the converter's `(skip, upto]` arithmetic). Subsets
/// are named off the ORIGINAL basename (`<basename>.temp.<chunk>`, NO prefix, NO
/// `.gz` — written plain so the converter's suffix-based gz detection reads them
/// plain; A-Imp7/B-I5). Returns the `n` subset paths in chunk order.
fn split_contiguous(
    input: &Path,
    temp_dir: &Path,
    n: u32,
    arity: usize,
    skip: Option<u64>,
    upto: Option<u64>,
) -> Result<Vec<PathBuf>> {
    let eff = count_effective(input, arity, skip, upto)?;
    let quota = quotas(eff, n);

    let base_name = input
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or_else(|| {
            AlignerError::Validation(format!("could not derive a file name from {input:?}"))
        })?;
    if !temp_dir.as_os_str().is_empty() {
        std::fs::create_dir_all(temp_dir)?;
    }
    let paths: Vec<PathBuf> = (0..n)
        .map(|i| temp_join(temp_dir, &format!("{base_name}.temp.{i}")))
        .collect();
    let mut writers: Vec<BufWriter<File>> = paths
        .iter()
        .map(|p| Ok(BufWriter::new(File::create(p)?)))
        .collect::<Result<_>>()?;

    let mut reader = crate::open_reader(input)?;
    let mut buf = Vec::new();
    let mut count: u64 = 0;
    let mut chunk: usize = 0;
    let mut in_chunk: u64 = 0;
    while read_record(reader.as_mut(), arity, &mut buf)? {
        count += 1;
        if past_upto(count, upto) {
            break;
        }
        if !in_window(count, skip, upto) {
            continue;
        }
        // Advance to the next chunk with remaining quota (skips 0-quota chunks).
        while chunk < writers.len() && in_chunk >= quota[chunk] {
            chunk += 1;
            in_chunk = 0;
        }
        // `chunk < len` holds because Σquota == eff == the number of effective reads.
        writers[chunk].write_all(&buf)?;
        in_chunk += 1;
    }
    for mut w in writers {
        w.flush()?;
    }
    Ok(paths)
}

/// Count effective PE *pairs* over the COMMON (min) record count — mirroring
/// `drive_merge_pe`'s break-on-first-incomplete (`lib.rs:1037–1049`). A pair exists
/// iff BOTH mates have a record; `skip`/`upto` apply on the pair ordinal (B-I6).
fn count_effective_pe(
    r1: &Path,
    r2: &Path,
    arity: usize,
    skip: Option<u64>,
    upto: Option<u64>,
) -> Result<u64> {
    let mut a = crate::open_reader(r1)?;
    let mut b = crate::open_reader(r2)?;
    let (mut buf1, mut buf2) = (Vec::new(), Vec::new());
    let mut count: u64 = 0;
    let mut eff: u64 = 0;
    loop {
        let has1 = read_record(a.as_mut(), arity, &mut buf1)?;
        let has2 = read_record(b.as_mut(), arity, &mut buf2)?;
        if !(has1 && has2) {
            break;
        }
        count += 1;
        if past_upto(count, upto) {
            break;
        }
        if in_window(count, skip, upto) {
            eff += 1;
        }
    }
    Ok(eff)
}

/// PE lockstep split: partition the COMMON (min) pair count into `n` contiguous
/// chunks, writing both mates' subset files on identical boundaries. Returns
/// `(r1 subset paths, r2 subset paths)`.
fn split_contiguous_pe(
    r1: &Path,
    r2: &Path,
    temp_dir: &Path,
    n: u32,
    arity: usize,
    skip: Option<u64>,
    upto: Option<u64>,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let eff = count_effective_pe(r1, r2, arity, skip, upto)?;
    let quota = quotas(eff, n);

    let name1 = r1
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or_else(|| {
            AlignerError::Validation(format!("could not derive a file name from {r1:?}"))
        })?;
    let name2 = r2
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or_else(|| {
            AlignerError::Validation(format!("could not derive a file name from {r2:?}"))
        })?;
    if !temp_dir.as_os_str().is_empty() {
        std::fs::create_dir_all(temp_dir)?;
    }
    let paths1: Vec<PathBuf> = (0..n)
        .map(|i| temp_join(temp_dir, &format!("{name1}.temp.{i}")))
        .collect();
    let paths2: Vec<PathBuf> = (0..n)
        .map(|i| temp_join(temp_dir, &format!("{name2}.temp.{i}")))
        .collect();
    let mut w1: Vec<BufWriter<File>> = paths1
        .iter()
        .map(|p| Ok(BufWriter::new(File::create(p)?)))
        .collect::<Result<_>>()?;
    let mut w2: Vec<BufWriter<File>> = paths2
        .iter()
        .map(|p| Ok(BufWriter::new(File::create(p)?)))
        .collect::<Result<_>>()?;

    let mut a = crate::open_reader(r1)?;
    let mut b = crate::open_reader(r2)?;
    let (mut buf1, mut buf2) = (Vec::new(), Vec::new());
    let mut count: u64 = 0;
    let mut chunk: usize = 0;
    let mut in_chunk: u64 = 0;
    loop {
        let has1 = read_record(a.as_mut(), arity, &mut buf1)?;
        let has2 = read_record(b.as_mut(), arity, &mut buf2)?;
        if !(has1 && has2) {
            break;
        }
        count += 1;
        if past_upto(count, upto) {
            break;
        }
        if !in_window(count, skip, upto) {
            continue;
        }
        while chunk < w1.len() && in_chunk >= quota[chunk] {
            chunk += 1;
            in_chunk = 0;
        }
        w1[chunk].write_all(&buf1)?;
        w2[chunk].write_all(&buf2)?;
        in_chunk += 1;
    }
    for mut w in w1 {
        w.flush()?;
    }
    for mut w in w2 {
        w.flush()?;
    }
    Ok((paths1, paths2))
}

/// Build per-chunk SE sinks: a real per-chunk BAM (+ optional ambig BAM) plus
/// **plain** (`AuxWriter::Plain`) `--unmapped`/`--ambiguous` temps (the gz happens at
/// the merge — PLAN §3.5). Constructs the crate-private [`Sinks`] directly.
fn open_chunk_se_sinks(
    header: &Header,
    bam: &Path,
    ambig: Option<&Path>,
    unmapped: Option<&Path>,
    ambiguous: Option<&Path>,
) -> Result<Sinks> {
    let bam = BamWriter::from_path(bam, header.clone())
        .map_err(|e| AlignerError::Validation(format!("failed to open chunk BAM: {e}")))?;
    let ambig_bam = match ambig {
        Some(p) => Some(BamWriter::from_path(p, header.clone()).map_err(|e| {
            AlignerError::Validation(format!("failed to open chunk ambig BAM: {e}"))
        })?),
        None => None,
    };
    let plain = |p: Option<&Path>| -> Result<Option<crate::AuxWriter>> {
        Ok(match p {
            Some(p) => Some(crate::AuxWriter::Plain(BufWriter::new(File::create(p)?))),
            None => None,
        })
    };
    Ok(Sinks {
        bam,
        ambig_bam,
        unmapped: plain(unmapped)?,
        ambiguous: plain(ambiguous)?,
    })
}

/// Build per-chunk PE sinks (plain `_1`/`_2` aux).
#[allow(clippy::too_many_arguments)]
fn open_chunk_pe_sinks(
    header: &Header,
    bam: &Path,
    ambig: Option<&Path>,
    unmapped_1: Option<&Path>,
    unmapped_2: Option<&Path>,
    ambiguous_1: Option<&Path>,
    ambiguous_2: Option<&Path>,
) -> Result<PeSinks> {
    let bam = BamWriter::from_path(bam, header.clone())
        .map_err(|e| AlignerError::Validation(format!("failed to open chunk BAM: {e}")))?;
    let ambig_bam = match ambig {
        Some(p) => Some(BamWriter::from_path(p, header.clone()).map_err(|e| {
            AlignerError::Validation(format!("failed to open chunk ambig BAM: {e}"))
        })?),
        None => None,
    };
    let plain = |p: Option<&Path>| -> Result<Option<crate::AuxWriter>> {
        Ok(match p {
            Some(p) => Some(crate::AuxWriter::Plain(BufWriter::new(File::create(p)?))),
            None => None,
        })
    };
    Ok(PeSinks {
        bam,
        ambig_bam,
        unmapped_1: plain(unmapped_1)?,
        unmapped_2: plain(unmapped_2)?,
        ambiguous_1: plain(ambiguous_1)?,
        ambiguous_2: plain(ambiguous_2)?,
    })
}

/// Process one SE chunk: open plain per-chunk sinks at temp paths, run
/// [`crate::process_se_chunk`] against the subset, finalise, and clean up the
/// chunk-local converted + subset temps. `cfg` has skip/upto cleared.
fn se_chunk_job(
    cfg: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    header: &Header,
    subset: &Path,
    opts: &ConvertOptions,
) -> Result<SeChunkOutcome> {
    let base = subset
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .expect("subset has a file name");
    let td = &cfg.output.temp_dir;
    let tok = cfg.aligner.token();
    let bam = temp_join(td, &format!("{base}_bismark_{tok}.bam"));
    let ambig = cfg
        .ambig_bam
        .then(|| temp_join(td, &format!("{base}_bismark_{tok}.ambig.bam")));
    let unmapped = cfg
        .unmapped
        .then(|| temp_join(td, &format!("{base}_unmapped.tmp")));
    let ambiguous = cfg
        .ambiguous
        .then(|| temp_join(td, &format!("{base}_ambiguous.tmp")));

    let mut sinks = open_chunk_se_sinks(
        header,
        &bam,
        ambig.as_deref(),
        unmapped.as_deref(),
        ambiguous.as_deref(),
    )?;
    let mut counters = Counters::default();
    let converted =
        crate::process_se_chunk(cfg, genome, refid, subset, opts, &mut sinks, &mut counters)?;
    sinks.finish()?;

    for cr in &converted {
        let _ = std::fs::remove_file(&cr.path);
    }
    let _ = std::fs::remove_file(subset);

    Ok(SeChunkOutcome {
        bam,
        ambig_bam: ambig,
        unmapped,
        ambiguous,
        counters,
    })
}

/// Process one PE chunk.
fn pe_chunk_job(
    cfg: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    header: &Header,
    subset_1: &Path,
    subset_2: &Path,
    opts: &ConvertOptions,
) -> Result<PeChunkOutcome> {
    let base = subset_1
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .expect("subset has a file name");
    let td = &cfg.output.temp_dir;
    let tok = cfg.aligner.token();
    let bam = temp_join(td, &format!("{base}_bismark_{tok}_pe.bam"));
    let ambig = cfg
        .ambig_bam
        .then(|| temp_join(td, &format!("{base}_bismark_{tok}_pe.ambig.bam")));
    let unmapped_1 = cfg
        .unmapped
        .then(|| temp_join(td, &format!("{base}_unmapped_1.tmp")));
    let unmapped_2 = cfg
        .unmapped
        .then(|| temp_join(td, &format!("{base}_unmapped_2.tmp")));
    let ambiguous_1 = cfg
        .ambiguous
        .then(|| temp_join(td, &format!("{base}_ambiguous_1.tmp")));
    let ambiguous_2 = cfg
        .ambiguous
        .then(|| temp_join(td, &format!("{base}_ambiguous_2.tmp")));

    let mut sinks = open_chunk_pe_sinks(
        header,
        &bam,
        ambig.as_deref(),
        unmapped_1.as_deref(),
        unmapped_2.as_deref(),
        ambiguous_1.as_deref(),
        ambiguous_2.as_deref(),
    )?;
    let mut counters = Counters::default();
    let converted = crate::process_pe_chunk(
        cfg,
        genome,
        refid,
        subset_1,
        subset_2,
        opts,
        &mut sinks,
        &mut counters,
    )?;
    sinks.finish()?;

    for ((_m, _k), cr) in &converted {
        let _ = std::fs::remove_file(&cr.path);
    }
    let _ = std::fs::remove_file(subset_1);
    let _ = std::fs::remove_file(subset_2);

    Ok(PeChunkOutcome {
        bam,
        ambig_bam: ambig,
        unmapped_1,
        unmapped_2,
        ambiguous_1,
        ambiguous_2,
        counters,
    })
}

/// Ordered BAM merge: write `parts` (in chunk order) into one BAM under the shared
/// `header`, copying each per-chunk record (skipping per-chunk headers) via a single
/// writer. Reads each part as a **raw `RecordBuf`** (`noodles_bam::io::Reader`), NOT
/// the validating `BismarkRecord` reader — the `--ambig_bam` holds bare aligner
/// records with no `XR`/`XG`/`XM` tags (the Bismark reader would reject them) and
/// nothing here needs the Bismark classification. noodles record-stream copy =
/// byte-identical *decompressed* content to single-core (PLAN §3.4.1/Q3).
fn merge_bams(final_path: &Path, header: &Header, parts: &[PathBuf]) -> Result<()> {
    let mut writer = BamWriter::from_path(final_path, header.clone()).map_err(|e| {
        AlignerError::Validation(format!("failed to open merged BAM {final_path:?}: {e}"))
    })?;
    for part in parts {
        let file = File::open(part)?;
        let mut reader = noodles_bam::io::Reader::new(BufReader::new(file));
        let part_header = reader.read_header()?;
        for rec in reader.record_bufs(&part_header) {
            let rec = rec?;
            writer.write_raw_record(&rec).map_err(|e| {
                AlignerError::Validation(format!("failed to write merged BAM: {e}"))
            })?;
        }
    }
    writer
        .finish()
        .map_err(|e| AlignerError::Validation(format!("failed to finalise merged BAM: {e}")))?;
    Ok(())
}

/// Ordered aux merge: concatenate the plain per-chunk parts (in chunk order) through
/// ONE `GzEncoder` at `Compression::default()` with no mid-stream flush → a
/// single-member gz stream raw-identical to `--parallel 1` (PLAN §3.5).
fn merge_aux_gz(final_path: &Path, plain_parts: &[PathBuf]) -> Result<()> {
    let mut enc = GzEncoder::new(
        BufWriter::new(File::create(final_path)?),
        Compression::default(),
    );
    for part in plain_parts {
        let mut f = File::open(part)?;
        io::copy(&mut f, &mut enc)?;
    }
    enc.finish()?;
    Ok(())
}

/// Sum total size (bytes) of the bisulfite index files for `basename` (its sibling
/// `<basename>.*` files in the parent dir). `None` if the dir can't be read.
fn estimate_index_bytes(basename: &Path) -> Option<u64> {
    let parent = basename.parent()?;
    let stem = basename.file_name()?.to_string_lossy().into_owned();
    let prefix = format!("{stem}.");
    let mut total = 0u64;
    for entry in std::fs::read_dir(parent).ok()? {
        let entry = entry.ok()?;
        if entry.file_name().to_string_lossy().starts_with(&prefix)
            && let Ok(meta) = entry.metadata()
        {
            total += meta.len();
        }
    }
    Some(total)
}

/// Emit the one-line memory-estimate warning (PLAN §3.7, Q5 option 2). No cap — the
/// user chose `N`; this just flags the multiplication. STDERR only (not gated).
fn emit_memory_warning(cfg: &RunConfig, n: u32) {
    let instances = match cfg.library {
        LibraryType::NonDirectional => 4u32,
        LibraryType::Directional | LibraryType::Pbat => 2u32,
    };
    let loads = instances * n;
    let detail = match estimate_index_bytes(&cfg.genome.ct_index_basename) {
        Some(bytes) if bytes > 0 => {
            let gb = (bytes as f64) * (loads as f64) / 1e9;
            format!(" (peak resident bounded by, not equal to, ~{gb:.1} GB of Bowtie 2 index)")
        }
        _ => String::new(),
    };
    eprintln!(
        "Note: --parallel {n} runs up to {loads} concurrent Bowtie 2 instances, each loading a \
         bisulfite index{detail}. Memory scales with --parallel; reduce it if memory-limited."
    );
}

/// Extract a human-readable message from a thread-panic payload (`JoinHandle::join`'s
/// `Box<dyn Any>`), so a panicking chunk worker surfaces as a clean error rather than
/// swallowing the payload. (A worker panic is a should-never-happen bug; we convert it
/// to an `Err` — a deterministic CLI exit 1 — instead of re-panicking/aborting. This is
/// a documented deviation from PLAN §3.8's "re-panics on join"; the cleaner exit was
/// preferred and the no-orphan guarantee still holds via the streams' `Drop` reaping.)
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Collect chunk results (already in chunk order) into a `Vec`, returning the
/// **lowest-chunk-index** error (deterministic; B-O3). `std::thread::scope` has
/// already joined every worker by the time this runs.
fn collect_in_order<T>(results: Vec<Result<T>>) -> Result<Vec<T>> {
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        out.push(r?);
    }
    Ok(out)
}

/// SE `--parallel N` (N > 1): per read file, split into N contiguous chunks, process
/// them concurrently under [`std::thread::scope`], and merge per-chunk BAM/aux/
/// counters in chunk order into the single final outputs.
pub(crate) fn run_se_multicore(config: &RunConfig, reads: &[String], n: u32) -> Result<()> {
    let started = std::time::Instant::now();

    // The genome/header/refid are loaded ONCE and shared read-only across workers.
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let directional = matches!(config.library, LibraryType::Directional);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let fasta = matches!(config.format, ReadFormat::FastA);
    let arity = if fasta { 2 } else { 4 };

    // skip/upto are applied at the split; clear them on a cloned config so neither
    // the per-chunk converter nor `drive_merge` re-applies them (PLAN §3.6).
    let (orig_skip, orig_upto) = (config.read_processing.skip, config.read_processing.upto);
    let mut cfg = config.clone();
    cfg.read_processing.skip = None;
    cfg.read_processing.upto = None;
    let chunk_opts = ConvertOptions::from_config(&cfg);

    emit_memory_warning(config, n);

    let (cfg_ref, genome_ref, refid_ref, header_ref, opts_ref) =
        (&cfg, &genome, &refid, &header, &chunk_opts);

    for read_file in reads {
        let subsets = split_contiguous(
            Path::new(read_file),
            &cfg.output.temp_dir,
            n,
            arity,
            orig_skip,
            orig_upto,
        )?;

        let results: Vec<Result<SeChunkOutcome>> = std::thread::scope(|s| {
            let handles: Vec<_> = subsets
                .iter()
                .map(|subset| {
                    s.spawn(move || {
                        se_chunk_job(cfg_ref, genome_ref, refid_ref, header_ref, subset, opts_ref)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|e| {
                        Err(AlignerError::Validation(format!(
                            "a Phase-9b chunk worker panicked: {}",
                            panic_message(e.as_ref())
                        )))
                    })
                })
                .collect()
        });
        let outcomes = collect_in_order(results)?;

        let tok = cfg.aligner.token();
        let bam_path =
            crate::derive_output_path(read_file, &cfg, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let bams: Vec<PathBuf> = outcomes.iter().map(|o| o.bam.clone()).collect();
        merge_bams(&bam_path, &header, &bams)?;

        if config.ambig_bam {
            let ap = crate::derive_output_path(
                read_file,
                &cfg,
                &format!("_bismark_{tok}.ambig.bam"),
                ".ambig.bam",
            );
            let parts: Vec<PathBuf> = outcomes
                .iter()
                .filter_map(|o| o.ambig_bam.clone())
                .collect();
            merge_bams(&ap, &header, &parts)?;
        }

        let filename = crate::basename(read_file);
        let (prefix, base) = (cfg.output.prefix.as_deref(), cfg.output.basename.as_deref());
        if config.unmapped {
            let name =
                aux_out::aux_filename(&filename, prefix, base, AuxKind::Unmapped, fasta, None);
            let parts: Vec<PathBuf> = outcomes.iter().filter_map(|o| o.unmapped.clone()).collect();
            merge_aux_gz(&cfg.output.output_dir.join(name), &parts)?;
        }
        if config.ambiguous {
            let name =
                aux_out::aux_filename(&filename, prefix, base, AuxKind::Ambiguous, fasta, None);
            let parts: Vec<PathBuf> = outcomes
                .iter()
                .filter_map(|o| o.ambiguous.clone())
                .collect();
            merge_aux_gz(&cfg.output.output_dir.join(name), &parts)?;
        }

        let mut total = Counters::default();
        for o in &outcomes {
            total.merge(&o.counters);
        }
        let report_path = crate::derive_output_path(
            read_file,
            &cfg,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &cfg.aligner_options,
                aligner: cfg.aligner,
                library: cfg.library,
            },
        )?;
        report::print_final_analysis_report_single_end(&mut report, &total, directional)?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        cleanup_se(&outcomes);
        eprintln!("{}", crate::counters_summary(read_file, &total));
    }
    Ok(())
}

/// Best-effort cleanup of an SE chunk's per-chunk BAM/aux temps (post-merge).
fn cleanup_se(outcomes: &[SeChunkOutcome]) {
    for o in outcomes {
        let _ = std::fs::remove_file(&o.bam);
        for p in [&o.ambig_bam, &o.unmapped, &o.ambiguous]
            .into_iter()
            .flatten()
        {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// PE `--parallel N` (N > 1).
pub(crate) fn run_pe_multicore(
    config: &RunConfig,
    mates1: &[String],
    mates2: &[String],
    n: u32,
) -> Result<()> {
    let started = std::time::Instant::now();

    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let directional = matches!(config.library, LibraryType::Directional);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let fasta = matches!(config.format, ReadFormat::FastA);
    let arity = if fasta { 2 } else { 4 };

    let (orig_skip, orig_upto) = (config.read_processing.skip, config.read_processing.upto);
    let mut cfg = config.clone();
    cfg.read_processing.skip = None;
    cfg.read_processing.upto = None;
    let chunk_opts = ConvertOptions::from_config(&cfg);

    emit_memory_warning(config, n);

    let (cfg_ref, genome_ref, refid_ref, header_ref, opts_ref) =
        (&cfg, &genome, &refid, &header, &chunk_opts);

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let (subsets_1, subsets_2) = split_contiguous_pe(
            Path::new(read_1),
            Path::new(read_2),
            &cfg.output.temp_dir,
            n,
            arity,
            orig_skip,
            orig_upto,
        )?;

        let results: Vec<Result<PeChunkOutcome>> = std::thread::scope(|s| {
            let handles: Vec<_> = subsets_1
                .iter()
                .zip(&subsets_2)
                .map(|(s1, s2)| {
                    s.spawn(move || {
                        pe_chunk_job(cfg_ref, genome_ref, refid_ref, header_ref, s1, s2, opts_ref)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|e| {
                        Err(AlignerError::Validation(format!(
                            "a Phase-9b chunk worker panicked: {}",
                            panic_message(e.as_ref())
                        )))
                    })
                })
                .collect()
        });
        let outcomes = collect_in_order(results)?;

        let tok = cfg.aligner.token();
        let bam_path =
            crate::derive_output_path(read_1, &cfg, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let bams: Vec<PathBuf> = outcomes.iter().map(|o| o.bam.clone()).collect();
        merge_bams(&bam_path, &header, &bams)?;

        if config.ambig_bam {
            let ap = crate::derive_output_path(
                read_1,
                &cfg,
                &format!("_bismark_{tok}_pe.ambig.bam"),
                "_pe.ambig.bam",
            );
            let parts: Vec<PathBuf> = outcomes
                .iter()
                .filter_map(|o| o.ambig_bam.clone())
                .collect();
            merge_bams(&ap, &header, &parts)?;
        }

        let (b1, b2) = (crate::basename(read_1), crate::basename(read_2));
        let (prefix, base) = (cfg.output.prefix.as_deref(), cfg.output.basename.as_deref());
        let merge_pe_aux =
            |kind: AuxKind, parts1: Vec<PathBuf>, parts2: Vec<PathBuf>| -> Result<()> {
                let n1 = aux_out::aux_filename(&b1, prefix, base, kind, fasta, Some(1));
                let n2 = aux_out::aux_filename(&b2, prefix, base, kind, fasta, Some(2));
                merge_aux_gz(&cfg.output.output_dir.join(n1), &parts1)?;
                merge_aux_gz(&cfg.output.output_dir.join(n2), &parts2)?;
                Ok(())
            };
        if config.unmapped {
            let p1 = outcomes
                .iter()
                .filter_map(|o| o.unmapped_1.clone())
                .collect();
            let p2 = outcomes
                .iter()
                .filter_map(|o| o.unmapped_2.clone())
                .collect();
            merge_pe_aux(AuxKind::Unmapped, p1, p2)?;
        }
        if config.ambiguous {
            let p1 = outcomes
                .iter()
                .filter_map(|o| o.ambiguous_1.clone())
                .collect();
            let p2 = outcomes
                .iter()
                .filter_map(|o| o.ambiguous_2.clone())
                .collect();
            merge_pe_aux(AuxKind::Ambiguous, p1, p2)?;
        }

        let mut total = Counters::default();
        for o in &outcomes {
            total.merge(&o.counters);
        }
        let report_path = crate::derive_output_path(
            read_1,
            &cfg,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &cfg.aligner_options,
                aligner: cfg.aligner,
                library: cfg.library,
            },
        )?;
        report::print_final_analysis_report_paired_ends(&mut report, &total, directional)?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        cleanup_pe(&outcomes);
        eprintln!("{}", crate::counters_summary_pe(read_1, read_2, &total));
    }
    Ok(())
}

/// Best-effort cleanup of a PE chunk's per-chunk BAM/aux temps (post-merge).
fn cleanup_pe(outcomes: &[PeChunkOutcome]) {
    for o in outcomes {
        let _ = std::fs::remove_file(&o.bam);
        for p in [
            &o.ambig_bam,
            &o.unmapped_1,
            &o.unmapped_2,
            &o.ambiguous_1,
            &o.ambiguous_2,
        ]
        .into_iter()
        .flatten()
        {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    fn write(p: &Path, bytes: &[u8]) {
        std::fs::write(p, bytes).unwrap();
    }

    /// A 4-line FastQ record for read id `n` (distinct seq per id).
    fn fq_record(n: usize) -> Vec<u8> {
        format!("@r{n}\nACGTAC\n+\nIIIIII\n").into_bytes()
    }

    fn fq_file(dir: &Path, name: &str, n: usize) -> PathBuf {
        let p = dir.join(name);
        let mut data = Vec::new();
        for i in 1..=n {
            data.extend_from_slice(&fq_record(i));
        }
        write(&p, &data);
        p
    }

    fn read_all(p: &Path) -> Vec<u8> {
        let mut s = Vec::new();
        File::open(p).unwrap().read_to_end(&mut s).unwrap();
        s
    }

    #[test]
    fn quotas_balanced_contiguous() {
        assert_eq!(quotas(10, 4), vec![3, 3, 2, 2]); // 10 = 3+3+2+2
        assert_eq!(quotas(8, 4), vec![2, 2, 2, 2]);
        assert_eq!(quotas(3, 4), vec![1, 1, 1, 0]); // eff < n → trailing empty
        assert_eq!(quotas(0, 3), vec![0, 0, 0]);
    }

    #[test]
    fn split_concatenation_equals_input_no_skip() {
        let dir = TempDir::new().unwrap();
        let input = fq_file(dir.path(), "reads.fq", 13); // coprime-ish to {2,4,8}
        let td = dir.path();
        let parts = split_contiguous(&input, td, 4, 4, None, None).unwrap();
        assert_eq!(parts.len(), 4);
        // Concatenating the 4 subsets in chunk order reproduces the input bytes.
        let mut cat = Vec::new();
        for p in &parts {
            cat.extend_from_slice(&read_all(p));
        }
        assert_eq!(cat, read_all(&input));
        // 13 over 4 → 4,3,3,3 records.
        let recs = |p: &Path| {
            read_all(p)
                .split(|&b| b == b'\n')
                .filter(|l| l.starts_with(b"@r"))
                .count()
        };
        assert_eq!(recs(&parts[0]), 4);
        assert_eq!(recs(&parts[1]), 3);
        assert_eq!(recs(&parts[2]), 3);
        assert_eq!(recs(&parts[3]), 3);
    }

    #[test]
    fn split_skip_and_upto_both_set_straddles_boundary() {
        // 10 reads, --skip 2 --upto 8 → effective set = reads 3..=8 (6 reads).
        // Over n=4 → quotas 2,2,1,1, straddling chunk boundaries.
        let dir = TempDir::new().unwrap();
        let input = fq_file(dir.path(), "reads.fq", 10);
        let parts = split_contiguous(&input, dir.path(), 4, 4, Some(2), Some(8)).unwrap();
        // Concatenated subsets == exactly reads 3..=8 (the converter's (skip,upto] window).
        let mut expected = Vec::new();
        for i in 3..=8 {
            expected.extend_from_slice(&fq_record(i));
        }
        let mut cat = Vec::new();
        for p in &parts {
            cat.extend_from_slice(&read_all(p));
        }
        assert_eq!(cat, expected, "effective window must be reads 3..=8");
        assert_eq!(count_effective(&input, 4, Some(2), Some(8)).unwrap(), 6);
    }

    #[test]
    fn split_empty_and_eff_lt_n_make_trailing_empty_chunks() {
        let dir = TempDir::new().unwrap();
        // 2 reads over 4 chunks → chunks 0,1 have 1 read each; 2,3 empty.
        let input = fq_file(dir.path(), "reads.fq", 2);
        let parts = split_contiguous(&input, dir.path(), 4, 4, None, None).unwrap();
        assert!(!read_all(&parts[0]).is_empty());
        assert!(!read_all(&parts[1]).is_empty());
        assert!(read_all(&parts[2]).is_empty());
        assert!(read_all(&parts[3]).is_empty());
        // empty input → all chunks empty.
        let empty = dir.path().join("empty.fq");
        write(&empty, b"");
        let ep = split_contiguous(&empty, dir.path(), 3, 4, None, None).unwrap();
        assert_eq!(ep.len(), 3);
        assert!(ep.iter().all(|p| read_all(p).is_empty()));
    }

    #[test]
    fn pe_split_partitions_common_min_count_in_lockstep() {
        // R1 has 7 records, R2 has 5 → common = 5 pairs (drive_merge_pe truncates).
        let dir = TempDir::new().unwrap();
        let r1 = fq_file(dir.path(), "r1.fq", 7);
        let r2 = fq_file(dir.path(), "r2.fq", 5);
        assert_eq!(count_effective_pe(&r1, &r2, 4, None, None).unwrap(), 5);
        let (p1, p2) = split_contiguous_pe(&r1, &r2, dir.path(), 2, 4, None, None).unwrap();
        // Each chunk's R1 record count == its R2 record count (lockstep).
        let recs = |p: &Path| {
            read_all(p)
                .split(|&b| b == b'\n')
                .filter(|l| l.starts_with(b"@r"))
                .count()
        };
        assert_eq!(recs(&p1[0]), recs(&p2[0]));
        assert_eq!(recs(&p1[1]), recs(&p2[1]));
        // 5 pairs over 2 chunks → 3 + 2.
        assert_eq!(recs(&p1[0]) + recs(&p1[1]), 5);
    }

    #[test]
    fn counters_merge_is_field_wise_sum() {
        let mut a = Counters {
            sequences_count: 3,
            unique_best_alignment_count: 2,
            total_me_cpg: 5,
            ..Default::default()
        };
        let b = Counters {
            sequences_count: 4,
            unique_best_alignment_count: 1,
            total_me_cpg: 7,
            ga_ct_ct_count: 9,
            ..Default::default()
        };
        a.merge(&b);
        assert_eq!(a.sequences_count, 7);
        assert_eq!(a.unique_best_alignment_count, 3);
        assert_eq!(a.total_me_cpg, 12);
        assert_eq!(a.ga_ct_ct_count, 9);
    }

    #[test]
    fn merge_aux_gz_decompresses_to_concatenation() {
        use flate2::read::GzDecoder;
        let dir = TempDir::new().unwrap();
        let a = dir.path().join("a.tmp");
        let b = dir.path().join("b.tmp");
        write(&a, b"@r1\nACGT\n+\nIIII\n");
        write(&b, b"@r2\nTTTT\n+\nIIII\n");
        let out = dir.path().join("out.gz");
        merge_aux_gz(&out, &[a, b]).unwrap();
        let mut s = String::new();
        GzDecoder::new(File::open(&out).unwrap())
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "@r1\nACGT\n+\nIIII\n@r2\nTTTT\n+\nIIII\n");
    }
}
