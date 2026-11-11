//! Notices media files changing on disk while a show is open. A thread of
//! its own looks at the files' stamps, so a slow or hanging drive never
//! holds up the show thread.
//!
//! By default it waits for the system's file events (inotify, FSEvents,
//! ReadDirectoryChangesW) on the folders holding the files and only then
//! stats the files: an idle show costs neither CPU nor disk. File events
//! miss changes made by other machines on a network share, so the files can
//! also be polled ([`WatchMode::Poll`]), and [`MediaWatcher::rescan`] looks
//! at them on request.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use jq_show::paths::{self, FileStamp};
use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::settings::WatchMode;

/// Current stamp of every watched file; `None` = missing.
pub type Stamps = Arc<HashMap<PathBuf, Option<FileStamp>>>;

/// File events come in bursts (a file being written); the files are looked
/// at once the burst is this quiet …
const SETTLE: Duration = Duration::from_millis(150);
/// … or has gone on for this long.
const SETTLE_LONGEST: Duration = Duration::from_secs(1);

enum Msg {
    Files(Vec<PathBuf>),
    Mode(WatchMode),
    Rescan,
    /// From the system's file watcher.
    Event(notify::Result<notify::Event>),
}

pub struct MediaWatcher {
    msgs: Sender<Msg>,
    snapshots: Receiver<Stamps>,
    warnings: Receiver<String>,
}

impl MediaWatcher {
    /// Watches in `mode`; polling looks at the files every `interval`.
    /// Dropping the watcher ends its thread.
    pub fn new(mode: WatchMode, interval: Duration) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel();
        let (snap_tx, snap_rx) = mpsc::channel();
        let (warn_tx, warn_rx) = mpsc::channel();
        let events = msg_tx.clone();
        std::thread::Builder::new()
            .name("jq-watch".into())
            .spawn(move || {
                Thread {
                    msgs: msg_rx,
                    events,
                    snapshots: snap_tx,
                    warnings: warn_tx,
                    interval,
                    mode,
                    system: None,
                    files: Vec::new(),
                    dirs: HashMap::new(),
                    last: None,
                }
                .run();
            })
            .expect("spawn watcher thread");
        let _ = msg_tx.send(Msg::Mode(mode));
        Self { msgs: msg_tx, snapshots: snap_rx, warnings: warn_rx }
    }

    /// Replaces the watched files; they are looked at right away.
    pub fn watch(&self, files: Vec<PathBuf>) {
        let _ = self.msgs.send(Msg::Files(files));
    }

    pub fn set_mode(&self, mode: WatchMode) {
        let _ = self.msgs.send(Msg::Mode(mode));
    }

    /// Looks at every file again; a snapshot follows even if nothing
    /// changed.
    pub fn rescan(&self) {
        let _ = self.msgs.send(Msg::Rescan);
    }

    /// The latest snapshot if anything changed since the last call (or the
    /// watched files were replaced, or a rescan was asked for).
    pub fn poll(&self) -> Option<Stamps> {
        self.snapshots.try_iter().last()
    }

    /// Problems of the system's file watcher (it then polls instead).
    pub fn take_warnings(&self) -> Vec<String> {
        self.warnings.try_iter().collect()
    }
}

/// What changed since the files were last looked at.
#[derive(Default)]
struct Pending {
    look: bool,
    /// Send a snapshot even if it is the same.
    force: bool,
    /// The folders to watch may have changed.
    dirs: bool,
}

struct Thread {
    msgs: Receiver<Msg>,
    /// Handed to the system's watcher, which sends its events here.
    events: Sender<Msg>,
    snapshots: Sender<Stamps>,
    warnings: Sender<String>,
    interval: Duration,
    mode: WatchMode,
    /// `None` while polling (by choice or because it failed).
    system: Option<RecommendedWatcher>,
    files: Vec<PathBuf>,
    /// Watched folders and what they were when the watch began; a folder
    /// replaced since must be watched anew.
    dirs: HashMap<PathBuf, Option<DirId>>,
    last: Option<Stamps>,
}

