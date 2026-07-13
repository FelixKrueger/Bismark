//! In-process rammap [`SamStream`] — the load-bearing seam for byte-identical
//! `bismark --rammap` WITHOUT spawning the `rammap` subprocess (epic
//! `06152026_rammap-library-integration`, Phase 1).
//!
//! For each converted FastQ read, [`InProcessAlignerStream`] calls
//! `rammap::Aligner::map_seq_with` and yields a [`SamRecord`] **field-identical
//! to what the subprocess [`AlignerStream`](crate::aligner::align::AlignerStream) parses
//! from the CLI's SAM** — so the existing (byte-frozen) merge → XM → output
//! produces identical results. The stream is a third `SamStream` impl; the merge
//! cannot tell it apart from the subprocess one.
//!
//! ## Feature gate (Rev 2.1)
//!
//! `rammap-core` is an OPTIONAL dependency behind the **default-OFF
//! `rammap-inprocess`** Cargo feature. Everything that names a `rammap::*` type
//! (the [`InProcessAlignerStream`], [`build_sam_record`], the rammap-typed tests)
//! is `#[cfg(feature = "rammap-inprocess")]`; the pure-Rust CIGAR-reconstruction
//! helpers ([`reconstruct_cigar`] / [`consumed_read_len`]) compile on ANY
//! toolchain (so the feature-independent CIGAR unit test always runs). Phase 1
//! adds the module only — `lib.rs`/`config.rs` routing is Phase 2.
//!
//! ## The CIGAR is THE load-bearing field (Rev 2)
//!
//! The Phase-0 spike found the rammap library reproduces the subprocess CLI's
//! per-read primary alignment 100% on rname/POS/strand/AS/MD — the ONLY gap is
//! the CIGAR **soft-clip flanks**: the library returns the aligned-CORE CIGAR
//! (`46M3I7M`), while the SAM convention wraps it with `{query_start}S` +
//! `{read_len − query_end}S` (`297S46M3I7M1379S`). [`reconstruct_cigar`] rebuilds
//! that. This is load-bearing because `methylation::
//! extract_corresponding_genomic_sequence_single_end` fully parses the CIGAR and
//! a wrong consumed-length **silently skips the read** (`lib.rs` length guard) —
//! no wrong base, a *missing record*. So the reconstruction MUST make
//! `consumed_read_len(cigar) == read_len` exactly.

// `SamStream`/`SamRecord` are used only by the feature-gated stream + builder; the
// pure CIGAR helpers don't need them, so the imports are feature-gated to keep the
// default (feature-OFF) build warning-clean under `-D warnings`.
#[cfg(feature = "rammap-inprocess")]
use crate::aligner::align::{SamRecord, SamStream};
#[cfg(feature = "rammap-inprocess")]
use crate::aligner::error::{AlignerError, Result};
#[cfg(feature = "rammap-inprocess")]
use std::io::BufRead;
#[cfg(feature = "rammap-inprocess")]
use std::sync::Arc;
// #995: parallel per-read mapping on a shared pool. `par_iter` needs the prelude.
#[cfg(feature = "rammap-inprocess")]
use rayon::prelude::*;

// ===========================================================================
// Pure CIGAR helpers (feature-INDEPENDENT — compile + tested on any toolchain).
// ===========================================================================

