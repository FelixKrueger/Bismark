//! Phase D unit tests — M-bias.txt writer, filename derivation,
//! MbiasTable::max_position, SE-vs-PE section ordering, finalize ordering.
//!
//! Test names mirror plan §7.1's labels. End-to-end smoke that runs the
//! binary on real BAMs lives at `tests/mbias_writer_phase_d_smoke.rs`.

#![allow(non_snake_case)]

use std::fs;
use std::path::Path;

use bismark::extractor::call::CytosineContext;
use bismark::extractor::mbias::{MbiasPos, MbiasTable};
use bismark::extractor::mbias_writer::{derive_mbias_basename, mbias_txt_path, write_mbias_txt};
use bismark::extractor::pipeline::derive_basename;

// ─────────────────────────────────────────────────────────────────────────
// 1. Filename derivation
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn derive_mbias_basename_strips_known_suffixes() {
    // Per Perl `bismark_methylation_extractor:632-642` — strips
    // `gz`/`sam`/`bam`/`cram`/`txt` in that order, each exactly once,
    // WITHOUT the leading dot (preserves trailing `.`).
    assert_eq!(derive_mbias_basename(Path::new("sample.bam")), "sample.");
    assert_eq!(derive_mbias_basename(Path::new("sample.sam")), "sample.");
    assert_eq!(derive_mbias_basename(Path::new("sample.cram")), "sample.");
    // Rev 1 (Reviewer A I1): `.txt` input.
    assert_eq!(derive_mbias_basename(Path::new("sample.txt")), "sample.");
    // Rev 1 (Reviewer A I2): `.bam.gz` — `gz$` strips first leaving
    // `sample.bam.`, then subsequent `bam$` doesn't match the new tail `.`.
    assert_eq!(
        derive_mbias_basename(Path::new("sample.bam.gz")),
        "sample.bam."
    );
    assert_eq!(
        derive_mbias_basename(Path::new("sample.sam.gz")),
        "sample.sam."
    );
    // No extension: regex chain is no-op.
    assert_eq!(derive_mbias_basename(Path::new("sample")), "sample");
    // Path components stripped.
    assert_eq!(
        derive_mbias_basename(Path::new("/abs/path/sample.bam")),
        "sample."
    );
    // Rev 2 (Reviewer B Low): trailing-dot stop semantic. After `bam`
    // strips, the next attempt sees a trailing `.` and skips — does NOT
    // continue stripping. Matches Perl's `s/X$//` requiring literal X.
    assert_eq!(
        derive_mbias_basename(Path::new("foo.txt.bam")),
        "foo.txt.",
        "after bam strip, txt attempt should NOT match the trailing dot"
    );
    assert_eq!(
        derive_mbias_basename(Path::new("foo.bam.txt")),
        "foo.bam.",
        "after txt strip, bam attempt should NOT match the trailing dot"
    );
}

#[test]
fn derive_basename_vs_derive_mbias_basename_lock_divergence() {
    // Rev 1 (Reviewer A I3 / Reviewer B O5): explicit side-by-side fixture
    // so a future maintainer can't accidentally swap these two helpers.
    // Phase B's derive_basename strips with the dot ("sample"); Phase D's
    // derive_mbias_basename strips without the dot ("sample.").
    for &input in &["sample.bam", "sample.sam", "sample.cram"] {
        let phase_b = derive_basename(Path::new(input));
        let phase_d = derive_mbias_basename(Path::new(input));
        assert_eq!(phase_b, "sample", "phase B basename for {input}");
        assert_eq!(phase_d, "sample.", "phase D mbias basename for {input}");
        // Critically: they must differ for these inputs.
        assert_ne!(
            phase_b, phase_d,
            "divergence not detected for {input}: phase_b={phase_b}, phase_d={phase_d}"
        );
    }
    // For .bam.gz: Phase B doesn't strip (single-suffix); Phase D strips
    // the .gz layer leaving sample.bam.
    let input = "sample.bam.gz";
    let phase_b = derive_basename(Path::new(input));
    let phase_d = derive_mbias_basename(Path::new(input));
    assert_eq!(phase_b, "sample.bam.gz");
    assert_eq!(phase_d, "sample.bam.");
    assert_ne!(phase_b, phase_d);
}

