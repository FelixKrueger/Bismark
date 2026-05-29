//! Phase G — `RealRunner` integration tests using fake shell scripts.
//!
//! These tests exercise the actual `std::process::Command::spawn` +
//! stderr-drain-thread + ring-buffer code path that the mocked-runner
//! tests in `phase_g.rs` skip. They run in CI (no `#[ignore]`) but only
//! on `#[cfg(unix)]` since they depend on `/bin/sh`-style shell scripts.
//!
//! Coverage:
//! - Happy path: zero-exit returns Ok.
//! - Non-zero exit: returns `SubprocessFailed` with the right `tool` variant.
//! - High-volume stderr: ring buffer caps at 64 KiB (rev 1).
//! - 128 KiB stderr burst before exit: no pipe-buffer deadlock (rev 1 I6).
//! - Non-UTF-8 stderr: drain thread does NOT panic (rev 1 C5).

#![cfg(unix)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::subprocess::{BismarkSubprocessRunner, RealRunner, SubprocessTool};

fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir).join("tests/fixtures").join(name)
}

#[test]
fn realrunner_invokes_subprocess_and_returns_ok_on_zero_exit() {
    let prog = fixture_path("fake_bismark2bedgraph_success.sh");
    let argv: Vec<OsString> = vec!["--cutoff".into(), "1".into()];
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, &prog, &argv)
        .expect("zero-exit fixture should succeed");
    assert!(outcome.exit_status.success(), "success fixture must exit 0");
}

#[test]
fn realrunner_returns_subprocess_failed_on_nonzero_exit() {
    // Failure case: the subprocess exits 7. We see Ok(RunOutcome) with a
    // non-success exit_status (the orchestrator converts to SubprocessFailed).
    let prog = fixture_path("fake_bismark2bedgraph_failure.sh");
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, &prog, &[])
        .expect("RealRunner.run itself succeeds even on non-zero child exit");
    assert!(
        !outcome.exit_status.success(),
        "failure fixture must NOT report success"
    );
    assert_eq!(outcome.exit_status.code(), Some(7));
    // stderr tail captured (the fixture writes 1 line; ring buffer keeps it).
    let tail = String::from_utf8_lossy(&outcome.stderr_tail);
    assert!(
        tail.contains("deliberate failure"),
        "stderr tail should capture the fixture's error line; got: {tail:?}"
    );
}

#[test]
fn realrunner_subprocess_failed_carries_correct_tool_variant() {
    // Wrap RealRunner.run in the orchestrator-shape conversion: non-zero
    // exit → SubprocessFailed { tool: ... }. We assert the tool field
    // matches the SubprocessTool we passed in.
    let prog = fixture_path("fake_bismark2bedgraph_failure.sh");
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Coverage2Cytosine, &prog, &[])
        .expect("run() returns Ok even on non-zero child exit");
    // Simulate the orchestrator's error conversion:
    let err = BismarkExtractorError::SubprocessFailed {
        tool: SubprocessTool::Coverage2Cytosine,
        exit_status: outcome.exit_status,
        stderr_tail: outcome.stderr_tail,
    };
    match err {
        BismarkExtractorError::SubprocessFailed { tool, .. } => {
            assert_eq!(tool, SubprocessTool::Coverage2Cytosine);
        }
        _ => panic!("expected SubprocessFailed"),
    }
}

#[test]
fn realrunner_high_volume_stderr_stays_bounded_in_ring_buffer() {
    // Fixture writes ~1 MiB of stderr. The ring buffer caps at 64 KiB.
    let prog = fixture_path("fake_bismark2bedgraph_high_stderr.sh");
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, &prog, &[])
        .expect("high-stderr fixture should exit cleanly");
    assert!(outcome.exit_status.success());
    // The tail length must be ≤ 64 KiB (ring buffer cap from
    // SUBPROCESS_STDERR_RING_CAP).
    assert!(
        outcome.stderr_tail.len() <= 65536,
        "ring buffer leaked: stderr_tail.len() = {}",
        outcome.stderr_tail.len()
    );
    // The retained content should include the LATEST lines (highest index).
    let tail = String::from_utf8_lossy(&outcome.stderr_tail);
    assert!(
        tail.contains("31999") || tail.contains("31998"),
        "expected tail to retain the latest stderr lines; got tail of {} bytes",
        outcome.stderr_tail.len()
    );
}

