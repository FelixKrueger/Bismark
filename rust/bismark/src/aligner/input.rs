//! Read files held in memory instead of on disk (`bismark run` streams Trim
//! Galore's output here).
//!
//! A path registered with [`register`] is never opened on disk: every
//! [`open`] of it reads the bytes a producer pushes. In the streaming
//! configuration the aligner reads each read file exactly twice from start to
//! end — the conversion pass and the methylation-call merge — plus short reads
//! of its start (the CRLF check). So the stream keeps its first [`PREFIX`] bytes
//! for those, and frees every later chunk once [`FULL_READERS`] readers have
//! read it. The producer waits while more than [`LIMIT`] bytes are held, which
//! bounds memory to the lag between the two passes.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Bytes kept from the start of the stream for start-of-file checks.
const PREFIX: usize = 1 << 20;
/// Readers that read the whole stream: the conversion pass and the merge.
const FULL_READERS: u8 = 2;
/// Bytes beyond the prefix the producer may hold before it waits.
const LIMIT: usize = 256 << 20;

#[derive(Default)]
struct State {
    chunks: Vec<Option<Arc<[u8]>>>,
    /// Full readers that have read past each chunk.
    passed: Vec<u8>,
    /// `chunks[..prefix_chunks]` are never freed.
    prefix_chunks: usize,
    prefix_bytes: usize,
    /// Bytes held in chunks past the prefix.
    held: usize,
    /// The producer finished: `Ok` at the end of the data, `Err` if it failed.
    done: Option<Result<(), String>>,
    /// The consumer side stopped (the aligner returned): pushes fail.
    closed: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static STREAMS: Mutex<Vec<(PathBuf, Arc<Shared>)>> = Mutex::new(Vec::new());

fn streams() -> MutexGuard<'static, Vec<(PathBuf, Arc<Shared>)>> {
    STREAMS.lock().unwrap_or_else(|e| e.into_inner())
}

fn lookup(path: &Path) -> Option<Arc<Shared>> {
    streams()
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, s)| Arc::clone(s))
}

/// Serve `path` from memory; the returned writer pushes its bytes.
pub fn register(path: &Path) -> StreamWriter {
    let shared = Arc::new(Shared {
        state: Mutex::new(State::default()),
        changed: Condvar::new(),
    });
    streams().push((path.to_path_buf(), Arc::clone(&shared)));
    StreamWriter(shared)
}

/// Stop serving `path`: a producer still pushing gets an error.
pub fn unregister(path: &Path) {
    let mut all = streams();
    if let Some(i) = all.iter().position(|(p, _)| p == path) {
        let (_, shared) = all.swap_remove(i);
        shared.lock().closed = true;
        shared.changed.notify_all();
    }
}

/// True if `path` is served from memory (it need not exist on disk).
pub fn is_stream(path: &Path) -> bool {
    lookup(path).is_some()
}

/// Open a read file: from memory if `path` is registered, else from disk.
pub(crate) fn open(path: &Path) -> io::Result<Box<dyn Read + Send>> {
    Ok(match lookup(path) {
        Some(shared) => Box::new(StreamReader {
            shared,
            chunk: 0,
            offset: 0,
        }),
        None => Box::new(File::open(path)?),
    })
}

/// The producer side of a registered stream.
pub struct StreamWriter(Arc<Shared>);

impl StreamWriter {
    /// Append `bytes`, waiting while the readers lag more than [`LIMIT`] behind.
    pub fn push(&self, bytes: Vec<u8>) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let mut st = self.0.lock();
        while st.held > LIMIT && !st.closed {
            st = self.0.changed.wait(st).unwrap_or_else(|e| e.into_inner());
        }
        if st.closed {
            return Err(io::Error::other("the aligner stopped reading"));
        }
        let len = bytes.len();
        if st.prefix_bytes < PREFIX {
            st.prefix_bytes += len;
            st.prefix_chunks += 1;
        } else {
            st.held += len;
        }
        st.chunks.push(Some(bytes.into()));
        st.passed.push(0);
        self.0.changed.notify_all();
        Ok(())
    }

    /// End the stream: readers see the end of the data, or `Err(message)`.
    pub fn finish(self, result: Result<(), String>) {
        self.0.lock().done = Some(result);
        self.0.changed.notify_all();
    }
}

