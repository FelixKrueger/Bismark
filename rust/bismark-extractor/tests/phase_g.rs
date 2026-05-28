//! Phase G — orchestrator (`run_phase_g_chain`) integration tests using a
//! mock subprocess runner. Validates:
//!
//! - No-op when neither `--bedGraph` nor `--cytosine_report` is set.
//! - `bismark2bedGraph`-only invocation when only `--bedGraph` is set.
//! - `bismark2bedGraph` then `coverage2cytosine`, in order, when
//!   `--cytosine_report` triggers both.
//! - First-tool failure does NOT invoke the second tool.
//! - Kept-file argv tail + absolute paths.
//! - Empty-kept-set + `--cytosine_report` UX warning (rev 1 I14).
//! - `--gzip` dispatch matrix to both subprocesses.
//! - Long-form flag names (`--remove_spaces`, `--zero_based`).
//!
//! Discovery is exercised via `BISMARK_BIN` pointing at a tempdir of fake
//! binaries; the actual subprocess invocation is intercepted by the
//! `MockRunner` and never spawns a child.

#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};

use bismark_extractor::cli::{OutputMode, PairedMode, ResolvedConfig};
use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::subprocess::{
    BismarkSubprocessRunner, RunOutcome, SubprocessTool, run_phase_g_chain,
};

/// Serialises env-var manipulation; required for BISMARK_BIN.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_env<F: FnOnce()>(vars: &[(&str, Option<&str>)], body: F) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prior: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
        .collect();
    for (k, v) in vars {
        match v {
            Some(val) => unsafe { std::env::set_var(k, val) },
            None => unsafe { std::env::remove_var(k) },
        }
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    for (k, v) in &prior {
        match v {
            Some(val) => unsafe { std::env::set_var(k, val) },
            None => unsafe { std::env::remove_var(k) },
        }
    }
    if let Err(p) = result {
        std::panic::resume_unwind(p);
    }
}

/// Create a tempdir with two fake executables named after the Perl tools.
fn make_bismark_bin_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in &["bismark2bedGraph", "coverage2cytosine"] {
        let p = dir.path().join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).unwrap();
    }
    dir
}

#[derive(Debug, Clone)]
struct MockCall {
    tool: SubprocessTool,
    argv: Vec<OsString>,
}

/// Mock runner. Records calls; returns success (or per-tool override).
struct MockRunner {
    calls: Arc<Mutex<Vec<MockCall>>>,
    /// Per-tool: return non-zero exit when this tool is invoked.
    fail_for: Vec<SubprocessTool>,
}

impl MockRunner {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_for: Vec::new(),
        }
    }
    fn fail_for(mut self, tool: SubprocessTool) -> Self {
        self.fail_for.push(tool);
        self
    }
    fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().unwrap().clone()
    }
}

impl BismarkSubprocessRunner for MockRunner {
    fn run(
        &self,
        tool: SubprocessTool,
        _program: &Path,
        argv: &[OsString],
    ) -> Result<RunOutcome, BismarkExtractorError> {
        self.calls.lock().unwrap().push(MockCall {
            tool,
            argv: argv.to_vec(),
        });
        let exit_status = if self.fail_for.contains(&tool) {
            ExitStatus::from_raw(7 << 8) // exit code 7 (Unix exit-status encoding)
        } else {
            ExitStatus::from_raw(0)
        };
        Ok(RunOutcome {
            exit_status,
            stderr_tail: Vec::new(),
        })
    }
}

fn base_config() -> ResolvedConfig {
    ResolvedConfig {
        files: vec![PathBuf::from("input.bam")],
        paired_mode: PairedMode::SingleEnd,
        output_mode: OutputMode::Default,
        ignore_5p_r1: 0,
        ignore_3p_r1: 0,
        ignore_5p_r2: 0,
        ignore_3p_r2: 0,
        no_overlap: false,
        output_dir: PathBuf::from("/tmp"),
        no_header: false,
        gzip: false,
        emit_splitting_report: true,
        fasta_annotation: false,
        mbias_off: false,
        bedgraph: false,
        cytosine_report: false,
        cutoff: 1,
        remove_spaces: false,
        counts: true,
        zero_based: false,
        cx_context: false,
        split_by_chromosome: false,
        ucsc: false,
        buffer_size: None,
        gazillion: false,
        ample_memory: false,
        genome_folder: None,
        parallel: 1,
    }
}

