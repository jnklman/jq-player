//! Disk writes that must not hold up the show thread: the machine settings
//! file and the autosave. One long-lived thread writes them one after the
//! other, in the order they were queued, so the settings file's
//! read-modify-write never interleaves with another one. A write still
//! queued is replaced by a newer one for the same file (only the newest state
//! matters, and it is never dropped). Explicit saves go through the same
//! thread and wait for their result ([`Writer::wait`]), so they are ordered
//! after any autosave queued before them.
//!
//! A failed or panicking job is reported as a [`Notice`] on the receiver
//! returned by [`Writer::spawn`]; later jobs still run.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use jq_core::Notice;
use jq_i18n::t;

/// What a queued write is for: a newer job with the same key replaces one
/// still waiting. Jobs with different keys may share a file (settings of a
/// show and the startup flag both live in the settings file); each of them
/// reads the file again before writing, so they do not undo one another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    path: PathBuf,
    part: String,
}

impl Key {
    pub fn new(path: PathBuf, part: impl Into<String>) -> Self {
        Self { path, part: part.into() }
    }
}

type Run = Box<dyn FnOnce() -> Result<(), String> + Send>;

enum Item {
    /// `None`: never replaced, and nothing queued later moves before it.
    Job(Option<Key>, Run),
    /// Answered once everything queued before it is done.
    Flush(Sender<()>),
}

/// Handle to the writer thread; clones queue to the same thread.
#[derive(Clone)]
pub struct Writer {
    tx: Sender<Item>,
    /// For jobs run in place when the thread is gone.
    notices: Sender<Notice>,
}

impl Writer {
    /// Starts the writer thread. Its notices (failed writes) arrive on the
    /// receiver. If the thread cannot be started, jobs run on the caller's
    /// thread instead: slower, but nothing is lost.
    pub fn spawn() -> (Self, Receiver<Notice>) {
        let (tx, queue) = mpsc::channel();
        let (notices, reports) = mpsc::channel();
        let report = notices.clone();
        let spawned =
            std::thread::Builder::new().name("jq-writer".into()).spawn(move || run(&queue, &report));
        if let Err(e) = spawned {
            eprintln!("writer thread not started, writing in place: {e}");
        }
        (Self { tx, notices }, reports)
    }

    /// Queues `job`; an error it returns is reported as a warning (it is
    /// shown as it is, so it should be a full message). A job still queued
    /// with the same `key` is dropped in favour of this one.
    pub fn submit(&self, key: Option<Key>, job: impl FnOnce() -> Result<(), String> + Send + 'static) {
        if let Err(mpsc::SendError(Item::Job(_, job))) = self.tx.send(Item::Job(key, Box::new(job))) {
            execute(job, &self.notices);
        }
    }

    /// Runs `job` on the writer thread after everything queued before it and
    /// waits for its result. A panic in it comes back as an error.
    pub fn wait<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        let run: Run = Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(job)).map_err(|_| t!("writer.panicked"));
            let _ = tx.send(result);
            Ok(())
        });
        if let Err(mpsc::SendError(Item::Job(_, run))) = self.tx.send(Item::Job(None, run)) {
            let _ = run();
        }
        rx.recv().unwrap_or_else(|_| Err(t!("writer.panicked")))
    }

    /// Waits until everything queued so far is written, at most `timeout`.
    /// False if the writes are still under way then (a hanging disk); they
    /// go on in the background.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Item::Flush(tx)).is_err() {
            // No thread: every job ran in place already.
            return true;
        }
        rx.recv_timeout(timeout).is_ok()
    }

    /// Waits until everything queued so far is written, however long it
    /// takes (tests).
    #[cfg(test)]
    pub fn flush_all(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Item::Flush(tx)).is_ok() {
            let _ = rx.recv();
        }
    }

    /// Holds the writer thread up until the returned sender is used or
    /// dropped, so the jobs queued meanwhile wait together (tests).
    #[cfg(test)]
    pub fn hold(&self) -> Sender<()> {
        let (release, wait) = mpsc::channel::<()>();
        let (started, running) = mpsc::channel();
        self.submit(None, move || {
            let _ = started.send(());
            let _ = wait.recv();
            Ok(())
        });
        let _ = running.recv();
        release
    }
}

fn run(queue: &Receiver<Item>, notices: &Sender<Notice>) {
    let mut pending = VecDeque::new();
    loop {
        if pending.is_empty() {
            match queue.recv() {
                Ok(item) => push(&mut pending, item),
                // Every handle is gone: nothing more can come.
                Err(_) => return,
            }
        }
        // What arrived during the last write, so newer states replace older ones.
        for item in queue.try_iter() {
            push(&mut pending, item);
        }
        match pending.pop_front() {
            Some(Item::Job(_, job)) => execute(job, notices),
            Some(Item::Flush(done)) => {
                let _ = done.send(());
            }
            None => {}
        }
    }
}

