//! Decoded-buffer cache with a background loader, so the cues after the
//! playhead are in RAM before GO ("arm").

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use jq_engine::AudioBuffer;
use jq_show::paths::{self, FileStamp};

use crate::watch::Stamps;

pub trait MediaLoader: Send + Sync + 'static {
    fn load(&self, path: &Path, sample_rate: u32) -> Result<AudioBuffer, String>;
}

/// Silence of a streamed file is reported once no more came for this long,
/// as one stretch, not every few milliseconds while it lasts.
const UNDERRUN_QUIET: Duration = Duration::from_millis(500);
/// A stretch of silence that goes on is reported after this long anyway.
const UNDERRUN_LONGEST: Duration = Duration::from_secs(5);

/// Silence of one streamed file not reported yet.
struct Underruns {
    /// The stream's underrun count at the last look.
    seen: u64,
    /// Frames of the current stretch.
    pending: u64,
    first: Instant,
    last: Instant,
}

/// Decodes files from disk; long files are streamed.
pub struct FileLoader;

impl MediaLoader for FileLoader {
    fn load(&self, path: &Path, sample_rate: u32) -> Result<AudioBuffer, String> {
        jq_media::load(path, sample_rate).map_err(|e| e.to_string())
    }
}

/// The file's stamp is taken before reading it, so a change during the load
/// shows up as a differing stamp afterwards; `None` if it changed while being
/// read.
type LoadResult = (PathBuf, Option<Option<FileStamp>>, Result<Arc<AudioBuffer>, String>);

pub struct BufferCache {
    loader: Arc<dyn MediaLoader>,
    sample_rate: u32,
    ready: HashMap<PathBuf, (Arc<AudioBuffer>, Option<FileStamp>)>,
    failed: HashMap<PathBuf, (String, Option<FileStamp>)>,
    /// Files queued for the loader thread and still wanted; it skips the
    /// others, and their results are dropped.
    pending: Arc<Mutex<HashSet<PathBuf>>>,
    requests: Sender<PathBuf>,
    results: Receiver<LoadResult>,
    /// Underruns of streamed buffers, by file.
    underruns: HashMap<PathBuf, Underruns>,
    /// The watcher's latest snapshot, to catch loads that read an old file.
    stamps: Stamps,
    /// Files read as something else than the watcher's snapshot says (or
    /// changing while read): the watcher may have missed a change. They are
    /// read again once its next snapshot comes, not over and over meanwhile.
    waiting: HashSet<PathBuf>,
    /// Of those, the ones the watcher has not been asked about yet.
    suspects: Vec<PathBuf>,
}

