//! Phase G — subprocess chain to Perl `bismark2bedGraph` + `coverage2cytosine`.
//!
//! Per SPEC §6.6 and the Phase G plan, the v1.0 extractor shells out to the
//! existing Perl tools rather than implementing the bedGraph / cytosine_report
//! algorithms inline. The Rust port's responsibilities here are:
//!
//! 1. **Subprocess discovery** — `BISMARK_BIN` env (strict if set), then `PATH`,
//!    then alongside the Rust binary (`current_exe()` parent).
//! 2. **Argv construction** — match Perl `bismark_methylation_extractor:323-428`
//!    byte-for-byte modulo Perl's GetOptions prefix-abbreviation (Perl pushes
//!    `--remove`/`--zero`; Rust pushes the long forms `--remove_spaces`/
//!    `--zero_based` that the subprocess GetOptions resolves to the same flag).
//! 3. **Stderr handling** — TEE: spawn the child with piped stderr, drain it on
//!    a side thread that writes each line live to the parent's stderr and
//!    retains the trailing 64 KiB in a ring buffer for error reporting.
//! 4. **Error propagation** — non-zero exit → `SubprocessFailed { tool,
//!    exit_status, stderr_tail }`; missing tool → `SubprocessNotFound`; spawn
//!    failure → `SubprocessSpawnFailed`.
//!
//! ## Byte-identity invariant
//!
//! `argv` passed to Perl `bismark2bedGraph` and `coverage2cytosine` from the
//! Rust port MUST match the Perl extractor's `@args` byte-for-byte, modulo the
//! long-form flag expansion documented in the Phase G plan §2.4.4. The
//! `tests/phase_g_argv_parity.rs` golden tests assert this for the three
//! canonical Phase G configurations.
//!
//! ## Filename-derivation quirk (rev 1 C3)
//!
//! Perl `:325-330` strips the **literal** trailing letters `gz`, `sam`, `bam`,
//! `txt` (no leading dot). Chained extensions therefore preserve a trailing
//! dot: `foo.bam.gz` → `foo.bam.bedGraph`. No-extension inputs produce no
//! leading dot: `foo` → `foobedGraph`. The `derive_*` functions below mirror
//! Perl's regex pipeline step-by-step.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use crate::cli::ResolvedConfig;
use crate::error::BismarkExtractorError;

/// Maximum stderr-tail bytes retained for error reporting. The full stderr
/// stream is still tee'd live to the parent's stderr; this cap only bounds
/// the in-memory copy attached to a [`BismarkExtractorError::SubprocessFailed`].
const SUBPROCESS_STDERR_RING_CAP: usize = 65536; // 64 KiB

/// Identifies which Perl subprocess is being invoked. Used for discovery,
/// argv-building dispatch, and error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubprocessTool {
    /// `bismark2bedGraph` — Perl `bismark_methylation_extractor:377`.
    Bismark2BedGraph,
    /// `coverage2cytosine` — Perl `bismark_methylation_extractor:424`.
    Coverage2Cytosine,
}

impl SubprocessTool {
    /// The on-disk binary name the subprocess is expected to be installed as.
    /// Matches Perl's `$RealBin/<name>` invocation.
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::Bismark2BedGraph => "bismark2bedGraph",
            Self::Coverage2Cytosine => "coverage2cytosine",
        }
    }
}

impl fmt::Display for SubprocessTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.binary_name())
    }
}

// ─── Ring buffer ────────────────────────────────────────────────────────────

/// Bounded byte-level FIFO with O(1) eviction. Owned by the stderr drain
/// thread; never shared. Snapshot returned via thread `join`.
pub(crate) struct RingBuffer {
    buf: VecDeque<u8>,
    cap: usize,
}

impl RingBuffer {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap),
            cap,
        }
    }

    /// Append `bytes`, evicting from the front to stay within `cap`. If
    /// `bytes.len() >= cap`, the buffer is fully replaced by the trailing
    /// `cap` bytes of the input.
    pub(crate) fn push_bytes(&mut self, bytes: &[u8]) {
        if bytes.len() >= self.cap {
            self.buf.clear();
            self.buf.extend(&bytes[bytes.len() - self.cap..]);
            return;
        }
        while self.buf.len() + bytes.len() > self.cap {
            self.buf.pop_front();
        }
        self.buf.extend(bytes);
    }

    /// Consume into a `Vec<u8>` snapshot (used at thread-join time).
    pub(crate) fn into_vec(self) -> Vec<u8> {
        self.buf.into_iter().collect()
    }

    #[allow(dead_code)] // used by inline tests
    pub(crate) fn len(&self) -> usize {
        self.buf.len()
    }
}