/// Reconstruct the SAM CIGAR from the rammap library's aligned-core CIGAR by
/// re-adding the query soft-clip flanks (the Phase-0 finding), **strand-aware**.
///
/// rammap's `Mapping.query_start`/`query_end` are in the read's ORIGINAL FORWARD
/// orientation (the minimap2/PAF convention). For a SAM line the CIGAR is written
/// in REFERENCE orientation, so on the reverse strand the SAM SEQ is the
/// reverse-complement and the soft-clip flanks SWAP:
/// - **forward:** `soft(query_start) + core + soft(read_len − query_end)`
/// - **reverse:** `soft(read_len − query_end) + core + soft(query_start)`
///
/// `soft(n) = "{n}S"` if `n > 0` else `""` (a zero-length soft clip is omitted,
/// matching the CLI's SAM, which never emits `0S`). The aligned CORE is identical
/// for both strands (the rammap library already returns it in reference
/// orientation, exactly as the CLI's SAM).
///
/// > 🔴 The strand swap was MISSED in the original Phase-0/Rev-2 reconstruction
/// > (the spike's 5k sample showed only the flank *lengths*, not their order vs
/// > strand) and CAUGHT by the Phase-1 record-level cross-check: every reverse
/// > (FLAG 0x10) read had the leading/trailing soft clips swapped. A swapped flank
/// > keeps `consumed_read_len == read_len` (so the methylation guard would NOT skip
/// > it) but mis-places the soft clip → a WRONG genomic window in
/// > `extract_corresponding_genomic_sequence_single_end` → a silent miscall. The
/// > cross-check is exactly the gate that surfaced it.
///
/// # Guards (Rev 2)
///
/// - `debug_assert!` the `core` does NOT already contain `S` — the library
///   returns the aligned core only; if it ever included a soft clip we would
///   DOUBLE-clip (wrong length → silent skip). Caught in debug builds; release
///   builds rely on the cross-check's `consumed_read_len == read_len` assertion.
/// - `query_end <= read_len` (else a soft clip underflows). A violation is a logic
///   bug in the caller / library contract.
///
/// NB: this does NOT itself assert `consumed_read_len == read_len` — that is the
/// cross-check's job (it needs the parsed runs), and a *core* CIGAR with an
/// internal `D` legitimately consumes fewer query bases than `target` span. The
/// soft-clip arithmetic here only restores the QUERY flanks.
pub fn reconstruct_cigar(
    query_start: usize,
    query_end: usize,
    read_len: usize,
    reverse: bool,
    core: &str,
) -> String {
    debug_assert!(
        !core.contains('S'),
        "rammap core CIGAR already contains a soft clip ('S'); soft-clip reconstruction would \
         double-clip and break the read-length identity: core={core:?}"
    );
    debug_assert!(
        query_end <= read_len,
        "query_end ({query_end}) > read_len ({read_len}); soft clip would underflow"
    );
    let fwd_lead = query_start;
    let fwd_trail = read_len.saturating_sub(query_end);
    // Reverse strand → the SAM CIGAR is in reference orientation, so the flanks swap.
    let (leading, trailing) = if reverse {
        (fwd_trail, fwd_lead)
    } else {
        (fwd_lead, fwd_trail)
    };
    let mut out = String::new();
    if leading > 0 {
        out.push_str(&format!("{leading}S"));
    }
    out.push_str(core);
    if trailing > 0 {
        out.push_str(&format!("{trailing}S"));
    }
    out
}

/// The number of QUERY bases a CIGAR consumes — `M`, `I`, `S`, `=`, `X` advance
/// the query; `D`, `N`, `H`, `P` do not. Used by the cross-check to assert the
/// reconstructed CIGAR consumes exactly `read_len` (else the methylation length
/// guard silently skips the read). Returns `None` on a malformed CIGAR (the same
/// digit/op parse as `methylation::parse_cigar`, kept local + total here so the
/// helper is feature-independent and never panics on input from a real run).
pub fn consumed_read_len(cigar: &str) -> Option<usize> {
    if cigar == "*" {
        return Some(0);
    }
    let bytes = cigar.as_bytes();
    let mut i = 0;
    let mut total: usize = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start || i == bytes.len() {
            return None; // missing length or trailing length with no op
        }
        let len: usize = cigar[start..i].parse().ok()?;
        let op = bytes[i];
        i += 1;
        match op {
            // query-consuming ops
            b'M' | b'I' | b'S' | b'=' | b'X' => total += len,
            // reference-only / no-query ops
            b'D' | b'N' | b'H' | b'P' => {}
            _ => return None,
        }
    }
    Some(total)
}

// ===========================================================================
// SamRecord builder (feature-gated — names `rammap::Mapping`).
// ===========================================================================