#[test]
fn mbias_txt_path_appends_to_basename_in_output_dir() {
    let out = Path::new("/tmp/x");
    let result = mbias_txt_path(out, Path::new("/abs/sample.bam"));
    assert_eq!(result, Path::new("/tmp/x/sample.M-bias.txt"));
}

#[test]
fn mbias_txt_path_no_extension_input() {
    // sample → sampleM-bias.txt (no dot between basename and M-bias.txt).
    let out = Path::new("/tmp");
    let result = mbias_txt_path(out, Path::new("/abs/sample"));
    assert_eq!(result, Path::new("/tmp/sampleM-bias.txt"));
}

// ─────────────────────────────────────────────────────────────────────────
// 2. MbiasTable::max_position
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn mbias_table_max_position_empty() {
    let t = MbiasTable::default();
    assert_eq!(t.max_position(), 0);
}

#[test]
fn mbias_table_max_position_single_context() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 100, true);
    assert_eq!(t.max_position(), 100);
}

#[test]
fn mbias_table_max_position_max_across_contexts() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 50, true);
    t.accumulate(CytosineContext::CHG, 100, true);
    t.accumulate(CytosineContext::CHH, 75, false);
    assert_eq!(t.max_position(), 100);
}

#[test]
fn mbias_table_max_position_only_slot_0_returns_zero() {
    // Rev 1 (Reviewer A I4): construct a table with cpg.len() == 1 and only
    // slot 0 allocated → max_position == 0. This case never arises in
    // production because Phase B/C's route_call always passes pos_1based >= 1
    // (now also debug_assert'd), but pins the writer's `1..=0` empty-loop
    // behaviour against regressions.
    let t = MbiasTable {
        cpg: vec![MbiasPos::default()], // length 1, slot 0 only
        chg: vec![],
        chh: vec![],
    };
    assert_eq!(t.max_position(), 0);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "position must be 1-based")]
fn mbias_accumulate_position_zero_debug_panics() {
    // Rev 1 (Reviewer A Optional / Reviewer B O2): debug_assert! catches the
    // slot-0-misuse case in debug builds. Release builds skip the test.
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 0, true);
}

// ─────────────────────────────────────────────────────────────────────────
// 3. write_mbias_txt — section header bytes
// ─────────────────────────────────────────────────────────────────────────

fn build_se_table_with_data() -> MbiasTable {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 1, true);
    t.accumulate(CytosineContext::CpG, 2, false);
    t.accumulate(CytosineContext::CHG, 1, true);
    t.accumulate(CytosineContext::CHH, 1, false);
    t
}

fn write_to_tempfile(mbias: &[MbiasTable; 2], is_paired: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.M-bias.txt");
    write_mbias_txt(&path, mbias, is_paired).unwrap();
    fs::read_to_string(&path).unwrap()
}

#[test]
fn write_mbias_txt_se_emits_3_sections() {
    let t = build_se_table_with_data();
    let content = write_to_tempfile(&[t, MbiasTable::default()], /*is_paired=*/ false);
    // 3 sections; no (R1)/(R2) suffixes.
    assert!(content.contains("CpG context\n===========\n"));
    assert!(content.contains("CHG context\n===========\n"));
    assert!(content.contains("CHH context\n===========\n"));
    assert!(!content.contains("(R1)"));
    assert!(!content.contains("(R2)"));
}