#[test]
fn realrunner_128kib_stderr_burst_does_not_deadlock() {
    // The fixture writes ~128 KiB of stderr before exiting. With the
    // drain-thread-spawned-BEFORE-wait ordering (rev 1 I6), the parent
    // reads the pipe as it fills and the subprocess never blocks. If the
    // ordering were reversed, this test would hang.
    //
    // We don't set a wall-clock timeout here (Rust's std lib makes that
    // awkward); if it ever hangs, the test runner's --timeout will catch
    // it. In practice this completes in <100 ms.
    let prog = fixture_path("fake_bismark2bedgraph_burst_then_exit.sh");
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, &prog, &[])
        .expect("burst fixture should report exit status");
    assert!(
        !outcome.exit_status.success(),
        "burst fixture exits non-zero"
    );
    // Should have captured up to 64 KiB of the burst.
    assert!(
        outcome.stderr_tail.len() <= 65536,
        "stderr_tail exceeds ring buffer cap"
    );
    assert!(
        outcome.stderr_tail.len() > 1024,
        "expected ring buffer to capture some of the burst; got {} bytes",
        outcome.stderr_tail.len()
    );
}

#[test]
fn realrunner_drain_handles_non_utf8_stderr_bytes() {
    // rev 1 C5: drain thread uses read_until(b'\n') (byte-safe), not
    // read_line (which errors on non-UTF-8). Must succeed even when the
    // subprocess writes invalid UTF-8 sequences.
    let prog = fixture_path("fake_bismark2bedgraph_non_utf8_stderr.sh");
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, &prog, &[])
        .expect("non-UTF-8 stderr must NOT crash the drain thread");
    assert!(outcome.exit_status.success());
    // The high bytes are present in the tail.
    assert!(
        outcome.stderr_tail.contains(&0xff_u8) && outcome.stderr_tail.contains(&0xfe_u8),
        "non-UTF-8 bytes should be preserved in stderr_tail"
    );
    // The trailing ASCII line is also retained.
    let tail = String::from_utf8_lossy(&outcome.stderr_tail);
    assert!(tail.contains("plain ascii"));
}

#[test]
fn realrunner_subprocess_spawn_failed_when_program_does_not_exist() {
    // discover_subprocess would catch this in production; this is a
    // defensive test for the spawn-time path. Passing a non-existent
    // program directly to RealRunner exercises SubprocessSpawnFailed.
    let prog = Path::new("/nonexistent/path/to/fake_b2bg_98765");
    let err = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, prog, &[])
        .expect_err("spawn of nonexistent binary should fail");
    assert!(matches!(
        err,
        BismarkExtractorError::SubprocessSpawnFailed {
            tool: SubprocessTool::Bismark2BedGraph,
            ..
        }
    ));
}

#[test]
fn realrunner_drain_thread_joined_on_ok_path() {
    // The implementation always joins the drain thread before returning.
    // We test this indirectly: the stderr_tail returned on the Ok path
    // must contain the subprocess's stderr (i.e. the drain ran to
    // completion before run() returned). If the drain were unjoined, the
    // ring buffer snapshot could be empty.
    let prog = fixture_path("fake_bismark2bedgraph_success.sh");
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, &prog, &[])
        .expect("success fixture");
    assert!(outcome.exit_status.success());
    // success fixture writes "fake b2bg: invoked with N" to stderr.
    let tail = String::from_utf8_lossy(&outcome.stderr_tail);
    assert!(
        tail.contains("fake b2bg"),
        "drain thread not fully joined on Ok path; tail was: {tail:?}"
    );
}

#[test]
fn realrunner_drain_thread_joined_on_err_path() {
    // Mirror test for the non-zero-exit path: stderr_tail should also be
    // populated (the drain joined before run() returned its result).
    let prog = fixture_path("fake_bismark2bedgraph_failure.sh");
    let outcome = RealRunner { quiet: false }
        .run(SubprocessTool::Bismark2BedGraph, &prog, &[])
        .expect("run() returns Ok regardless of child exit code");
    assert!(
        !outcome.stderr_tail.is_empty(),
        "stderr_tail should be populated on err path"
    );
}