impl BufferCache {
    pub fn new(loader: Arc<dyn MediaLoader>, sample_rate: u32) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<PathBuf>();
        let (res_tx, res_rx) = mpsc::channel();
        let worker = loader.clone();
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let wanted = pending.clone();
        std::thread::Builder::new()
            .name("jq-loader".into())
            .spawn(move || {
                for path in req_rx {
                    if !lock(&wanted).contains(&path) {
                        continue;
                    }
                    let stamp = paths::stamp(&path);
                    let result = worker.load(&path, sample_rate).map(Arc::new);
                    let stable = (paths::stamp(&path) == stamp).then_some(stamp);
                    if res_tx.send((path, stable, result)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn loader thread");
        Self {
            loader,
            sample_rate,
            ready: HashMap::new(),
            failed: HashMap::new(),
            pending,
            requests: req_tx,
            results: res_rx,
            underruns: HashMap::new(),
            stamps: Stamps::default(),
            waiting: HashSet::new(),
            suspects: Vec::new(),
        }
    }

    /// Queues `path` for background decoding unless it is known already.
    pub fn request(&mut self, path: &Path) {
        if self.ready.contains_key(path) || self.failed.contains_key(path) || self.waiting.contains(path) {
            return;
        }
        if lock(&self.pending).insert(path.to_owned()) {
            let _ = self.requests.send(path.to_owned());
        }
    }

    /// Collects finished background loads; true if any arrived.
    pub fn poll(&mut self) -> bool {
        let mut any = false;
        while let Ok((path, stamp, result)) = self.results.try_recv() {
            // No longer wanted (or loaded on the spot meanwhile).
            if !lock(&self.pending).remove(&path) {
                continue;
            }
            any = true;
            let Some(stamp) = stamp.filter(|s| !self.is_outdated(&path, *s)) else {
                if self.stamps.contains_key(&path) {
                    if self.waiting.insert(path.clone()) {
                        self.suspects.push(path);
                    }
                } else {
                    // Not watched: nobody to ask, so it is just read again.
                    self.request(&path);
                }
                continue;
            };
            match result {
                Ok(buf) => {
                    self.ready.insert(path, (buf, stamp));
                }
                Err(e) => {
                    self.failed.insert(path, (e, stamp));
                }
            }
        }
        any
    }

    /// Takes the watcher's snapshot: buffers and failures of files that
    /// changed since they were read are dropped, so the next request reads
    /// the file again, as do the files waiting for the snapshot. Voices keep
    /// the buffer they play. Returns the files.
    pub fn check(&mut self, stamps: Stamps) -> Vec<PathBuf> {
        self.stamps = stamps;
        let mut stale: Vec<PathBuf> = (self.ready.iter().map(|(p, (_, s))| (p, *s)))
            .chain(self.failed.iter().map(|(p, (_, s))| (p, *s)))
            .filter(|(p, s)| self.is_outdated(p, *s))
            .map(|(p, _)| p.clone())
            .collect();
        stale.extend(self.waiting.drain());
        self.suspects.clear();
        for path in &stale {
            self.ready.remove(path);
            self.failed.remove(path);
            self.underruns.remove(path);
        }
        stale
    }

    /// Files that were read as something else than the watcher saw since
    /// the last call: it should look at its files again ([`crate::MediaWatcher::rescan`]).
    pub fn take_suspects(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.suspects)
    }

    /// The watcher saw a different file than the one read with `stamp`.
    fn is_outdated(&self, path: &Path, stamp: Option<FileStamp>) -> bool {
        self.stamps.get(path).is_some_and(|now| *now != stamp)
    }

    pub fn buffer(&self, path: &Path) -> Option<&Arc<AudioBuffer>> {
        self.ready.get(path).map(|(b, _)| b)
    }

    /// Streamed files that played silence (disk too slow), with the number of frames missed: one
    /// entry per stretch of silence, once it is over (or long).
    pub fn take_underruns(&mut self, now: Instant) -> Vec<(PathBuf, u64)> {
        let mut new = Vec::new();
        for (path, (buffer, _)) in &self.ready {
            let Some(stream) = buffer.stream() else { continue };
            let count = stream.underruns();
            let u = self.underruns.entry(path.clone()).or_insert(Underruns {
                seen: 0,
                pending: 0,
                first: now,
                last: now,
            });
            // A reloaded file has a new stream counting from zero again.
            u.seen = u.seen.min(count);
            if count > u.seen {
                if u.pending == 0 {
                    u.first = now;
                }
                u.pending += count - u.seen;
                u.seen = count;
                u.last = now;
            }
            let over = now.saturating_duration_since(u.last) >= UNDERRUN_QUIET;
            let long = now.saturating_duration_since(u.first) >= UNDERRUN_LONGEST;
            if u.pending > 0 && (over || long) {
                new.push((path.clone(), u.pending));
                u.pending = 0;
            }
        }
        new
    }

    /// Whether files are queued for the loader thread.
    pub fn is_loading(&self) -> bool {
        !lock(&self.pending).is_empty()
    }

    pub fn is_ready(&self, path: &Path) -> bool {
        self.ready.contains_key(path)
    }

    pub fn error(&self, path: &Path) -> Option<&str> {
        self.failed.get(path).map(|(e, _)| e.as_str())
    }

    /// Returns the buffer, decoding synchronously if it is not armed yet.
    /// The bool says whether it had to be loaded on the spot.
    pub fn get_now(&mut self, path: &Path) -> Result<(Arc<AudioBuffer>, bool), String> {
        self.poll();
        if let Some((b, _)) = self.ready.get(path) {
            return Ok((b.clone(), false));
        }
        // A background load of the file still under way is not needed.
        lock(&self.pending).remove(path);
        self.waiting.remove(path);
        let stamp = paths::stamp(path);
        let buf = Arc::new(self.loader.load(path, self.sample_rate)?);
        self.failed.remove(path);
        self.ready.insert(path.to_owned(), (buf.clone(), stamp));
        Ok((buf, true))
    }

    /// Drops every buffer not in `keep`; also forgets failures, so a fixed
    /// file is tried again.
    pub fn retain(&mut self, keep: &HashSet<PathBuf>) {
        lock(&self.pending).retain(|p| keep.contains(p));
        self.ready.retain(|p, _| keep.contains(p));
        self.failed.retain(|p, _| keep.contains(p));
        self.underruns.retain(|p, _| keep.contains(p));
        self.waiting.retain(|p| keep.contains(p));
        self.suspects.retain(|p| keep.contains(p));
    }
}

/// The lock never guards anything a panic could leave half-changed.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
