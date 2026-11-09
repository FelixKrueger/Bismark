//! `[#1104]` Integration gates for `--five_base_emit_multiplicity` (simplex consensus)
//! and the deterministic consensus emission order it builds on.
//!
//! All tests drive the standalone `--five_base_consensus_from_bam` entry over
//! hand-crafted BAMs (built from SAM text via samtools), so no aligner is needed.
//! The in-run entry point is covered in `aligner_five_base_groundtruth.rs`.
//!
//! Fixture geometry: a genome of repeated 22-bp blocks, each holding one CpG and
//! no other C/G anywhere — so every consensus XM is dots except the CpG calls,
//! and expected strings can be stated from first principles.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bismark_bin() -> PathBuf {
    // target/debug/deps/<test binary> -> target/debug/bismark
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("bismark")
}

fn samtools_available() -> bool {
    let present = Command::new("samtools")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    // Fail loud in CI so these gates never pass vacuously (same guard as the
    // #1095 suite).
    if !present && std::env::var_os("CI").is_some() {
        panic!("samtools not found but $CI is set: refusing to no-op.");
    }
    present
}

/// One 22-bp block: a 20-bp window whose only C/G is the CpG at window
/// offsets 6/7, plus a 2-bp spacer. Window `i` starts at genome offset
/// `10 + 22*i` (0-based).
const WINDOW: &str = "AATTAACGTTAATTAATTAA";
const LEAD: &str = "AATTAATTAA";

fn genome_seq(n_windows: usize) -> String {
    let mut s = String::from(LEAD);
    for _ in 0..n_windows {
        s.push_str(WINDOW);
        s.push_str("TT");
    }
    s
}

/// 0-based genome offset of window `i`'s first base.
fn window_start(i: usize) -> usize {
    LEAD.len() + 22 * i
}

/// Write `>chrT` + the sequence as the genome folder's FASTA.
fn write_genome(dir: &Path, n_windows: usize) {
    std::fs::write(
        dir.join("chrT.fa"),
        format!(">chrT\n{}\n", genome_seq(n_windows)),
    )
    .unwrap();
}

/// A proper FR read pair fully covering window `i`, as two SAM lines.
/// `ot`: molecule strand (true = OT: R1 forward; false = OB: R1 reverse).
/// `seq`: the +ref-oriented 20-bp SEQ both mates carry.
fn pair(qname: &str, i: usize, ot: bool, seq: &str, mapq_r1: u8, mapq_r2: u8) -> String {
    let pos = window_start(i) + 1; // SAM is 1-based
    let dots = ".".repeat(seq.len());
    // FLAG combos: OT = 99/147 (R1 fwd + R2 rev), OB = 83/163 (R1 rev + R2 fwd).
    let (f1, f2, t1, t2) = if ot {
        (99, 147, 20, -20)
    } else {
        (83, 163, -20, 20)
    };
    let mut s = String::new();
    for (flag, tlen, mapq, xr) in [(f1, t1, mapq_r1, "CT"), (f2, t2, mapq_r2, "GA")] {
        s.push_str(&format!(
            "{qname}\t{flag}\tchrT\t{pos}\t{mapq}\t20M\t=\t{pos}\t{tlen}\t{seq}\t\
             IIIIIIIIIIIIIIIIIIII\tXM:Z:{dots}\tXR:Z:{xr}\tXG:Z:{}\n",
            if ot { "CT" } else { "GA" }
        ));
    }
    s
}