/// Build a [`SamRecord`] from a converted read + its rammap primary [`Mapping`]
/// (or `None` for unmapped), matching what the subprocess CLI SAM parses on every
/// merge-consumed field.
///
/// - **Mapped:** `flag` = `0x10` if `Strand::Reverse` else `0`; `rname` =
///   `m.target_name`; `pos` = `m.target_start + 1` (0-based → 1-based); `cigar` =
///   [`reconstruct_cigar`] over `m.cigar`; `mapq` = `m.mapq`; `alignment_score` =
///   `Some(m.score)`; `second_best` = `None` (rammap's `s2:i:` chaining 2nd-best is
///   IGNORED, exactly as the subprocess parse drops it); `md_tag` = `m.md`.
/// - **`seq`/`qual`** (Q1, Rev 2 — DOWNSTREAM-DEAD: SE output re-reads the original
///   `seq_uc`, never `bowtie_sequence`): the input read fwd for `+`, the nucleotide
///   reverse-complement for `-` (the CLI SAM convention), so it is faithful but NOT
///   gate-critical. `qual` likewise (reversed for `-`).
/// - **`raw_line`:** a reconstructed 11-column SAM line that round-trips through
///   `SamRecord::parse` (Q2 — only consumed by `--ambig_bam`/combined-index, neither
///   reached by rammap; the round-trip is a safeguard).
/// - **Unmapped** (`None`): `flag = 0x4`, `rname = "*"`, `pos = 0`, `cigar = "*"`,
///   `mapq = 0`, scores/`md` `None` — the shape `SamRecord::parse` yields for an
///   unmapped CLI line.
#[cfg(feature = "rammap-inprocess")]
pub fn build_sam_record(
    qname: &str,
    read: &[u8],
    qual: &[u8],
    m: Option<&rammap::Mapping>,
) -> SamRecord {
    match m {
        None => {
            // Unmapped: SEQ/QUAL still carry the forward read (as the CLI emits for
            // a FLAG-4 line); the merge ignores them (only the FLAG matters).
            let seq = String::from_utf8_lossy(read).into_owned();
            let qual_s = String::from_utf8_lossy(qual).into_owned();
            let raw_line = format!(
                "{qname}\t4\t*\t0\t0\t*\t*\t0\t0\t{seq}\t{qual_s}",
                seq = if seq.is_empty() {
                    "*".to_string()
                } else {
                    seq.clone()
                },
                qual_s = if qual_s.is_empty() {
                    "*".to_string()
                } else {
                    qual_s.clone()
                },
            );
            SamRecord {
                qname: qname.to_string(),
                flag: 4,
                rname: "*".to_string(),
                pos: 0,
                mapq: 0,
                cigar: "*".to_string(),
                seq: if seq.is_empty() { "*".to_string() } else { seq },
                qual: if qual_s.is_empty() {
                    "*".to_string()
                } else {
                    qual_s
                },
                alignment_score: None,
                second_best: None,
                md_tag: None,
                raw_line,
            }
        }
        Some(m) => {
            let reverse = m.strand == rammap::Strand::Reverse;
            let flag: u16 = if reverse { 0x10 } else { 0 };
            // 0-based target_start → 1-based SAM POS.
            let pos = (m.target_start as u32) + 1;
            // The library returns the aligned-core CIGAR; re-add the soft-clip
            // flanks (strand-aware — reverse swaps the flanks).
            let core = m.cigar.as_deref().unwrap_or_default();
            let cigar = reconstruct_cigar(m.query_start, m.query_end, read.len(), reverse, core);
            // SEQ/QUAL: the CLI emits the reverse-complement (and reversed qual) for a
            // reverse-strand alignment. Downstream-dead, emitted for fidelity only.
            let (seq_bytes, qual_bytes) = if reverse {
                (crate::aligner::output::revcomp(read), {
                    let mut q = qual.to_vec();
                    q.reverse();
                    q
                })
            } else {
                (read.to_vec(), qual.to_vec())
            };
            let seq = String::from_utf8_lossy(&seq_bytes).into_owned();
            let qual_s = String::from_utf8_lossy(&qual_bytes).into_owned();
            let mapq: u8 = m.mapq.clamp(0, u8::MAX as i32) as u8;
            let alignment_score = Some(m.score as i64);
            let md_tag = m.md.clone();

            // raw_line: an 11+-column SAM line that round-trips through
            // SamRecord::parse (the merge re-derives every field; raw_line itself is
            // only consumed by --ambig_bam/combined-index, neither reached by rammap).
            let as_value = m.score as i64;
            let mut raw_line = format!(
                "{qname}\t{flag}\t{rname}\t{pos}\t{mapq}\t{cigar}\t*\t0\t0\t{seq}\t{qual_s}\tAS:i:{as_value}",
                rname = m.target_name,
            );
            if let Some(md) = md_tag.as_deref() {
                raw_line.push_str(&format!("\tMD:Z:{md}"));
            }

            SamRecord {
                qname: qname.to_string(),
                flag,
                rname: m.target_name.to_string(),
                pos,
                mapq,
                cigar,
                seq,
                qual: qual_s,
                alignment_score,
                second_best: None,
                md_tag,
                raw_line,
            }
        }
    }
}

