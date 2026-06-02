//! Perl-oracle byte-identity gate (the primary acceptance test).
//!
//! For each fixture shape, build a report set in a temp dir, run BOTH the
//! Perl `bismark2summary` (v0.25.1) and the Rust `bismark2summary_rs`, and
//! assert:
//! - `.txt` is **byte-for-byte identical**;
//! - `.html` is **byte-identical after normalizing the single timestamp
//!   line** (Perl `localtime` cannot be pinned; the gate normalizes it and
//!   asserts exactly one occurrence in each file).
//!
//! Auto-skips (prints a notice, passes) when `perl`, the Perl source, or the
//! `plotly/` assets are unavailable — mirroring the genomeprep/c2c oracle
//! pattern so the suite stays green on Perl-less runners.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

fn perl_script() -> Option<PathBuf> {
    // crate dir = .../rust/bismark-summary ; Perl script = repo-root/bismark2summary
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bismark2summary");
    let plotly = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plotly/plot.ly");
    if !p.exists() || !plotly.exists() {
        return None;
    }
    // `perl` must be runnable.
    let ok = Command::new("perl")
        .arg("-e")
        .arg("1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok.then(|| p.canonicalize().unwrap())
}

/// Normalize the single `Report generated on <ctime></p>` timestamp line to a
/// fixed token. Asserts exactly one occurrence (so a stray timestamp-shaped
/// string elsewhere can't mask a real divergence).
fn normalize_timestamp(html: &str) -> String {
    let anchor = "Report generated on ";
    assert_eq!(
        html.matches(anchor).count(),
        1,
        "expected exactly one timestamp line"
    );
    let start = html.find(anchor).unwrap();
    let after = start + anchor.len();
    let end = after + html[after..].find("</p>").expect("timestamp line has </p>");
    format!("{}__TS__{}", &html[..after], &html[end..])
}

fn run_perl(dir: &Path, script: &Path, basename: &str) -> std::process::ExitStatus {
    Command::new("perl")
        .arg(script)
        .args(["-o", basename, "--title", "Oracle"])
        .current_dir(dir)
        .status()
        .expect("run perl bismark2summary")
}

fn run_rust(dir: &Path, basename: &str) -> std::process::ExitStatus {
    Command::new(env!("CARGO_BIN_EXE_bismark2summary_rs"))
        .args([
            "-o",
            basename,
            "--title",
            "Oracle",
            "--__test_timestamp",
            "1780272000",
        ])
        .current_dir(dir)
        .status()
        .expect("run bismark2summary_rs")
}

/// Run both tools in `dir` and assert `.txt` + `.html` byte-identity.
fn assert_identical(dir: &Path, script: &Path) {
    run_perl(dir, script, "perl_out");
    let rust_status = run_rust(dir, "rust_out");
    assert!(
        rust_status.success(),
        "rust exited non-zero: {rust_status:?}"
    );

    let p_txt = std::fs::read(dir.join("perl_out.txt")).unwrap();
    let r_txt = std::fs::read(dir.join("rust_out.txt")).unwrap();
    assert_eq!(p_txt, r_txt, ".txt differs");

    let p_html = std::fs::read_to_string(dir.join("perl_out.html")).unwrap();
    let r_html = std::fs::read_to_string(dir.join("rust_out.html")).unwrap();
    assert_eq!(
        normalize_timestamp(&p_html),
        normalize_timestamp(&r_html),
        ".html differs (modulo timestamp)"
    );
}

#[test]
fn oracle_wgbs_two_sample() {
    let Some(script) = perl_script() else {
        eprintln!("skipping: perl / source / plotly assets unavailable");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    common::build_wgbs_two_sample(dir.path());
    assert_identical(dir.path(), &script);
}

#[test]
fn oracle_all_rrbs_raw_mode() {
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // 2 SE RRBS samples, no dedup → raw mode in both numbers and percentages.
    common::bam(d, "r1_bismark_bt2.bam");
    common::se_alignment(d, "r1_bismark_bt2_SE_report.txt", 5000, 4000, 800, 200, 0);
    common::splitting(
        d,
        "r1_bismark_bt2_splitting_report.txt",
        250000,
        5000,
        500,
        1000,
        45000,
        24500,
        174000,
    );
    common::bam(d, "r2_bismark_bt2.bam");
    common::se_alignment(d, "r2_bismark_bt2_SE_report.txt", 6000, 5000, 700, 300, 0);
    common::splitting(
        d,
        "r2_bismark_bt2_splitting_report.txt",
        300000,
        6000,
        600,
        1200,
        54000,
        29400,
        208800,
    );
    assert_identical(d, &script);
}

#[test]
fn oracle_single_rrbs_section_asymmetry() {
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // ONE RRBS sample: numbers section takes the DEDUP layout while the
    // percentage section takes the RAW layout (§2.9 ⚠ box / Reviewer A C2).
    common::bam(d, "r1_bismark_bt2.bam");
    common::se_alignment(d, "r1_bismark_bt2_SE_report.txt", 5000, 4000, 800, 200, 3);
    common::splitting(
        d,
        "r1_bismark_bt2_splitting_report.txt",
        250000,
        5000,
        500,
        1000,
        45000,
        24500,
        174000,
    );
    assert_identical(d, &script);
}

#[test]
fn oracle_plot_excluded_sample() {
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // g1 plots; g2 has zero CHH calls → excluded from the graphs but present
    // in the .txt (exercises the num_samples-vs-plotted x-array mismatch).
    common::bam(d, "g1_bismark_bt2.bam");
    common::se_alignment(d, "g1_bismark_bt2_SE_report.txt", 5000, 4000, 800, 200, 0);
    common::dedup(
        d,
        "g1_bismark_bt2.deduplication_report.txt",
        "g1_bismark_bt2.bam",
        4000,
        1000,
        3000,
    );
    common::splitting(
        d,
        "g1_bismark_bt2.deduplicated_splitting_report.txt",
        250000,
        5000,
        500,
        1000,
        45000,
        24500,
        174000,
    );
    common::bam(d, "g2_bismark_bt2.bam");
    common::se_alignment(d, "g2_bismark_bt2_SE_report.txt", 4000, 3000, 700, 300, 0);
    common::dedup(
        d,
        "g2_bismark_bt2.deduplication_report.txt",
        "g2_bismark_bt2.bam",
        3000,
        800,
        2200,
    );
    common::splitting(
        d,
        "g2_bismark_bt2.deduplicated_splitting_report.txt",
        200000,
        4000,
        400,
        0, // zero methylated CHH …
        36000,
        19600,
        0, // … and zero unmethylated CHH → excluded from plots
    );
    assert_identical(d, &script);
}

#[test]
fn oracle_mixed_types_die_writes_txt_not_html() {
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // One WGBS (dedup) + one RRBS (raw) → both tools die during the alignment
    // percentage step, AFTER writing the .txt, BEFORE the .html.
    common::bam(d, "w1_bismark_bt2.bam");
    common::se_alignment(d, "w1_bismark_bt2_SE_report.txt", 5000, 4000, 800, 200, 0);
    common::dedup(
        d,
        "w1_bismark_bt2.deduplication_report.txt",
        "w1_bismark_bt2.bam",
        4000,
        1000,
        3000,
    );
    common::splitting(
        d,
        "w1_bismark_bt2.deduplicated_splitting_report.txt",
        250000,
        5000,
        500,
        1000,
        45000,
        24500,
        174000,
    );
    common::bam(d, "r1_bismark_bt2.bam");
    common::se_alignment(d, "r1_bismark_bt2_SE_report.txt", 6000, 5000, 700, 300, 0);
    common::splitting(
        d,
        "r1_bismark_bt2_splitting_report.txt",
        300000,
        6000,
        600,
        1200,
        54000,
        29400,
        208800,
    );

    let perl_status = run_perl(d, &script, "perl_out");
    let rust_status = run_rust(d, "rust_out");
    assert!(!perl_status.success(), "perl should die on mixed types");
    assert!(!rust_status.success(), "rust should error on mixed types");

    // Both wrote the .txt before dying; neither wrote the .html.
    let p_txt = std::fs::read(d.join("perl_out.txt")).unwrap();
    let r_txt = std::fs::read(d.join("rust_out.txt")).unwrap();
    assert_eq!(p_txt, r_txt, ".txt differs in the die case");
    assert!(!d.join("perl_out.html").exists(), "perl wrote html on die");
    assert!(!d.join("rust_out.html").exists(), "rust wrote html on die");
}

#[test]
fn oracle_mixed_case_glob_row_order() {
    // SPEC §7.8 (mandatory): auto-glob with mixed-case basenames. Perl's
    // case-folded glob order is apple, Mango, zebra; a bytewise `.sort()`
    // would emit Mango, apple, zebra and DIVERGE. The Perl-vs-Rust cmp here
    // catches a bytewise regression end-to-end.
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    common::wgbs_se_sample(d, "Mango_R1");
    common::wgbs_se_sample(d, "apple_R1");
    common::wgbs_se_sample(d, "zebra_R1");
    assert_identical(d, &script);

    // Explicit row-order assertion (guards even if Perl ever changed).
    let rust_txt = std::fs::read_to_string(d.join("rust_out.txt")).unwrap();
    let order: Vec<&str> = rust_txt
        .lines()
        .skip(1)
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(
        order,
        vec![
            "apple_R1_bismark_bt2.bam",
            "Mango_R1_bismark_bt2.bam",
            "zebra_R1_bismark_bt2.bam"
        ],
        "case-folded glob row order"
    );
}

#[test]
fn oracle_nontrivial_g15_tail() {
    // SPEC §7.9: a CpG meth% of 99.99 → unmeth = 100 - 99.99 = the FP-artifact
    // %.15g string "0.0100000000000051". Exercises the asymmetric unmeth
    // formatting at the integration level (single WGBS SE sample, dedup mode).
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    common::bam(d, "g15_bismark_bt2.bam");
    common::se_alignment(d, "g15_bismark_bt2_SE_report.txt", 5000, 4000, 800, 200, 0);
    common::dedup(
        d,
        "g15_bismark_bt2.deduplication_report.txt",
        "g15_bismark_bt2.bam",
        4000,
        1000,
        3000,
    );
    // CpG: 9999 meth / 1 unmeth → 99.99% → unmeth 0.0100000000000051.
    common::splitting(
        d,
        "g15_bismark_bt2.deduplicated_splitting_report.txt",
        210000,
        9999,
        450,
        900,
        1,
        20000,
        150000,
    );
    assert_identical(d, &script);
    let html = std::fs::read_to_string(d.join("rust_out.html")).unwrap();
    assert!(
        html.contains("0.0100000000000051"),
        "expected the %.15g unmeth tail in the rendered HTML"
    );
}

#[test]
fn oracle_plot_excluded_in_middle() {
    // SPEC §7.6: the EXCLUDED sample sits in the middle of the sorted order
    // (aaa, mmm, zzz; mmm has zero CHH). num_samples/x-values = 3 (total),
    // categories/y-arrays = 2 (plotted) — the deliberate length mismatch.
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    common::wgbs_se_sample(d, "aaa");
    common::wgbs_se_sample(d, "zzz");
    // mmm: zero CHH calls → excluded from plots, present in .txt.
    common::bam(d, "mmm_bismark_bt2.bam");
    common::se_alignment(d, "mmm_bismark_bt2_SE_report.txt", 4000, 3000, 700, 300, 0);
    common::dedup(
        d,
        "mmm_bismark_bt2.deduplication_report.txt",
        "mmm_bismark_bt2.bam",
        3000,
        800,
        2200,
    );
    common::splitting(
        d,
        "mmm_bismark_bt2.deduplicated_splitting_report.txt",
        200000,
        4000,
        400,
        0,
        36000,
        19600,
        0,
    );
    assert_identical(d, &script);
}

#[test]
fn oracle_single_wgbs() {
    // SPEC §7.4: one WGBS sample → consistent dedup-mode layout in both
    // numbers and percentages (no all-commas, no die).
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    common::wgbs_se_sample(dir.path(), "only");
    assert_identical(dir.path(), &script);
}

#[test]
fn oracle_all_excluded_zero_plotted() {
    // SPEC §7.7: every sample missing a context → zero plotted samples; empty
    // joins → `^,{1,}$` false → dedup `else` branch; percentage loops empty.
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    for prefix in ["x1", "x2"] {
        common::bam(d, &format!("{prefix}_bismark_bt2.bam"));
        common::se_alignment(
            d,
            &format!("{prefix}_bismark_bt2_SE_report.txt"),
            4000,
            3000,
            700,
            300,
            0,
        );
        common::dedup(
            d,
            &format!("{prefix}_bismark_bt2.deduplication_report.txt"),
            &format!("{prefix}_bismark_bt2.bam"),
            3000,
            800,
            2200,
        );
        // zero CHH → excluded
        common::splitting(
            d,
            &format!("{prefix}_bismark_bt2.deduplicated_splitting_report.txt"),
            200000,
            4000,
            400,
            0,
            36000,
            19600,
            0,
        );
    }
    assert_identical(d, &script);
}

#[test]
fn oracle_explicit_argv_order() {
    // SPEC §7.10: explicit BAM args are used in argv order (NOT glob order).
    // Pass z before a; both tools must emit rows z, a.
    let Some(script) = perl_script() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    common::wgbs_se_sample(d, "a");
    common::wgbs_se_sample(d, "z");

    let bams = ["z_bismark_bt2.bam", "a_bismark_bt2.bam"];
    let perl_ok = Command::new("perl")
        .arg(&script)
        .args(["-o", "perl_out", "--title", "Oracle"])
        .args(bams)
        .current_dir(d)
        .status()
        .unwrap()
        .success();
    let rust_ok = Command::new(env!("CARGO_BIN_EXE_bismark2summary_rs"))
        .args([
            "-o",
            "rust_out",
            "--title",
            "Oracle",
            "--__test_timestamp",
            "1780272000",
        ])
        .args(bams)
        .current_dir(d)
        .status()
        .unwrap()
        .success();
    assert!(perl_ok && rust_ok);

    let p_txt = std::fs::read(d.join("perl_out.txt")).unwrap();
    let r_txt = std::fs::read(d.join("rust_out.txt")).unwrap();
    assert_eq!(p_txt, r_txt, "argv-order .txt differs");

    let r = String::from_utf8(r_txt).unwrap();
    let first_col: Vec<&str> = r
        .lines()
        .skip(1)
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(
        first_col,
        vec!["z_bismark_bt2.bam", "a_bismark_bt2.bam"],
        "argv order must win over glob sort"
    );
}

#[test]
fn oracle_basename_zero_truthiness() {
    // SPEC §7.10 / §2.2: `-o 0` is falsy in Perl → default basename. Verify
    // BOTH tools write `bismark_summary_report.txt` (not `0.txt`) and agree.
    let Some(script) = perl_script() else {
        return;
    };
    let pdir = tempfile::tempdir().unwrap();
    let rdir = tempfile::tempdir().unwrap();
    common::wgbs_se_sample(pdir.path(), "s");
    common::wgbs_se_sample(rdir.path(), "s");

    Command::new("perl")
        .arg(&script)
        .args(["-o", "0"])
        .current_dir(pdir.path())
        .status()
        .unwrap();
    Command::new(env!("CARGO_BIN_EXE_bismark2summary_rs"))
        .args(["-o", "0", "--__test_timestamp", "1780272000"])
        .current_dir(rdir.path())
        .status()
        .unwrap();

    assert!(
        pdir.path().join("bismark_summary_report.txt").exists(),
        "perl should default the basename on `-o 0`"
    );
    assert!(
        !pdir.path().join("0.txt").exists(),
        "perl must NOT write 0.txt"
    );
    assert!(
        rdir.path().join("bismark_summary_report.txt").exists(),
        "rust should default the basename on `-o 0`"
    );
    assert!(
        !rdir.path().join("0.txt").exists(),
        "rust must NOT write 0.txt"
    );

    let p = std::fs::read(pdir.path().join("bismark_summary_report.txt")).unwrap();
    let r = std::fs::read(rdir.path().join("bismark_summary_report.txt")).unwrap();
    assert_eq!(p, r, "`-o 0` default-basename .txt differs");
}
