//! 🔑 Record-level cross-check (the Phase-1 fidelity gate, epic
//! `06152026_rammap-library-integration`).
//!
//! Runs the production [`InProcessAlignerStream`] AND parses the subprocess
//! `rammap` CLI SAM on the SAME converted-read sample, then asserts the
//! `SamRecord`s are **field-identical on every merge-consumed field**
//! (flag / rname / pos / **cigar** / mapq / alignment_score / second_best /
//! md_tag), PLUS (a) `consumed_read_len(cigar) == read_len` for every mapped read
//! (else the methylation length guard at `lib.rs` SILENTLY skips it), and (b)
//! **in-process mapped count == subprocess mapped count** (the no-silent-skip
//! proof). `seq` is diffed for fidelity but is NOT gate-blocking (downstream-dead,
//! Rev 2).
//!
//! This is the proof that the merge produces identical output BEFORE Phase 2 wires
//! the stream into `lib.rs`/`config.rs`.
//!
//! ## Gating
//!
//! - **Feature:** the whole file is `#[cfg(feature = "rammap-inprocess")]` (it names
//!   `rammap::*` types) — it never compiles in the default/Mac build.
//! - **Env (data):** skipped (NOT failed) unless these three env vars are set, so it
//!   is hermetic on a CI runner without the rammap binary / index / data —
//!   `RAMMAP_BIN` (the `rammap` CLI binary), `RAMMAP_MMI` (a `.mmi` index, e.g.
//!   `BS_CT.mmi`), and `RAMMAP_READS` (a converted FastQ file, the `@id`-prefixed
//!   `_C_to_T` form, whose sample MUST include reads with leading-S, trailing-S,
//!   internal `I`, and `D` ops).
//!
//! Run on oxy with:
//! ```text
//! RAMMAP_BIN=~/rammap/target/release/rammap \
//! RAMMAP_MMI=~/bismark_benchmarks/genome/Bisulfite_Genome/CT_conversion/BS_CT.mmi \
//! RAMMAP_READS=/var/tmp/conv_sample_C_to_T.fastq \
//! cargo test -p bismark-aligner --features rammap-inprocess --test rammap_inprocess_crosscheck -- --nocapture
//! ```

#![cfg(feature = "rammap-inprocess")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use bismark_aligner::align::{SamRecord, SamStream};
use bismark_aligner::inprocess::{InProcessAlignerStream, consumed_read_len};
use bismark_aligner::rammap;

/// Read the three env vars; `None` (→ skip) if any is missing.
fn env_inputs() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let bin = std::env::var_os("RAMMAP_BIN")?;
    let mmi = std::env::var_os("RAMMAP_MMI")?;
    let reads = std::env::var_os("RAMMAP_READS")?;
    Some((bin.into(), mmi.into(), reads.into()))
}