impl Thread {
    fn run(&mut self) {
        loop {
            let first = match self.system {
                Some(_) => self.msgs.recv().map_err(|_| RecvTimeoutError::Disconnected),
                None => self.msgs.recv_timeout(self.interval),
            };
            let mut pending = Pending::default();
            match first {
                Ok(msg) => self.take(msg, &mut pending),
                // Polling.
                Err(RecvTimeoutError::Timeout) => pending.look = true,
                Err(RecvTimeoutError::Disconnected) => return,
            }
            // Let a burst of events settle; later messages join this look.
            let start = Instant::now();
            while pending.look && start.elapsed() < SETTLE_LONGEST {
                match self.msgs.recv_timeout(SETTLE) {
                    Ok(msg) => self.take(msg, &mut pending),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            if pending.dirs {
                self.update_dirs();
            }
            if pending.look && !self.look(pending.force) {
                return;
            }
        }
    }

    fn take(&mut self, msg: Msg, pending: &mut Pending) {
        match msg {
            Msg::Files(mut files) => {
                files.sort();
                files.dedup();
                self.files = files;
                self.last = None;
                *pending = Pending { look: true, force: true, dirs: true };
            }
            Msg::Mode(mode) => {
                self.mode = mode;
                self.set_system();
                *pending = Pending { look: true, force: pending.force, dirs: true };
            }
            Msg::Rescan => {
                // A folder may have been replaced without an event.
                *pending = Pending { look: true, force: true, dirs: true };
            }
            Msg::Event(Ok(event)) => {
                if self.system.is_some() && self.concerns(&event) {
                    pending.look = true;
                    // A folder appearing, going away or renamed moves the watches.
                    pending.dirs |= event.need_rescan()
                        || !matches!(
                            event.kind,
                            EventKind::Access(_)
                                | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Metadata(_))
                        );
                }
            }
            Msg::Event(Err(e)) => {
                if self.system.is_some() {
                    self.fail(&e);
                    *pending = Pending { look: true, force: pending.force, dirs: true };
                }
            }
        }
    }

    /// Whether `event` may change a watched file or the folders to watch:
    /// it names a file or a folder above one. Events of other files in the
    /// media folder are ignored.
    fn concerns(&self, event: &notify::Event) -> bool {
        event.need_rescan()
            || event.paths.is_empty()
            || event.paths.iter().any(|p| self.files.iter().any(|f| f.starts_with(p)))
    }

    /// Starts or stops the system's watcher to match the mode.
    fn set_system(&mut self) {
        self.dirs.clear();
        self.system = None;
        if self.mode == WatchMode::Events {
            let events = self.events.clone();
            match notify::recommended_watcher(move |e| {
                let _ = events.send(Msg::Event(e));
            }) {
                Ok(w) => self.system = Some(w),
                Err(e) => self.fail(&e),
            }
        }
    }

    /// The system's watcher cannot be used: the files are polled instead
    /// until the mode is set again.
    fn fail(&mut self, e: &notify::Error) {
        self.system = None;
        self.dirs.clear();
        let _ = self.warnings.send(jq_i18n::t!("watch.system_unavailable", error = e));
    }

    /// Watches the folders holding the files (or, while one is missing, the
    /// nearest folder above it, to notice it coming back). A folder, not
    /// the file itself: a file replaced by renaming keeps no watch.
    fn update_dirs(&mut self) {
        let Some(system) = self.system.as_mut() else { return };
        let parents: HashSet<&Path> = self.files.iter().filter_map(|f| f.parent()).collect();
        let wanted: HashSet<PathBuf> = parents.into_iter().filter_map(existing_dir).collect();
        let mut failed = None;
        self.dirs.retain(|dir, id| {
            let keep = wanted.contains(dir) && dir_id(dir) == *id;
            if !keep {
                let _ = system.unwatch(dir);
            }
            keep
        });
        for dir in wanted {
            if self.dirs.contains_key(&dir) {
                continue;
            }
            match system.watch(&dir, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    let id = dir_id(&dir);
                    self.dirs.insert(dir, id);
                }
                // Gone meanwhile: the next event of the folder above brings it.
                Err(e) if matches!(e.kind, notify::ErrorKind::PathNotFound) => {}
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = failed {
            self.fail(&e);
        }
    }

    /// Stats the files and sends a snapshot if it differs from the last one
    /// (or `force`). False once nobody listens any more.
    fn look(&mut self, force: bool) -> bool {
        let now: HashMap<_, _> = self.files.iter().map(|p| (p.clone(), paths::stamp(p))).collect();
        if !force && self.last.as_deref() == Some(&now) {
            return true;
        }
        let now = Arc::new(now);
        self.last = Some(now.clone());
        self.snapshots.send(now).is_ok()
    }
}

/// `dir`, or while it is missing the nearest existing folder above it.
fn existing_dir(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find(|d| d.is_dir()).map(Path::to_path_buf)
}

/// Identifies a folder, so one deleted and made again is told apart.
#[cfg(unix)]
type DirId = (u64, u64);
#[cfg(not(unix))]
type DirId = ();

fn dir_id(dir: &Path) -> Option<DirId> {
    let meta = std::fs::metadata(dir).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((meta.dev(), meta.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next(w: &MediaWatcher) -> Stamps {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(s) = w.poll() {
                return s;
            }
            assert!(Instant::now() < deadline, "no snapshot in time");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jq-core-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Files appearing, being replaced by renaming and written in place.
    fn reports_changes(mode: WatchMode, name: &str) {
        let dir = tempdir(name);
        let path = dir.join("a.wav");
        // Never polls by accident while watching events.
        let w = MediaWatcher::new(mode, Duration::from_millis(10));
        w.watch(vec![path.clone()]);
        assert_eq!(next(&w)[&path], None);

        std::fs::write(&path, b"one").unwrap();
        let first = next(&w)[&path].expect("file seen");
        // Unchanged: no new snapshot, also not for other files of the folder.
        std::fs::write(dir.join("other.wav"), b"x").unwrap();
        std::thread::sleep(SETTLE * 3);
        assert!(w.poll().is_none());

        let tmp = dir.join("a.tmp");
        std::fs::write(&tmp, b"two").unwrap();
        std::fs::rename(&tmp, &path).unwrap();
        let second = next(&w)[&path].expect("replaced file seen");
        assert_ne!(second, first);

        std::fs::write(&path, b"three!").unwrap();
        assert_ne!(next(&w)[&path], Some(second), "written in place");

        std::fs::remove_file(&path).unwrap();
        assert_eq!(next(&w)[&path], None);
        // Rescan: a snapshot even though nothing changed.
        w.rescan();
        assert_eq!(next(&w)[&path], None);
        assert!(w.take_warnings().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn events_report_changes() {
        reports_changes(WatchMode::Events, "events");
    }

    #[test]
    fn polling_reports_changes() {
        reports_changes(WatchMode::Poll, "poll");
    }

    #[test]
    fn folder_coming_back_is_watched_again() {
        let dir = tempdir("folder");
        let sub = dir.join("sub");
        let path = sub.join("a.wav");
        let w = MediaWatcher::new(WatchMode::Events, Duration::from_secs(3600));
        w.watch(vec![path.clone()]);
        assert_eq!(next(&w)[&path], None, "folder missing");

        std::fs::create_dir(&sub).unwrap();
        std::fs::write(&path, b"one").unwrap();
        assert!(next(&w)[&path].is_some(), "seen through the folder above");

        std::fs::remove_dir_all(&sub).unwrap();
        assert_eq!(next(&w)[&path], None);
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(&path, b"two").unwrap();
        assert!(next(&w)[&path].is_some(), "new folder of the same name");
        std::fs::write(&path, b"three").unwrap();
        assert!(next(&w).get(&path).is_some(), "and it is watched");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn switching_mode_keeps_watching() {
        let dir = tempdir("mode");
        let path = dir.join("a.wav");
        let w = MediaWatcher::new(WatchMode::Poll, Duration::from_millis(10));
        w.watch(vec![path.clone()]);
        assert_eq!(next(&w)[&path], None);
        w.set_mode(WatchMode::Events);
        std::thread::sleep(SETTLE * 3);
        while w.poll().is_some() {}
        std::fs::write(&path, b"one").unwrap();
        assert!(next(&w)[&path].is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