// ─── Filename derivation (Perl :325-330, :392-399, :419-420) ────────────────

/// Derive the bedGraph output filename from the input basename.
///
/// Mirrors Perl `bismark_methylation_extractor:325-330` step-by-step:
///
/// ```text
/// $out = basename($input);
/// $out =~ s/gz$//;   # strip literal "gz"  (no leading dot)
/// $out =~ s/sam$//;  # strip literal "sam"
/// $out =~ s/bam$//;  # strip literal "bam"
/// $out =~ s/txt$//;  # strip literal "txt"
/// $out =~ s/$/bedGraph/;  # append "bedGraph"
/// ```
///
/// Trailing-dot preservation is load-bearing for chained-extension inputs:
///
/// | Input | Output |
/// |-------|--------|
/// | `foo.bam` | `foo.bedGraph` |
/// | `foo.bam.gz` | **`foo.bam.bedGraph`** (trailing dot preserved) |
/// | `foo` (no ext) | **`foobedGraph`** (no leading dot) |
/// | `sample.fastq_bismark_bt2_pe.deduplicated.bam` | `sample.fastq_bismark_bt2_pe.deduplicated.bedGraph` |
pub fn derive_bedgraph_filename(input_basename: &str) -> String {
    let mut s = input_basename.to_string();
    for ext in &["gz", "sam", "bam", "txt"] {
        if let Some(stripped) = s.strip_suffix(ext) {
            s = stripped.to_string();
        }
    }
    s.push_str("bedGraph");
    s
}

/// Derive the `.bismark.cov.gz` coverage filename from the bedGraph filename.
/// Mirrors Perl `:419-420`: `s/bedGraph$/bismark.cov.gz/`.
pub fn derive_coverage_filename(bedgraph_filename: &str) -> String {
    if let Some(prefix) = bedgraph_filename.strip_suffix("bedGraph") {
        format!("{prefix}bismark.cov.gz")
    } else {
        // bedGraph filename produced by `derive_bedgraph_filename` always ends
        // in "bedGraph" by construction; this branch is defensive.
        format!("{bedgraph_filename}.bismark.cov.gz")
    }
}

/// Derive the cytosine-report filename from the bedGraph filename. Mirrors
/// Perl `:392-399`: strip `bedGraph` suffix; append `CpG_report.txt` (default)
/// or `CX_report.txt` (when `--CX`).
pub fn derive_cytosine_filename(bedgraph_filename: &str, cx_context: bool) -> String {
    let stem = bedgraph_filename
        .strip_suffix("bedGraph")
        .unwrap_or(bedgraph_filename);
    if cx_context {
        format!("{stem}CX_report.txt")
    } else {
        format!("{stem}CpG_report.txt")
    }
}

// ─── Subprocess discovery (BISMARK_BIN-first, strict; PATH; current_exe) ────

/// Locate the on-disk binary for `tool`. Search order:
///
/// 1. `BISMARK_BIN` env var (strict): if set and non-empty, `$BISMARK_BIN/<tool>`
///    must exist and be executable, else `SubprocessNotFound`. **No fallback.**
///    Empty string is treated as "unset" and falls through to step 2.
/// 2. `PATH` lookup via the `which` crate.
/// 3. `current_exe()`'s parent directory (mirrors Perl `$RealBin`). Under
///    `#[cfg(test)]`, the `BISMARK_TEST_CURRENT_EXE_DIR` env var overrides
///    `current_exe()` to support fake-binary placement without symlinks.
///
/// Returns `SubprocessNotFound { tool, searched_paths }` if all three fail.
pub fn discover_subprocess(tool: SubprocessTool) -> Result<PathBuf, BismarkExtractorError> {
    let mut searched: Vec<PathBuf> = Vec::new();
    let tool_name = tool.binary_name();

    // 1. BISMARK_BIN strict.
    if let Ok(bin_dir) = std::env::var("BISMARK_BIN")
        && !bin_dir.is_empty()
    {
        let candidate = PathBuf::from(&bin_dir).join(tool_name);
        searched.push(candidate.clone());
        if is_executable_file(&candidate) {
            return Ok(candidate);
        }
        return Err(BismarkExtractorError::SubprocessNotFound {
            tool,
            searched_paths: searched,
        });
    }

    // 2. PATH via `which`.
    match which::which(tool_name) {
        Ok(p) => return Ok(p),
        Err(_) => searched.push(PathBuf::from(format!("$PATH/{tool_name}"))),
    }

    // 3. current_exe() parent (test-overridable).
    let exe_dir = current_exe_dir_for_lookup();
    if let Some(dir) = exe_dir {
        let candidate = dir.join(tool_name);
        searched.push(candidate.clone());
        if is_executable_file(&candidate) {
            return Ok(candidate);
        }
    }

    Err(BismarkExtractorError::SubprocessNotFound {
        tool,
        searched_paths: searched,
    })
}

