//! Multicall dispatcher for the single `bismark` binary (Phase 3 — DECISIONS D2/D3/D4/D7).
//!
//! Every `src/bin/<name>.rs` calls [`dispatch`]. It routes on **`argv[0]`** (a classic
//! tool name → that module's `run_main`, byte-identical to the historic binary) and,
//! when invoked as `bismark`, on **`argv[1]`**: a subcommand → the module's
//! `run_from_args` (token stripped, `argv[0]` pinned to `bismark <sub>`); `align` or a
//! bare positional/flag → the aligner; `--help`/`-h`/no-args → the composed help;
//! `--version`/`-V` → the suite version. An unrecognised first token falls through to
//! the aligner (faithful to historic `bismark <genome> <reads>`).

use std::ffi::OsString;
use std::process::ExitCode;

/// "No args is never a valid run → show help" for the suite tools that require
/// input. When `argv_len` is `<= 1` (only the program name is present), render
/// `command`'s long help to stderr and return `Some(ExitCode 2)`; otherwise
/// `None`, and the caller parses `argv` as usual.
///
/// Implemented at the runtime entry (not via clap's `arg_required_else_help`) so
/// `Cli::parse_from([])` keeps returning a defaults `Cli` — the many unit tests
/// that parse an empty argv and then assert `validate()`'s "no input" error rely
/// on that. Auto-discovery tools (`bismark2report`, `bismark2summary`) opt OUT:
/// a bare run there scans the working directory and is a valid mode.
pub fn help_if_no_args(argv_len: usize, mut command: clap::Command) -> Option<ExitCode> {
    if argv_len > 1 {
        return None;
    }
    eprint!("{}", command.render_long_help());
    Some(ExitCode::from(2))
}

/// A classic binary name (from an `argv[0]` alias/symlink) → that module's `run_main`
/// (unchanged → byte-identical). Returns `None` if `prog` is not a classic tool name
/// (i.e. we were invoked as `bismark`).
fn run_legacy_alias(prog: &str) -> Option<ExitCode> {
    Some(match prog {
        "deduplicate_bismark" => crate::dedup::run_main(),
        "bismark_methylation_extractor" => crate::extractor::run_main(),
        "bismark2bedGraph" => crate::bedgraph::run_main(),
        "coverage2cytosine" => crate::coverage2cytosine::run_main(),
        "bismark_genome_preparation" => crate::genome_prep::run_main(),
        "bam2nuc" => crate::bam2nuc::run_main(),
        "NOMe_filtering" => crate::nome_filtering::run_main(),
        "filter_non_conversion" => crate::filter_nonconversion::run_main(),
        "methylation_consistency" => crate::methylation_consistency::run_main(),
        "bismark2report" => crate::report::run_main(),
        "bismark2summary" => crate::summary::run_main(),
        _ => return None,
    })
}

/// `bismark <sub> <args…>` → the module's `run_from_args` with argv reconstructed as
/// `["bismark <sub>", <args…>]` (the D3 map). `None` if `sub` is not a known subcommand.
fn run_subcommand(sub: &str, args_after: &[OsString]) -> Option<ExitCode> {
    let argv = || {
        std::iter::once(OsString::from(format!("bismark {sub}"))).chain(args_after.iter().cloned())
    };
    Some(match sub {
        "dedup" => crate::dedup::run_from_args(argv()),
        "extract" => crate::extractor::run_from_args(argv()),
        "bedgraph" => crate::bedgraph::run_from_args(argv()),
        "cov2cyt" => crate::coverage2cytosine::run_from_args(argv()),
        "prepare" => crate::genome_prep::run_from_args(argv()),
        "bam2nuc" => crate::bam2nuc::run_from_args(argv()),
        "nome" => crate::nome_filtering::run_from_args(argv()),
        "filter" => crate::filter_nonconversion::run_from_args(argv()),
        "consistency" => crate::methylation_consistency::run_from_args(argv()),
        "report" => crate::report::run_from_args(argv()),
        "summary" => crate::summary::run_from_args(argv()),
        _ => return None,
    })
}

/// `bismark align <args…>` → the aligner with the `align` token stripped, `argv[0]`
/// pinned to `bismark align`, and the `@PG CL:` `command_line` = `args[2..]` (so it is
/// byte-identical to bare `bismark <args…>`).
fn run_align_subcommand(raw: &[OsString]) -> ExitCode {
    let tail = align_command_line(raw);
    let command_line = tail.join(" ");
    let mut argv = vec![String::from("bismark align")];
    argv.extend(tail);
    crate::aligner::run_dispatch(argv, command_line)
}

