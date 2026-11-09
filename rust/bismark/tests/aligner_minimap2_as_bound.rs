//! 🔑 Real-aligner gate for the premise `AS <= 2 × read_length` (#1081).
//!
//! Bismark normalizes minimap2/rammap MAPQ by `max(1, 2·read_length - score_min)`. That
//! denominator is only correct if minimap2 cannot report an `AS:i:` above `2 × read_length`,
//! which rests on two upstream facts: the match score is 2 for every preset Bismark can select
//! (`options.c:47,96,103,155`), and `AS:i:` is the raw DP accumulation (`format.c:403` prints
//! `dp_score`, fed only `ez->max`/`ez->score`) with no additive bonus — in particular `-x sr`'s
//! `end_bonus = 10` is a ksw2 traceback tie-breaker, not a score term
//! (`ksw2_extz2_sse.c:304`).
//!
//! Those are source facts about a program we shell out to, so a future minimap2 could break
//! them. Every OTHER test of this fix is arithmetic over hardcoded scores and would stay green;
//! this one actually runs the aligner, so it is the only thing that fails if the premise stops
//! holding. Investigated in `plans/08012026_minimap2-mapq-denominator/SPIKE.md` (§F1-F3).
//!
//! ## Gating
//!
//! Skipped locally when `minimap2` is absent; **panics when `$CI` is set and it is absent**,
//! so it can never pass vacuously (the #787 pattern in `aligner_five_base_groundtruth.rs`).
//! CI installs minimap2 already (`rust_ci.yml`). Note CI runs the distro package rather than
//! the 2.31-r1302 the spike measured, so this also extends the evidence to a second version.

use std::io::Write;
use std::process::Command as StdCommand;

use tempfile::TempDir;

/// Presets Bismark can select (`options::minimap2_options` — selectors only, closed string).
const REACHABLE_PRESETS: [&str; 3] = ["map-ont", "map-pb", "sr"];

/// Bismark's minimap2 option string, verbatim, minus the varied `-x <preset>`.
const BISMARK_OPTS: [&str; 6] = ["-a", "--MD", "--secondary=no", "-t", "2", "-K"];

fn have_minimap2() -> bool {
    let present = StdCommand::new("minimap2")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !present && std::env::var_os("CI").is_some() {
        panic!(
            "minimap2 not found but $CI is set: the #1081 AS-bound gate requires minimap2 on \
             PATH in CI (install it in the workflow) — refusing to no-op."
        );
    }
    present
}

/// Deterministic pseudo-random ACGT reference (fixed LCG → stable across runs and platforms).
fn gen_reference(n: usize) -> Vec<u8> {
    let bases = [b'A', b'C', b'G', b'T'];
    let mut x: u64 = 0x1081_5EED_1081_5EED;
    (0..n)
        .map(|_| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            bases[((x >> 33) % 4) as usize]
        })
        .collect()
}

fn flip(b: u8) -> u8 {
    match b {
        b'A' => b'C',
        b'C' => b'A',
        b'G' => b'T',
        _ => b'G',
    }
}

/// Every primary alignment's `(qname, read_len, AS)` from a minimap2 run.
fn run_minimap2(
    ref_fa: &std::path::Path,
    reads_fq: &std::path::Path,
    preset: &str,
) -> Vec<(String, usize, i64)> {
    let out = StdCommand::new("minimap2")
        .args(BISMARK_OPTS)
        .arg("250K")
        .arg("-x")
        .arg(preset)
        .arg(ref_fa)
        .arg(reads_fq)
        .output()
        .expect("minimap2 run");
    assert!(
        out.status.success(),
        "minimap2 -x {preset} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut hits = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if line.starts_with('@') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let flag: u16 = f[1].parse().unwrap();
        // Primary, mapped only: secondary/supplementary carry their own sub-range scores.
        if flag & 0x904 != 0 {
            continue;
        }
        if let Some(as_i) = f[11..]
            .iter()
            .find_map(|t| t.strip_prefix("AS:i:").and_then(|v| v.parse::<i64>().ok()))
        {
            hits.push((f[0].to_string(), f[9].len(), as_i));
        }
    }
    hits
}