#[test]
fn phase_g_no_op_when_neither_bedgraph_nor_cytosine_report() {
    let cfg = base_config(); // bedgraph=false, cytosine_report=false
    let runner = MockRunner::new();
    let result = run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner);
    assert!(result.is_ok());
    assert!(
        runner.calls().is_empty(),
        "no subprocess should have been invoked"
    );
}

#[test]
fn phase_g_runs_bismark2bedgraph_only_when_only_bedgraph_set() {
    let bin_dir = make_bismark_bin_dir();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner)
                .expect("chain should succeed");
            let calls = runner.calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].tool, SubprocessTool::Bismark2BedGraph);
        },
    );
}

#[test]
fn phase_g_runs_both_tools_in_order_when_cytosine_report_set() {
    let bin_dir = make_bismark_bin_dir();
    let genome_dir = tempfile::tempdir().unwrap();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true; // auto-triggered by cytosine_report in real CLI
            cfg.cytosine_report = true;
            cfg.genome_folder = Some(genome_dir.path().to_path_buf());
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner)
                .expect("chain should succeed");
            let calls = runner.calls();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].tool, SubprocessTool::Bismark2BedGraph);
            assert_eq!(calls[1].tool, SubprocessTool::Coverage2Cytosine);
        },
    );
}

#[test]
fn phase_g_subprocess_failed_first_tool_does_not_run_second() {
    let bin_dir = make_bismark_bin_dir();
    let genome_dir = tempfile::tempdir().unwrap();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            cfg.cytosine_report = true;
            cfg.genome_folder = Some(genome_dir.path().to_path_buf());
            let runner = MockRunner::new().fail_for(SubprocessTool::Bismark2BedGraph);
            let err = run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner)
                .expect_err("first tool failure should bubble");
            // First tool failed; second NOT invoked.
            let calls = runner.calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].tool, SubprocessTool::Bismark2BedGraph);
            // Error variant correct.
            assert!(matches!(
                err,
                BismarkExtractorError::SubprocessFailed {
                    tool: SubprocessTool::Bismark2BedGraph,
                    ..
                }
            ));
        },
    );
}

#[test]
fn phase_g_subprocess_failed_second_tool_bubbles_with_correct_variant() {
    let bin_dir = make_bismark_bin_dir();
    let genome_dir = tempfile::tempdir().unwrap();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            cfg.cytosine_report = true;
            cfg.genome_folder = Some(genome_dir.path().to_path_buf());
            let runner = MockRunner::new().fail_for(SubprocessTool::Coverage2Cytosine);
            let err = run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner)
                .expect_err("second tool failure should bubble");
            assert!(matches!(
                err,
                BismarkExtractorError::SubprocessFailed {
                    tool: SubprocessTool::Coverage2Cytosine,
                    ..
                }
            ));
        },
    );
}

#[test]
fn phase_g_passes_kept_files_in_b2bg_positional_tail() {
    let bin_dir = make_bismark_bin_dir();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            let kept = vec![
                PathBuf::from("/tmp/CpG_OT_input.txt"),
                PathBuf::from("/tmp/CpG_OB_input.txt"),
            ];
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &kept, &runner)
                .expect("chain should succeed");
            let calls = runner.calls();
            // The two kept paths must be the final argv entries.
            let argv = &calls[0].argv;
            assert_eq!(
                argv[argv.len() - 2],
                OsString::from("/tmp/CpG_OT_input.txt")
            );
            assert_eq!(
                argv[argv.len() - 1],
                OsString::from("/tmp/CpG_OB_input.txt")
            );
        },
    );
}

#[test]
fn phase_g_with_gzip_passes_gzip_flag_only_to_c2c_not_b2bg() {
    let bin_dir = make_bismark_bin_dir();
    let genome_dir = tempfile::tempdir().unwrap();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            cfg.cytosine_report = true;
            cfg.gzip = true;
            cfg.genome_folder = Some(genome_dir.path().to_path_buf());
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner)
                .expect("chain should succeed");
            let calls = runner.calls();
            // b2bg: NO --gzip.
            assert!(!calls[0].argv.contains(&OsString::from("--gzip")));
            // c2c: HAS --gzip.
            assert!(calls[1].argv.contains(&OsString::from("--gzip")));
        },
    );
}

