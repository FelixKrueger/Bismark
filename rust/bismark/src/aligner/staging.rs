//! Final outputs are written as `<name>.<pid>.partial` and renamed to `<name>` once
//! the input that produced them has finished, so a failed run never leaves a file
//! that looks complete.
//!
//! Staging is only allowed inside [`scope`] on the thread that runs it; a stage from
//! any other thread is an error, so a file can never be left staged unnoticed.

use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

/// Outputs not yet committed, and scratch files to delete on failure.
#[derive(Default)]
struct Pending {
    /// Set while a [`scope`] runs on this thread.
    active: bool,
    /// `(staged, final)` paths, in registration order.
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

fn outside_scope(path: &Path) -> io::Error {
    io::Error::other(format!(
        "internal: output staged outside a driver scope: {}",
        path.display()
    ))
}

/// Run `f` as one driver run: publish its staged outputs if it succeeds, delete them
/// and its scratch files if it fails.
pub(crate) fn scope<T, E: From<io::Error>>(f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let was_active = PENDING.with(|p| std::mem::replace(&mut p.borrow_mut().active, true));
    let result = f().and_then(|v| match commit() {
        Ok(()) => Ok(v),
        Err(e) => Err(E::from(e)),
    });
    if result.is_err() {
        discard();
    }
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        p.active = was_active;
        if !was_active {
            p.scratch.clear();
        }
    });
    result
}

/// Register `path` as a final output and return the path to write it under.
pub(crate) fn stage(path: &Path) -> io::Result<PathBuf> {
    let staged = partial_name(path);
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        if !p.active {
            return Err(outside_scope(path));
        }
        p.staged.push((staged.clone(), path.to_path_buf()));
        Ok(staged)
    })
}

/// [`File::create`] for a final output, under its staged name.
pub(crate) fn create(path: &Path) -> io::Result<File> {
    File::create(stage(path)?)
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

/// Register a scratch file that a failed [`scope`] deletes.
pub(crate) fn scratch(path: &Path) -> io::Result<()> {
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        if !p.active {
            return Err(outside_scope(path));
        }
        p.scratch.push(path.to_path_buf());
        Ok(())
    })
}

/// Rename every staged output to its final name, all or nothing. Outputs are renamed
/// in reverse registration order, so the BAM, staged first, is published last.
pub(crate) fn commit() -> io::Result<()> {
    let staged = PENDING.with(|p| std::mem::take(&mut p.borrow_mut().staged));
    let mut published: Vec<&Path> = Vec::new();
    for (from, to) in staged.iter().rev() {
        if let Err(e) = std::fs::rename(from, to) {
            for done in published {
                let _ = std::fs::remove_file(done);
            }
            for (left, _) in &staged {
                let _ = std::fs::remove_file(left);
            }
            return Err(io::Error::new(
                e.kind(),
                format!("publishing {} as {}: {e}", from.display(), to.display()),
            ));
        }
        published.push(to);
    }
    Ok(())
}

/// Delete every staged output and scratch file of the failed run.
fn discard() {
    let (staged, scratch) = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        (
            std::mem::take(&mut p.staged),
            std::mem::take(&mut p.scratch),
        )
    });
    for path in staged.into_iter().map(|(s, _)| s).chain(scratch) {
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
    fn scope_publishes_on_success_and_removes_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join("a.bam");
        let drop_ = dir.path().join("b.bam");
        let stale = partial_name(&dir.path().join("c.bam"));
        std::fs::write(&stale, b"left by a killed run").unwrap();

        scope(|| -> io::Result<()> {
            std::fs::write(stage(&keep)?, b"done")?;
            assert!(current(&keep).to_string_lossy().ends_with(".partial"));
            Ok(())
        })
        .unwrap();
        assert_eq!(std::fs::read(&keep).unwrap(), b"done");
        assert_eq!(current(&keep), keep);

        let tmp = dir.path().join(".scratch");
        let err = scope(|| -> io::Result<()> {
            io::Write::write_all(&mut create(&drop_)?, b"half")?;
            std::fs::write(&tmp, b"x")?;
            scratch(&tmp)?;
            Err(io::Error::other("engine failed"))
        });
        assert!(err.is_err());
        assert!(!drop_.exists() && !partial_name(&drop_).exists() && !tmp.exists());
        assert!(keep.exists() && stale.exists());
    }

    #[test]
    fn staging_outside_a_scope_or_on_another_thread_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("x.bam");
        assert!(stage(&out).is_err());
        assert!(scratch(&out).is_err());
        scope(|| -> io::Result<()> {
            let other = std::thread::scope(|s| s.spawn(|| stage(&out).is_err()).join().unwrap());
            assert!(other, "a worker thread must not stage");
            Ok(())
        })
        .unwrap();
        assert!(!out.exists());
    }

    #[test]
    fn a_failed_rename_publishes_nothing() {
        // A directory at a final path makes that rename fail (EISDIR on Linux and macOS).
        for blocked in ["r.txt", "a.bam"] {
            let dir = tempfile::tempdir().unwrap();
            let bam = dir.path().join("a.bam");
            let report = dir.path().join("r.txt");
            std::fs::create_dir(dir.path().join(blocked)).unwrap();
            let err = scope(|| -> io::Result<()> {
                std::fs::write(stage(&bam)?, b"bam")?;
                std::fs::write(stage(&report)?, b"report")?;
                Ok(())
            })
            .unwrap_err();
            assert!(err.to_string().contains(blocked), "{err}");
            for f in [&bam, &report] {
                assert!(!partial_name(f).exists());
                assert!(f.is_dir() || !f.exists(), "{} was published", f.display());
            }
        }
    }
}