/// Assemble a BAM from SAM record lines via samtools.
fn make_bam(dir: &Path, name: &str, n_windows: usize, records: &str) -> PathBuf {
    let sam = format!(
        "@HD\tVN:1.6\n@SQ\tSN:chrT\tLN:{}\n{records}",
        genome_seq(n_windows).len()
    );
    let sam_path = dir.join(format!("{name}.sam"));
    std::fs::write(&sam_path, sam).unwrap();
    let bam_path = dir.join(format!("{name}.bam"));
    let out = Command::new("samtools")
        .args([
            "view",
            "-b",
            "-o",
            &bam_path.to_string_lossy(),
            &sam_path.to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "samtools view -b failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    bam_path
}

/// Run the standalone consensus over `bam` with extra args; assert success.
fn run_consensus(genome_dir: &Path, out_dir: &Path, bam: &Path, extra: &[&str]) -> String {
    let out = Command::new(bismark_bin())
        .arg("--illumina_5base")
        .arg("--five_base_consensus_from_bam")
        .arg(bam)
        .arg("--genome")
        .arg(genome_dir)
        .arg("--output_dir")
        .arg(out_dir)
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "consensus run failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Full decompressed SAM text (header + records), order preserved, with `@PG`
/// lines dropped — they embed the run's own command line (per-run temp paths),
/// matching the suite's byte-gate convention.
fn sam_text(bam: &Path) -> String {
    let out = Command::new("samtools")
        .args(["view", "-h", &bam.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "samtools view failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.starts_with("@PG"))
        .map(|l| format!("{l}\n"))
        .collect()
}

/// A methylated-OT window SEQ: 5mC→T at the CpG C (window offset 6).
fn seq_ot_methylated() -> String {
    let mut s = WINDOW.to_string();
    s.replace_range(6..7, "T");
    s
}

// ---- step 0: deterministic emission order --------------------------------------------

/// Two runs of the SAME binary over a multi-family BAM must produce identical
/// consensus output. Before #1104's step 0 the emit loop followed HashMap
/// iteration order (random per process), so this was not true.
#[test]
fn consensus_emission_order_is_deterministic() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 6;
    write_genome(tmp.path(), n);
    // Six duplex families: an OT pair and an OB pair on each window.
    let mut recs = String::new();
    for i in 0..n {
        recs.push_str(&pair(
            &format!("ot{i}"),
            i,
            true,
            &seq_ot_methylated(),
            60,
            60,
        ));
        recs.push_str(&pair(&format!("ob{i}"), i, false, WINDOW, 60, 60));
    }
    let bam = make_bam(tmp.path(), "duplex6", n, &recs);

    let out_a = tempfile::tempdir().unwrap();
    let out_b = tempfile::tempdir().unwrap();
    run_consensus(tmp.path(), out_a.path(), &bam, &[]);
    run_consensus(tmp.path(), out_b.path(), &bam, &[]);
    let sam_a = sam_text(&out_a.path().join("five_base_consensus.bam"));
    let sam_b = sam_text(&out_b.path().join("five_base_consensus.bam"));
    assert!(
        sam_a.lines().filter(|l| !l.starts_with('@')).count() >= 2 * n,
        "fixture must yield at least {n} families x 2 records or the gate is vacuous"
    );
    assert_eq!(
        sam_a, sam_b,
        "consensus output must be byte-stable across runs of the same binary"
    );
    // The order itself is the sorted family order: qnames are dpx:{chrom}:{start}-{end}:NA.
    let starts: Vec<u32> = sam_a
        .lines()
        .filter(|l| l.starts_with("dpx:"))
        .map(|l| {
            l.split(':')
                .nth(2)
                .unwrap()
                .split('-')
                .next()
                .unwrap()
                .parse::<u32>()
                .unwrap()
        })
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted, "families must emit in ascending span order");
}

// ---- #1104 default-mode regression --------------------------------------------------

/// V1b: with the flag absent, output is byte-identical to the step-0 baseline and no
/// simplex BAM appears. The mx tag must NOT be written on the default path.
#[test]
fn default_mode_output_is_unchanged_and_untagged() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 3;
    write_genome(tmp.path(), n);
    let mut recs = String::new();
    for i in 0..n {
        recs.push_str(&pair(
            &format!("ot{i}"),
            i,
            true,
            &seq_ot_methylated(),
            60,
            60,
        ));
        recs.push_str(&pair(&format!("ob{i}"), i, false, WINDOW, 60, 60));
    }
    // A simplex family too: present in the input, ignored by the default mode.
    recs.push_str(&pair("lonely", 0, true, &seq_ot_methylated(), 60, 60));
    let bam = make_bam(tmp.path(), "mixed", n, &recs);

    let out = tempfile::tempdir().unwrap();
    let stderr = run_consensus(tmp.path(), out.path(), &bam, &[]);
    let sam = sam_text(&out.path().join("five_base_consensus.bam"));
    assert!(
        !out.path().join("five_base_simplex.bam").exists(),
        "duplex mode must not create a simplex BAM"
    );
    assert!(
        !sam.contains("mx:i:"),
        "the default path must write no mx tag; got:\n{sam}"
    );
    assert!(
        !sam.contains("spx:"),
        "duplex mode must emit no simplex records"
    );
    assert!(
        stderr.contains("5-Base duplex consensus (PE):") && !stderr.contains("simplex"),
        "default stderr must be the duplex line only; got:\n{stderr}"
    );
}

// ---- #1104 simplex emission ---------------------------------------------------------

/// V3: one record per simplex family, on the molecule's own strand, with the exact XM
/// each orientation must carry — the OT-only family calls the `+` CpG (window offset 6)
/// and the OB-only family the `-` CpG (offset 7).
#[test]
fn both_mode_emits_one_own_strand_record_per_simplex_family() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 3;
    write_genome(tmp.path(), n);
    let mut recs = String::new();
    // Window 0: a duplex family (both strands).
    recs.push_str(&pair("dup_ot", 0, true, &seq_ot_methylated(), 60, 60));
    recs.push_str(&pair("dup_ob", 0, false, WINDOW, 60, 60));
    // Window 1: OT-only (methylated `+` CpG). Window 2: OB-only (unmethylated).
    recs.push_str(&pair("spx_ot", 1, true, &seq_ot_methylated(), 60, 60));
    recs.push_str(&pair("spx_ob", 2, false, WINDOW, 60, 60));
    let bam = make_bam(tmp.path(), "mixed", n, &recs);

    let out = tempfile::tempdir().unwrap();
    let stderr = run_consensus(
        tmp.path(),
        out.path(),
        &bam,
        &["--five_base_emit_multiplicity", "both"],
    );
    let spx = sam_text(&out.path().join("five_base_simplex.bam"));
    let dpx = sam_text(&out.path().join("five_base_consensus.bam"));

    let spx_recs: Vec<&str> = spx.lines().filter(|l| !l.starts_with('@')).collect();
    assert_eq!(
        spx_recs.len(),
        2,
        "exactly one record per simplex family; got:\n{spx}"
    );
    assert!(
        spx_recs.iter().all(|l| l.starts_with("spx:")),
        "simplex records carry the spx: prefix; got:\n{spx}"
    );
    assert!(
        !spx.contains("dpx:"),
        "no duplex record may leak into the simplex BAM"
    );
    assert!(
        spx_recs.iter().all(|l| l.contains("mx:i:1")),
        "every simplex record is tagged mx:i:1; got:\n{spx}"
    );
    // The count first: `.all()` holds vacuously on a header-only BAM, so without this
    // a `both` run that dropped every duplex family would still pass the tag check.
    let dpx_recs: Vec<&str> = dpx.lines().filter(|l| !l.starts_with('@')).collect();
    assert_eq!(
        dpx_recs.len(),
        2,
        "the window-0 duplex family emits both records in `both` mode; got:\n{dpx}"
    );
    assert!(
        dpx_recs.iter().all(|l| l.contains("mx:i:2")),
        "every duplex record is tagged mx:i:2 in a non-default mode; got:\n{dpx}"
    );

    // Own strand only: the OT family emits FLAG 0, the OB family FLAG 16.
    let flags: Vec<&str> = spx_recs
        .iter()
        .map(|l| l.split('\t').nth(1).unwrap())
        .collect();
    assert!(
        flags.contains(&"0") && flags.contains(&"16"),
        "one forward (OT) and one reverse (OB) record; got {flags:?}"
    );

    // Exact XM per orientation. The 20-bp window's only CpG sits at offsets 6/7.
    let xm_of = |flag: &str| -> String {
        spx_recs
            .iter()
            .find(|l| l.split('\t').nth(1) == Some(flag))
            .unwrap()
            .split('\t')
            .find(|f| f.starts_with("XM:Z:"))
            .unwrap()
            .trim_start_matches("XM:Z:")
            .to_string()
    };
    assert_eq!(
        xm_of("0"),
        "......Z.............",
        "OT-only: methylated `+` CpG at offset 6, everything else no-call"
    );
    assert_eq!(
        xm_of("16"),
        ".......z............",
        "OB-only: the `-` CpG cytosine at offset 7 only — never a `+`-strand call"
    );

    assert!(
        stderr.contains("5-Base simplex consensus (PE): 2 consensus read(s) emitted"),
        "simplex report line missing; got:\n{stderr}"
    );
    // `both` mode prints its own duplex line, so the simplex line must not repeat the figure.
    assert!(
        !stderr.contains("duplex family(ies) not written in this mode"),
        "the excluded-duplex clause belongs to simplex-only mode; got:\n{stderr}"
    );
}

/// V5: `--five_base_min_mapq` filters per record, so one mate of a pair can be dropped
/// while the other survives — a 1-read family. It must still emit, and bucket as 1 read
/// (the reason the histogram counts reads, not fragments).
#[test]
fn mapq_orphaned_mate_forms_a_one_read_family() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 2;
    write_genome(tmp.path(), n);
    // R1 passes at MAPQ 60, R2 is dropped at MAPQ 5 by --five_base_min_mapq 20.
    let recs = pair("orphan", 0, true, &seq_ot_methylated(), 60, 5);
    let bam = make_bam(tmp.path(), "orphan", n, &recs);

    let out = tempfile::tempdir().unwrap();
    let stderr = run_consensus(
        tmp.path(),
        out.path(),
        &bam,
        &[
            "--five_base_emit_multiplicity",
            "simplex",
            "--five_base_min_mapq",
            "20",
        ],
    );
    let spx = sam_text(&out.path().join("five_base_simplex.bam"));
    let recs: Vec<&str> = spx.lines().filter(|l| !l.starts_with('@')).collect();
    assert_eq!(
        recs.len(),
        1,
        "the surviving mate alone must still collapse and emit; got:\n{spx}"
    );
    // The surviving mate is R1 of an OT molecule, so the call is the `+` CpG at
    // window offset 6 — a one-read family calls exactly as a two-read one does.
    assert_eq!(
        recs[0]
            .split('\t')
            .find(|f| f.starts_with("XM:Z:"))
            .unwrap()
            .trim_start_matches("XM:Z:"),
        "......Z.............",
        "orphan record must still carry the methylated `+` CpG call; got:\n{spx}"
    );
    assert!(
        stderr.contains("reads per family 1:1 2:0"),
        "a MAPQ-orphaned family holds ONE read, so it buckets at 1; got:\n{stderr}"
    );
}

