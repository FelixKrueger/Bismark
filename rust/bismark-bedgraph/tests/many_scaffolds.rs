//! Functional (not byte-identity) test: a scaffold-heavy genome.
//!
//! Perl `bismark2bedGraph`'s default mode opens **one temp filehandle per
//! chromosome** (`:274-283`), so a genome with thousands of scaffolds exceeds
//! `ulimit -n` (~1024) and Perl dies — which is the whole reason Perl's
//! `--gazillion` mode exists. The Rust port aggregates in memory with no
//! filehandle limit, so it handles such genomes natively in default mode.
//!
//! This verifies that: the binary succeeds on N = 3000 scaffolds (well past
//! the Perl filehandle wall), emits one correct row per scaffold, and orders
//! scaffolds **bytewise/ASCII** (the port's documented order — which matches
//! Perl's *default*-mode order, NOT Perl `--gazillion`'s `sort -V` natural
//! order; that divergence is accepted and documented, SPEC §1.1 D2). It is
//! therefore a correctness/scale test, not a Perl-parity cell (Perl-default
//! can't even run here).

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};

use assert_cmd::Command;
use flate2::read::GzDecoder;
use tempfile::TempDir;

#[test]
fn handles_thousands_of_scaffolds_in_bytewise_order() {
    const N: usize = 3000; // > the ~1024 open-filehandle limit that kills Perl default mode

    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("CpG_OT_scaffolds.txt"); // basename starts with CpG → default mode
    {
        let mut f = File::create(&input).unwrap();
        writeln!(f, "Bismark methylation extractor version v0.25.1").unwrap(); // header (dropped)
        for i in 0..N {
            // One methylated CpG per scaffold at pos 100 → 100%, counts 1/0.
            writeln!(f, "r\t+\tscaffold_{i}\t100\tZ").unwrap();
        }
    }

    Command::cargo_bin("bismark2bedGraph_rs")
        .unwrap()
        .current_dir(tmp.path())
        .args(["-o", "out.bedGraph", "CpG_OT_scaffolds.txt"])
        .assert()
        .success(); // the headline: Rust does NOT crash where Perl default would

    // Read the coverage rows.
    let bytes = fs::read(tmp.path().join("out.bismark.cov.gz")).unwrap();
    let rows: Vec<String> = BufReader::new(GzDecoder::new(&bytes[..]))
        .lines()
        .map(Result::unwrap)
        .collect();

    // One row per scaffold, all present, all correctly counted.
    assert_eq!(
        rows.len(),
        N,
        "expected {N} coverage rows, got {}",
        rows.len()
    );
    let mut names = BTreeSet::new();
    let mut prev: Option<String> = None;
    for row in &rows {
        let f: Vec<&str> = row.split('\t').collect();
        // cov: chr  start  end  meth%  count_meth  count_unmeth
        assert_eq!(f.len(), 6, "malformed cov row: {row:?}");
        assert!(f[0].starts_with("scaffold_"), "unexpected chr: {}", f[0]);
        assert_eq!(f[1], "100", "pos");
        assert_eq!(f[3], "100", "meth% (1 meth / 0 unmeth → 100)");
        assert_eq!(f[4], "1", "count_meth");
        assert_eq!(f[5], "0", "count_unmeth");
        // Bytewise non-decreasing chromosome order (the port's documented order).
        if let Some(p) = &prev {
            assert!(
                p.as_str() <= f[0],
                "scaffolds not in bytewise order: {p:?} then {:?}",
                f[0]
            );
        }
        prev = Some(f[0].to_string());
        names.insert(f[0].to_string());
    }
    assert_eq!(names.len(), N, "duplicate or missing scaffolds");
}
