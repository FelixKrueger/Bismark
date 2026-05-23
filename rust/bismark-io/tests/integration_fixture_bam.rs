//! Integration test against a committed Bismark-Perl-generated BAM fixture.
//!
//! Validates that `BamReader` + `BismarkRecord` together reproduce the
//! expected per-record strand classification and read-identity decoding
//! on a real (small) Bismark BAM. The fixture is documented in
//! `test_files/README.md`; it is generated once by Bismark Perl v0.25.1
//! and committed.

use bismark_io::{BamReader, BismarkPair, BismarkStrand, ReadIdentity};
use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("test_files");
    p.push("tiny_pe_bismark.bam");
    p
}

#[test]
fn fixture_bam_opens_and_yields_expected_record_count() {
    let mut reader = BamReader::from_path(&fixture_path()).expect("open fixture BAM");
    let records: Vec<_> = reader.records().collect();
    // 203 alignment records, all mapped (the unmapped filter would drop
    // unmapped reads — none are present in this fixture by construction).
    assert_eq!(
        records.len(),
        203,
        "fixture has 203 mapped records; got {}",
        records.len()
    );
    // Every record should classify cleanly (no errors).
    for (i, rec) in records.iter().enumerate() {
        assert!(
            rec.is_ok(),
            "record {i} classification failed: {:?}",
            rec.as_ref().err()
        );
    }
}

#[test]
fn fixture_bam_strand_distribution_matches_perl_output() {
    // Expected per-record strand distribution (verified directly against
    // the fixture via `samtools view + awk` at fixture-generation time;
    // see test_files/README.md):
    //   55 OT     (XR:CT XG:CT)  -- R1 of OT-pairs
    //   55 CTOT   (XR:GA XG:CT)  -- R2 of OT-pairs
    //   47 OB     (XR:CT XG:GA)  -- R1 of OB-pairs
    //   46 CTOB   (XR:GA XG:GA)  -- R2 of OB-pairs
    // Totals: R1 = 55 + 47 = 102; R2 = 55 + 46 = 101.
    // The head-208 cutoff (8 header lines + 200 alignment) crossed the
    // pair boundary slightly, leaving +2 extra R1 and +1 extra R2 over
    // 100 pairs.
    let mut reader = BamReader::from_path(&fixture_path()).expect("open fixture BAM");
    let mut counts: HashMap<BismarkStrand, usize> = HashMap::new();
    for rec in reader.records() {
        let rec = rec.expect("record classification");
        *counts.entry(rec.record_strand()).or_insert(0) += 1;
    }
    assert_eq!(counts.get(&BismarkStrand::OT).copied().unwrap_or(0), 55);
    assert_eq!(counts.get(&BismarkStrand::CTOT).copied().unwrap_or(0), 55);
    assert_eq!(counts.get(&BismarkStrand::OB).copied().unwrap_or(0), 47);
    assert_eq!(counts.get(&BismarkStrand::CTOB).copied().unwrap_or(0), 46);
}

#[test]
fn fixture_bam_read_identity_decoding() {
    // PE library — every record has either R1 or R2 identity, never SE.
    let mut reader = BamReader::from_path(&fixture_path()).expect("open fixture BAM");
    let mut r1_count = 0;
    let mut r2_count = 0;
    let mut se_count = 0;
    for rec in reader.records() {
        let rec = rec.expect("record classification");
        match rec.read_identity() {
            ReadIdentity::R1 => r1_count += 1,
            ReadIdentity::R2 => r2_count += 1,
            ReadIdentity::Single => se_count += 1,
        }
    }
    // 100 PE pairs + boundary effects from the head-208 cutoff:
    // R1 = 102 (= 55 OT + 47 OB), R2 = 101 (= 55 CTOT + 46 CTOB).
    // SE = 0 (PE library never produces SE records).
    assert_eq!(se_count, 0, "PE library should never produce SE records");
    assert_eq!(
        r1_count, 102,
        "expected exactly 102 R1 records, got {r1_count}"
    );
    assert_eq!(
        r2_count, 101,
        "expected exactly 101 R2 records, got {r2_count}"
    );
    // Together they equal the total record count (no SE).
    let total_records: usize = {
        let mut reader = BamReader::from_path(&fixture_path()).expect("open fixture BAM");
        reader.records().count()
    };
    assert_eq!(r1_count + r2_count, total_records);
}

