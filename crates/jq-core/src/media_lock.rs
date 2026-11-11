//! Keeps the show's media files from being changed during a performance.
//!
//! Only Windows can enforce this: a file held open without write and delete
//! sharing cannot be written, replaced, renamed or deleted by anyone, while
//! reading (our own loader included) stays possible. Linux and macOS only
//! know advisory locks, which other programs ignore; there the watcher's
//! warnings are all there is.

use std::fs::File;
use std::io;
use std::path::PathBuf;

pub struct MediaLock {
    _files: Vec<File>,
}

impl MediaLock {
    /// Whether this platform can lock at all.
    pub const SUPPORTED: bool = cfg!(windows);

    /// Locks the existing files of `paths` (missing ones are reported
    /// elsewhere). Returns the files that could not be locked, e.g. because
    /// another program has them open for writing.
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> (Self, Vec<(PathBuf, io::Error)>) {
        let mut files = Vec::new();
        let mut failed = Vec::new();
        if Self::SUPPORTED {
            for path in paths.into_iter().filter(|p| p.is_file()) {
                match open_locked(&path) {
                    Ok(f) => files.push(f),
                    Err(e) => failed.push((path, e)),
                }
            }
        }
        (Self { _files: files }, failed)
    }
}

#[cfg(windows)]
fn open_locked(path: &std::path::Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x1;
    std::fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(path)
}

#[cfg(not(windows))]
fn open_locked(_: &std::path::Path) -> io::Result<File> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn locked_file_can_be_read_but_not_changed() {
        let dir = std::env::temp_dir().join(format!("jq-core-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        std::fs::write(&path, b"one").unwrap();

        let (lock, failed) = MediaLock::new([path.clone(), dir.join("missing.wav")]);
        assert!(failed.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
        assert!(std::fs::write(&path, b"two").is_err());
        assert!(std::fs::rename(&path, dir.join("b.wav")).is_err());
        assert!(std::fs::remove_file(&path).is_err());

        drop(lock);
        std::fs::write(&path, b"two").unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
