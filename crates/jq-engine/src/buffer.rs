use std::sync::Arc;
use std::time::Duration;

use crate::stream::StreamData;

/// Stereo audio, interleaved L/R, at the engine's sample rate: either fully
/// decoded in RAM or streamed from disk in chunks (long files).
///
/// Buffers are shared with the audio thread via `Arc` and never mutated
/// after construction (a stream's chunks change, its identity does not).
pub struct AudioBuffer {
    sample_rate: u32,
    data: Data,
}

enum Data {
    Memory(Vec<f32>),
    Stream(Arc<StreamData>),
}

impl std::fmt::Debug for AudioBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = if self.stream().is_some() { "stream" } else { "memory" };
        write!(f, "AudioBuffer({kind}, {} frames @ {} Hz)", self.frames(), self.sample_rate)
    }
}

impl AudioBuffer {
    pub const CHANNELS: usize = 2;

    /// Wraps interleaved stereo samples. A trailing odd sample is dropped.
    pub fn from_interleaved_stereo(sample_rate: u32, mut samples: Vec<f32>) -> Self {
        samples.truncate(samples.len() - samples.len() % Self::CHANNELS);
        Self { sample_rate, data: Data::Memory(samples) }
    }

    /// A buffer whose chunks a reader thread loads into `stream`.
    pub fn streamed(sample_rate: u32, stream: Arc<StreamData>) -> Self {
        Self { sample_rate, data: Data::Stream(stream) }
    }

    pub fn stream(&self) -> Option<&Arc<StreamData>> {
        match &self.data {
            Data::Stream(s) => Some(s),
            Data::Memory(_) => None,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn frames(&self) -> usize {
        match &self.data {
            Data::Memory(s) => s.len() / Self::CHANNELS,
            Data::Stream(s) => s.frames(),
        }
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.frames() as f64 / self.sample_rate as f64)
    }

    /// The decoded samples; empty for a streamed buffer.
    pub fn samples(&self) -> &[f32] {
        match &self.data {
            Data::Memory(s) => s,
            Data::Stream(_) => &[],
        }
    }

    /// Returns the (left, right) pair of `frame`. Panics if out of range in
    /// memory; a streamed buffer gives silence where nothing is loaded.
    #[inline]
    pub fn frame(&self, frame: usize) -> (f32, f32) {
        match &self.data {
            Data::Memory(s) => {
                let i = frame * Self::CHANNELS;
                (s[i], s[i + 1])
            }
            Data::Stream(s) => s.frame(frame),
        }
    }

    /// Lets a stream's reader follow the voice in `slot`; no-op in memory.
    #[inline]
    pub fn report_position(&self, slot: usize, frame: usize) {
        if let Data::Stream(s) = &self.data {
            s.report_position(slot, frame);
        }
    }

    /// True if playback can start at `frame` without waiting for the disk.
    pub fn is_ready_at(&self, frame: usize) -> bool {
        match &self.data {
            Data::Memory(_) => true,
            Data::Stream(s) => s.is_loaded(frame / crate::stream::CHUNK_FRAMES),
        }
    }

    /// Size of the decoded data in bytes (0 for a stream).
    pub fn byte_size(&self) -> usize {
        std::mem::size_of_val(self.samples())
    }
}

impl Drop for AudioBuffer {
    fn drop(&mut self) {
        // The reader thread ends and frees the chunks; never on the audio
        // thread, which hands its buffers back via `Event::Finished`.
        if let Data::Stream(s) = &self.data {
            s.stop();
        }
    }
}