/// Resolve the `current_exe()` parent directory for discovery step 3. The
/// `BISMARK_TEST_CURRENT_EXE_DIR` env var, if set + non-empty, overrides
/// `current_exe()` to support test placement of fake binaries without
/// symlinking the test binary itself. Harmless in production (the env var
/// won't be set); intentionally not gated behind `#[cfg(test)]` because
/// integration-test crates don't inherit the lib's cfg(test) flag.
fn current_exe_dir_for_lookup() -> Option<PathBuf> {
    if let Ok(test_dir) = std::env::var("BISMARK_TEST_CURRENT_EXE_DIR")
        && !test_dir.is_empty()
    {
        return Some(PathBuf::from(test_dir));
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    // Non-unix: just check file presence; permission semantics differ.
    std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

// ─── Argv builders (Perl :333-373 + :388-422) ───────────────────────────────

/// Build the argv list (excluding argv[0]) for `bismark2bedGraph`. Mirrors
/// Perl `:333-373` order and flag selection. Long-form flag names are used
/// throughout; Perl uses GetOptions prefix-abbreviation (`--remove`,
/// `--zero`) but `bismark2bedGraph` resolves them to the same long-form
/// flags (`--remove_spaces`, `--zero_based`).
///
/// `kept_split_files` are appended verbatim as positional arguments (the
/// post-empty-sweep set, absolute paths, sorted lexicographically).
pub fn build_bismark2bedgraph_argv(
    config: &ResolvedConfig,
    kept_split_files: &[PathBuf],
    bedgraph_filename: &str,
    output_dir: &Path,
) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::new();

    if config.remove_spaces {
        argv.push("--remove_spaces".into());
    }
    if config.cx_context {
        argv.push("--CX_context".into());
    }
    if config.no_header {
        argv.push("--no_header".into());
    }
    if config.gazillion {
        argv.push("--gazillion".into());
    }
    if config.ample_memory {
        argv.push("--ample_memory".into());
    } else {
        // Per Perl :347-352: when !ample_memory, ALWAYS push --buffer_size.
        // Default is "2G" matching Perl's $sort_size default at :1305.
        let size: &str = config.buffer_size.as_deref().unwrap_or("2G");
        argv.push("--buffer_size".into());
        argv.push(size.into());
    }
    if config.ucsc {
        argv.push("--ucsc".into());
    }
    if config.zero_based {
        argv.push("--zero_based".into());
    }
    argv.push("--cutoff".into());
    argv.push(config.cutoff.to_string().into());
    argv.push("--output".into());
    argv.push(bedgraph_filename.into());
    argv.push("--dir".into());
    argv.push(output_dir.as_os_str().to_owned());
    for f in kept_split_files {
        argv.push(f.as_os_str().to_owned());
    }
    argv
}

/// Build the argv for `coverage2cytosine`. Mirrors Perl `:388-422`. Note
/// that `--parent_dir` is intentionally set equal to `--dir` (rev 1 I13),
/// matching Perl `:404`.
pub fn build_coverage2cytosine_argv(
    config: &ResolvedConfig,
    coverage_input_filename: &str,
    cytosine_output_filename: &str,
    output_dir: &Path,
    genome_folder: &Path,
) -> Vec<OsString> {
    // Header section is unconditional; use vec![] for clippy + readability.
    // --parent_dir == --dir per Perl :404 (rev 1 I13).
    let mut argv: Vec<OsString> = vec![
        "--output".into(),
        cytosine_output_filename.into(),
        "--dir".into(),
        output_dir.as_os_str().to_owned(),
        "--genome".into(),
        genome_folder.as_os_str().to_owned(),
        "--parent_dir".into(),
        output_dir.as_os_str().to_owned(),
    ];
    if config.zero_based {
        argv.push("--zero_based".into());
    }
    if config.cx_context {
        argv.push("--CX_context".into());
    }
    if config.split_by_chromosome {
        argv.push("--split_by_chromosome".into());
    }
    if config.gzip {
        argv.push("--gzip".into());
    }
    // Positional: .bismark.cov.gz filename (basename; coverage2cytosine
    // reads from --dir).
    argv.push(coverage_input_filename.into());
    argv
}

// ─── Runner trait + RealRunner ─────────────────────────────────────────────

/// Outcome of a single subprocess invocation. On non-zero exit the orchestrator
/// converts this into a `SubprocessFailed` error; on success it's discarded
/// (the stderr was already tee'd live to the parent's stderr).
#[derive(Debug)]
pub struct RunOutcome {
    /// Exit status reported by the OS.
    pub exit_status: ExitStatus,
    /// Trailing ≤ 64 KiB of stderr captured via the tee drain thread.
    pub stderr_tail: Vec<u8>,
}

/// Abstracts subprocess invocation so tests can swap a mock implementation
/// without spawning real children. Production: [`RealRunner`] shells out via
/// `std::process::Command`. Tests: a closure-based runner that captures the
/// call and synthesises an outcome.
pub trait BismarkSubprocessRunner {
    /// Run `program` with `argv`. The implementor is responsible for stderr
    /// tee + ring-buffer semantics (for `RealRunner` — see module docs).
    fn run(
        &self,
        tool: SubprocessTool,
        program: &Path,
        argv: &[OsString],
    ) -> Result<RunOutcome, BismarkExtractorError>;
}

/// Production implementation. Spawns the child via `Command`; spawns a stderr
/// drain thread BEFORE calling `child.wait()` (prevents pipe-buffer-full
/// deadlock on >64 KiB stderr bursts); reads via `read_until(b'\n', ...)`
/// (byte-safe; doesn't require UTF-8 stderr); always joins the drain thread
/// before returning (prevents stderr-tail races + thread leaks).
pub struct RealRunner;

impl BismarkSubprocessRunner for RealRunner {
    fn run(
        &self,
        tool: SubprocessTool,
        program: &Path,
        argv: &[OsString],
    ) -> Result<RunOutcome, BismarkExtractorError> {
        // Pre-spawn audit eprintln (rev 1 O2). Always-on; cheap.
        eprintln!(
            "[bismark-extractor] spawning: {} {}",
            program.display(),
            argv.iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        );

        let mut child = Command::new(program)
            .args(argv)
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            // stdout inherited (subprocesses write outputs to files via --output)
            .spawn()
            .map_err(|source| BismarkExtractorError::SubprocessSpawnFailed { tool, source })?;

        // Take stderr immediately; spawn drain thread BEFORE child.wait()
        // (rev 1 I6 — prevents pipe-buffer-full deadlock).
        let stderr = child
            .stderr
            .take()
            .expect("stderr was set to piped at spawn time");

        let drain_handle = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut ring = RingBuffer::new(SUBPROCESS_STDERR_RING_CAP);
            let mut reader = BufReader::new(stderr);
            let mut buf: Vec<u8> = Vec::with_capacity(4096);
            let stderr_out = io::stderr();
            let mut stderr_lock = stderr_out.lock();
            loop {
                buf.clear();
                // read_until is byte-safe (unlike read_line which errors on
                // non-UTF-8). rev 1 C5.
                let n = reader.read_until(b'\n', &mut buf)?;
                if n == 0 {
                    break; // EOF
                }
                stderr_lock.write_all(&buf)?;
                ring.push_bytes(&buf);
            }
            stderr_lock.flush()?;
            Ok(ring.into_vec())
        });

        // Phase G rev 2 (code-review A ER1 fix): join the drain BEFORE
        // inspecting `child.wait()`'s outcome. The previous code returned
        // via `?` on a `wait()` failure and skipped the drain join,
        // contradicting the rev-1 I1 "always join" contract. The drain
        // naturally exits when the subprocess closes stderr (which happens
        // on exit, success or fail), so joining first is correct ordering
        // — `wait()` then returns immediately because the child has already
        // exited.
        let wait_result = child.wait();
        let stderr_tail = match drain_handle.join() {
            Ok(Ok(tail)) => tail,
            Ok(Err(e)) => {
                return Err(BismarkExtractorError::InternalError {
                    message: format!("stderr drain thread io error: {e}"),
                });
            }
            Err(_panic) => {
                return Err(BismarkExtractorError::InternalError {
                    message: "stderr drain thread panicked".to_string(),
                });
            }
        };
        let exit_status = wait_result
            .map_err(|source| BismarkExtractorError::SubprocessSpawnFailed { tool, source })?;

        Ok(RunOutcome {
            exit_status,
            stderr_tail,
        })
    }
}