/// Parse the subprocess `rammap` SAM (skip `@` header lines), keyed by QNAME →
/// the FIRST record per QNAME (the primary, emitted first under `--secondary=no`)
/// — exactly what the subprocess `AlignerStream` feeds the merge.
fn subprocess_sam_by_qname(
    bin: &PathBuf,
    mmi: &PathBuf,
    reads: &PathBuf,
) -> HashMap<String, SamRecord> {
    // The same invocation shape the Rust subprocess backend uses (Perl 7022/7025):
    // `<opts> <index>.mmi <input>`, with `--secondary=no` so the primary is first.
    let out = Command::new(bin)
        .args([
            "-a",
            "--MD",
            "--secondary=no",
            "-t",
            "1",
            "-x",
            "map-ont",
            "-K",
            "250K",
        ])
        .arg(mmi)
        .arg(reads)
        .output()
        .expect("failed to run the rammap subprocess");
    assert!(
        out.status.success(),
        "rammap subprocess exited unsuccessfully: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut by_qname: HashMap<String, SamRecord> = HashMap::new();
    for line in out.stdout.split(|&b| b == b'\n') {
        if line.is_empty() || line[0] == b'@' {
            continue;
        }
        let s = String::from_utf8_lossy(line);
        let rec = SamRecord::parse(&s).expect("subprocess SAM line must parse");
        // Keep the FIRST record per qname (primary). A supplementary (later) line
        // for the same qname is dropped, matching the subprocess stream's store-first.
        by_qname.entry(rec.qname.clone()).or_insert(rec);
    }
    by_qname
}

/// Index the converted FastQ by qname → (read_len) so the cross-check can assert
/// `consumed_read_len == read_len` per read (the converted file is the SAME bytes
/// the subprocess + in-process stream both consume).
fn read_lengths_by_qname(reads: &PathBuf) -> HashMap<String, usize> {
    let f = std::fs::File::open(reads).expect("open RAMMAP_READS");
    let mut r = BufReader::new(f);
    let (mut id, mut seq, mut id2, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut out = HashMap::new();
    loop {
        id.clear();
        seq.clear();
        id2.clear();
        qual.clear();
        let n1 = r.read_until(b'\n', &mut id).unwrap();
        let n2 = r.read_until(b'\n', &mut seq).unwrap();
        let n3 = r.read_until(b'\n', &mut id2).unwrap();
        let n4 = r.read_until(b'\n', &mut qual).unwrap();
        if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
            break;
        }
        let qname = String::from_utf8_lossy(&id)
            .trim_end()
            .trim_start_matches('@')
            .to_string();
        let len = seq
            .iter()
            .take_while(|&&b| b != b'\n' && b != b'\r')
            .count();
        out.insert(qname, len);
    }
    out
}

#[test]
fn inprocess_matches_subprocess_record_for_record() {
    let Some((bin, mmi, reads)) = env_inputs() else {
        eprintln!(
            "SKIP rammap cross-check: set RAMMAP_BIN / RAMMAP_MMI / RAMMAP_READS to run \
             (needs the rammap binary, a .mmi index, and a converted-read sample)."
        );
        return;
    };

    // --- in-process: drive the production stream over the converted reads. ---
    let mmi_str = mmi.to_str().expect("RAMMAP_MMI must be UTF-8");
    let aligner = Arc::new(
        rammap::Aligner::from_index(mmi_str, rammap::Preset::MapOnt)
            .expect("load .mmi via rammap::Aligner::from_index"),
    );
    // Read the whole converted file into memory so the in-process stream consumes
    // the identical bytes the subprocess CLI reads.
    let bytes = std::fs::read(&reads).expect("read RAMMAP_READS");
    // #995: the stream takes a shared rayon pool; a 1-thread pool here keeps the field-level
    // cross-check vs the subprocess deterministic + simplest (thread-invariance is covered by
    // the hermetic `parallel_refill_is_thread_invariant_across_chunks` unit test).
    let pool = std::sync::Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap(),
    );
    let mut stream = InProcessAlignerStream::new(aligner, Cursor::new(bytes), pool)
        .expect("build in-process stream");

    let read_lens = read_lengths_by_qname(&reads);
    let sub = subprocess_sam_by_qname(&bin, &mmi, &reads);

    let mut total = 0usize;
    let mut inproc_mapped = 0usize;
    let mut field_matches = 0usize;
    let mut field_mismatches = 0usize;
    let mut seq_mismatches = 0usize;
    // op-coverage proof: the sample MUST exercise leading-S / trailing-S / I / D.
    let (mut saw_lead_s, mut saw_trail_s, mut saw_i, mut saw_d) = (false, false, false, false);

    while let Some(rec) = stream.current() {
        total += 1;
        let qname = rec.qname.clone();
        let s = sub.get(&qname).unwrap_or_else(|| {
            panic!("qname {qname} present in-process but missing from subprocess SAM")
        });

        // FLAG carries mapped/unmapped; compare the merge-consumed fields.
        assert_eq!(rec.flag, s.flag, "FLAG mismatch for {qname}");
        let mapped_inproc = !rec.is_unmapped();
        let mapped_sub = !s.is_unmapped();
        assert_eq!(
            mapped_inproc, mapped_sub,
            "mapped/unmapped disagreement for {qname}"
        );

        if mapped_inproc {
            inproc_mapped += 1;
            // Every merge-consumed field, field by field.
            let same = rec.rname == s.rname
                && rec.pos == s.pos
                && rec.cigar == s.cigar
                && rec.mapq == s.mapq
                && rec.alignment_score == s.alignment_score
                && rec.second_best == s.second_best
                && rec.md_tag == s.md_tag;
            if same {
                field_matches += 1;
            } else {
                field_mismatches += 1;
                eprintln!(
                    "FIELD MISMATCH {qname}:\n  inproc: rname={} pos={} cigar={} mapq={} AS={:?} 2nd={:?} md={:?}\n  subpro: rname={} pos={} cigar={} mapq={} AS={:?} 2nd={:?} md={:?}",
                    rec.rname,
                    rec.pos,
                    rec.cigar,
                    rec.mapq,
                    rec.alignment_score,
                    rec.second_best,
                    rec.md_tag,
                    s.rname,
                    s.pos,
                    s.cigar,
                    s.mapq,
                    s.alignment_score,
                    s.second_best,
                    s.md_tag,
                );
            }
            // seq is downstream-dead (Rev 2) — diff for fidelity, non-blocking.
            if rec.seq != s.seq {
                seq_mismatches += 1;
            }

            // 🔴 consumed_read_len == read_len (else the methylation guard skips it).
            let rl = *read_lens
                .get(&qname)
                .unwrap_or_else(|| panic!("no read length for mapped {qname}"));
            assert_eq!(
                consumed_read_len(&rec.cigar),
                Some(rl),
                "reconstructed CIGAR {} for {qname} does not consume read_len {rl} \
                 (the methylation length guard would SILENTLY skip this read)",
                rec.cigar
            );

            // op coverage on the reconstructed (in-process) CIGAR. A leading soft
            // clip = the FIRST CIGAR run is `S` (i.e. `^\d+S`): the run terminator
            // immediately after the leading digits is `S`.
            let first_op = rec.cigar.trim_start_matches(|c: char| c.is_ascii_digit());
            if first_op.starts_with('S') {
                saw_lead_s = true;
            }
            if rec.cigar.ends_with('S') {
                saw_trail_s = true;
            }
            if rec.cigar.contains('I') {
                saw_i = true;
            }
            if rec.cigar.contains('D') {
                saw_d = true;
            }
        }

        stream.advance().expect("advance in-process stream");
    }

    let sub_mapped = sub.values().filter(|r| !r.is_unmapped()).count();

    eprintln!(
        "rammap cross-check: total={total} inproc_mapped={inproc_mapped} sub_mapped={sub_mapped} \
         field_matches={field_matches} field_mismatches={field_mismatches} seq_mismatches={seq_mismatches} \
         (lead_S={saw_lead_s} trail_S={saw_trail_s} I={saw_i} D={saw_d})"
    );

    // --- the gate ---
    assert!(total > 0, "no reads processed — check RAMMAP_READS");
    assert_eq!(
        field_mismatches, 0,
        "{field_mismatches} record(s) differ on a merge-consumed field"
    );
    // no-silent-skip proof: in-process mapped count == subprocess mapped count.
    assert_eq!(
        inproc_mapped, sub_mapped,
        "in-process mapped count {inproc_mapped} != subprocess mapped count {sub_mapped} \
         (a silent skip / extra map)"
    );
    // op coverage: the sample MUST exercise all four ops (Rev 2 hardening).
    assert!(
        saw_lead_s && saw_trail_s && saw_i && saw_d,
        "the cross-check sample must include leading-S / trailing-S / internal-I / D reads \
         (got lead_S={saw_lead_s} trail_S={saw_trail_s} I={saw_i} D={saw_d}); pick a larger / more diverse sample"
    );
}
