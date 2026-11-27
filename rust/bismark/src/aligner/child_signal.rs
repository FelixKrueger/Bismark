//! The first signal that killed an aligner child, so `bismark align` can exit 128 + N.
//!
//! The `bowtie2`/`hisat2` wrapper scripts turn a signal that kills the real aligner into
//! exit status 1 and a `… died with signal N` line on stderr, so their stderr is relayed
//! through [`relay`], which watches for that line. Directly spawned aligners report the
//! signal in their exit status ([`observe`]). The first signal recorded wins.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

static FIRST_SIGNAL: AtomicI32 = AtomicI32::new(0);
static RELAYS: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

/// Record `sig` unless a signal was already recorded.
pub(crate) fn note(sig: i32) {
    if sig > 0 {
        let _ = FIRST_SIGNAL.compare_exchange(0, sig, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Record the signal that ended a child Bismark did not kill itself, if any.
pub(crate) fn observe(status: &ExitStatus) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            note(sig);
        }
    }
    #[cfg(not(unix))]
    let _ = status;
}

/// The signal number in a wrapper's `bowtie2-align died with signal 9 (KILL)` line.
pub(crate) fn wrapper_signal(line: &str) -> Option<i32> {
    let rest = line.split_once("-align died with signal ")?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|&s| s > 0)
}

/// Copy `stderr` to this process's stderr line by line, noting a wrapper's signal line.
/// Lines read after `killed` is set came from a child Bismark killed itself and are not noted.
pub(crate) fn relay(stderr: impl Read + Send + 'static, killed: Arc<AtomicBool>) {
    let handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = Vec::new();
        while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            let _ = std::io::stderr().lock().write_all(&line);
            if !killed.load(Ordering::SeqCst)
                && let Some(sig) = wrapper_signal(&String::from_utf8_lossy(&line))
            {
                note(sig);
            }
            line.clear();
        }
    });
    if let Ok(mut relays) = RELAYS.lock() {
        relays.push(handle);
    }
}

/// Exit code for a failed run: 128 + the first recorded signal, else 1.
/// Waits for the stderr relays first; their children are reaped by now.
pub(crate) fn failure_exit_code() -> u8 {
    let handles: Vec<_> = RELAYS
        .lock()
        .map(|mut r| std::mem::take(&mut *r))
        .unwrap_or_default();
    for h in handles {
        let _ = h.join();
    }
    match FIRST_SIGNAL.load(Ordering::SeqCst) {
        0 => 1,
        sig => u8::try_from(128 + sig).unwrap_or(255),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn relay_ignores_lines_once_killed() {
        let killed = Arc::new(AtomicBool::new(true));
        relay(
            std::io::Cursor::new(b"(ERR): bowtie2-align died with signal 9 (KILL) \n".to_vec()),
            killed,
        );
        let handles: Vec<_> = std::mem::take(&mut *RELAYS.lock().unwrap());
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(FIRST_SIGNAL.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ignores_other_lines() {
        assert_eq!(
            wrapper_signal("(ERR): bowtie2-align exited with value 1\n"),
            None
        );
        assert_eq!(wrapper_signal("10000 reads; of these:\n"), None);
        assert_eq!(wrapper_signal("bowtie2-align died with signal x\n"), None);
    }
}