#[test]
fn write_mbias_txt_pe_emits_6_sections() {
    let t1 = build_se_table_with_data();
    let mut t2 = MbiasTable::default();
    t2.accumulate(CytosineContext::CpG, 1, true);
    let content = write_to_tempfile(&[t1, t2], /*is_paired=*/ true);
    // 6 sections: R1 first, then R2; both have 16-equals rules.
    assert!(content.contains("CpG context (R1)\n================\n"));
    assert!(content.contains("CHG context (R1)\n================\n"));
    assert!(content.contains("CHH context (R1)\n================\n"));
    assert!(content.contains("CpG context (R2)\n================\n"));
    assert!(content.contains("CHG context (R2)\n================\n"));
    assert!(content.contains("CHH context (R2)\n================\n"));
    // No SE-style "{ctx} context\n===========\n" lines.
    assert!(!content.contains("CpG context\n===========\n"));
}

#[test]
fn write_mbias_txt_se_section_header_format_bytes() {
    let t = build_se_table_with_data();
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    // Byte-exact: 11 equals exactly.
    assert!(
        content.starts_with("CpG context\n===========\n"),
        "got:\n{content}"
    );
}

#[test]
fn write_mbias_txt_pe_section_header_format_bytes() {
    let t1 = build_se_table_with_data();
    let mut t2 = MbiasTable::default();
    t2.accumulate(CytosineContext::CpG, 1, true);
    let content = write_to_tempfile(&[t1, t2], true);
    assert!(
        content.starts_with("CpG context (R1)\n================\n"),
        "got:\n{content}"
    );
    // R2 header — verify 16 equals byte-exact.
    assert!(content.contains("CpG context (R2)\n================\n"));
}