impl Drop for StreamWriter {
    fn drop(&mut self) {
        let mut st = self.0.lock();
        st.done
            .get_or_insert_with(|| Err("the read stream stopped before its end".into()));
        self.0.changed.notify_all();
    }
}

struct StreamReader {
    shared: Arc<Shared>,
    chunk: usize,
    offset: usize,
}

impl Read for StreamReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut st = self.shared.lock();
        loop {
            if self.chunk < st.chunks.len() {
                let Some(data) = st.chunks[self.chunk].clone() else {
                    return Err(io::Error::other(
                        "a streamed read file was read more than twice; bismark run supports \
                         only the aligner's streaming mode (pass --skip_trim otherwise)",
                    ));
                };
                let n = (data.len() - self.offset).min(buf.len());
                buf[..n].copy_from_slice(&data[self.offset..self.offset + n]);
                self.offset += n;
                if self.offset == data.len() {
                    if self.chunk >= st.prefix_chunks {
                        st.passed[self.chunk] += 1;
                        if st.passed[self.chunk] == FULL_READERS {
                            st.chunks[self.chunk] = None;
                            st.held -= data.len();
                            self.shared.changed.notify_all();
                        }
                    }
                    self.chunk += 1;
                    self.offset = 0;
                }
                return Ok(n);
            }
            match &st.done {
                Some(Ok(())) => return Ok(0),
                Some(Err(e)) => return Err(io::Error::other(e.clone())),
                None => {
                    st = self
                        .shared
                        .changed
                        .wait(st)
                        .unwrap_or_else(|e| e.into_inner())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(path: &Path) -> io::Result<Vec<u8>> {
        let mut v = Vec::new();
        open(path)?.read_to_end(&mut v)?;
        Ok(v)
    }

    #[test]
    fn two_full_readers_and_a_peek_see_the_same_bytes() {
        let path = Path::new("/input-test/two.fq");
        let w = register(path);
        let data: Vec<u8> = (0..3 * PREFIX).map(|i| (i % 251) as u8).collect();
        let producer = {
            let data = data.clone();
            std::thread::spawn(move || {
                for c in data.chunks(64 * 1024) {
                    w.push(c.to_vec()).unwrap();
                }
                w.finish(Ok(()));
            })
        };
        let a = std::thread::spawn(move || read_all(Path::new("/input-test/two.fq")).unwrap());
        let b = read_all(path).unwrap();
        // A late short read of the start is served from the kept prefix.
        let mut start = [0u8; 100];
        open(path).unwrap().read_exact(&mut start).unwrap();
        producer.join().unwrap();
        assert_eq!(a.join().unwrap(), data);
        assert_eq!(b, data);
        assert_eq!(&start[..], &data[..100]);
        // Both passes done: everything past the prefix was freed.
        let shared = lookup(path).unwrap();
        assert_eq!(shared.lock().held, 0);
        // A third full read is refused, not silently short.
        assert!(read_all(path).is_err());
        unregister(path);
        assert!(!is_stream(path));
    }

    #[test]
    fn a_producer_that_stops_early_is_an_error() {
        let path = Path::new("/input-test/stopped.fq");
        let w = register(path);
        w.push(b"@r\nACGT\n+\nIIII\n".to_vec()).unwrap();
        drop(w);
        assert!(read_all(path).is_err());
        unregister(path);
    }

    #[test]
    fn unregister_unblocks_a_waiting_producer() {
        let path = Path::new("/input-test/closed.fq");
        let w = register(path);
        unregister(path);
        assert!(w.push(vec![1]).is_err());
    }
}
