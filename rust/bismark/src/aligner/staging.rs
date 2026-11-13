//! Final outputs are written as `<name>.<pid>.partial` and renamed to `<name>` once
//! the input that produced them has finished, so a failed run never leaves a file
//! that looks complete.
//!
//! The registry is per thread: a driver stages and commits on the thread that runs
//! it, and concurrent unit tests never see each other's files.

use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

/// Outputs not yet committed, and scratch files to delete on failure.
#[derive(Default)]
struct Pending {
    /// `(staged, final)` paths.
    staged: Vec<(PathBuf, PathBuf)>,
    scratch: Vec<PathBuf>,
}

thread_local! {
    static PENDING: RefCell<Pending> = RefCell::new(Pending::default());
}

/// The staged name for `path`: `<name>.<pid>.partial` in the same directory.
fn partial_name(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.partial", std::process::id()));
    PathBuf::from(name)
}

/// Register `path` as a final output and return the path to write it under.
pub(crate) fn stage(path: &Path) -> PathBuf {
    let staged = partial_name(path);
    PENDING.with(|p| {
        p.borrow_mut()
            .staged
            .push((staged.clone(), path.to_path_buf()))
    });
    staged
}

/// [`File::create`] for a final output, under its staged name.
pub(crate) fn create(path: &Path) -> io::Result<File> {
    File::create(stage(path))
}

/// Where a final output currently is: its staged name until committed.
pub(crate) fn current(path: &Path) -> PathBuf {
    PENDING.with(|p| {
        p.borrow()
            .staged
            .iter()
            .rev()
            .find(|(_, f)| f == path)
            .map_or_else(|| path.to_path_buf(), |(s, _)| s.clone())
    })
}

/// Register a scratch file that [`discard`] deletes if the run fails.
pub(crate) fn scratch(path: &Path) {
    PENDING.with(|p| p.borrow_mut().scratch.push(path.to_path_buf()));
}

/// Rename every staged output to its final name. On a failed rename the rest stay
/// staged, so [`discard`] still removes them.
pub(crate) fn commit() -> io::Result<()> {
    let staged = PENDING.with(|p| std::mem::take(&mut p.borrow_mut().staged));
    for (i, (from, to)) in staged.iter().enumerate() {
        if let Err(e) = std::fs::rename(from, to) {
            PENDING.with(|p| p.borrow_mut().staged.extend_from_slice(&staged[i..]));
            return Err(e);
        }
    }
    Ok(())
}

/// Delete every staged output and scratch file of the failed run.
pub(crate) fn discard() {
    let pending = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for path in pending
        .staged
        .into_iter()
        .map(|(s, _)| s)
        .chain(pending.scratch)
    {
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != io::ErrorKind::NotFound
        {
            eprintln!(
                "Warning: could not remove {} after the failed run: {e}",
                path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_renames_and_discard_removes_only_staged_files() {
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join("a.bam");
        let drop_ = dir.path().join("b.bam");
        let stale = partial_name(&dir.path().join("c.bam"));
        std::fs::write(&stale, b"left by a killed run").unwrap();

        std::fs::write(stage(&keep), b"done").unwrap();
        assert!(current(&keep).to_string_lossy().ends_with(".partial"));
        commit().unwrap();
        assert_eq!(std::fs::read(&keep).unwrap(), b"done");
        assert_eq!(current(&keep), keep);

        let mut f = create(&drop_).unwrap();
        io::Write::write_all(&mut f, b"half").unwrap();
        let tmp = dir.path().join(".scratch");
        std::fs::write(&tmp, b"x").unwrap();
        scratch(&tmp);
        discard();
        assert!(!drop_.exists() && !partial_name(&drop_).exists() && !tmp.exists());
        assert!(keep.exists() && stale.exists());
    }
}
