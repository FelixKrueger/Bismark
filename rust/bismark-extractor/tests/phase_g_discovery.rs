//! Phase G — `discover_subprocess` integration tests.
//!
//! Lives in a separate integration-test crate (not inline in `subprocess.rs`)
//! because the crate-level `#![forbid(unsafe_code)]` in `src/lib.rs` blocks
//! `std::env::set_var` / `remove_var` (both `unsafe` in Rust 2024+). The
//! integration-test crate inherits no such forbid.
//!
//! Tests serialise their env-var manipulations via a process-wide `Mutex`
//! since the `cargo test` harness runs tests in parallel by default.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::subprocess::{SubprocessTool, discover_subprocess};

/// Serialises env-var manipulation across parallel tests.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_env<F: FnOnce()>(vars: &[(&str, Option<&str>)], body: F) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Capture prior state for restoration.
    let prior: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
        .collect();
    // Apply requested state.
    for (k, v) in vars {
        match v {
            Some(val) => unsafe { std::env::set_var(k, val) },
            None => unsafe { std::env::remove_var(k) },
        }
    }
    // Run.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    // Restore.
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

fn make_fake_executable(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

#[test]
fn discover_subprocess_bismark_bin_set_with_tool_returns_bismark_bin_path() {
    let dir = tempfile::tempdir().unwrap();
    let fake = make_fake_executable(dir.path(), "bismark2bedGraph");
    with_env(
        &[
            ("BISMARK_BIN", Some(dir.path().to_str().unwrap())),
            ("BISMARK_TEST_CURRENT_EXE_DIR", None),
        ],
        || {
            let p = discover_subprocess(SubprocessTool::Bismark2BedGraph).unwrap();
            assert_eq!(p, fake);
        },
    );
}

#[test]
fn discover_subprocess_bismark_bin_set_but_tool_not_present_returns_not_found_strict() {
    // rev 1 I12 strict mode: BISMARK_BIN set means LOCK the source; no fallback.
    let dir = tempfile::tempdir().unwrap();
    // Don't create the binary.
    with_env(
        &[
            ("BISMARK_BIN", Some(dir.path().to_str().unwrap())),
            ("BISMARK_TEST_CURRENT_EXE_DIR", None),
        ],
        || {
            let err = discover_subprocess(SubprocessTool::Bismark2BedGraph).unwrap_err();
            assert!(matches!(
                err,
                BismarkExtractorError::SubprocessNotFound {
                    tool: SubprocessTool::Bismark2BedGraph,
                    ..
                }
            ));
        },
    );
}

#[test]
fn discover_subprocess_bismark_bin_set_but_tool_not_executable_returns_not_found() {
    // rev 1 I16: file exists but lacks the exec bit → strict mode rejects.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("bismark2bedGraph");
    std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    // Default mode (rw-r--r--), no exec bit.
    with_env(
        &[
            ("BISMARK_BIN", Some(dir.path().to_str().unwrap())),
            ("BISMARK_TEST_CURRENT_EXE_DIR", None),
        ],
        || {
            let err = discover_subprocess(SubprocessTool::Bismark2BedGraph).unwrap_err();
            assert!(matches!(
                err,
                BismarkExtractorError::SubprocessNotFound { .. }
            ));
        },
    );
}

#[test]
fn discover_subprocess_bismark_bin_empty_string_falls_through_to_path() {
    // rev 1 I16: empty BISMARK_BIN is treated as unset.
    let empty_dir = tempfile::tempdir().unwrap();
    with_env(
        &[
            ("BISMARK_BIN", Some("")),
            (
                "BISMARK_TEST_CURRENT_EXE_DIR",
                Some(empty_dir.path().to_str().unwrap()),
            ),
        ],
        || {
            let result = discover_subprocess(SubprocessTool::Bismark2BedGraph);
            if let Err(BismarkExtractorError::SubprocessNotFound { searched_paths, .. }) = &result {
                // BISMARK_BIN empty did NOT short-circuit: searched_paths must
                // NOT include a path like "/bismark2bedGraph" (which would be
                // the empty-BISMARK_BIN-joined-with-tool result).
                for p in searched_paths {
                    let s = p.to_string_lossy();
                    assert!(
                        s != "/bismark2bedGraph" && s != "bismark2bedGraph",
                        "empty BISMARK_BIN appears to have leaked into discovery: {s}"
                    );
                }
            }
            // If the local env happens to have bismark2bedGraph on PATH (e.g.
            // a Bismark dev install), result is Ok(found); the test still
            // passes because the strict-empty-BISMARK_BIN-rejection branch
            // was NOT taken.
        },
    );
}

#[test]
fn discover_subprocess_falls_back_to_test_current_exe_dir_env_hatch() {
    // rev 1 I17: under #[cfg(test)], BISMARK_TEST_CURRENT_EXE_DIR overrides
    // current_exe() for the fallback step. Avoids the brittle symlink-test-
    // binary trick.
    let dir = tempfile::tempdir().unwrap();
    let _fake = make_fake_executable(dir.path(), "coverage2cytosine");
    with_env(
        &[
            ("BISMARK_BIN", None),
            (
                "BISMARK_TEST_CURRENT_EXE_DIR",
                Some(dir.path().to_str().unwrap()),
            ),
        ],
        || {
            let result = discover_subprocess(SubprocessTool::Coverage2Cytosine);
            match result {
                Ok(found) => {
                    // Either PATH found it (Bismark installed locally) OR
                    // the fallback found our fake. Both are acceptable —
                    // both prove discovery didn't fail.
                    assert!(
                        found
                            .file_name()
                            .map(|n| n == "coverage2cytosine")
                            .unwrap_or(false),
                        "discovered path did not end in coverage2cytosine: {found:?}"
                    );
                }
                Err(e) => panic!("expected discovery to find fake or PATH-installed binary: {e:?}"),
            }
        },
    );
}

#[test]
fn discover_subprocess_returns_not_found_when_all_paths_exhausted() {
    // Skip if a real Bismark install is on PATH.
    if which::which("bismark2bedGraph").is_ok() {
        eprintln!(
            "(skipping: bismark2bedGraph is on PATH; can't reliably test the 'not found' path)"
        );
        return;
    }
    let empty_dir = tempfile::tempdir().unwrap();
    with_env(
        &[
            ("BISMARK_BIN", None),
            (
                "BISMARK_TEST_CURRENT_EXE_DIR",
                Some(empty_dir.path().to_str().unwrap()),
            ),
        ],
        || {
            let err = discover_subprocess(SubprocessTool::Bismark2BedGraph).unwrap_err();
            match err {
                BismarkExtractorError::SubprocessNotFound {
                    tool,
                    searched_paths,
                } => {
                    assert_eq!(tool, SubprocessTool::Bismark2BedGraph);
                    assert!(
                        !searched_paths.is_empty(),
                        "searched_paths should list every attempted location"
                    );
                }
                other => panic!("expected SubprocessNotFound, got: {other:?}"),
            }
        },
    );
}
