//! Peaks of a whole file, fine enough to draw any view down to [`BLOCK`]
//! frames per pixel; closer views decode the stretch itself
//! ([`crate::waveform_range`]). Built once and kept on disk by the caller.

use std::path::Path;

use crate::loudness::{Loudness, Meter};
use crate::{MediaError, Waveform, peaks, stream};

/// Frames (at the file's own rate) per stored peak.
pub const BLOCK: usize = 256;

/// Peak level below which a file counts as silent for trimming: −50 dBFS,
/// above the noise floor of most recordings, below soft attacks and tails.
pub const AUDIBLE: f32 = 0.003_162_3;

/// Where the audible part of a file lies, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Audible {
    /// Start of the first block above [`AUDIBLE`].
    pub start: f64,
    /// End of the last one; `None` if that is the end of the file.
    pub end: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Peaks {
    /// The file's own sample rate.
    pub rate: u32,
    pub frames: u64,
    /// Loudest sample of each block of [`BLOCK`] frames (the last one may be
    /// shorter), left and right, 0…`u16::MAX` for 0…1 (clipped).
    pub blocks: Vec<[u16; 2]>,
}

impl Peaks {
    /// Decodes `path` (without resampling, piece by piece, so long files
    /// never sit in RAM). Gives up with [`MediaError::Cancelled`] as soon as
    /// `keep_going` returns false.
    pub fn build(path: &Path, keep_going: &dyn Fn() -> bool) -> Result<Self, MediaError> {
        Ok(Self::build_measured(path, keep_going)?.0)
    }

    /// Like [`Peaks::build`], measuring the file's loudness in the same pass.
    pub fn build_measured(
        path: &Path,
        keep_going: &dyn Fn() -> bool,
    ) -> Result<(Self, Loudness), MediaError> {
        let mut decoder = stream::StreamDecoder::open(path, None)?;
        let rate = decoder.target_rate();
        let mut meter = Meter::new(rate);
        let (mut blocks, mut frames, mut piece) = (Vec::new(), 0u64, Vec::new());
        loop {
            if !keep_going() {
                return Err(MediaError::Cancelled);
            }
            piece.clear();
            // Whole blocks: `read` only returns fewer frames at the end.
            let read = decoder.read(&mut piece, BLOCK * 64)?;
            if read == 0 {
                break;
            }
            frames += read as u64;
            meter.add(&piece);
            blocks.extend(piece.chunks(BLOCK * 2).map(|block| {
                let peak = |side: usize| {
                    block.iter().skip(side).step_by(2).fold(0.0f32, |max, sample| max.max(sample.abs()))
                };
                [quantize(peak(0)), quantize(peak(1))]
            }));
        }
        Ok((Self { rate, frames, blocks }, meter.result()))
    }

    /// Seconds.
    pub fn duration(&self) -> f64 {
        self.frames as f64 / self.rate as f64
    }

    /// The audible part of the file (see [`AUDIBLE`]), to the block;
    /// `None` if the file is silent throughout.
    pub fn audible(&self) -> Option<Audible> {
        let loud = |block: &[u16; 2]| level(*block) >= AUDIBLE;
        let first = self.blocks.iter().position(loud)?;
        let last = self.blocks.iter().rposition(loud)?;
        let rate = self.rate as f64;
        let end = ((last + 1) as u64 * BLOCK as u64).min(self.frames);
        Some(Audible {
            start: (first * BLOCK) as f64 / rate,
            end: (end < self.frames).then(|| end as f64 / rate),
        })
    }

    /// The whole file as `buckets` peaks (fewer if it has fewer blocks).
    pub fn overview(&self, buckets: usize) -> Waveform {
        let levels: Vec<f32> = self.blocks.iter().map(|&block| level(block)).collect();
        Waveform { duration: self.duration(), peaks: peaks(&levels, 1, buckets) }
    }

    /// Like [`crate::waveform_range`] for `from`–`to` seconds; `None` if a
    /// bucket would be shorter than a block, so the stretch must be decoded.
    pub fn range(&self, from: f64, to: f64, buckets: usize) -> Option<Waveform> {
        let rate = self.rate as f64;
        let from = from.max(0.0);
        let requested = (to - from) * rate;
        if buckets == 0 || requested < (BLOCK * buckets) as f64 {
            return None;
        }
        let start = ((from * rate).round() as u64).min(self.frames);
        let end = ((to * rate).round() as u64).clamp(start, self.frames);
        let length = end - start;
        // As dense as asked for, also where the file ends before `to`.
        let count = ((length as f64 * buckets as f64 / requested).round() as u64).clamp(1, length.max(1));
        let peaks = (0..count)
            .filter(|_| length > 0)
            .map(|bucket| {
                let first = (start + bucket * length / count) as usize / BLOCK;
                let last = ((start + (bucket + 1) * length / count) as usize).div_ceil(BLOCK).max(first + 1);
                let blocks = self.blocks.get(first..last.min(self.blocks.len())).unwrap_or_default();
                blocks.iter().fold(0.0f32, |max, &block| max.max(level(block)))
            })
            .collect();
        Some(Waveform { duration: length as f64 / rate, peaks })
    }
}

fn quantize(peak: f32) -> u16 {
    (peak.min(1.0) * u16::MAX as f32).round() as u16
}

/// Both sides of a block, 0…1.
fn level([left, right]: [u16; 2]) -> f32 {
    left.max(right) as f32 / u16::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peaks(levels: &[f32], frames: u64) -> Peaks {
        let blocks = levels.iter().map(|&level| [quantize(level), quantize(level / 2.0)]).collect();
        Peaks { rate: 25_600, frames, blocks }
    }

    #[test]
    fn audible_skips_silence_and_noise_at_both_ends() {
        let found = peaks(&[0.0, 0.001, 0.5, 0.0, 0.2, 0.002, 0.0], 7 * 256 - 10).audible();
        assert_eq!(found, Some(Audible { start: 0.02, end: Some(0.05) }));
    }

    #[test]
    fn audible_to_the_end_of_the_file_leaves_the_end_open() {
        assert_eq!(peaks(&[0.1, 0.1], 400).audible(), Some(Audible { start: 0.0, end: None }));
    }

    #[test]
    fn a_silent_file_has_no_audible_part() {
        assert_eq!(peaks(&[0.0, 0.003], 512).audible(), None);
        assert_eq!(peaks(&[], 0).audible(), None);
    }
}