/// V6: a lone proper pair is a 2-read simplex family.
#[test]
fn single_fragment_simplex_family_is_emitted_and_bucketed_at_two() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 2;
    write_genome(tmp.path(), n);
    let bam = make_bam(
        tmp.path(),
        "lone",
        n,
        &pair("lone", 0, true, &seq_ot_methylated(), 60, 60),
    );

    let out = tempfile::tempdir().unwrap();
    let stderr = run_consensus(
        tmp.path(),
        out.path(),
        &bam,
        &["--five_base_emit_multiplicity", "both"],
    );
    assert_eq!(
        sam_text(&out.path().join("five_base_simplex.bam"))
            .lines()
            .filter(|l| !l.starts_with('@'))
            .count(),
        1
    );
    assert!(
        stderr.contains("reads per family 1:0 2:1"),
        "one pair = 2 reads; got:\n{stderr}"
    );
    // `both` with no duplex family still leaves a valid header-only duplex BAM.
    let dpx = out.path().join("five_base_consensus.bam");
    assert!(dpx.exists(), "both mode must create the duplex BAM");
    assert_eq!(
        sam_text(&dpx)
            .lines()
            .filter(|l| !l.starts_with('@'))
            .count(),
        0,
        "no duplex families here, so the duplex BAM is header-only"
    );
}