#[test]
fn write_mbias_txt_column_header_bytes_exact() {
    let t = build_se_table_with_data();
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    assert!(
        content
            .contains("position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n")
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 4. Per-position rows
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn write_mbias_txt_per_position_row_with_calls() {
    // Build a table where CpG slot 5 has meth=30, unmeth=70 → percent=30.00, coverage=100.
    let mut t = MbiasTable::default();
    for _ in 0..30 {
        t.accumulate(CytosineContext::CpG, 5, true);
    }
    for _ in 0..70 {
        t.accumulate(CytosineContext::CpG, 5, false);
    }
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    // Row at pos 5: "5\t30\t70\t30.00\t100\n"
    assert!(
        content.contains("5\t30\t70\t30.00\t100\n"),
        "got:\n{content}"
    );
}

#[test]
fn write_mbias_txt_per_position_row_zero_coverage_empty_percent() {
    // Build a table where CpG slot 5 has 0 calls but max_position is 10.
    // Row at pos 5 should be "5\t0\t0\t\t0\n" (note \t\t between unmeth and coverage).
    let mut t = MbiasTable::default();
    // Push max_position to 10 via CpG slot 10.
    t.accumulate(CytosineContext::CpG, 10, true);
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    // Pos 5 row: zero coverage → empty percent → literal \t\t.
    assert!(
        content.contains("5\t0\t0\t\t0\n"),
        "expected '5\\t0\\t0\\t\\t0\\n'; got:\n{content}"
    );
}

#[test]
fn write_mbias_txt_iterates_all_positions_up_to_max() {
    // Max position 10; CpG populated only at positions 3 and 7.
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 3, true);
    t.accumulate(CytosineContext::CpG, 7, false);
    t.accumulate(CytosineContext::CpG, 10, true); // pushes max_position
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    // Each of positions 1-10 must appear at start-of-line in the CpG section.
    for pos in 1..=10 {
        assert!(
            content.contains(&format!("\n{pos}\t")),
            "missing position {pos} row; content:\n{content}"
        );
    }
}

#[test]
fn write_mbias_txt_blank_line_between_sections() {
    let t = build_se_table_with_data();
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    // The CpG section ends with a row line + a blank line, then CHG section
    // starts. Search for "\n\nCHG" (blank line immediately before CHG).
    assert!(
        content.contains("\n\nCHG context"),
        "expected blank line before CHG section; got:\n{content}"
    );
}

#[test]
fn write_mbias_txt_empty_mbias_emits_headers_only() {
    let content = write_to_tempfile(&[MbiasTable::default(), MbiasTable::default()], false);
    // 3 SE sections with headers + column headers, no per-position rows.
    assert!(content.contains("CpG context\n===========\n"));
    assert!(content.contains("CHG context\n===========\n"));
    assert!(content.contains("CHH context\n===========\n"));
    // No "1\t" or "2\t" rows (no positions emitted when max_position == 0).
    assert!(!content.contains("\n1\t"));
    assert!(!content.contains("\n2\t"));
}

#[test]
fn write_mbias_txt_pe_empty_r2_section_still_emitted() {
    // R1 has data; R2 entirely empty. Output has 6 sections; R2 sections
    // headers + column headers only, no per-position rows.
    let t1 = build_se_table_with_data();
    let content = write_to_tempfile(&[t1, MbiasTable::default()], true);
    assert!(content.contains("CpG context (R2)\n================\n"));
    assert!(content.contains("CHG context (R2)\n================\n"));
    assert!(content.contains("CHH context (R2)\n================\n"));
    // R2 sections have column headers.
    let r2_pos = content.find("CpG context (R2)").unwrap();
    let r2_tail = &content[r2_pos..];
    assert!(r2_tail.contains("position\tcount methylated"));
    // R2 sections have no per-position rows (no "1\t" right after column header).
    // We do this by finding the R2 CpG section's column-header newline and
    // asserting the next char is '\n' (blank line) or 'C' (next section header).
    let r2_chg_pos = r2_tail.find("CHG context (R2)").unwrap();
    let r2_cpg_block = &r2_tail[..r2_chg_pos];
    // The CpG R2 block should not contain any per-position row like "\n1\t".
    assert!(!r2_cpg_block.contains("\n1\t"), "R2 CpG should be empty");
}

#[test]
fn write_mbias_txt_percent_precision_2dp() {
    // meth=1, unmeth=2 → 33.33
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 1, true);
    t.accumulate(CytosineContext::CpG, 1, false);
    t.accumulate(CytosineContext::CpG, 1, false);
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    assert!(content.contains("1\t1\t2\t33.33\t3\n"));

    // meth=2, unmeth=1 → 66.67
    let mut t2 = MbiasTable::default();
    t2.accumulate(CytosineContext::CpG, 1, true);
    t2.accumulate(CytosineContext::CpG, 1, true);
    t2.accumulate(CytosineContext::CpG, 1, false);
    let content2 = write_to_tempfile(&[t2, MbiasTable::default()], false);
    assert!(content2.contains("1\t2\t1\t66.67\t3\n"));
}

#[test]
fn write_mbias_txt_percent_rounding_matches_perl_at_midpoint() {
    // Rev 1 (Reviewer B O3): exercise midpoint values to lock rounding
    // behaviour. Rust's `{:.2}` uses banker's rounding (round-half-to-even);
    // Perl's `sprintf("%.2f", ...)` typically uses round-half-away-from-zero.
    // For typical floating-point math, these agree on most inputs; this test
    // snapshots specific (meth, un) pairs that could land on midpoints.
    //
    // For meth=1, unmeth=7 → 100*1/8 = 12.5 (exact half). Rust gives
    // "12.50" (round-to-even, no rounding needed since the next digit is 0).
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 1, true);
    for _ in 0..7 {
        t.accumulate(CytosineContext::CpG, 1, false);
    }
    let content = write_to_tempfile(&[t, MbiasTable::default()], false);
    assert!(
        content.contains("1\t1\t7\t12.50\t8\n"),
        "midpoint 1/8 = 12.50 (no rounding ambiguity); got:\n{content}"
    );

    // meth=3, unmeth=5 → 100*3/8 = 37.5 → "37.50" similarly.
    let mut t2 = MbiasTable::default();
    for _ in 0..3 {
        t2.accumulate(CytosineContext::CpG, 1, true);
    }
    for _ in 0..5 {
        t2.accumulate(CytosineContext::CpG, 1, false);
    }
    let content2 = write_to_tempfile(&[t2, MbiasTable::default()], false);
    assert!(content2.contains("1\t3\t5\t37.50\t8\n"));

    // meth=1, unmeth=5 → 100*1/6 = 16.666... → rounds to "16.67" in both
    // Rust and Perl (round-half-up beyond the 6.6/7 boundary).
    let mut t3 = MbiasTable::default();
    t3.accumulate(CytosineContext::CpG, 1, true);
    for _ in 0..5 {
        t3.accumulate(CytosineContext::CpG, 1, false);
    }
    let content3 = write_to_tempfile(&[t3, MbiasTable::default()], false);
    assert!(content3.contains("1\t1\t5\t16.67\t6\n"));
}

