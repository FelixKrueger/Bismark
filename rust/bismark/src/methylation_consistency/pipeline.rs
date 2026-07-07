//! End-to-end orchestration: read a Bismark BAM, classify each read (or PE
//! pair), route to one of three BAMs, and write the consistency report.
//!
//! Mirrors Perl `methylation_consistency`'s `process_file` (lines 138–352).
//! Per-file flow:
//!
//! 1. Open the BAM **no-sort-check** (`BamReader::without_sort_check`) — SE is
//!    never sort-checked in Perl, so the reader must not reject coordinate
//!    sort itself; we apply the guard for PE only (SPEC §4.11).
//! 2. **Empty check** (`bam_isEmpty`): zero records → skip the file, no
//!    outputs.
//! 3. Resolve SE/PE (`-s`/`-p`, else `detect_paired_from_header`; a missing
//!    Bismark `@PG` falls through to **single-end**).
//! 4. PE only: reject `@HD SO:coordinate` (the *correct* guard Perl intended;
//!    its `/^\@SO/` check is dead code — SPEC §4.6).
//! 5. **Eager-open all three** `BamWriter`s with the verbatim input header, so
//!    empty buckets become valid empty BAMs (SPEC §5.2).
//! 6. Stream records: `count_xm` → `classify` → route / discard / skip.
//!    A **missing-XM** record is a graceful STOP (Perl `last`): finalize the
//!    partial output + report, exit 0. Other reader errors are fatal.
//! 7. Finalize every writer (BGZF EOF) on all paths; write the report; echo
//!    the summary to STDERR (unless `--quiet`).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::iter::Peekable;
use std::path::Path;

use crate::io::{BamReader, BamWriter, BismarkIoError, BismarkRecord};
use noodles_sam::Header;
use noodles_sam::header::record::value::map::header::sort_order::COORDINATE;
use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;

use crate::methylation_consistency::classify::{Bucket, Routing, classify, count_xm};
use crate::methylation_consistency::cli::{LibraryMode, ResolvedConfig};
use crate::methylation_consistency::error::MethConsError;
use crate::methylation_consistency::filename;
use crate::methylation_consistency::logging::Logger;
use crate::methylation_consistency::report::Tally;

/// Run the tool over every input file. Each file is processed independently
/// (its own three BAMs + report). The startup banners (thresholds, CHH
/// warning) are emitted once, before the per-file loop.
pub fn run(config: &ResolvedConfig) -> Result<(), MethConsError> {
    let logger = Logger::new(config.quiet);
    logger.thresholds(config.lower, config.upper);
    if config.chh {
        logger.chh_experimental();
    }
    for file in &config.files {
        process_file(file, config, &logger)?;
    }
    Ok(())
}

/// The three per-bucket output BAM writers, opened eagerly so empty buckets
/// still produce a valid (header + BGZF EOF) BAM.
struct BucketWriters {
    all_meth: BamWriter<BufWriter<File>>,
    all_unmeth: BamWriter<BufWriter<File>>,
    mixed: BamWriter<BufWriter<File>>,
}

impl BucketWriters {
    /// Open all three bucket BAMs adjacent to the input (via `root`), each
    /// with a clone of the verbatim input header.
    fn open(root: &str, chh: bool, header: &Header) -> Result<Self, MethConsError> {
        Ok(Self {
            all_meth: BamWriter::from_path(
                &filename::bucket_path(root, chh, Bucket::AllMeth),
                header.clone(),
            )?,
            all_unmeth: BamWriter::from_path(
                &filename::bucket_path(root, chh, Bucket::AllUnmeth),
                header.clone(),
            )?,
            mixed: BamWriter::from_path(
                &filename::bucket_path(root, chh, Bucket::Mixed),
                header.clone(),
            )?,
        })
    }

    /// Write one record to the writer for `bucket`.
    fn write(&mut self, bucket: Bucket, rec: &BismarkRecord) -> Result<(), MethConsError> {
        match bucket {
            Bucket::AllMeth => self.all_meth.write_record(rec)?,
            Bucket::AllUnmeth => self.all_unmeth.write_record(rec)?,
            Bucket::Mixed => self.mixed.write_record(rec)?,
        }
        Ok(())
    }

    /// Finalize all three writers (BGZF EOF). Consumes `self`.
    ///
    /// Attempts **all three** `finish()` calls before returning, so a failure
    /// on the first writer does not leave the other two un-finalized (the
    /// finalize-on-all-paths contract; both code reviewers, 2026-05-29).
    /// Returns the first error encountered, in writer order.
    fn finish(self) -> Result<(), MethConsError> {
        let r1 = self.all_meth.finish();
        let r2 = self.all_unmeth.finish();
        let r3 = self.mixed.finish();
        r1.and(r2).and(r3).map_err(MethConsError::from)
    }
}

