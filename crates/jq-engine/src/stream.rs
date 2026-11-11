//! Shared state of a streamed buffer: the file split into chunks, of which a
//! reader thread (in `jq-media`) keeps loaded what playback needs next.
//!
//! The audio thread only loads chunk pointers and stores positions; it never
//! allocates, locks or frees. The reader frees an evicted chunk only after a
//! grace period ([`GRACE`]), long after any audio callback that could still
//! be reading it has returned.
//!
//! The reader follows positions only while a voice holds a slot; otherwise
//! nothing it must load can change without the control thread saying so
//! ([`StreamData::wait_for_change`]), so an idle stream costs nothing.

use std::ops::Range;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// Frames per chunk (about 1.4 s at 48 kHz).
pub const CHUNK_FRAMES: usize = 1 << 16;
/// How long an evicted chunk stays alive before it is freed.
pub const GRACE: Duration = Duration::from_secs(2);
/// Voices that can report their position per buffer; more voices on one
/// buffer play, but the reader does not follow them.
pub const POSITION_SLOTS: usize = 16;
/// Marks a free position slot.
pub const NO_POSITION: usize = usize::MAX;

type Chunk = Vec<f32>;

pub struct StreamData {
    /// Total frames; an estimate from the file header until the reader finds the end.
    frames: AtomicUsize,
    chunks: Box<[AtomicPtr<Chunk>]>,
    /// Positions of the voices playing this buffer, by slot.
    positions: [AtomicUsize; POSITION_SLOTS],
    /// Voice id holding each slot; 0 = free. Claimed and released by the
    /// control thread, so two voices never share a slot.
    owners: [AtomicU64; POSITION_SLOTS],
    /// Frame ranges that must stay loaded (loop and vamp regions), by slot.
    /// Only the control and reader threads touch this.
    pins: Mutex<[Vec<Range<usize>>; POSITION_SLOTS]>,
    /// Frames to keep ready while nothing plays there (the start points of
    /// the cues that may come next), so GO starts at once.
    /// Only the control and reader threads touch this.
    hints: Mutex<Vec<usize>>,
    /// Frames played as silence because their chunk was not loaded.
    underruns: AtomicU64,
    stop: AtomicBool,
    /// Counts the control thread's changes of slots, pins, hints and stop,
    /// so an idle reader can sleep until the next one. Never touched by the
    /// audio thread.
    changes: Mutex<u64>,
    changed: Condvar,
}

impl StreamData {
    pub fn new(frames: usize) -> Self {
        // Room for header estimates that are a little short.
        let count = frames.div_ceil(CHUNK_FRAMES) + 2;
        Self {
            frames: AtomicUsize::new(frames),
            chunks: (0..count).map(|_| AtomicPtr::new(ptr::null_mut())).collect(),
            positions: std::array::from_fn(|_| AtomicUsize::new(NO_POSITION)),
            owners: std::array::from_fn(|_| AtomicU64::new(0)),
            pins: Mutex::new(std::array::from_fn(|_| Vec::new())),
            hints: Mutex::new(vec![0]),
            underruns: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            changes: Mutex::new(0),
            changed: Condvar::new(),
        }
    }

    pub fn frames(&self) -> usize {
        self.frames.load(Ordering::Relaxed)
    }

