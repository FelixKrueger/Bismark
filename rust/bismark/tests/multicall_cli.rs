//! Phase 3 — multicall dispatch tests (DECISIONS D2/D3/D4/D7).
//!
//! The classic-name byte-identity tests live in each tool's own `*_…` test files
//! (invoked via the classic `[[bin]]` → `argv[0]` alias → unchanged `run_main`).
//! These cover the NEW `bismark <subcommand>` / bare-`bismark` dispatch surface.
//! The full aligner `@PG CL` byte-identity (bare vs `bismark align`) is by
//! construction (the `command_line` slice is `argv[1..]` bare / `argv[2..]` for
//! `align` — identical for the same user args) and is gated end-to-end by the
//! `perl-oracle` CI cell.

use assert_cmd::Command;
use predicates::prelude::*;

fn bismark() -> Command {
    Command::cargo_bin("bismark").unwrap()
}

/// The 11 non-aligner subcommand ↔ classic-name (argv[0] alias) pairs, mirroring
/// `cli.rs` (`run_subcommand` / `run_legacy_alias` / `print_top_level_help`). The
/// aligner (bare `bismark` / `bismark align` ↔ the `bismark` binary) is exercised
/// separately (`bare_version_prints_suite_line`, `align_version_short_circuits`).
/// This table drives the dual-path routing gate: proving both entrypoints reach
/// each tool means `bismark <sub>` is byte-identical to the classic path *by
/// construction*, so the per-tool byte-identity gates (which run via the classic
/// names) transitively cover the subcommand path — no per-tool oracle duplication.
const PAIRS: &[(&str, &str)] = &[
    ("dedup", "deduplicate_bismark"),
    ("extract", "bismark_methylation_extractor"),
    ("bedgraph", "bismark2bedGraph"),
    ("cov2cyt", "coverage2cytosine"),
    ("prepare", "bismark_genome_preparation"),
    ("bam2nuc", "bam2nuc"),
    ("nome", "NOMe_filtering"),
    ("filter", "filter_non_conversion"),
    ("consistency", "methylation_consistency"),
    ("report", "bismark2report"),
    ("summary", "bismark2summary"),
];

#[test]
fn bare_version_prints_suite_line() {
    for flag in ["--version", "-V"] {
        bismark()
            .arg(flag)
            .assert()
            .success()
            .stdout(predicate::str::contains("bismark (Bismark Rust suite) v"));
    }
}

#[test]
fn no_args_and_help_show_composed_help() {
    // bare no-args (D7) and -h/--help all print the composed top-level help.
    for args in [vec![], vec!["--help"], vec!["-h"]] {
        bismark()
            .args(&args)
            .assert()
            .success()
            .stdout(predicate::str::contains("SUBCOMMANDS"))
            .stdout(predicate::str::contains("dedup"))
            .stdout(predicate::str::contains("deduplicate_bismark"));
    }
}

#[test]
fn align_version_short_circuits() {
    // I1: `bismark align --version` (and version after other flags) must print the
    // version and exit 0 via the aligner's run_dispatch short-circuit — NOT try to align.
    bismark()
        .args(["align", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bismark (Bismark Rust suite) v"));
    bismark()
        .args(["align", "--parallel", "2", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bismark (Bismark Rust suite) v"));
}

#[test]
fn every_subcommand_routes_with_pinned_argv0() {
    // Dual-path routing (subcommand side): `bismark <sub> --help` reaches the tool
    // (not the aligner fallthrough) AND pins argv[0] so the usage line reads
    // `bismark <sub>`. Covers all 11 non-aligner subcommands (was a 2-tool spot-check).
    for (sub, _classic) in PAIRS {
        bismark()
            .args([sub, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("Usage: bismark {sub}")));
    }
}

#[test]
fn typo_falls_through_to_aligner() {
    // An unknown first token is a bare aligner positional (faithful to historic
    // `bismark <genome> <reads>`) → routes to the aligner, which errors on the
    // missing reads. The aligner-specific message proves the routing.
    bismark()
        .arg("dedpu")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Bismark mapping"));
}

#[test]
fn every_classic_alias_routes_to_its_tool() {
    // Dual-path routing (alias side): each classic-named binary (argv[0] alias)
    // reaches its tool's run_main and reports its own canonical name. Combined with
    // the per-tool byte-identity gates (which run via these classic names), this is
    // the by-construction proof that `bismark <sub>` == the classic path for all 11.
    for (_sub, classic) in PAIRS {
        Command::cargo_bin(classic)
            .unwrap()
            .arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "{classic} (Bismark Rust suite) v"
            )));
    }
}