/// Process a single input file.
fn process_file(
    path: &Path,
    config: &ResolvedConfig,
    logger: &Logger,
) -> Result<(), MethConsError> {
    let file_label = path.to_string_lossy().into_owned();
    logger.processing_file(&file_label);

    // v1.0 is BAM-only (Perl is BAM-only in practice). Open no-sort-check so
    // SE input is accepted regardless of sort order; PE applies its own guard.
    let mut reader = BamReader::without_sort_check(BufReader::new(File::open(path)?))?;
    let header = reader.header().clone();
    let mut records = reader.records().peekable();

    // Empty check (Perl `bam_isEmpty`): skip the file before any output.
    if records.peek().is_none() {
        logger.skipping_empty();
        return Ok(());
    }

    let is_paired = resolve_mode(config.mode, &header, logger);
    if is_paired && is_coordinate_sorted(&header) {
        return Err(MethConsError::CoordinateSorted {
            input: path.to_path_buf(),
        });
    }

    let root = filename::output_root(path);
    let mut writers = BucketWriters::open(&root, config.chh, &header)?;
    let mut tally = Tally::default();

    let stream_result = if is_paired {
        stream_pe(&mut records, &mut writers, &mut tally, config)
    } else {
        stream_se(&mut records, &mut writers, &mut tally, config)
    };

    match stream_result {
        Ok(()) => {
            // Finalize, then write the report + echo the summary.
            writers.finish()?;
            let body = tally.render(
                is_paired,
                config.lower,
                config.upper,
                config.min_count,
                config.chh,
            );
            std::fs::write(filename::report_path(&root, config.chh), &body)?;
            logger.summary(&file_label, &body);
            Ok(())
        }
        Err(e) => {
            // Best-effort finalize so we never leave an EOF-less/undecodable
            // BAM, then propagate the fatal error (no report on fatal paths).
            let _ = writers.finish();
            Err(e)
        }
    }
}

/// Single-end stream: one record per read.
fn stream_se<I>(
    records: &mut Peekable<I>,
    writers: &mut BucketWriters,
    tally: &mut Tally,
    config: &ResolvedConfig,
) -> Result<(), MethConsError>
where
    I: Iterator<Item = Result<BismarkRecord, BismarkIoError>>,
{
    for item in records.by_ref() {
        let rec = match item {
            Ok(r) => r,
            // Missing XM ⇒ graceful stop (Perl `last`): keep the tally so far.
            Err(e) if is_missing_xm(&e) => break,
            Err(e) => return Err(e.into()),
        };
        let counts = count_xm(rec.xm(), config.chh);
        route(counts, &rec, &rec, false, writers, tally, config)?;
    }
    Ok(())
}

/// Paired-end stream: two adjacent records per pair, counts summed.
fn stream_pe<I>(
    records: &mut Peekable<I>,
    writers: &mut BucketWriters,
    tally: &mut Tally,
    config: &ResolvedConfig,
) -> Result<(), MethConsError>
where
    I: Iterator<Item = Result<BismarkRecord, BismarkIoError>>,
{
    loop {
        // R1
        let r1 = match records.next() {
            None => break, // clean end of file
            Some(Ok(r)) => r,
            Some(Err(e)) if is_missing_xm(&e) => break, // graceful stop
            Some(Err(e)) => return Err(e.into()),
        };
        // R2 (the immediately following record)
        let r2 = match records.next() {
            None => break, // odd trailing R1 → dropped, uncounted (Perl `last`)
            Some(Ok(r)) => r,
            Some(Err(e)) if is_missing_xm(&e) => break, // R1's pair discarded
            Some(Err(e)) => return Err(e.into()),
        };

        // Exact qname-equality check (no /1,/2 stripping — SPEC §4.6 / B3).
        if r1.inner().name() != r2.inner().name() {
            return Err(MethConsError::MateMismatch {
                id1: name_string(&r1),
                id2: name_string(&r2),
            });
        }

        let counts = count_xm(r1.xm(), config.chh) + count_xm(r2.xm(), config.chh);
        route(counts, &r1, &r2, true, writers, tally, config)?;
    }
    Ok(())
}

/// Classify `counts` and route the read(s) to a bucket (writing `rec1`, and
/// `rec2` too when `paired`), or discard/skip. Increments the tally.
fn route(
    counts: crate::methylation_consistency::classify::Counts,
    rec1: &BismarkRecord,
    rec2: &BismarkRecord,
    paired: bool,
    writers: &mut BucketWriters,
    tally: &mut Tally,
    config: &ResolvedConfig,
) -> Result<(), MethConsError> {
    match classify(counts, config.min_count, config.lower, config.upper) {
        Routing::Discard => tally.discarded += 1,
        Routing::Skip => {}
        Routing::Route(bucket) => {
            tally.record(bucket);
            writers.write(bucket, rec1)?;
            if paired {
                writers.write(bucket, rec2)?;
            }
        }
    }
    Ok(())
}

/// Resolve the SE/PE mode. Auto-detect via the Bismark `@PG` line; a missing
/// Bismark `@PG` (`None`) falls through to **single-end** (Perl, SPEC §2.3).
fn resolve_mode(mode: LibraryMode, header: &Header, logger: &Logger) -> bool {
    match mode {
        LibraryMode::Single => false,
        LibraryMode::Paired => true,
        LibraryMode::Auto => {
            let is_paired = crate::io::detect_paired_from_header(header).unwrap_or(false);
            logger.info(&format!(
                "{} mode selected (auto-detected)\n",
                if is_paired {
                    "Paired-end (PE)"
                } else {
                    "Single-end (SE)"
                }
            ));
            is_paired
        }
    }
}

/// True if the header declares coordinate sort (`@HD SO:coordinate`). Mirrors
/// `bismark_io`'s private `check_not_coordinate_sorted`.
fn is_coordinate_sorted(header: &Header) -> bool {
    if let Some(hd) = header.header()
        && let Some(so) = hd.other_fields().get(&SORT_ORDER)
    {
        let so_bytes: &[u8] = AsRef::as_ref(so);
        return so_bytes == COORDINATE;
    }
    false
}

/// True if `e` is the reader's missing-`XM` error (the graceful-stop trigger).
fn is_missing_xm(e: &BismarkIoError) -> bool {
    matches!(e, BismarkIoError::MissingTag { tag } if *tag == "XM")
}

/// A record's qname as a (lossy) `String` for error messages.
fn name_string(rec: &BismarkRecord) -> String {
    rec.inner()
        .name()
        .map(|n| String::from_utf8_lossy(AsRef::<[u8]>::as_ref(n)).into_owned())
        .unwrap_or_default()
}
