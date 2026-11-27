//! The first signal that killed an aligner child, so `bismark align` can exit 128 + N.
//! Seen in a child's exit status ([`observe`]) or a wrapper's `died with signal N` line ([`Relay`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a relay may still run once its child has ended (a grandchild can hold stderr).
const RELAY_GRACE: Duration = Duration::from_secs(5);

static FIRST: Mutex<Option<(i32, &'static str)>> = Mutex::new(None);

/// Forget any signal recorded by an earlier run in this process.
pub(crate) fn reset() {
    if let Ok(mut first) = FIRST.lock() {
        *first = None;
    }
}

/// Record that `who` was killed by `sig`, unless a signal was already recorded.
pub(crate) fn note(sig: i32, who: &'static str) {
    if (1..=127).contains(&sig)
        && let Ok(mut first) = FIRST.lock()
        && first.is_none()
    {
        *first = Some((sig, who));
    }
}

/// The first recorded signal and the aligner it killed.
pub(crate) fn first() -> Option<(i32, &'static str)> {
    FIRST.lock().ok().and_then(|f| *f)
}

/// Record the signal that ended a child Bismark did not kill itself, if any.
pub(crate) fn observe(status: &ExitStatus, who: &'static str) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            note(sig, who);
        }
    }
    #[cfg(not(unix))]
    let _ = (status, who);
}

/// The signal number in a wrapper's `bowtie2-align died with signal 9 (KILL)` line.
pub(crate) fn wrapper_signal(line: &str) -> Option<i32> {
    let rest = line.split_once("-align died with signal ")?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|s| (1..=127).contains(s))
}

/// Exit code (128 + the first recorded signal, else 1) and root-cause line for a failed run.
pub(crate) fn failure() -> (u8, Option<String>) {
    match first() {
        Some((sig, who)) => {
            let oom = if sig == 9 {
                " (a kill by signal 9 usually means it ran out of memory)"
            } else {
                ""
            };
            let code = u8::try_from(128 + sig).unwrap_or(255);
            (code, Some(format!("{who} was killed by signal {sig}{oom}")))
        }
        None => (1, None),
    }
}

/// Copies one aligner's stderr to Bismark's stderr, noting a wrapper's signal line.
pub(crate) struct Relay {
    handle: Option<JoinHandle<()>>,
    killed: Arc<AtomicBool>,
}

impl Relay {
    /// Start relaying `stderr`; a signal line is recorded against `who`.
    pub(crate) fn spawn(stderr: impl Read + Send + 'static, who: &'static str) -> Self {
        Self::spawn_with(stderr, false, move |sig| note(sig, who))
    }

    fn spawn_with(
        stderr: impl Read + Send + 'static,
        killed: bool,
        on_signal: impl Fn(i32) + Send + 'static,
    ) -> Self {
        let killed = Arc::new(AtomicBool::new(killed));
        let ignore = Arc::clone(&killed);
        let handle = std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
                let _ = std::io::stderr().lock().write_all(&line);
                if !ignore.load(Ordering::SeqCst)
                    && let Some(sig) = wrapper_signal(&String::from_utf8_lossy(&line))
                {
                    on_signal(sig);
                }
                line.clear();
            }
        });
        Self {
            handle: Some(handle),
            killed,
        }
    }

    /// Bismark is about to kill this child: lines from now on are not its own failure.
    pub(crate) fn mark_killed(&self) {
        self.killed.store(true, Ordering::SeqCst);
    }

    /// Wait up to a few seconds for the relay to copy the rest of the child's stderr.
    pub(crate) fn finish(&mut self) {
        if let Some(handle) = self.handle.take() {
            let deadline = Instant::now() + RELAY_GRACE;
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    fn relayed(input: &[u8], killed: bool) -> i32 {
        let seen = Arc::new(AtomicI32::new(0));
        let sink = Arc::clone(&seen);
        let mut relay = Relay::spawn_with(std::io::Cursor::new(input.to_vec()), killed, move |s| {
            sink.store(s, Ordering::SeqCst)
        });
        relay.finish();
        seen.load(Ordering::SeqCst)
    }

    #[test]
    fn parses_bowtie2_and_hisat2_wrapper_lines() {
        assert_eq!(
            wrapper_signal("(ERR): bowtie2-align died with signal 9 (KILL) \n"),
            Some(9)
        );
        assert_eq!(
            wrapper_signal("(ERR): hisat2-align died with signal 15 (TERM) (core dumped)\n"),
            Some(15)
        );
    }

    #[test]
    fn ignores_other_lines_and_bad_numbers() {
        for line in [
            "(ERR): bowtie2-align exited with value 1\n",
            "10000 reads; of these:\n",
            "bowtie2-align died with signal x\n",
            "bowtie2-align died with signal 0 (ZERO)\n",
            "bowtie2-align died with signal 4000000000 (BIG)\n",
        ] {
            assert_eq!(wrapper_signal(line), None, "{line}");
        }
    }

    #[test]
    fn relay_notes_a_signal_line_after_other_output() {
        let mut input = b"Warning: skipping read\n".repeat(5000);
        input.extend_from_slice(b"(ERR): bowtie2-align died with signal 9 (KILL) \n");
        assert_eq!(relayed(&input, false), 9);
    }

    #[test]
    fn relay_ignores_a_child_bismark_killed() {
        assert_eq!(
            relayed(b"(ERR): bowtie2-align died with signal 9 (KILL) \n", true),
            0
        );
    }
}