// ─── Orchestrator ──────────────────────────────────────────────────────────

/// Run the Phase G subprocess chain. Invoked from
/// [`crate::state::ExtractState::finalize`] after split files + splitting
/// report + M-bias.txt have been written, when `config.bedgraph` is true.
///
/// Order:
///   1. If `config.bedgraph` is false: no-op return Ok(()).
///   2. If `kept_split_files` is empty AND `config.cytosine_report` is set:
///      emit a UX warning to stderr (chain still runs).
///   3. Discover `bismark2bedGraph`; build argv; run via `runner`. On non-zero
///      exit, return `SubprocessFailed` (coverage2cytosine never invoked).
///   4. If `config.cytosine_report` is set: discover `coverage2cytosine`;
///      build argv referring to the `.bismark.cov.gz` produced by step 3;
///      run. Same error semantics.
///
/// All filename derivations preserve Perl's trailing-dot quirk per
/// [`derive_bedgraph_filename`].
pub fn run_phase_g_chain<R: BismarkSubprocessRunner>(
    config: &ResolvedConfig,
    input_basename: &str,
    output_dir: &Path,
    kept_split_files: &[PathBuf],
    runner: &R,
) -> Result<(), BismarkExtractorError> {
    if !config.bedgraph {
        return Ok(());
    }

    // UX warning for empty-kept-set + cytosine_report (rev 1 I14).
    if kept_split_files.is_empty() && config.cytosine_report {
        eprintln!(
            "note: extractor produced no methylation calls; \
             cytosine_report will scan the genome anyway"
        );
    }

    // ── Step 1: bismark2bedGraph ──
    let bedgraph_filename = derive_bedgraph_filename(input_basename);
    let b2bg_program = discover_subprocess(SubprocessTool::Bismark2BedGraph)?;
    let b2bg_argv =
        build_bismark2bedgraph_argv(config, kept_split_files, &bedgraph_filename, output_dir);
    let b2bg_outcome = runner.run(SubprocessTool::Bismark2BedGraph, &b2bg_program, &b2bg_argv)?;
    if !b2bg_outcome.exit_status.success() {
        return Err(BismarkExtractorError::SubprocessFailed {
            tool: SubprocessTool::Bismark2BedGraph,
            exit_status: b2bg_outcome.exit_status,
            stderr_tail: b2bg_outcome.stderr_tail,
        });
    }

    // ── Step 2: coverage2cytosine (if engaged) ──
    if config.cytosine_report {
        let coverage_filename = derive_coverage_filename(&bedgraph_filename);
        let cytosine_filename = derive_cytosine_filename(&bedgraph_filename, config.cx_context);
        let genome_folder = config
            .genome_folder
            .as_ref()
            .expect("CLI validation guarantees genome_folder is Some when cytosine_report is set");
        let c2c_program = discover_subprocess(SubprocessTool::Coverage2Cytosine)?;
        let c2c_argv = build_coverage2cytosine_argv(
            config,
            &coverage_filename,
            &cytosine_filename,
            output_dir,
            genome_folder,
        );
        let c2c_outcome = runner.run(SubprocessTool::Coverage2Cytosine, &c2c_program, &c2c_argv)?;
        if !c2c_outcome.exit_status.success() {
            return Err(BismarkExtractorError::SubprocessFailed {
                tool: SubprocessTool::Coverage2Cytosine,
                exit_status: c2c_outcome.exit_status,
                stderr_tail: c2c_outcome.stderr_tail,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{OutputMode, PairedMode};

    // ─── Helper: build a minimal ResolvedConfig for argv-builder tests ──

    fn default_config() -> ResolvedConfig {
        ResolvedConfig {
            files: vec![PathBuf::from("input.bam")],
            paired_mode: PairedMode::SingleEnd,
            output_mode: OutputMode::Default,
            ignore_5p_r1: 0,
            ignore_3p_r1: 0,
            ignore_5p_r2: 0,
            ignore_3p_r2: 0,
            no_overlap: false,
            output_dir: PathBuf::from("/out"),
            no_header: false,
            gzip: false,
            emit_splitting_report: true,
            fasta_annotation: false,
            mbias_off: false,
            bedgraph: true,
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

    // ─── SubprocessTool Display ─────────────────────────────────────────

    #[test]
    fn subprocess_tool_display_matches_binary_name() {
        assert_eq!(
            format!("{}", SubprocessTool::Bismark2BedGraph),
            "bismark2bedGraph"
        );
        assert_eq!(
            format!("{}", SubprocessTool::Coverage2Cytosine),
            "coverage2cytosine"
        );
        // Round-trip via binary_name() — same source of truth.
        assert_eq!(
            SubprocessTool::Bismark2BedGraph.binary_name(),
            "bismark2bedGraph"
        );
        assert_eq!(
            SubprocessTool::Coverage2Cytosine.binary_name(),
            "coverage2cytosine"
        );
    }

    // ─── Filename derivation (rev 1 C3 — trailing-dot quirk) ────────────

    #[test]
    fn derive_bedgraph_filename_foo_bam() {
        assert_eq!(derive_bedgraph_filename("foo.bam"), "foo.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_sam() {
        assert_eq!(derive_bedgraph_filename("foo.sam"), "foo.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_txt() {
        assert_eq!(derive_bedgraph_filename("foo.txt"), "foo.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_bam_gz_preserves_trailing_dot() {
        // rev 1 C3 critical guard. Perl s/gz$// strips "gz" not ".gz", so
        // foo.bam.gz becomes "foo.bam." → "foo.bam.bedGraph".
        assert_eq!(derive_bedgraph_filename("foo.bam.gz"), "foo.bam.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_txt_gz_preserves_trailing_dot() {
        assert_eq!(derive_bedgraph_filename("foo.txt.gz"), "foo.txt.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_no_extension_has_no_leading_dot() {
        // rev 1 C3: input "foo" with no recognised extension → "foobedGraph".
        // The leading dot is NOT introduced because Perl's regex doesn't
        // strip anything (no match), and "s/$/bedGraph/" appends literally.
        assert_eq!(derive_bedgraph_filename("foo"), "foobedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_real_bismark_pe_naming() {
        assert_eq!(
            derive_bedgraph_filename("sample.fastq_bismark_bt2_pe.deduplicated.bam"),
            "sample.fastq_bismark_bt2_pe.deduplicated.bedGraph"
        );
    }

    #[test]
    fn derive_bedgraph_filename_real_bismark_pe_gz_naming() {
        // Chained-extension case on real Bismark output names.
        assert_eq!(
            derive_bedgraph_filename("sample.fastq_bismark_bt2_pe.deduplicated.bam.gz"),
            "sample.fastq_bismark_bt2_pe.deduplicated.bam.bedGraph"
        );
    }

    #[test]
    fn derive_coverage_filename_basic() {
        assert_eq!(
            derive_coverage_filename("foo.bedGraph"),
            "foo.bismark.cov.gz"
        );
    }

    #[test]
    fn derive_coverage_filename_preserves_trailing_dot_for_chained_extensions() {
        // foo.bam.bedGraph (from foo.bam.gz input) → foo.bam.bismark.cov.gz.
        assert_eq!(
            derive_coverage_filename("foo.bam.bedGraph"),
            "foo.bam.bismark.cov.gz"
        );
    }

    #[test]
    fn derive_cytosine_filename_cpg_default() {
        assert_eq!(
            derive_cytosine_filename("foo.bedGraph", false),
            "foo.CpG_report.txt"
        );
    }

    #[test]
    fn derive_cytosine_filename_cx_context_when_flag_set() {
        assert_eq!(
            derive_cytosine_filename("foo.bedGraph", true),
            "foo.CX_report.txt"
        );
    }

    #[test]
    fn derive_cytosine_filename_preserves_trailing_dot_for_chained_extensions() {
        // The whole chain: foo.bam.gz → foo.bam.bedGraph → foo.bam.CpG_report.txt.
        assert_eq!(
            derive_cytosine_filename("foo.bam.bedGraph", false),
            "foo.bam.CpG_report.txt"
        );
    }

    // ─── bismark2bedGraph argv-builder ──────────────────────────────────

    #[test]
    fn build_bismark2bedgraph_argv_default_no_optional_flags() {
        let cfg = default_config();
        let kept: Vec<PathBuf> = Vec::new();
        let argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        // Default config: !remove_spaces, !cx, !no_header, !gazillion,
        // !ample_memory (so buffer_size 2G), !ucsc, !zero_based.
        // Always: --buffer_size 2G, --cutoff 1, --output, --dir.
        assert_eq!(
            argv,
            vec![
                OsString::from("--buffer_size"),
                OsString::from("2G"),
                OsString::from("--cutoff"),
                OsString::from("1"),
                OsString::from("--output"),
                OsString::from("foo.bedGraph"),
                OsString::from("--dir"),
                OsString::from("/out"),
            ]
        );
    }

    #[test]
    fn build_bismark2bedgraph_argv_all_optional_flags_set() {
        let mut cfg = default_config();
        cfg.remove_spaces = true;
        cfg.cx_context = true;
        cfg.no_header = true;
        cfg.gazillion = true;
        // ample_memory mutex with buffer_size — set ample_memory, leave buffer_size None.
        cfg.ample_memory = true;
        cfg.ucsc = true;
        cfg.zero_based = true;
        cfg.cutoff = 5;
        let kept: Vec<PathBuf> = Vec::new();
        let argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        assert_eq!(
            argv,
            vec![
                OsString::from("--remove_spaces"),
                OsString::from("--CX_context"),
                OsString::from("--no_header"),
                OsString::from("--gazillion"),
                OsString::from("--ample_memory"),
                OsString::from("--ucsc"),
                OsString::from("--zero_based"),
                OsString::from("--cutoff"),
                OsString::from("5"),
                OsString::from("--output"),
                OsString::from("foo.bedGraph"),
                OsString::from("--dir"),
                OsString::from("/out"),
            ]
        );
    }

    #[test]
    fn build_bismark2bedgraph_argv_uses_long_form_remove_spaces() {
        let mut cfg = default_config();
        cfg.remove_spaces = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        // rev 1 §2.4.4: Perl pushes "--remove" (GetOptions prefix abbrev);
        // Rust pushes the long form "--remove_spaces" explicitly.
        assert!(argv.contains(&OsString::from("--remove_spaces")));
        assert!(!argv.contains(&OsString::from("--remove")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_uses_long_form_zero_based() {
        let mut cfg = default_config();
        cfg.zero_based = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(argv.contains(&OsString::from("--zero_based")));
        assert!(!argv.contains(&OsString::from("--zero")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_passes_buffer_size_2g_default_when_neither_flag_set() {
        // rev 1 I5: when both buffer_size and ample_memory unset, ALWAYS push
        // --buffer_size 2G (Perl :347-352 in the else branch, with $sort_size
        // defaulting to "2G" at :1305).
        let cfg = default_config();
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        let idx = argv
            .iter()
            .position(|a| a == &OsString::from("--buffer_size"))
            .expect("--buffer_size should be present");
        assert_eq!(argv[idx + 1], OsString::from("2G"));
    }

    #[test]
    fn build_bismark2bedgraph_argv_passes_explicit_buffer_size_when_set() {
        let mut cfg = default_config();
        cfg.buffer_size = Some("4G".to_string());
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        let idx = argv
            .iter()
            .position(|a| a == &OsString::from("--buffer_size"))
            .unwrap();
        assert_eq!(argv[idx + 1], OsString::from("4G"));
    }

    #[test]
    fn build_bismark2bedgraph_argv_passes_ample_memory_instead_of_buffer_size() {
        let mut cfg = default_config();
        cfg.ample_memory = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(argv.contains(&OsString::from("--ample_memory")));
        assert!(!argv.contains(&OsString::from("--buffer_size")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_omits_counts_flag() {
        // Perl :362-364 comments out the --counts push. Rust mirrors:
        // --counts is never in the argv regardless of config.counts.
        let mut cfg = default_config();
        cfg.counts = true; // forced; Perl ON by default
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(!argv.contains(&OsString::from("--counts")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_appends_kept_files_as_positional_tail() {
        let cfg = default_config();
        let kept = vec![
            PathBuf::from("/out/CpG_OT_input.txt"),
            PathBuf::from("/out/CpG_OB_input.txt"),
        ];
        let argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        // The two paths must be the final argv entries, in input order
        // (the caller sorts; this fn preserves).
        assert_eq!(
            argv[argv.len() - 2],
            OsString::from("/out/CpG_OT_input.txt")
        );
        assert_eq!(
            argv[argv.len() - 1],
            OsString::from("/out/CpG_OB_input.txt")
        );
    }

    #[test]
    fn build_bismark2bedgraph_argv_does_not_pass_gzip() {
        // bismark2bedGraph has no --gzip flag (`bismark2bedGraph:637-651`).
        let mut cfg = default_config();
        cfg.gzip = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(!argv.contains(&OsString::from("--gzip")));
    }

    // ─── coverage2cytosine argv-builder ────────────────────────────────

    #[test]
    fn build_coverage2cytosine_argv_default_cpg_only() {
        let cfg = default_config();
        let argv = build_coverage2cytosine_argv(
            &cfg,
            "foo.bismark.cov.gz",
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        // Order per Perl :388-422 + rev 1 I13 (--parent_dir == --dir).
        // --zero_based / --CX_context / --split_by_chromosome / --gzip all off.
        assert_eq!(
            argv,
            vec![
                OsString::from("--output"),
                OsString::from("foo.CpG_report.txt"),
                OsString::from("--dir"),
                OsString::from("/out"),
                OsString::from("--genome"),
                OsString::from("/genome"),
                OsString::from("--parent_dir"),
                OsString::from("/out"),
                OsString::from("foo.bismark.cov.gz"),
            ]
        );
    }

    #[test]
    fn build_coverage2cytosine_argv_with_cx_context_flag() {
        let mut cfg = default_config();
        cfg.cx_context = true;
        let argv = build_coverage2cytosine_argv(
            &cfg,
            "foo.bismark.cov.gz",
            "foo.CX_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert!(argv.contains(&OsString::from("--CX_context")));
    }

    #[test]
    fn build_coverage2cytosine_argv_with_split_by_chromosome() {
        let mut cfg = default_config();
        cfg.split_by_chromosome = true;
        let argv = build_coverage2cytosine_argv(
            &cfg,
            "foo.bismark.cov.gz",
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert!(argv.contains(&OsString::from("--split_by_chromosome")));
    }

    #[test]
    fn build_coverage2cytosine_argv_with_gzip() {
        let mut cfg = default_config();
        cfg.gzip = true;
        let argv = build_coverage2cytosine_argv(
            &cfg,
            "foo.bismark.cov.gz",
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert!(argv.contains(&OsString::from("--gzip")));
    }

    #[test]
    fn build_coverage2cytosine_argv_positional_is_coverage_file() {
        let cfg = default_config();
        let argv = build_coverage2cytosine_argv(
            &cfg,
            "foo.bismark.cov.gz",
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        // Last argv element is the .bismark.cov.gz positional infile.
        assert_eq!(argv.last(), Some(&OsString::from("foo.bismark.cov.gz")));
    }

    #[test]
    fn build_coverage2cytosine_argv_passes_parent_dir_equal_to_dir() {
        // rev 1 I13: --parent_dir takes the SAME value as --dir per Perl :404.
        let cfg = default_config();
        let argv = build_coverage2cytosine_argv(
            &cfg,
            "foo.bismark.cov.gz",
            "foo.CpG_report.txt",
            Path::new("/some/output/dir"),
            Path::new("/genome"),
        );
        let dir_idx = argv
            .iter()
            .position(|a| a == &OsString::from("--dir"))
            .unwrap();
        let parent_idx = argv
            .iter()
            .position(|a| a == &OsString::from("--parent_dir"))
            .unwrap();
        assert_eq!(argv[dir_idx + 1], argv[parent_idx + 1]);
        assert_eq!(argv[dir_idx + 1], OsString::from("/some/output/dir"));
    }

    // ─── Ring buffer ────────────────────────────────────────────────────

    #[test]
    fn ring_buffer_under_capacity_returns_full_content() {
        let mut rb = RingBuffer::new(1024);
        rb.push_bytes(b"hello");
        rb.push_bytes(b" world");
        assert_eq!(rb.into_vec(), b"hello world".to_vec());
    }

    #[test]
    fn ring_buffer_evicts_oldest_when_capacity_exceeded() {
        // Push 1.5x cap; final snapshot is exactly the last `cap` bytes.
        let cap = 100;
        let mut rb = RingBuffer::new(cap);
        let big: Vec<u8> = (0u8..150).collect();
        rb.push_bytes(&big);
        let snap = rb.into_vec();
        assert_eq!(snap.len(), cap);
        assert_eq!(snap, big[50..].to_vec());
    }

    #[test]
    fn ring_buffer_line_exactly_at_capacity_replaces_entirely() {
        let cap = 100;
        let mut rb = RingBuffer::new(cap);
        rb.push_bytes(&[b'a'; 50]);
        // Now push something exactly cap-sized — should replace entirely
        // (push_bytes's >= cap branch fires).
        let exact: Vec<u8> = vec![b'b'; 100];
        rb.push_bytes(&exact);
        assert_eq!(rb.into_vec(), exact);
    }

    #[test]
    fn ring_buffer_line_larger_than_capacity_keeps_trailing_cap_bytes() {
        // rev 1: pin exact length + trailing-bytes assertion (replaces
        // rev 0's "some bounded substring" vacuous test).
        let cap = 64;
        let mut rb = RingBuffer::new(cap);
        let big: Vec<u8> = (0u8..200).cycle().take(128).collect();
        rb.push_bytes(&big);
        let snap = rb.into_vec();
        assert_eq!(snap.len(), cap);
        assert_eq!(snap, big[64..].to_vec());
    }

    // Subprocess discovery tests use `std::env::{set_var, remove_var}` which
    // are `unsafe` in Rust 2024+; the crate's `#![forbid(unsafe_code)]` blocks
    // them at the inline-test level. They live in
    // `tests/phase_g_discovery.rs` (separate crate; forbid does not apply).
}