#[test]
fn fixture_bam_pair_strand_counts_directional_library() {
    // PLAN.md §Phase F2: read the fixture, build BismarkPair instances
    // from adjacent R1/R2 records, verify pair_strand counts.
    //
    // For a directional library (this fixture):
    //   - Every successfully-paired R1+R2 must have pair_strand of either
    //     OT or OB. NEVER CTOT or CTOB (those only appear in non-
    //     directional libraries).
    //   - Specifically: 55 OT-pairs, 46 OB-pairs = 101 complete pairs.
    //     (R2 count is 101; R1 has 1 extra unpaired at the tail.)
    let mut reader = BamReader::from_path(&fixture_path()).expect("open fixture BAM");
    let records: Vec<_> = reader
        .records()
        .map(|r| r.expect("record classification"))
        .collect();

    let mut pair_strand_counts: HashMap<BismarkStrand, usize> = HashMap::new();
    let mut unpaired = 0usize;

    // Walk the records pair-by-pair (Bismark output is name-grouped, so
    // consecutive R1 + R2 with matching qname form a pair).
    let mut i = 0;
    while i < records.len() {
        if i + 1 < records.len()
            && records[i].read_identity() == ReadIdentity::R1
            && records[i + 1].read_identity() == ReadIdentity::R2
        {
            let r1 = records[i].clone();
            let r2 = records[i + 1].clone();
            match BismarkPair::from_mates(r1, r2) {
                Ok(pair) => {
                    *pair_strand_counts.entry(pair.pair_strand()).or_insert(0) += 1;
                    i += 2;
                    continue;
                }
                Err(_) => {
                    unpaired += 1;
                    i += 1;
                    continue;
                }
            }
        }
        unpaired += 1;
        i += 1;
    }

    let total_pairs: usize = pair_strand_counts.values().sum();
    assert_eq!(
        total_pairs, 101,
        "expected 101 complete pairs, got {total_pairs}"
    );
    assert_eq!(
        unpaired, 1,
        "expected exactly 1 unpaired record (the extra R1 at the tail), got {unpaired}"
    );

    // The structural-correctness promise: directional libraries produce
    // ONLY OT and OB pair-strands. CTOT and CTOB pair-strands appear
    // only in non-directional library prep.
    assert_eq!(
        pair_strand_counts
            .get(&BismarkStrand::CTOT)
            .copied()
            .unwrap_or(0),
        0,
        "directional library must have zero CTOT-pairs"
    );
    assert_eq!(
        pair_strand_counts
            .get(&BismarkStrand::CTOB)
            .copied()
            .unwrap_or(0),
        0,
        "directional library must have zero CTOB-pairs"
    );

    // OT-pairs + OB-pairs should account for all pairs.
    let ot = pair_strand_counts
        .get(&BismarkStrand::OT)
        .copied()
        .unwrap_or(0);
    let ob = pair_strand_counts
        .get(&BismarkStrand::OB)
        .copied()
        .unwrap_or(0);
    assert_eq!(ot, 55, "expected 55 OT-pairs, got {ot}");
    assert_eq!(ob, 46, "expected 46 OB-pairs, got {ob}");
    assert_eq!(ot + ob, total_pairs);
}

#[test]
fn fixture_bam_directional_library_pair_strand_invariant() {
    // For a directional library (this fixture), R1 records carry strand
    // OT or OB; R2 records carry the complement CTOT or CTOB. No record
    // should have a strand inconsistent with its read identity.
    let mut reader = BamReader::from_path(&fixture_path()).expect("open fixture BAM");
    for rec in reader.records() {
        let rec = rec.expect("record classification");
        match (rec.read_identity(), rec.record_strand()) {
            (ReadIdentity::R1, BismarkStrand::OT | BismarkStrand::OB) => {}
            (ReadIdentity::R1, other) => panic!(
                "R1 record has non-R1 strand: {other:?} (expected OT or OB \
                 in a directional library)"
            ),
            (ReadIdentity::R2, BismarkStrand::CTOT | BismarkStrand::CTOB) => {}
            (ReadIdentity::R2, other) => panic!(
                "R2 record has non-R2 strand: {other:?} (expected CTOT or CTOB \
                 in a directional library)"
            ),
            (ReadIdentity::Single, _) => {
                panic!("PE library should not produce SE-identity records");
            }
        }
    }
}
