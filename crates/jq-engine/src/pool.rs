//! Stretchers for voices whose pitch does not follow their rate ("keep
//! pitch", transposition). They are made on the control thread when the
//! engine is created and lent to voices in the audio thread, so nothing is
//! allocated or freed there.

use jq_stretch::{CHANNELS, Stretcher};

use crate::voice::{MAX_RATE, Pending};

/// Output frames the stretcher computes at a time.
pub(crate) const BLOCK: usize = 64;

pub(crate) struct StretchUnit {
    pub(crate) stretcher: Stretcher,
    /// Input of one block (interleaved), room for [`MAX_RATE`].
    pub(crate) input: Vec<f32>,
    /// Output of the current block (interleaved) and how much of it is used.
    pub(crate) output: Vec<f32>,
    pub(crate) used: usize,
    /// Frames before the switch point, fed as pre-roll.
    pub(crate) history: Vec<f32>,
    /// Fraction of an input frame carried to the next block.
    pub(crate) carry: f64,
    /// Loop exits of the feed, waiting until they are heard (oldest first;
    /// the capacity is never exceeded).
    pub(crate) pending: Vec<Pending>,
    /// Exits of the feed queued so far.
    pub(crate) queued: u64,
    /// When each block is heard and the file position it starts at.
    trail: Vec<(i64, usize)>,
    /// Blocks noted, and the newest one heard by now.
    noted: usize,
    heard: Option<usize>,
}

/// Queued exits per voice; more at once are passed on right away.
const PENDING: usize = 16;
/// Blocks remembered for the heard position: the latency plus the pre-roll
/// ahead of it, also at 192 kHz.
const TRAIL: usize = 2048;

impl StretchUnit {
    pub(crate) fn new(sample_rate: u32) -> Self {
        let stretcher = Stretcher::new(sample_rate);
        // Signalsmith wants one block plus one interval of pre-roll; its
        // presets are at most 140 ms long together.
        let history = (sample_rate as usize * 14).div_ceil(100);
        Self {
            stretcher,
            input: vec![0.0; (BLOCK as f64 * MAX_RATE as f64).ceil() as usize * CHANNELS + 2 * CHANNELS],
            output: vec![0.0; BLOCK * CHANNELS],
            used: BLOCK,
            history: vec![0.0; history * CHANNELS],
            carry: 0.0,
            pending: Vec::with_capacity(PENDING),
            queued: 0,
            trail: vec![(i64::MAX, 0); TRAIL],
            noted: 0,
            heard: None,
        }
    }

    /// Forgets the queued exits and the heard positions (a new switch).
    pub(crate) fn clear(&mut self, exits: u64) {
        self.pending.clear();
        self.queued = exits;
        self.noted = 0;
        self.heard = None;
    }

    /// Queues an exit, or hands it back if the queue is full.
    pub(crate) fn queue(&mut self, p: Pending) -> Option<Pending> {
        if self.pending.len() < self.pending.capacity() {
            self.pending.push(p);
            None
        } else {
            Some(p)
        }
    }

    /// Notes that the block starting at file position `pos` is heard at
    /// output frame `at`.
    pub(crate) fn note(&mut self, at: i64, pos: usize) {
        self.trail[self.noted % TRAIL] = (at, pos);
        self.noted += 1;
        // Never fall more than the trail behind.
        if let Some(h) = self.heard
            && self.noted - h > TRAIL
        {
            self.heard = Some(self.noted - TRAIL);
        }
    }

    /// File position heard at output frame `clock` (call with a clock that
    /// only grows); `None` before the first block is heard.
    pub(crate) fn heard_at(&mut self, clock: i64) -> Option<usize> {
        let mut next = self.heard.map_or(0, |h| h + 1);
        while next < self.noted && self.trail[next % TRAIL].0 <= clock {
            self.heard = Some(next);
            next += 1;
        }
        self.heard.map(|h| self.trail[h % TRAIL].1)
    }

    /// Input frames that fit into [`StretchUnit::input`].
    pub(crate) fn input_frames(&self) -> usize {
        self.input.len() / CHANNELS
    }

    pub(crate) fn history_frames(&self) -> usize {
        self.history.len() / CHANNELS
    }

    /// Output frames by which the output lags behind the input at time
    /// ratio `rate`, so that input fed now is heard that much later.
    pub(crate) fn latency(&self, rate: f64) -> usize {
        let s = &self.stretcher;
        (s.input_latency() as f64 / rate.max(0.01) + s.output_latency() as f64).round() as usize
    }
}

/// How many voices may be stretched at once on this machine, from what one
/// stretcher costs: together at most a quarter of the CPU's real time.
pub fn stretcher_pool_size(sample_rate: u32) -> usize {
    pool_size_for(Stretcher::cost(sample_rate))
}

/// [`stretcher_pool_size`] and [`prime_ratio`] from one measurement.
pub(crate) fn measure(sample_rate: u32) -> (usize, u32) {
    let cost = Stretcher::cost(sample_rate);
    (pool_size_for(cost), prime_ratio_for(cost))
}

fn pool_size_for(cost: f64) -> usize {
    if cost.is_nan() || cost <= 0.0 {
        return 2;
    }
    ((0.25 / cost) as usize).clamp(2, 32)
}

/// Output frames of pre-roll a voice may compute per output frame of a
/// callback, keeping that work to about a quarter of the callback too.
pub fn prime_ratio(sample_rate: u32) -> u32 {
    prime_ratio_for(Stretcher::cost(sample_rate))
}

fn prime_ratio_for(cost: f64) -> u32 {
    if cost.is_nan() || cost <= 0.0 {
        return 4;
    }
    ((0.25 / cost) as u32).clamp(4, 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_and_pre_roll_follow_the_cost() {
        assert_eq!(pool_size_for(0.007), 32);
        assert_eq!(pool_size_for(0.03), 8);
        assert_eq!(pool_size_for(0.5), 2);
        assert_eq!(pool_size_for(f64::NAN), 2);
        assert_eq!(prime_ratio_for(0.007), 35);
        assert_eq!(prime_ratio_for(0.1), 4);
    }
}