/// The histogram's upper buckets and its `>=5` clamp: every other fixture here has
/// 1- or 2-read families, so a mis-clamp would go unseen. Window 0 gets three OT pairs
/// (6 reads, one family — same span, no UMI) and window 1 a pair plus a lone mate
/// (3 reads).
#[test]
fn histogram_buckets_deeper_families_and_clamps_at_five() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 2;
    write_genome(tmp.path(), n);
    let seq = seq_ot_methylated();
    let mut recs = String::new();
    // Window 0: 3 pairs = 6 reads on one family -> the >=5 bucket.
    for r in 0..3 {
        recs.push_str(&pair(&format!("deep{r}"), 0, true, &seq, 60, 60));
    }
    // Window 1: one pair + one lone mate = 3 reads -> the 3 bucket.
    recs.push_str(&pair("three_a", 1, true, &seq, 60, 60));
    recs.push_str(&format!(
        "{}\n",
        pair("three_b", 1, true, &seq, 60, 60)
            .lines()
            .next()
            .unwrap()
    ));
    let bam = make_bam(tmp.path(), "deep", n, &recs);

    let out = tempfile::tempdir().unwrap();
    let stderr = run_consensus(
        tmp.path(),
        out.path(),
        &bam,
        &["--five_base_emit_multiplicity", "simplex"],
    );
    assert_eq!(
        sam_text(&out.path().join("five_base_simplex.bam"))
            .lines()
            .filter(|l| !l.starts_with('@'))
            .count(),
        2,
        "two families, one record each"
    );
    assert!(
        stderr.contains("reads per family 1:0 2:0 3:1 4:0 >=5:1"),
        "a 3-read family buckets at 3 and a 6-read family clamps into >=5; got:\n{stderr}"
    );
    assert!(
        stderr.contains("of 2 single-strand family(ies)"),
        "both families must be counted; got:\n{stderr}"
    );
}

