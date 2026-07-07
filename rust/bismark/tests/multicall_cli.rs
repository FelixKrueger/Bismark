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
fn subcommand_help_uses_pinned_argv0() {
    // argv[0] pin: `bismark <sub> --help` usage line reads `bismark <sub>`, not the path.
    for (sub, _classic) in [
        ("dedup", "deduplicate_bismark"),
        ("extract", "bismark_methylation_extractor"),
    ] {
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
fn classic_alias_binary_reports_its_own_name() {
    // The classic-named binary (argv[0] alias) routes to its tool's run_main.
    Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "deduplicate_bismark (Bismark Rust suite) v",
        ));
}