/// Queues `item`, dropping a job with the same key that is still waiting,
/// unless a flush or an unkeyed job (an explicit save) lies between them:
/// those must see the older state written.
fn push(pending: &mut VecDeque<Item>, item: Item) {
    if let Item::Job(Some(key), _) = &item {
        for i in (0..pending.len()).rev() {
            match &pending[i] {
                Item::Job(Some(other), _) if other == key => {
                    pending.remove(i);
                    break;
                }
                Item::Job(Some(_), _) => {}
                Item::Job(None, _) | Item::Flush(_) => break,
            }
        }
    }
    pending.push_back(item);
}

/// Runs a job; its error or panic becomes a notice.
fn execute(job: Run, notices: &Sender<Notice>) {
    let notice = match catch_unwind(AssertUnwindSafe(job)) {
        Ok(Ok(())) => return,
        Ok(Err(e)) => Notice::Warning(e),
        Err(_) => Notice::Error(t!("writer.panicked")),
    };
    let _ = notices.send(notice);
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn log() -> Arc<Mutex<Vec<String>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn note(
        log: &Arc<Mutex<Vec<String>>>,
        text: &str,
    ) -> impl FnOnce() -> Result<(), String> + Send + 'static {
        let (log, text) = (log.clone(), text.to_owned());
        move || {
            log.lock().unwrap().push(text);
            Ok(())
        }
    }

    fn key(part: &str) -> Option<Key> {
        Some(Key::new(PathBuf::from("/settings.json"), part))
    }

    #[test]
    fn jobs_run_in_order_and_flush_waits_for_them() {
        let (writer, _notices) = Writer::spawn();
        let done = log();
        let release = writer.hold();
        for i in 0..20 {
            writer.submit(None, note(&done, &i.to_string()));
        }
        assert!(done.lock().unwrap().is_empty(), "queued, not written yet");
        assert!(!writer.flush(Duration::from_millis(20)), "the writer is still held up");
        drop(release);
        writer.flush_all();
        let expected: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        assert_eq!(*done.lock().unwrap(), expected);
        assert!(writer.flush(Duration::from_secs(5)));
    }

    #[test]
    fn a_queued_write_is_replaced_by_the_newest_for_the_same_key() {
        let (writer, _notices) = Writer::spawn();
        let done = log();
        let release = writer.hold();
        writer.submit(key("a"), note(&done, "a1"));
        writer.submit(key("b"), note(&done, "b1"));
        writer.submit(key("a"), note(&done, "a2"));
        writer.submit(key("a"), note(&done, "a3"));
        drop(release);
        writer.flush_all();
        assert_eq!(*done.lock().unwrap(), ["b1", "a3"], "the newest state is written, once");
    }

    #[test]
    fn a_flush_or_an_explicit_save_keeps_the_older_write() {
        let (writer, _notices) = Writer::spawn();
        let done = log();
        let release = writer.hold();
        writer.submit(key("a"), note(&done, "a1"));
        // Queued as `flush` queues it, without waiting here.
        let (flushed_tx, flushed) = mpsc::channel();
        writer.tx.send(Item::Flush(flushed_tx)).map_err(|_| ()).unwrap();
        writer.submit(key("a"), note(&done, "a2"));
        // An unkeyed job, as `wait` queues an explicit save.
        writer.submit(key("a"), note(&done, "a3"));
        writer.submit(None, note(&done, "save"));
        writer.submit(key("a"), note(&done, "a4"));
        drop(release);
        flushed.recv().unwrap();
        writer.flush_all();
        assert_eq!(
            *done.lock().unwrap(),
            ["a1", "a3", "save", "a4"],
            "a1 before the flush, a3 (replacing a2) before the save"
        );
    }

    #[test]
    fn errors_and_panics_are_reported_and_later_jobs_still_run() {
        let (writer, notices) = Writer::spawn();
        let done = log();
        writer.submit(None, || Err("disk full".into()));
        writer.submit(None, || panic!("bug in a job"));
        writer.submit(None, note(&done, "after"));
        writer.flush_all();
        assert_eq!(*done.lock().unwrap(), ["after"]);
        let reported: Vec<Notice> = notices.try_iter().collect();
        assert_eq!(reported.len(), 2);
        assert!(matches!(&reported[0], Notice::Warning(e) if e == "disk full"));
        assert!(matches!(&reported[1], Notice::Error(_)));
    }

    #[test]
    fn wait_returns_the_result_after_what_was_queued_before() {
        let (writer, notices) = Writer::spawn();
        let done = log();
        let release = writer.hold();
        writer.submit(key("autosave"), note(&done, "autosave"));
        let saving = std::thread::spawn({
            let (writer, done) = (writer.clone(), done.clone());
            move || writer.wait(move || done.lock().unwrap().push("save".into()))
        });
        drop(release);
        saving.join().unwrap().unwrap();
        assert_eq!(*done.lock().unwrap(), ["autosave", "save"], "done when it returns");

        assert_eq!(writer.wait(|| 42), Ok(42));
        assert!(writer.wait(|| -> u32 { panic!("bug") }).is_err());
        assert_eq!(writer.wait(|| "still there"), Ok("still there"));
        assert!(notices.try_iter().next().is_none(), "the caller reports it, not the writer");
    }
}
