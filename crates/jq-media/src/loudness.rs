//! Loudness of a whole file after EBU R128 (integrated loudness and true
//! peak), measured in the background so cues can be normalized to a target.
//! The file itself is never changed.

use std::path::Path;

use ebur128::{EbuR128, Mode};

use crate::{MediaError, stream};

/// Frames decoded per step.
const STEP: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loudness {
    /// Integrated loudness, LUFS; `None` for a silent file.
    pub lufs: Option<f64>,
    /// Highest true peak of both sides, dBTP; `None` for a silent file.
    pub peak_db: Option<f64>,
}

impl Loudness {
    /// Decodes `path` (stereo, at the file's own rate, piece by piece) and
    /// measures it. Gives up with [`MediaError::Cancelled`] as soon as
    /// `keep_going` returns false.
    pub fn measure(path: &Path, keep_going: &dyn Fn() -> bool) -> Result<Self, MediaError> {
        let mut decoder = stream::StreamDecoder::open(path, None)?;
        let mut meter = Meter::new(decoder.target_rate());
        let mut piece = Vec::new();
        loop {
            if !keep_going() {
                return Err(MediaError::Cancelled);
            }
            piece.clear();
            if decoder.read(&mut piece, STEP)? == 0 {
                return Ok(meter.result());
            }
            meter.add(&piece);
        }
    }

    /// Offset that brings the file to `target` LUFS, at most so much that
    /// its true peak stays at `ceiling` dBTP, rounded to 0.1 dB; `None` for
    /// a silent file.
    pub fn offset(&self, target: f64, ceiling: f64) -> Option<f64> {
        let offset = target - self.lufs?;
        let offset = match self.peak_db {
            Some(peak) => offset.min(ceiling - peak),
            None => offset,
        };
        Some((offset * 10.0).round() / 10.0)
    }
}

/// Measures interleaved stereo pieces as they are decoded.
pub(crate) struct Meter(Option<EbuR128>);

impl Meter {
    /// A rate the meter cannot take gives no result instead of an error.
    pub(crate) fn new(rate: u32) -> Self {
        Self(EbuR128::new(2, rate, Mode::I | Mode::TRUE_PEAK).ok())
    }

    pub(crate) fn add(&mut self, stereo: &[f32]) {
        if let Some(meter) = &mut self.0
            && meter.add_frames_f32(stereo).is_err()
        {
            self.0 = None;
        }
    }

    pub(crate) fn result(&self) -> Loudness {
        let Some(meter) = &self.0 else { return Loudness { lufs: None, peak_db: None } };
        let lufs = meter.loudness_global().ok().filter(|l| l.is_finite());
        let peak = (0..2).filter_map(|c| meter.true_peak(c).ok()).fold(0.0f64, f64::max);
        let peak_db = (peak > 0.0).then(|| 20.0 * peak.log10());
        Loudness { lufs, peak_db }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(db: f64, secs: f64, rate: u32) -> Vec<f32> {
        let amplitude = 10f64.powf(db / 20.0);
        (0..(secs * rate as f64) as usize)
            .flat_map(|i| {
                let s =
                    (amplitude * (2.0 * std::f64::consts::PI * 997.0 * i as f64 / rate as f64).sin()) as f32;
                [s, s]
            })
            .collect()
    }

    #[test]
    fn sine_on_both_sides_measures_by_the_standard() {
        let mut meter = Meter::new(48_000);
        for piece in sine(-20.0, 5.0, 48_000).chunks(STEP * 2) {
            meter.add(piece);
        }
        let loudness = meter.result();
        // A 1 kHz sine on both channels measures its peak level (EBU Tech 3341).
        let lufs = loudness.lufs.unwrap();
        assert!((lufs + 20.0).abs() < 0.1, "{lufs}");
        let peak = loudness.peak_db.unwrap();
        assert!((peak + 20.0).abs() < 0.2, "{peak}");
    }

    #[test]
    fn silence_has_no_loudness() {
        let mut meter = Meter::new(48_000);
        meter.add(&vec![0.0; 48_000 * 2]);
        assert_eq!(meter.result(), Loudness { lufs: None, peak_db: None });
        assert_eq!(meter.result().offset(-20.0, -1.0), None);
    }

    #[test]
    fn offset_reaches_the_target_below_the_ceiling() {
        let quiet = Loudness { lufs: Some(-27.04), peak_db: Some(-12.0) };
        assert_eq!(quiet.offset(-20.0, -1.0), Some(7.0));
        // +7 would push the peak above −1 dBTP.
        let peaky = Loudness { lufs: Some(-27.0), peak_db: Some(-4.0) };
        assert_eq!(peaky.offset(-20.0, -1.0), Some(3.0));
        let loud = Loudness { lufs: Some(-12.0), peak_db: Some(0.5) };
        assert_eq!(loud.offset(-20.0, -1.0), Some(-8.0));
    }
}