#[test]
fn phase_g_uses_long_form_flag_names_in_argv() {
    let bin_dir = make_bismark_bin_dir();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            cfg.remove_spaces = true;
            cfg.zero_based = true;
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner).unwrap();
            let argv = &runner.calls()[0].argv;
            // Long-form names; NOT the Perl abbreviations.
            assert!(argv.contains(&OsString::from("--remove_spaces")));
            assert!(argv.contains(&OsString::from("--zero_based")));
            assert!(!argv.contains(&OsString::from("--remove")));
            assert!(!argv.contains(&OsString::from("--zero")));
        },
    );
}

#[test]
fn phase_g_passes_buffer_size_2g_default_when_no_explicit_setting() {
    // rev 1 I5: always push --buffer_size 2G when !ample_memory and no
    // explicit value. The argv-builder test covers this directly; here we
    // verify it surfaces through the orchestrator.
    let bin_dir = make_bismark_bin_dir();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner).unwrap();
            let argv = &runner.calls()[0].argv;
            let idx = argv
                .iter()
                .position(|a| a == &OsString::from("--buffer_size"))
                .unwrap();
            assert_eq!(argv[idx + 1], OsString::from("2G"));
        },
    );
}

#[test]
fn phase_g_c2c_argv_has_parent_dir_equal_to_dir() {
    // rev 1 I13: --parent_dir == --dir per Perl :404.
    let bin_dir = make_bismark_bin_dir();
    let genome_dir = tempfile::tempdir().unwrap();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            cfg.cytosine_report = true;
            cfg.genome_folder = Some(genome_dir.path().to_path_buf());
            cfg.output_dir = PathBuf::from("/some/output/dir");
            let runner = MockRunner::new();
            run_phase_g_chain(
                &cfg,
                "input.bam",
                Path::new("/some/output/dir"),
                &[],
                &runner,
            )
            .unwrap();
            let argv = &runner.calls()[1].argv;
            let dir_idx = argv
                .iter()
                .position(|a| a == &OsString::from("--dir"))
                .unwrap();
            let parent_idx = argv
                .iter()
                .position(|a| a == &OsString::from("--parent_dir"))
                .unwrap();
            assert_eq!(argv[dir_idx + 1], argv[parent_idx + 1]);
        },
    );
}

#[test]
fn phase_g_empty_kept_set_with_cytosine_report_does_not_skip_chain() {
    // The plan §3.8 says "let subprocess decide" — chain still runs on
    // empty kept set. The UX warning is emitted (verified separately;
    // here we just check that the subprocess IS invoked).
    let bin_dir = make_bismark_bin_dir();
    let genome_dir = tempfile::tempdir().unwrap();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            cfg.cytosine_report = true;
            cfg.genome_folder = Some(genome_dir.path().to_path_buf());
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "input.bam", Path::new("/tmp"), &[], &runner)
                .expect("chain should succeed");
            assert_eq!(runner.calls().len(), 2);
        },
    );
}

#[test]
fn phase_g_chained_extension_input_produces_trailing_dot_filenames() {
    // rev 1 C3: foo.bam.gz → b2bg argv --output "foo.bam.bedGraph",
    // c2c argv positional "foo.bam.bismark.cov.gz", c2c argv --output
    // "foo.bam.CpG_report.txt". Pin the byte-identity quirk through the
    // orchestrator.
    let bin_dir = make_bismark_bin_dir();
    let genome_dir = tempfile::tempdir().unwrap();
    with_env(
        &[("BISMARK_BIN", Some(bin_dir.path().to_str().unwrap()))],
        || {
            let mut cfg = base_config();
            cfg.bedgraph = true;
            cfg.cytosine_report = true;
            cfg.genome_folder = Some(genome_dir.path().to_path_buf());
            let runner = MockRunner::new();
            run_phase_g_chain(&cfg, "foo.bam.gz", Path::new("/tmp"), &[], &runner).unwrap();
            let calls = runner.calls();
            // b2bg --output value
            let b2bg_out_idx = calls[0]
                .argv
                .iter()
                .position(|a| a == &OsString::from("--output"))
                .unwrap();
            assert_eq!(
                calls[0].argv[b2bg_out_idx + 1],
                OsString::from("foo.bam.bedGraph")
            );
            // c2c positional (last argv) = coverage file
            assert_eq!(
                calls[1].argv.last().unwrap(),
                &OsString::from("foo.bam.bismark.cov.gz")
            );
            // c2c --output = cytosine report
            let c2c_out_idx = calls[1]
                .argv
                .iter()
                .position(|a| a == &OsString::from("--output"))
                .unwrap();
            assert_eq!(
                calls[1].argv[c2c_out_idx + 1],
                OsString::from("foo.bam.CpG_report.txt")
            );
        },
    );
}