// ===========================================================================
// InProcessAlignerStream (feature-gated — holds an `Arc<rammap::Aligner>`).
// ===========================================================================

/// Reads mapped per parallel refill (#995). Bounds the in-memory buffer to
/// `CHUNK × (2/4 instances) × SamRecord` (~hundreds of MB at 2048 for long-ONT
/// records, where `raw_line` dominates) — well under the ~31 GB index footprint.
#[cfg(feature = "rammap-inprocess")]
const CHUNK: usize = 2048;

/// A converted-read source presented as a [`SamStream`], backed by an in-process
/// `rammap::Aligner` (NO subprocess). **#995: maps a CHUNK of reads in PARALLEL per
/// refill on a SHARED [`rayon::ThreadPool`]** (sized by `--multicore`), buffering the
/// results in INPUT ORDER and serving them one-per-read through `current`/`advance` —
/// so the byte-frozen lockstep merge is unchanged. `--multicore 1` (a 1-thread pool)
/// is byte-identical to the former one-at-a-time path: per-read `map_seq` is
/// deterministic + the parallel `collect` preserves input order. Emits exactly ONE
/// primary record per read in input order (the merge's lockstep join requires it).
///
/// The pool is SHARED across the 2/4 streams (NOT one per stream): the lockstep merge
/// drains+refills one stream at a time, so refills never overlap — a shared pool of N
/// gives the same throughput as `instances × N` threads with none of the waste.
///
/// `reads` is the SAME converted FastQ bytes the subprocess CLI reads.
#[cfg(feature = "rammap-inprocess")]
pub struct InProcessAlignerStream<R: BufRead> {
    aligner: Arc<rammap::Aligner>,
    reads: R,
    pool: Arc<rayon::ThreadPool>,
    buf: Vec<SamRecord>, // current chunk, INPUT ORDER
    pos: usize,          // cursor into `buf`
    // scratch line buffers reused while reading a chunk (4-line FastQ).
    id: Vec<u8>,
    seq: Vec<u8>,
    id2: Vec<u8>,
    qual: Vec<u8>,
}

/// Map one converted read + build its `SamRecord` — the per-read work run in parallel.
#[cfg(feature = "rammap-inprocess")]
fn map_and_build(aligner: &rammap::Aligner, qname: &str, read: &[u8], qual: &[u8]) -> SamRecord {
    let result = aligner.map_seq_with(
        qname,
        read,
        rammap::MapOpts {
            cs: None,
            md: Some(true),
        },
    );
    // Primary = the one non-supplementary primary mapping (supplementary-only → unmapped).
    let primary = result
        .mappings
        .iter()
        .find(|m| m.is_primary && !m.is_supplementary);
    build_sam_record(qname, read, qual, primary)
}

#[cfg(feature = "rammap-inprocess")]
impl<R: BufRead> InProcessAlignerStream<R> {
    /// Build the stream (sharing `pool`) and map the first chunk (`buf` empty for an
    /// empty source). `Arc<Aligner>` + `Arc<ThreadPool>` are shared across instances.
    pub fn new(
        aligner: Arc<rammap::Aligner>,
        reads: R,
        pool: Arc<rayon::ThreadPool>,
    ) -> Result<Self> {
        let mut s = InProcessAlignerStream {
            aligner,
            reads,
            pool,
            buf: Vec::new(),
            pos: 0,
            id: Vec::new(),
            seq: Vec::new(),
            id2: Vec::new(),
            qual: Vec::new(),
        };
        s.refill()?;
        Ok(s)
    }