/// V7: `simplex` mode writes ONLY the simplex BAM (DRAGEN emit-multiplicity semantics),
/// and the count identity emitted+skipped == single-strand families holds.
#[test]
fn simplex_mode_writes_only_the_simplex_bam() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 4;
    write_genome(tmp.path(), n);
    let mut recs = String::new();
    recs.push_str(&pair("dup_ot", 0, true, &seq_ot_methylated(), 60, 60));
    recs.push_str(&pair("dup_ob", 0, false, WINDOW, 60, 60));
    for i in 1..n {
        recs.push_str(&pair(
            &format!("s{i}"),
            i,
            true,
            &seq_ot_methylated(),
            60,
            60,
        ));
    }
    let bam = make_bam(tmp.path(), "mixed", n, &recs);

    let out = tempfile::tempdir().unwrap();
    let stderr = run_consensus(
        tmp.path(),
        out.path(),
        &bam,
        &["--five_base_emit_multiplicity", "simplex"],
    );
    assert!(
        !out.path().join("five_base_consensus.bam").exists(),
        "simplex mode writes no duplex BAM"
    );
    let spx = sam_text(&out.path().join("five_base_simplex.bam"));
    assert_eq!(
        spx.lines().filter(|l| !l.starts_with('@')).count(),
        3,
        "three single-strand families; got:\n{spx}"
    );
    assert!(
        stderr.contains("3 consensus read(s) emitted, 0 family(ies) skipped, of 3 single-strand"),
        "emitted + skipped must equal the single-strand family count; got:\n{stderr}"
    );
    assert!(
        !stderr.contains("5-Base duplex consensus (PE):"),
        "no duplex report line when no duplex BAM is written; got:\n{stderr}"
    );
    // The fixture's dup_ot/dup_ob pair is the one duplex family, and no duplex line
    // reports it in this mode, so the simplex line must.
    assert!(
        stderr.contains("; 1 duplex family(ies) not written in this mode"),
        "simplex-only mode must name the duplex families it excluded; got:\n{stderr}"
    );
    // A non-default mode names itself in the standalone Note: line (the default-mode
    // wording is frozen, asserted in default_mode_output_is_unchanged_and_untagged).
    assert!(
        stderr.contains("emit multiplicity: simplex"),
        "a non-default mode must name itself in the Note: line; got:\n{stderr}"
    );
}

// ---- #1104 CLI guards ---------------------------------------------------------------

/// V8: the flag is meaningless without a consensus run, and must not be silently
/// ignored by the bisulfite standalone (which dispatches before resolve()).
#[test]
fn rejects_emit_multiplicity_without_a_consensus_run() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(bismark_bin())
        .arg("--illumina_5base")
        .arg("--five_base_emit_multiplicity")
        .arg("both")
        .arg("--genome")
        .arg(tmp.path())
        .arg("-1")
        .arg("r1.fq")
        .arg("-2")
        .arg("r2.fq")
        .arg("--output_dir")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "should refuse without a consensus run"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--five_base_emit_multiplicity requires --five_base_consensus"),
        "message must name both flags; got:\n{err}"
    );
}

