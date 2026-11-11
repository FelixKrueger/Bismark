//! In-memory record taps for `bismark run`.
//!
//! `bismark run` registers a channel for an output BAM path before a step starts.
//! The first [`crate::io::BamWriter`] / [`crate::io::ThreadedBamWriter`] opened at
//! that path still writes the file, and also sends its header, each record, and an
//! end marker on `finish()` to the channel. The next step reads a [`TapSource`]
//! instead of re-reading the file from disk, and runs at the same time.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crossbeam_channel::{Receiver, Sender};
use noodles_sam::Header;

use crate::io::error::BismarkIoError;
use crate::io::record::BismarkRecord;

/// One message from a tapped writer: the header first, then records, then `End`.
pub enum Tapped {
    /// The header the writer wrote.
    Header(Header),
    /// One record, in file order.
    Record(BismarkRecord),
    /// The writer finished cleanly; nothing more follows.
    End,
}

static TAPS: Mutex<Vec<(PathBuf, Sender<Tapped>)>> = Mutex::new(Vec::new());

/// Messages in flight between two steps (bounds memory if the reader is slower).
const CAPACITY: usize = 1 << 14;

/// Tap the next writer opened at `path`; the returned receiver is the next step's input.
pub fn register(path: &Path) -> Receiver<Tapped> {
    let (tx, rx) = crossbeam_channel::bounded(CAPACITY);
    TAPS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((path.to_path_buf(), tx));
    rx
}

/// Remove and return the tap registered for `path`, if any (one writer per tap).
pub(crate) fn take(path: &Path) -> Option<Sender<Tapped>> {
    let mut taps = TAPS.lock().unwrap_or_else(|e| e.into_inner());
    let i = taps.iter().position(|(p, _)| p == path)?;
    Some(taps.swap_remove(i).1)
}

fn downstream_stopped() -> BismarkIoError {
    std::io::Error::other("the next `bismark run` step stopped reading").into()
}

/// The writer side of a tap. Every send error means the reader is gone.
pub(crate) struct TapSink(Sender<Tapped>);

impl TapSink {
    /// Look up a tap for `path` and send it the header.
    pub(crate) fn open(path: &Path, header: &Header) -> Result<Option<Self>, BismarkIoError> {
        let Some(tx) = take(path) else {
            return Ok(None);
        };
        tx.send(Tapped::Header(header.clone()))
            .map_err(|_| downstream_stopped())?;
        Ok(Some(Self(tx)))
    }

    pub(crate) fn record(&self, record: &BismarkRecord) -> Result<(), BismarkIoError> {
        self.0
            .send(Tapped::Record(record.clone()))
            .map_err(|_| downstream_stopped())
    }

    pub(crate) fn end(self) -> Result<(), BismarkIoError> {
        self.0.send(Tapped::End).map_err(|_| downstream_stopped())
    }
}

/// The reader side of a tap: the header, then the records.
pub struct TapSource {
    header: Header,
    rx: Receiver<Tapped>,
    done: bool,
}

impl TapSource {
    /// Wait for the writer to open and send its header.
    pub fn open(rx: Receiver<Tapped>) -> Result<Self, BismarkIoError> {
        match rx.recv() {
            Ok(Tapped::Header(header)) => Ok(Self {
                header,
                rx,
                done: false,
            }),
            _ => Err(upstream_failed()),
        }
    }

    /// The header the writer wrote.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The records in file order. A writer that stops without `finish()` (its
    /// step failed) yields an error instead of a silently short stream.
    pub fn records(&mut self) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        std::iter::from_fn(move || {
            if self.done {
                return None;
            }
            loop {
                return match self.rx.recv() {
                    // File readers drop unmapped records (flag 0x4); so does the tap.
                    Ok(Tapped::Record(r)) if u16::from(r.inner().flags()) & 0x4 != 0 => continue,
                    Ok(Tapped::Record(r)) => Some(Ok(r)),
                    Ok(Tapped::End) => {
                        self.done = true;
                        None
                    }
                    Ok(Tapped::Header(_)) | Err(_) => {
                        self.done = true;
                        Some(Err(upstream_failed()))
                    }
                };
            }
        })
    }

    /// [`Self::records`] with each record's UMI taken from its read name, as
    /// the file readers' `records_with_umi` do.
    pub fn records_with_umi(
        &mut self,
        extractor: fn(&[u8]) -> Option<&[u8]>,
    ) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        self.records().map(move |r| {
            r.map(|mut r| {
                let umi = r.inner().name().and_then(|n| extractor(n.as_ref()));
                r.set_umi(umi.map(crate::io::record::Umi::from_slice));
                r
            })
        })
    }
}

fn upstream_failed() -> BismarkIoError {
    std::io::Error::other("the previous `bismark run` step stopped before finishing its BAM").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_errors_when_the_writer_never_finishes() {
        let rx = register(Path::new("/tap-test/unfinished.bam"));
        let sink = TapSink::open(Path::new("/tap-test/unfinished.bam"), &Header::default())
            .unwrap()
            .expect("registered");
        let mut src = TapSource::open(rx).unwrap();
        drop(sink); // writer dropped without End
        let got: Vec<_> = src.records().collect();
        assert_eq!(got.len(), 1);
        assert!(got[0].is_err());
    }

    #[test]
    fn source_ends_cleanly_and_a_tap_is_taken_once() {
        let path = Path::new("/tap-test/finished.bam");
        let rx = register(path);
        let sink = TapSink::open(path, &Header::default()).unwrap().unwrap();
        assert!(TapSink::open(path, &Header::default()).unwrap().is_none());
        let mut src = TapSource::open(rx).unwrap();
        sink.end().unwrap();
        assert_eq!(src.records().count(), 0);
    }
}