    /// Read up to `CHUNK` converted FastQ records (4 lines each; a truncated final
    /// record is dropped, matching `convert.rs`), map them in PARALLEL on the shared
    /// pool, and replace `buf` (input order) with the results; `pos = 0`. An empty
    /// `buf` after refill ⇒ EOF.
    fn refill(&mut self) -> Result<()> {
        // Read the chunk serially into owned (qname, read, qual) tuples (cheap I/O;
        // the cost is `map_seq`). Owned bytes so the parallel closure is `Send`.
        let mut chunk: Vec<(String, Vec<u8>, Vec<u8>)> = Vec::with_capacity(CHUNK);
        while chunk.len() < CHUNK {
            self.id.clear();
            self.seq.clear();
            self.id2.clear();
            self.qual.clear();
            let n1 = self.reads.read_until(b'\n', &mut self.id)?;
            let n2 = self.reads.read_until(b'\n', &mut self.seq)?;
            let n3 = self.reads.read_until(b'\n', &mut self.id2)?;
            let n4 = self.reads.read_until(b'\n', &mut self.qual)?;
            // any missing line ends the stream; a truncated final record is dropped.
            if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
                break;
            }
            // QNAME = the `@id` line minus the leading `@` + trailing newline.
            let id_line = chomp(&self.id);
            let qname_bytes = id_line.strip_prefix(b"@").unwrap_or(id_line);
            let qname = std::str::from_utf8(qname_bytes)
                .map_err(|e| {
                    AlignerError::Validation(format!("converted read ID is not valid UTF-8: {e}"))
                })?
                .to_string();
            chunk.push((qname, chomp(&self.seq).to_vec(), chomp(&self.qual).to_vec()));
        }
        // Parallel map on the SHARED pool; `collect` into a `Vec` preserves input order
        // (the lockstep merge contract). A 1-thread pool runs it serially → byte-identical
        // to the former one-at-a-time path.
        let aligner = &self.aligner;
        let buf: Vec<SamRecord> = self.pool.install(|| {
            chunk
                .par_iter()
                .map(|(q, r, ql)| map_and_build(aligner, q, r, ql))
                .collect()
        });
        self.buf = buf;
        self.pos = 0;
        Ok(())
    }
}

#[cfg(feature = "rammap-inprocess")]
impl<R: BufRead> SamStream for InProcessAlignerStream<R> {
    fn current(&self) -> Option<&SamRecord> {
        self.buf.get(self.pos)
    }
    fn advance(&mut self) -> Result<()> {
        self.pos += 1;
        if self.pos >= self.buf.len() {
            // Chunk drained → map the next chunk (resets `pos = 0`); an empty `buf`
            // after refill leaves `current() == None` (EOF).
            self.refill()?;
        }
        Ok(())
    }
}

