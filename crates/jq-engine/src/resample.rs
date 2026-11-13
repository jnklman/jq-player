//! Varispeed: reading a voice faster or slower, pitch following, by
//! band-limited (windowed sinc) interpolation between the frames the read
//! head delivers. At a rate of exactly 1 and phase 0 every frame comes out
//! unchanged, so a voice can switch from direct playback without a seam.

use std::f64::consts::PI;
use std::sync::Arc;

/// Zero crossings of the kernel on each side at a rate of at most 1.
const HALF_BASE: usize = 16;
/// Most taps on each side (rates up to [`crate::voice::MAX_RATE`]).
pub(crate) const HALF_MAX: usize = 64;
/// Table resolution: kernel values per zero crossing.
const RESOLUTION: usize = 256;
/// Frames kept around the read position; a power of two.
const RING: usize = 256;
/// Faster than 1, the cutoff sits a little below the new Nyquist frequency,
/// so the kernel's transition band does not fold back.
const ALIAS_MARGIN: f64 = 0.97;

/// One side of the kernel, `sinc(u)` under a Blackman window, for `u` from 0
/// to [`HALF_BASE`] in steps of `1 / RESOLUTION`; exactly 0 at whole `u > 0`.
pub(crate) fn kernel_table() -> Arc<[f32]> {
    (0..=HALF_BASE * RESOLUTION + 1)
        .map(|i| {
            if i == 0 {
                return 1.0;
            }
            if i % RESOLUTION == 0 {
                return 0.0;
            }
            let u = i as f64 / RESOLUTION as f64;
            let t = (u / HALF_BASE as f64).min(1.0);
            let window = 0.42 + 0.5 * (PI * t).cos() + 0.08 * (2.0 * PI * t).cos();
            ((PI * u).sin() / (PI * u) * window) as f32
        })
        .collect()
}

#[inline]
fn kernel(table: &[f32], u: f64) -> f64 {
    let x = u.abs() * RESOLUTION as f64;
    let i = x as usize;
    match (table.get(i), table.get(i + 1)) {
        (Some(&a), Some(&b)) => {
            let f = x - i as f64;
            a as f64 + (b as f64 - a as f64) * f
        }
        _ => 0.0,
    }
}

/// Frames around the read position, the fractional phase between the
/// frame at `center` and the next, and the file position of each frame.
#[derive(Clone, Copy)]
pub(crate) struct Resampler {
    frames: [(f32, f32); RING],
    positions: [usize; RING],
    /// Frames pushed so far (the next one goes to `pushed % RING`).
    pushed: usize,
    /// Index (in pushed frames) of the frame the output stands at.
    center: usize,
    phase: f64,
    /// Index of the first frame after the voice's end (silence pushed after
    /// it); `None` while the head delivers.
    ended_at: Option<usize>,
}

impl Resampler {
    pub(crate) fn new() -> Self {
        Self {
            frames: [(0.0, 0.0); RING],
            positions: [0; RING],
            pushed: 0,
            center: 0,
            phase: 0.0,
            ended_at: None,
        }
    }

    /// Adds the next frame the head delivered (direct playback keeps this
    /// up to date, so a later switch has the frames before the position).
    #[inline]
    pub(crate) fn push(&mut self, frame: (f32, f32), position: usize) {
        let i = self.pushed % RING;
        self.frames[i] = frame;
        self.positions[i] = position;
        self.pushed += 1;
    }

    /// The head has nothing more: what follows is silence.
    pub(crate) fn push_end(&mut self, position: usize) {
        if self.ended_at.is_none() {
            self.ended_at = Some(self.pushed);
        }
        self.push((0.0, 0.0), position);
    }

    /// Starts interpolating at the frame after the last one pushed (where
    /// direct playback would go on); the caller then pushes [`HALF_MAX`]
    /// frames of lookahead and up to one more per output frame.
    pub(crate) fn engage(&mut self) {
        self.center = self.pushed;
        self.phase = 0.0;
    }

    /// Frames still to push before [`Resampler::next`] can run.
    #[inline]
    pub(crate) fn wanted(&self) -> usize {
        (self.center + HALF_MAX + 1).saturating_sub(self.pushed)
    }

    /// True once the output has passed the last real frame.
    pub(crate) fn drained(&self) -> bool {
        self.ended_at.is_some_and(|end| self.center >= end)
    }

    /// The head delivers again (a seek back from its end): the silence
    /// pushed after the end that the output has not reached yet is pushed
    /// again with what the head delivers now.
    pub(crate) fn resume_delivery(&mut self) {
        if let Some(end) = self.ended_at.take() {
            self.pushed = self.pushed.min(end.max(self.center));
        }
    }

    /// Starts afresh at the frame the head delivers next, as if engaged on
    /// silence (a seek while nothing is heard): the lookahead pushed from
    /// before is dropped.
    pub(crate) fn restart(&mut self) {
        self.frames = [(0.0, 0.0); RING];
        self.ended_at = None;
        self.engage();
    }

    /// File position the output stands at; `next` (the head's position)
    /// while the frame there is not pushed yet (right after engaging).
    pub(crate) fn position(&self, next: usize) -> usize {
        if self.center < self.pushed { self.positions[self.center % RING] } else { next }
    }

    /// The output frame at the current position, then steps `rate` frames
    /// ahead. Call only with [`Resampler::wanted`] at 0.
    #[inline]
    pub(crate) fn next(&mut self, table: &[f32], rate: f64) -> (f32, f32) {
        let out = if self.phase == 0.0 && rate == 1.0 {
            self.frames[self.center % RING]
        } else {
            let cutoff = if rate > 1.0 { ALIAS_MARGIN / rate } else { 1.0 };
            let half = ((HALF_BASE as f64 / cutoff).ceil() as usize).min(HALF_MAX);
            let (mut l, mut r, mut sum) = (0.0f64, 0.0f64, 0.0f64);
            for k in 0..2 * half {
                let offset = k as isize - half as isize + 1;
                let w = kernel(table, (offset as f64 - self.phase) * cutoff);
                let (fl, fr) = self.frames[(self.center as isize + offset) as usize % RING];
                l += fl as f64 * w;
                r += fr as f64 * w;
                sum += w;
            }
            // Normalized, so the level stays exact at every phase.
            if sum.abs() > 1e-9 { ((l / sum) as f32, (r / sum) as f32) } else { (0.0, 0.0) }
        };
        self.phase += rate;
        let whole = self.phase.floor();
        self.center += whole as usize;
        self.phase -= whole;
        out
    }
}
