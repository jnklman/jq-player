//! Media work that must not hold up the show thread: reading file lengths,
//! copying files into the media folder and searching for missing files.
//! Each runs on a thread of its own; the session collects the results.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use jq_show::Show;
use jq_show::paths::{self, FileStamp, FoundOutside, NameCheck, Relinked};

/// A file's length as read by the [`Prober`].
pub struct Probed {
    /// Stored path, as requested.
    pub stored: String,
    pub version: u32,
    /// Taken before reading, so a change meanwhile is noticed.
    pub stamp: Option<FileStamp>,
    /// Seconds; `None` if the file is missing, broken or does not say.
    pub duration: Option<f64>,
    /// Why a file that is there cannot be read (not audio, broken).
    pub unreadable: Option<String>,
}

/// Reads file lengths from their headers, one after the other.
pub struct Prober {
    requests: Sender<(String, PathBuf, u32)>,
    results: Receiver<Probed>,
}

impl Prober {
    pub fn new() -> Self {
        let (requests, jobs) = mpsc::channel::<(String, PathBuf, u32)>();
        let (done, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("jq-probe".into())
            .spawn(move || {
                for (stored, path, version) in jobs {
                    let stamp = paths::stamp(&path);
                    let (duration, unreadable) = match jq_media::probe_duration(&path) {
                        Ok(duration) => (duration, None),
                        // A missing file is reported as such elsewhere.
                        Err(e) => (None, stamp.is_some().then(|| e.reason())),
                    };
                    if done.send(Probed { stored, version, stamp, duration, unreadable }).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn probe thread");
        Self { requests, results }
    }

    /// Queues `path` (stored as `stored`); its result carries `version`.
    pub fn request(&self, stored: &str, path: PathBuf, version: u32) {
        let _ = self.requests.send((stored.to_owned(), path, version));
    }

    pub fn poll(&self) -> Vec<Probed> {
        self.results.try_iter().collect()
    }
}

/// A copy into the media folder, queued or under way.
pub struct Copy {
    /// Stored path the cues already use.
    pub stored: String,
    pub target: PathBuf,
    /// Bytes copied so far, written by the copy thread.
    pub done: Arc<AtomicU64>,
    pub total: u64,
}

impl Copy {
    /// 0…1.
    pub fn progress(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done.load(Ordering::Relaxed) as f64 / self.total as f64) as f32
        }
    }
}

/// A finished copy: its target and whether it worked.
pub type Copied = (PathBuf, Result<(), String>);

/// Copies files one after the other. Dropping it stops the copy under way
/// (nothing half-written is left) and skips the rest.
pub struct Copier {
    jobs: Sender<(PathBuf, PathBuf, Arc<AtomicU64>)>,
    results: Receiver<Copied>,
    stop: Arc<AtomicBool>,
}

impl Copier {
    pub fn new() -> Self {
        let (jobs, queue) = mpsc::channel::<(PathBuf, PathBuf, Arc<AtomicU64>)>();
        let (done, results) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("jq-copy".into())
            .spawn(move || {
                for (from, to, progress) in queue {
                    if stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = paths::copy_file(&from, &to, |bytes| {
                        progress.store(bytes, Ordering::Relaxed);
                        !stopped.load(Ordering::Relaxed)
                    });
                    let result = result.map_err(|e| format!("{}: {e}", from.display()));
                    if done.send((to, result)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn copy thread");
        Self { jobs, results, stop }
    }

    /// Queues a copy of `from` to `to`; returns it for display.
    pub fn copy(&self, from: PathBuf, to: PathBuf, stored: String) -> Copy {
        let total = std::fs::metadata(&from).map_or(0, |m| m.len());
        let done = Arc::new(AtomicU64::new(0));
        let _ = self.jobs.send((from, to.clone(), done.clone()));
        Copy { stored, target: to, done, total }
    }

    pub fn poll(&self) -> Vec<Copied> {
        self.results.try_iter().collect()
    }
}

impl Drop for Copier {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// What a search for missing files found.
pub struct Found {
    /// Found inside the media folder: relink right away.
    pub inside: Vec<Relinked>,
    /// Found outside it, and whether the media folder already holds the same
    /// file under its name.
    pub outside: Vec<(FoundOutside, bool)>,
}

/// Searches `dir` for the missing files of `show` on a thread of its own
/// (a big folder takes a while); the result arrives on the receiver.
pub fn search(show: Show, media_dir: PathBuf, dir: PathBuf) -> Receiver<Found> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut show = show;
        let (inside, outside) = paths::relink(&mut show, &media_dir, &dir);
        let files: Vec<PathBuf> = outside.iter().map(|(_, _, found)| found.clone()).collect();
        let checks = paths::name_clashes(&media_dir, &files).unwrap_or_default();
        let identical = |i: usize| checks.get(i).is_some_and(|c| *c == NameCheck::Identical);
        let outside = outside.into_iter().enumerate().map(|(i, found)| (found, identical(i))).collect();
        let _ = tx.send(Found { inside, outside });
    });
    rx
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jq-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait<T>(mut poll: impl FnMut() -> Vec<T>) -> Vec<T> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let found = poll();
            if !found.is_empty() || Instant::now() > deadline {
                return found;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn copies_in_the_background_and_reports_progress() {
        let dir = tempdir("copier");
        let from = dir.join("a.wav");
        std::fs::write(&from, vec![1u8; 3 << 20]).unwrap();
        let copier = Copier::new();
        let copy = copier.copy(from.clone(), dir.join("media/a.wav"), "a.wav".into());
        assert_eq!(copy.total, 3 << 20);
        let done = wait(|| copier.poll());
        assert_eq!(done.len(), 1);
        assert_eq!(done[0], (dir.join("media/a.wav"), Ok(())));
        assert_eq!(copy.progress(), 1.0);
        assert_eq!(std::fs::read(dir.join("media/a.wav")).unwrap().len(), 3 << 20);

        // An existing file is never overwritten.
        copier.copy(from, dir.join("media/a.wav"), "a.wav".into());
        assert!(wait(|| copier.poll())[0].1.is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dropped_copier_leaves_nothing_half_written() {
        let dir = tempdir("copier-drop");
        let from = dir.join("big.wav");
        std::fs::write(&from, vec![1u8; 64 << 20]).unwrap();
        let copier = Copier::new();
        for i in 0..3 {
            copier.copy(from.clone(), dir.join(format!("media/{i}.wav")), String::new());
        }
        drop(copier);
        std::thread::sleep(Duration::from_millis(300));
        let left: Vec<_> = std::fs::read_dir(dir.join("media")).map_or(Vec::new(), |d| d.flatten().collect());
        assert!(left.len() < 3, "the rest was skipped");
        for entry in &left {
            // Only a copy that finished before the stop may be there, complete.
            assert_eq!(entry.metadata().unwrap().len(), 64 << 20, "{:?}", entry.file_name());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prober_reads_lengths_and_missing_files() {
        let prober = Prober::new();
        prober.request("weg.wav", PathBuf::from("/does/not/exist.wav"), 3);
        let probed = wait(|| prober.poll());
        assert_eq!((probed[0].stored.as_str(), probed[0].version), ("weg.wav", 3));
        assert_eq!((probed[0].stamp, probed[0].duration), (None, None));
        // Missing is no reading error.
        assert_eq!(probed[0].unreadable, None);

        let dir = std::env::temp_dir().join(format!("jq-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("notes.wav");
        std::fs::write(&text, "no audio here").unwrap();
        prober.request("notes.wav", text, 1);
        let probed = wait(|| prober.poll());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(probed[0].stamp.is_some());
        assert_eq!(probed[0].unreadable.as_deref(), Some("not a supported audio format"));
    }
}
