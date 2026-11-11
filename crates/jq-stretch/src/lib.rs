//! Time stretching and pitch shifting for jQ Player, a thin layer over
//! Signalsmith Stretch (MIT) so the engine knows nothing about C++ or FFI.
//!
//! A [`Stretcher`] is created (allocating) on the control thread; after that
//! [`Stretcher::process`], [`Stretcher::seek`], [`Stretcher::set_pitch`] and
//! [`Stretcher::reset`] never allocate, so they may run in the audio callback.
//! Checked against the header (1.3.1): `configure` reserves every buffer that
//! `process` resizes later. The one growing vector, the spectral peaks, has
//! room for `bands / 2`; between two peaks lies at least one band, so there
//! are at most ⌈bands / 2⌉ – enough as long as the band count is even, which
//! the tests check for the usual sample rates. Dropping a stretcher frees it,
//! so that must not happen on the audio thread either.

use signalsmith_stretch::Stretch;

/// Channels the engine works with (interleaved stereo).
pub const CHANNELS: usize = 2;

pub struct Stretcher {
    inner: Stretch,
    sample_rate: u32,
}

impl std::fmt::Debug for Stretcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Stretcher({} Hz, latency {})", self.sample_rate, self.latency())
    }
}

impl Stretcher {
    /// Stereo stretcher for `sample_rate`, with Signalsmith's "cheaper" preset
    /// (blocks of 100 ms, 40 ms apart). It spreads each block's FFT over
    /// several calls: with the default preset one call in every 30 ms does it
    /// all, which took half of a 128-frame callback for a single voice.
    /// Allocates.
    pub fn new(sample_rate: u32) -> Self {
        Self { inner: Stretch::preset_cheaper(CHANNELS as u32, sample_rate), sample_rate }
    }

    /// Signalsmith's default preset (blocks of 120 ms, 30 ms apart): a little
    /// better, but with the CPU spikes described at [`Stretcher::new`]; for
    /// comparisons only. Allocates.
    pub fn default_preset(sample_rate: u32) -> Self {
        Self { inner: Stretch::preset_default(CHANNELS as u32, sample_rate), sample_rate }
    }

    /// Share of real time (0.01 = 1 %) one stretcher takes on this machine,
    /// measured over a quarter second of audio in blocks of 64 frames.
    /// Allocates and takes a few milliseconds; for sizing a pool at startup.
    pub fn cost(sample_rate: u32) -> f64 {
        let mut s = Self::new(sample_rate);
        s.set_pitch(1.1);
        let frames = sample_rate as usize / 4;
        let input: Vec<f32> =
            (0..(frames * 2 + 256) * CHANNELS).map(|i| ((i as f32) * 0.013).sin() * 0.3).collect();
        let mut output = vec![0.0; 64 * CHANNELS];
        let start = std::time::Instant::now();
        let (mut pos, mut done) = (0, 0);
        while done < frames {
            // Somewhat slower than real time: more input per output, the dearer case.
            let take = 80;
            s.process(&input[pos * CHANNELS..(pos + take) * CHANNELS], &mut output);
            pos += take;
            done += 64;
        }
        start.elapsed().as_secs_f64() / (done as f64 / sample_rate as f64)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Frames between input passed to [`Stretcher::process`] and the centre of
    /// the effect.
    pub fn input_latency(&self) -> usize {
        self.inner.input_latency()
    }

    /// Frames between the centre of the effect and the output.
    pub fn output_latency(&self) -> usize {
        self.inner.output_latency()
    }

    /// Input and output latency together: how far the output lags behind the
    /// input at a time ratio of 1.
    pub fn latency(&self) -> usize {
        self.input_latency() + self.output_latency()
    }

    /// Frequency factor of the output (1 = unchanged, 2 = an octave up),
    /// independent of the time ratio. Non-finite or non-positive is 1.
    pub fn set_pitch(&mut self, factor: f32) {
        let factor = if factor.is_finite() && factor > 0.0 { factor } else { 1.0 };
        self.inner.set_transpose_factor(factor, None);
    }

    /// Back to the state after creation (pitch stays). Does not allocate.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Feeds `input` (interleaved stereo) as previous input without producing
    /// output, e.g. after [`Stretcher::reset`], so the first output is not
    /// built from silence. `rate` is the expected time ratio.
    pub fn seek(&mut self, input: &[f32], rate: f64) {
        let input = &input[..input.len() - input.len() % CHANNELS];
        self.inner.seek(input, rate);
    }

    /// Consumes `input` and fills `output` (both interleaved stereo); the
    /// ratio of their lengths is the time ratio (twice as much input as
    /// output plays twice as fast).
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let input = &input[..input.len() - input.len() % CHANNELS];
        let len = output.len() - output.len() % CHANNELS;
        self.inner.process(input, &mut output[..len]);
    }
}

#[cfg(test)]
mod tests;