    /// Called by the reader when it reaches the real end of the file.
    pub fn set_frames(&self, frames: usize) {
        self.frames.store(frames.min(self.chunks.len() * CHUNK_FRAMES), Ordering::Relaxed);
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Audio thread: the (left, right) pair of `frame`, or silence if its
    /// chunk is not loaded (counted as an underrun).
    #[inline]
    pub fn frame(&self, frame: usize) -> (f32, f32) {
        let Some(slot) = self.chunks.get(frame / CHUNK_FRAMES) else { return (0.0, 0.0) };
        let chunk = slot.load(Ordering::Acquire);
        // SAFETY: a non-null pointer came from `Box::into_raw` in `store` and
        // is freed only `GRACE` after it was swapped out (see `evict`).
        match unsafe { chunk.as_ref() } {
            Some(c) => {
                let i = (frame % CHUNK_FRAMES) * 2;
                match (c.get(i), c.get(i + 1)) {
                    (Some(&l), Some(&r)) => (l, r),
                    _ => (0.0, 0.0),
                }
            }
            None => {
                self.underruns.fetch_add(1, Ordering::Relaxed);
                (0.0, 0.0)
            }
        }
    }

    pub fn is_loaded(&self, chunk: usize) -> bool {
        self.chunks.get(chunk).is_some_and(|c| !c.load(Ordering::Acquire).is_null())
    }

    /// Reader: installs a decoded chunk (interleaved stereo). Returns the
    /// chunk it replaced, which the caller must keep for [`GRACE`].
    pub fn store(&self, chunk: usize, samples: Vec<f32>) -> Option<Retired> {
        let slot = self.chunks.get(chunk)?;
        let old = slot.swap(Box::into_raw(Box::new(samples)), Ordering::AcqRel);
        (!old.is_null()).then(|| Retired(old))
    }

    /// Reader: unloads a chunk; keep the result for [`GRACE`].
    pub fn evict(&self, chunk: usize) -> Option<Retired> {
        let old = self.chunks.get(chunk)?.swap(ptr::null_mut(), Ordering::AcqRel);
        (!old.is_null()).then(|| Retired(old))
    }

    /// Control thread: a free position slot for voice `owner` (non-zero),
    /// `None` if all are taken.
    pub fn claim_slot(&self, owner: u64) -> Option<usize> {
        let slot = self
            .owners
            .iter()
            .position(|o| o.compare_exchange(0, owner, Ordering::AcqRel, Ordering::Relaxed).is_ok());
        if slot.is_some() {
            self.notify();
        }
        slot
    }

    /// Control thread: frees the slot of voice `owner` along with its
    /// position and pins.
    pub fn release_slot(&self, owner: u64) {
        let Some(slot) = self.owners.iter().position(|o| o.load(Ordering::Acquire) == owner) else { return };
        self.set_pins(slot, Vec::new());
        self.report_position(slot, NO_POSITION);
        self.owners[slot].store(0, Ordering::Release);
        self.notify();
    }

    /// Whether a voice holds a slot: then positions move without notice.
    pub fn has_voices(&self) -> bool {
        self.owners.iter().any(|o| o.load(Ordering::Acquire) != 0)
    }

    /// Audio thread: where the voice in `slot` plays ([`NO_POSITION`] = none).
    #[inline]
    pub fn report_position(&self, slot: usize, frame: usize) {
        if let Some(p) = self.positions.get(slot) {
            p.store(frame, Ordering::Relaxed);
        }
    }

    pub fn positions(&self) -> impl Iterator<Item = usize> + '_ {
        self.positions.iter().map(|p| p.load(Ordering::Relaxed)).filter(|&p| p != NO_POSITION)
    }

    /// Control thread: frame ranges the voice in `slot` loops through.
    pub fn set_pins(&self, slot: usize, ranges: Vec<Range<usize>>) {
        if let Ok(mut pins) = self.pins.lock()
            && let Some(p) = pins.get_mut(slot)
        {
            *p = ranges;
        }
        self.notify();
    }

    pub fn pins(&self) -> Vec<Range<usize>> {
        self.pins.lock().map(|p| p.iter().flatten().cloned().collect()).unwrap_or_default()
    }

    /// Control thread: start frames to keep loaded.
    pub fn set_hints(&self, frames: Vec<usize>) {
        if let Ok(mut hints) = self.hints.lock() {
            *hints = frames;
        }
        self.notify();
    }

    pub fn hints(&self) -> Vec<usize> {
        self.hints.lock().map(|h| h.clone()).unwrap_or_default()
    }

    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Asks the reader thread to end.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.notify();
    }

    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Reader: the count of changes so far, taken before looking at what to
    /// load and handed to [`Self::wait_for_change`].
    pub fn changes(&self) -> u64 {
        *self.lock_changes()
    }

    /// Reader: sleeps until the control thread changed something since
    /// `seen` (at once if it did already), or `timeout` passed.
    pub fn wait_for_change(&self, seen: u64, timeout: Option<Duration>) {
        let changes = self.lock_changes();
        let unchanged = |c: &mut u64| *c == seen;
        match timeout {
            Some(t) => drop(self.changed.wait_timeout_while(changes, t, unchanged)),
            None => drop(self.changed.wait_while(changes, unchanged)),
        }
    }

    /// Control thread: wakes an idle reader.
    fn notify(&self) {
        *self.lock_changes() += 1;
        self.changed.notify_all();
    }

    /// The counter never guards anything a panic could leave half-changed.
    fn lock_changes(&self) -> MutexGuard<'_, u64> {
        self.changes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for StreamData {
    fn drop(&mut self) {
        // Nobody can read any more: the last owner is never the audio thread.
        for slot in self.chunks.iter() {
            let chunk = slot.swap(ptr::null_mut(), Ordering::AcqRel);
            if !chunk.is_null() {
                drop(Retired(chunk));
            }
        }
    }
}

/// A chunk taken out of the stream; frees it when dropped.
pub struct Retired(*mut Chunk);

// SAFETY: the chunk is plain data owned by this handle alone.
unsafe impl Send for Retired {}

impl Drop for Retired {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `Box::into_raw` and is owned only here.
        drop(unsafe { Box::from_raw(self.0) });
    }
}