/// The `@PG CL:` `command_line` args for `bismark align <args…>` = the args AFTER the
/// `align` token (`raw[2..]`). Byte-identical to bare `bismark <args…>`, which the
/// aligner's `run_main` reads from `env::args()[1..]` — so the emitted
/// `CL:"bismark <command_line>"` matches the historic binary. Extracted for the
/// byte-identity unit test (Rev-2 validation #2).
fn align_command_line(raw: &[OsString]) -> Vec<String> {
    raw.iter()
        .skip(2)
        .map(|s| s.to_string_lossy().into_owned())
        .collect()
}

/// Composed top-level help (D7). The dispatcher owns `bismark --help`/no-args so the
/// aligner's own help is not edited; `bismark <sub> --help` shows the tool's native help.
fn print_top_level_help() {
    println!("{}", crate::meta::version_line("bismark"));
    println!("\nThe Bismark bisulfite-sequencing suite — one binary, all tools.\n");
    println!("USAGE:");
    println!("  bismark [ALIGN OPTIONS]           run the bisulfite aligner (the default)");
    println!("  bismark align [ALIGN OPTIONS]     run the aligner (explicit)");
    println!("  bismark <SUBCOMMAND> [OPTIONS]    run a suite tool");
    println!("  <classic-name> [OPTIONS]          each tool's classic name also works (alias)\n");
    println!("SUBCOMMANDS (classic name in parentheses):");
    for (sub, classic) in [
        ("align", "bismark"),
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
    ] {
        println!("  {sub:<12}  ({classic})");
    }
    println!("\nBare `bismark <args>` runs the aligner. Run `bismark <subcommand> --help` for a");
    println!("tool's options, or `bismark align --help` for the aligner's.");
}

/// Multicall entry — see the module docs. Called by every `src/bin/<name>.rs`.
pub fn dispatch() -> ExitCode {
    let raw: Vec<OsString> = std::env::args_os().collect();
    let prog = raw
        .first()
        .map(|s| {
            std::path::Path::new(s)
                .file_name()
                .unwrap_or(s.as_os_str())
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_default();

    // 1. Invoked under a classic tool name (argv[0] alias) → unchanged tool, byte-identical.
    if let Some(code) = run_legacy_alias(&prog) {
        return code;
    }

    // 2. Invoked as `bismark` (or any non-classic name) — no args → composed help (D7).
    if raw.len() <= 1 {
        print_top_level_help();
        return ExitCode::SUCCESS;
    }

    // 3. Dispatch on argv[1].
    match raw[1].to_str() {
        Some("-h") | Some("--help") => {
            print_top_level_help();
            ExitCode::SUCCESS
        }
        Some("-V") | Some("--version") => {
            println!("{}", crate::meta::version_line("bismark"));
            ExitCode::SUCCESS
        }
        Some("align") => run_align_subcommand(&raw),
        Some(sub) => {
            let args_after: Vec<OsString> = raw.iter().skip(2).cloned().collect();
            // A known subcommand → route it; otherwise (flag / positional / typo) the
            // token is a bare aligner arg → fall through to the aligner (reads its own
            // env::args, byte-identical to the historic `bismark` binary).
            run_subcommand(sub, &args_after).unwrap_or_else(crate::aligner::run_main)
        }
        // argv[1] present but not valid UTF-8 → a bare aligner arg → the aligner.
        None => crate::aligner::run_main(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    /// Byte-identity gate (Rev-2 validation #2): `bismark align <args>` and bare
    /// `bismark <args>` must build the SAME `@PG CL:` `command_line`. The `align` path
    /// uses `raw[2..]`; the bare path (the aligner's `run_main`) uses `env::args()[1..]`.
    /// For the same effective args those slices are identical → identical `@PG CL`
    /// (which `generate_sam_header` builds as `"bismark {command_line}"`, itself
    /// unit-tested in `aligner::output`).
    #[test]
    fn align_subcommand_command_line_is_byte_identical_to_bare() {
        let align = align_command_line(&os(&["bismark", "align", "--genome", "G", "reads.fq"]));
        // Bare `bismark <args>`: everything after argv[0] — what the aligner's run_main reads.
        let bare: Vec<String> = os(&["bismark", "--genome", "G", "reads.fq"])
            .iter()
            .skip(1)
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            align, bare,
            "align-subcommand command_line must equal bare bismark's"
        );
        assert_eq!(align.join(" "), "--genome G reads.fq");
    }
}