/// A perfect read must score EXACTLY `2 × len`, and nothing may exceed it.
///
/// The equality half is what makes this discriminating: if `end_bonus` (or any other additive
/// term) ever leaked into the reported score, a perfect read under `-x sr` would come back at
/// `2·len + 10` or `+20` rather than `2·len`, and the inequality alone would not notice a
/// smaller leak.
#[test]
fn minimap2_reports_at_most_two_per_base() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (no $CI)");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let reference = gen_reference(30_000);

    let ref_fa = tmp.path().join("ref.fa");
    let mut fa = std::fs::File::create(&ref_fa).unwrap();
    writeln!(fa, ">chrS").unwrap();
    for chunk in reference.chunks(60) {
        fa.write_all(chunk).unwrap();
        fa.write_all(b"\n").unwrap();
    }
    drop(fa);

    // Offset 5000 keeps every read interior to the reference, so BOTH flanks are extendable —
    // the extensions are the only place `end_bonus` is passed (`align.c:791,883`).
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for len in [100usize, 150, 250, 1000] {
        let base = &reference[5000..5000 + len];
        cases.push((format!("perfect_{len}"), base.to_vec()));
        // Terminal mismatches: the local DP max sits before the last base, so extending to the
        // end of the query is chosen ONLY because of `end_bonus`. Under `-x sr` these come back
        // with a full-length CIGAR whose own score is lower than the reported AS — which is
        // exactly why AS must never be re-derived from a CIGAR for that preset.
        let mut last = base.to_vec();
        let n = last.len();
        last[n - 1] = flip(last[n - 1]);
        cases.push((format!("lastbase_mm_{len}"), last));
        let mut ends = base.to_vec();
        ends[0] = flip(ends[0]);
        ends[n - 1] = flip(ends[n - 1]);
        cases.push((format!("bothends_mm_{len}"), ends));
        let mut interior = base.to_vec();
        interior[n / 2] = flip(interior[n / 2]);
        cases.push((format!("interior_1mm_{len}"), interior));
    }

    let reads_fq = tmp.path().join("reads.fq");
    let mut fq = std::fs::File::create(&reads_fq).unwrap();
    for (name, seq) in &cases {
        writeln!(fq, "@{name}").unwrap();
        fq.write_all(seq).unwrap();
        writeln!(fq).unwrap();
        writeln!(fq, "+").unwrap();
        writeln!(fq, "{}", "I".repeat(seq.len())).unwrap();
    }
    drop(fq);

    let mut perfect_cells = 0usize;
    for preset in REACHABLE_PRESETS {
        let hits = run_minimap2(&ref_fa, &reads_fq, preset);
        assert!(
            !hits.is_empty(),
            "-x {preset} produced no primary alignments — the gate would be vacuous"
        );
        for (qname, read_len, as_i) in hits {
            let perfect = 2 * read_len as i64;
            assert!(
                as_i <= perfect,
                "-x {preset} {qname}: AS:i:{as_i} exceeds 2 x read_length ({perfect}). \
                 Bismark's MAPQ denominator assumes this cannot happen (#1081): if it can, \
                 bestOver/diff exceeds 1 again and minimap2 MAPQ re-saturates at the top rung."
            );
            if qname.starts_with("perfect_") {
                assert_eq!(
                    as_i, perfect,
                    "-x {preset} {qname}: a perfect alignment must score EXACTLY 2 x \
                     read_length; a higher value means an additive bonus (e.g. sr's \
                     end_bonus = 10) now reaches the reported score"
                );
                perfect_cells += 1;
            }
        }
    }
    // Ensure the equality half actually ran: `-x sr` and `map-pb` drop short/low-complexity
    // reads, so a silent zero here would leave only the inequality.
    assert!(
        perfect_cells >= 6,
        "only {perfect_cells} perfect-read cells were checked across 3 presets — too few for \
         the AS == 2·len assertion to be meaningful"
    );
}