/// Strip a single trailing `\n` (and a preceding `\r`) from a line — the
/// converted FastQ uses `\n` terminators (cf. `convert::chomp_newline`).
#[cfg(feature = "rammap-inprocess")]
fn chomp(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    if end > 0 && line[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && line[end - 1] == b'\r' {
            end -= 1;
        }
    }
    &line[..end]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Feature-INDEPENDENT: CIGAR reconstruction (runs on any toolchain) ----

    /// The Phase-0 spike example (FORWARD): core `46M3I7M`, query_start 297,
    /// query_end 353, read_len 1732 → `297S46M3I7M1379S` (trailing 1732 − 353 = 1379).
    #[test]
    fn reconstruct_cigar_spike_example_forward() {
        assert_eq!(
            reconstruct_cigar(297, 353, 1732, false, "46M3I7M"),
            "297S46M3I7M1379S"
        );
    }

    /// REVERSE strand swaps the flanks (the bug the Phase-1 cross-check caught):
    /// the SAME forward query coords (qs=297, qe=353, len=1732) on the reverse
    /// strand → leading = 1732 − 353 = 1379, trailing = 297 → `1379S46M3I7M297S`.
    #[test]
    fn reconstruct_cigar_spike_example_reverse_swaps_flanks() {
        assert_eq!(
            reconstruct_cigar(297, 353, 1732, true, "46M3I7M"),
            "1379S46M3I7M297S"
        );
        // The forward and reverse reconstructions are flank-mirrors of each other.
        assert_eq!(
            reconstruct_cigar(84, 84 + 100, 4664, true, "100M"),
            "4480S100M84S" // leading = 4664 − 184 = 4480, trailing = 84
        );
    }

    /// No leading clip (query_start 0) → no `0S` prefix; no trailing clip
    /// (query_end == read_len) → no `0S` suffix (forward strand).
    #[test]
    fn reconstruct_cigar_omits_zero_length_clips() {
        assert_eq!(reconstruct_cigar(0, 10, 10, false, "10M"), "10M");
        assert_eq!(reconstruct_cigar(0, 8, 10, false, "8M"), "8M2S");
        assert_eq!(reconstruct_cigar(3, 10, 10, false, "7M"), "3S7M");
        // reverse: qs=0/qe=8/len=10 → leading = read_len-qe = 2, trailing = qs = 0.
        assert_eq!(reconstruct_cigar(0, 8, 10, true, "8M"), "2S8M");
    }

    /// A core with internal `D`/`I` consumes FEWER query bases than its target span;
    /// the soft-clip arithmetic is on the QUERY flanks only, and the reconstructed
    /// CIGAR must still consume exactly `read_len` query bases (asserted in the
    /// round-trip test below). The second spike example: core `20M1D4M2I4M4I10M3D4M`,
    /// query_start 229, query_end 277 (= 229 + 48 query-consumed), read_len 1000 →
    /// trailing 1000 − 277 = 723.
    #[test]
    fn reconstruct_cigar_with_internal_indels() {
        let core = "20M1D4M2I4M4I10M3D4M"; // 20+4+2+4+4+10+4 = 48 query bases
        let cig = reconstruct_cigar(229, 277, 1000, false, core);
        assert_eq!(cig, "229S20M1D4M2I4M4I10M3D4M723S");
    }

    /// `consumed_read_len` counts query-consuming ops (M/I/S/=/X), ignores
    /// reference-only ops (D/N/H/P), and the reconstructed CIGAR (with flanks)
    /// consumes exactly `read_len`.
    #[test]
    fn consumed_read_len_counts_query_ops_and_matches_read_len() {
        assert_eq!(
            consumed_read_len("297S46M3I7M1379S"),
            Some(297 + 46 + 3 + 7 + 1379)
        );
        assert_eq!(consumed_read_len("297S46M3I7M1379S"), Some(1732)); // == read_len
        // D / N do NOT consume query.
        assert_eq!(consumed_read_len("10M2D5M"), Some(15));
        assert_eq!(consumed_read_len("5M3N5M"), Some(10));
        // unmapped sentinel.
        assert_eq!(consumed_read_len("*"), Some(0));
        // malformed.
        assert_eq!(consumed_read_len("10"), None);
        assert_eq!(consumed_read_len("10Z"), None);
    }

    /// End-to-end on a `D`-bearing core: reconstruct then assert consumed ==
    /// read_len, for BOTH strands (the flank swap must preserve the total).
    #[test]
    fn reconstruct_then_consumed_len_round_trip_with_deletion() {
        let core = "20M1D4M2I4M4I10M3D4M"; // 48 query bases
        let (qs, read_len) = (229usize, 1000usize);
        let qe = qs + 48;
        assert_eq!(
            consumed_read_len(&reconstruct_cigar(qs, qe, read_len, false, core)),
            Some(read_len)
        );
        assert_eq!(
            consumed_read_len(&reconstruct_cigar(qs, qe, read_len, true, core)),
            Some(read_len)
        );
    }

    #[test]
    #[should_panic(expected = "already contains a soft clip")]
    fn reconstruct_cigar_rejects_pre_clipped_core_in_debug() {
        // The library returns the aligned core only; a core that already has `S`
        // would double-clip. Guarded by debug_assert.
        let _ = reconstruct_cigar(5, 10, 20, false, "5S5M");
    }

    // ---- Feature-GATED: build_sam_record on canned rammap::Mapping ----

    #[cfg(feature = "rammap-inprocess")]
    #[allow(clippy::too_many_arguments)] // a test fixture builder; clarity > arg count
    fn canned_mapping(
        rname: &str,
        target_start: usize,
        query_start: usize,
        query_end: usize,
        strand: rammap::Strand,
        cigar: &str,
        score: i32,
        mapq: i32,
        md: &str,
    ) -> rammap::Mapping {
        rammap::Mapping {
            target_name: std::sync::Arc::from(rname),
            target_id: 0,
            target_len: 1_000_000,
            target_start,
            target_end: target_start + 50,
            query_start,
            query_end,
            strand,
            mapq,
            is_primary: true,
            is_supplementary: false,
            is_spliced: false,
            trans_strand: None,
            matches: 50,
            block_len: 50,
            edit_distance: 0,
            cigar: Some(cigar.to_string()),
            cigar_ops: None,
            cs: None,
            md: Some(md.to_string()),
            score,
            divergence: 0.0,
        }
    }

    #[cfg(feature = "rammap-inprocess")]
    #[test]
    fn build_sam_record_forward_mapped() {
        // read length 10, forward, full match.
        let read = b"ACGTACGTAC";
        let qual = b"IIIIIIIIII";
        let m = canned_mapping(
            "chr1_CT_converted",
            99, // 0-based → POS 100
            0,
            10,
            rammap::Strand::Forward,
            "10M",
            -3,
            42,
            "10",
        );
        let r = build_sam_record("r1", read, qual, Some(&m));
        assert_eq!(r.qname, "r1");
        assert_eq!(r.flag, 0); // forward
        assert_eq!(r.rname, "chr1_CT_converted"); // suffix kept raw
        assert_eq!(r.pos, 100); // target_start 99 + 1
        assert_eq!(r.mapq, 42);
        assert_eq!(r.cigar, "10M"); // no soft-clip flanks (qs=0, qe=read_len)
        assert_eq!(r.alignment_score, Some(-3));
        assert_eq!(r.second_best, None); // s2 ignored by construction
        assert_eq!(r.md_tag.as_deref(), Some("10"));
        assert_eq!(r.seq, "ACGTACGTAC"); // forward read unchanged
        assert_eq!(r.qual, "IIIIIIIIII");
        // raw_line round-trips through the subprocess parser to the same record.
        assert_eq!(SamRecord::parse(&r.raw_line).unwrap(), r);
        // consumed query length == read_len (no silent skip).
        assert_eq!(consumed_read_len(&r.cigar), Some(read.len()));
    }

    #[cfg(feature = "rammap-inprocess")]
    #[test]
    fn build_sam_record_reverse_mapped_soft_clipped() {
        // read length 14, reverse strand, soft-clipped core + an internal indel.
        // ASYMMETRIC flanks so the strand swap is actually exercised (a symmetric
        // sample would pass even WITH the swap bug the cross-check caught).
        let read = b"ACGTACGTACGTAC"; // len 14
        let qual = b"ABCDEFGHIJKLMN";
        // core 4M1I3M consumes 8 query bases; qs=2, qe=10. Forward flanks would be
        // 2S … 4S; REVERSE swaps them → 4S … 2S.
        let m = canned_mapping(
            "chr2_GA_converted",
            199, // POS 200
            2,
            10,
            rammap::Strand::Reverse,
            "4M1I3M",
            -7,
            30,
            "3A4",
        );
        let r = build_sam_record("r2", read, qual, Some(&m));
        assert_eq!(r.flag, 0x10); // reverse bit
        assert_eq!(r.rname, "chr2_GA_converted");
        assert_eq!(r.pos, 200);
        assert_eq!(r.mapq, 30);
        assert_eq!(r.cigar, "4S4M1I3M2S"); // reverse: flanks SWAPPED (4S lead, 2S trail)
        assert_eq!(r.alignment_score, Some(-7));
        assert_eq!(r.second_best, None);
        assert_eq!(r.md_tag.as_deref(), Some("3A4"));
        // reverse SEQ = nucleotide revcomp of the read; QUAL reversed.
        assert_eq!(
            r.seq,
            String::from_utf8(crate::aligner::output::revcomp(read)).unwrap()
        );
        assert_eq!(r.qual, "NMLKJIHGFEDCBA"); // qual reversed (len 14)
        assert_eq!(SamRecord::parse(&r.raw_line).unwrap(), r);
        // 4 + 4 + 1 + 3 + 2 = 14 == read_len.
        assert_eq!(consumed_read_len(&r.cigar), Some(read.len()));
    }

    #[cfg(feature = "rammap-inprocess")]
    #[test]
    fn build_sam_record_unmapped() {
        let read = b"ACGTACGT";
        let qual = b"IIIIIIII";
        let r = build_sam_record("r3", read, qual, None);
        assert_eq!(r.flag, 0x4);
        assert!(r.is_unmapped());
        assert_eq!(r.rname, "*");
        assert_eq!(r.pos, 0);
        assert_eq!(r.cigar, "*");
        assert_eq!(r.mapq, 0);
        assert_eq!(r.alignment_score, None);
        assert_eq!(r.second_best, None);
        assert_eq!(r.md_tag, None);
        // round-trips to the same unmapped shape the subprocess parse yields.
        assert_eq!(SamRecord::parse(&r.raw_line).unwrap(), r);
    }

    /// The stream reads converted FastQ records in input order and yields one
    /// primary record per read; an empty source → `None` immediately. Uses a
    /// canned `Aligner` built from in-memory sequences (no `.mmi` / no subprocess),
    /// so this is hermetic but still exercises the real `map_seq_with` + the
    /// FastQ-record reader + the builder.
    #[cfg(feature = "rammap-inprocess")]
    #[test]
    fn stream_yields_one_record_per_read_in_input_order() {
        use std::io::Cursor;
        use std::sync::Arc;

        // A tiny reference: one contig with a recognisable region.
        let reference = b"\
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT\
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT"
            .to_vec();
        let aligner = Arc::new(rammap::Aligner::from_seqs(
            vec![("chr1_CT_converted".to_string(), reference.clone())],
            rammap::Preset::MapOnt,
        ));

        // Converted FastQ: 2 records (`@` prefix as the converted file writes it).
        let fq = "@readA\nACGTACGTACGTACGTACGTACGT\n+\nIIIIIIIIIIIIIIIIIIIIIIII\n\
                  @readB\nTTTTTTTTTTTTTTTTTTTTTTTT\n+\nIIIIIIIIIIIIIIIIIIIIIIII\n";
        let mut s =
            InProcessAlignerStream::new(aligner.clone(), Cursor::new(fq.as_bytes()), pool(1))
                .unwrap();

        // Record 1 = readA, in input order.
        let c1 = s.current().expect("first record");
        assert_eq!(c1.qname, "readA");
        s.advance().unwrap();
        // Record 2 = readB.
        let c2 = s.current().expect("second record");
        assert_eq!(c2.qname, "readB");
        s.advance().unwrap();
        assert!(s.current().is_none()); // EOF

        // Empty source → None immediately.
        let empty = InProcessAlignerStream::new(aligner, Cursor::new(&b""[..]), pool(1)).unwrap();
        assert!(empty.current().is_none());
    }

    /// A rayon pool of `n` threads (test helper, #995).
    #[cfg(feature = "rammap-inprocess")]
    fn pool(n: usize) -> std::sync::Arc<rayon::ThreadPool> {
        std::sync::Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .unwrap(),
        )
    }

    /// #995 V2+V3 (hermetic — `from_seqs`, no `.mmi`/oxy): the parallel chunked refill is
    /// (V3) thread-count-invariant — every pool size in `1..=8` yields identical records
    /// (rev2 Alt-1: 8 is the auto default cap) — and (V2) yields every read in INPUT ORDER
    /// across MULTIPLE chunk boundaries (5000 reads > the 2048 `CHUNK`). Drains to comparable
    /// (qname, flag, rname, pos, cigar) tuples.
    #[cfg(feature = "rammap-inprocess")]
    #[test]
    fn parallel_refill_is_thread_invariant_across_chunks() {
        use std::io::Cursor;
        use std::sync::Arc;
        let reference = b"\
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT\
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT"
            .to_vec();
        let aligner = Arc::new(rammap::Aligner::from_seqs(
            vec![("chr1_CT_converted".to_string(), reference)],
            rammap::Preset::MapOnt,
        ));
        // 5000 records (> CHUNK=2048 → ≥3 chunks + the refill boundary).
        let mut fq = String::new();
        for i in 0..5000 {
            fq.push_str(&format!(
                "@read{i}\nACGTACGTACGTACGTACGTACGT\n+\nIIIIIIIIIIIIIIIIIIIIIIII\n"
            ));
        }
        let drain = |n: usize| -> Vec<(String, u16, String, u32, String)> {
            let mut s = InProcessAlignerStream::new(
                Arc::clone(&aligner),
                Cursor::new(fq.clone().into_bytes()),
                pool(n),
            )
            .unwrap();
            let mut v = Vec::new();
            while let Some(r) = s.current() {
                v.push((
                    r.qname.clone(),
                    r.flag,
                    r.rname.clone(),
                    r.pos,
                    r.cigar.clone(),
                ));
                s.advance().unwrap();
            }
            v
        };
        let serial = drain(1);
        assert_eq!(
            serial.len(),
            5000,
            "V2: every read, across chunk boundaries"
        );
        assert_eq!(serial[0].0, "read0", "input order preserved (first)");
        assert_eq!(serial[4999].0, "read4999", "input order preserved (last)");
        // V3 (rev2 Alt-1): thread-count-invariant across the WHOLE default range 1..=8
        // (8 = `RAMMAP_INPROCESS_THREAD_CAP`, the auto default N — so the machine-independence
        // claim covers every pool size a default run can pick, not just n=4).
        for n in 1..=8 {
            assert_eq!(
                drain(n),
                serial,
                "V3: --multicore {n} == --multicore 1 (thread-invariant)"
            );
        }
    }
}