// ─────────────────────────────────────────────────────────────────────────
// 5. mbias_off integration (via ExtractState::finalize)
// ─────────────────────────────────────────────────────────────────────────

mod finalize_integration {
    use super::*;
    use bismark::extractor::cli::Cli;
    use bismark::extractor::state::ExtractState;
    use clap::Parser;

    fn config_with(output_dir: &Path, mbias_off: bool) -> bismark::extractor::cli::ResolvedConfig {
        let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
        std::fs::write(tmp.path(), b"x").unwrap();
        let tmp_path = tmp.into_temp_path();
        let path_str = tmp_path.to_str().unwrap().to_string();
        let _ = tmp_path.keep().unwrap();
        let mut args = vec![
            "bismark_methylation_extractor_rs",
            path_str.as_str(),
            "--output_dir",
            output_dir.to_str().unwrap(),
        ];
        if mbias_off {
            args.push("--mbias_off");
        }
        Cli::try_parse_from(args).unwrap().validate().unwrap()
    }

    #[test]
    fn extract_state_new_se_sets_is_paired_false() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with(dir.path(), false);
        let state = ExtractState::new(&config, Path::new("/tmp/x.bam"), "x", false).unwrap();
        assert!(!state.is_paired);
    }

    #[test]
    fn extract_state_new_pe_sets_is_paired_true() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with(dir.path(), false);
        let state = ExtractState::new(&config, Path::new("/tmp/x.bam"), "x", true).unwrap();
        assert!(state.is_paired);
    }

    #[test]
    fn extract_state_finalize_writes_mbias_txt_when_not_mbias_off() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with(dir.path(), false);
        let mut state = ExtractState::new(&config, Path::new("/tmp/x.bam"), "x", false).unwrap();
        state.mbias[0].accumulate(CytosineContext::CpG, 5, true);
        state.finalize(&config).unwrap();
        let mbias_path = dir.path().join("x.M-bias.txt");
        assert!(
            mbias_path.exists(),
            "M-bias.txt should exist when !mbias_off"
        );
        let content = fs::read_to_string(&mbias_path).unwrap();
        assert!(content.contains("CpG context\n===========\n"));
    }

    #[test]
    fn extract_state_finalize_skips_mbias_txt_when_mbias_off() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with(dir.path(), true); // --mbias_off
        let mut state = ExtractState::new(&config, Path::new("/tmp/x.bam"), "x", false).unwrap();
        state.mbias[0].accumulate(CytosineContext::CpG, 5, true);
        state.finalize(&config).unwrap();
        let mbias_path = dir.path().join("x.M-bias.txt");
        assert!(
            !mbias_path.exists(),
            "M-bias.txt should NOT exist with --mbias_off"
        );
        // Splitting report still exists.
        let report_path = dir.path().join("x_splitting_report.txt");
        assert!(
            report_path.exists(),
            "splitting report should still exist with --mbias_off"
        );
    }
}