#[test]
fn rejects_emit_multiplicity_with_the_bisulfite_standalone() {
    let tmp = tempfile::tempdir().unwrap();
    let dummy = tmp.path().join("in.bam");
    std::fs::write(&dummy, b"").unwrap();
    let out = Command::new(bismark_bin())
        .arg("--illumina_5base")
        .arg("--five_base_bisulfite_bam")
        .arg(&dummy)
        .arg("--five_base_emit_multiplicity")
        .arg("simplex")
        .arg("--output_dir")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "the bisulfite standalone must reject the flag, not ignore it"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--five_base_emit_multiplicity has no effect"),
        "message must say the flag does not apply; got:\n{err}"
    );
}

/// The reason a simplex family emits its OWN strand only. This fixture's window
/// carries a non-CpG `G` (offset 12, not preceded by `C`), which reconciles as a
/// plain position: an OT-only family's consensus passes the TOP-strand `G` through
/// there. A reverse (GA-call) record built from it would report a bottom-strand
/// call at that column for a strand that was never sequenced — so no such record
/// is emitted, and no simplex record may carry a call there.
#[test]
fn simplex_never_calls_a_strand_it_did_not_sequence() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    // Window with the CpG at 6/7 AND a standalone G at offset 12.
    const W: &str = "AATTAACGTTAAGTAATTAA";
    let tmp = tempfile::tempdir().unwrap();
    let genome = format!("{LEAD}{W}TT");
    std::fs::write(tmp.path().join("chrT.fa"), format!(">chrT\n{genome}\n")).unwrap();

    let mut meth = W.to_string();
    meth.replace_range(6..7, "T"); // 5mC -> T at the CpG
    let pos = LEAD.len() + 1;
    let dots = ".".repeat(W.len());
    let mut recs = String::new();
    for (flag, tlen, xr) in [(99, 20, "CT"), (147, -20, "GA")] {
        recs.push_str(&format!(
            "otonly\t{flag}\tchrT\t{pos}\t60\t20M\t=\t{pos}\t{tlen}\t{meth}\t\
             IIIIIIIIIIIIIIIIIIII\tXM:Z:{dots}\tXR:Z:{xr}\tXG:Z:CT\n"
        ));
    }
    let sam = format!("@HD\tVN:1.6\n@SQ\tSN:chrT\tLN:{}\n{recs}", genome.len());
    std::fs::write(tmp.path().join("g.sam"), sam).unwrap();
    let bam = tmp.path().join("g.bam");
    assert!(
        Command::new("samtools")
            .args([
                "view",
                "-b",
                "-o",
                &bam.to_string_lossy(),
                &tmp.path().join("g.sam").to_string_lossy()
            ])
            .output()
            .unwrap()
            .status
            .success()
    );

    let out = tempfile::tempdir().unwrap();
    run_consensus(
        tmp.path(),
        out.path(),
        &bam,
        &["--five_base_emit_multiplicity", "simplex"],
    );
    let spx = sam_text(&out.path().join("five_base_simplex.bam"));
    let recs: Vec<&str> = spx.lines().filter(|l| !l.starts_with('@')).collect();
    assert_eq!(
        recs.len(),
        1,
        "OT-only family emits one record; got:\n{spx}"
    );
    assert_eq!(
        recs[0].split('\t').nth(1),
        Some("0"),
        "an OT molecule emits the forward record"
    );
    // No simplex record may carry a call at the non-CpG G column (offset 12).
    for r in &recs {
        let xm = r
            .split('\t')
            .find(|f| f.starts_with("XM:Z:"))
            .unwrap()
            .trim_start_matches("XM:Z:")
            .as_bytes();
        assert_eq!(
            xm[12],
            b'.',
            "offset 12 is the opposite strand's cytosine — it was never sequenced, so it \
             must be a no-call; got XM {}",
            String::from_utf8_lossy(xm)
        );
    }
}
