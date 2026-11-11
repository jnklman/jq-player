//! Decoding of media files into engine buffers.
//!
//! Files are converted to stereo and resampled to the device rate. Normal
//! files are decoded completely into RAM, so GO never touches the disk; long
//! ones are streamed in chunks (see [`stream`]).

pub mod loudness;
pub mod peaks;
pub mod stream;

use std::fs::File;
use std::path::{Path, PathBuf};

use jq_engine::AudioBuffer;
use jq_i18n::t;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

#[derive(Debug)]
pub enum MediaError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Decode {
        path: PathBuf,
        source: SymError,
    },
    NoAudioTrack {
        path: PathBuf,
    },
    Resample {
        path: PathBuf,
        message: String,
    },
    /// Work given up on request (see [`peaks::Peaks::build`]).
    Cancelled,
}

impl MediaError {
    /// What went wrong, without the path.
    pub fn reason(&self) -> String {
        match self {
            Self::Io { source, .. } => source.to_string(),
            Self::Decode { source, .. } => describe(source),
            Self::NoAudioTrack { .. } => t!("media.no_audio_track"),
            Self::Resample { message, .. } => t!("media.resample_failed", message = message),
            Self::Cancelled => t!("media.cancelled"),
        }
    }

    fn path(&self) -> Option<&Path> {
        match self {
            Self::Io { path, .. }
            | Self::Decode { path, .. }
            | Self::NoAudioTrack { path }
            | Self::Resample { path, .. } => Some(path),
            Self::Cancelled => None,
        }
    }
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.path() {
            Some(path) => write!(f, "{}: {}", path.display(), self.reason()),
            None => f.write_str(&self.reason()),
        }
    }
}

impl std::error::Error for MediaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Decode { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// A decoder error in a few words; its own (English) text adds nothing a
/// user could act on.
fn describe(error: &SymError) -> String {
    match error {
        SymError::IoError(e) => t!("media.read_error", error = e),
        SymError::Unsupported(_) => t!("media.unsupported"),
        SymError::DecodeError(_) => t!("media.corrupt"),
        SymError::SeekError(_) => t!("media.seek_failed"),
        SymError::LimitError(_) => t!("media.too_large"),
        _ => t!("media.decode_error"),
    }
}

/// Files at least this long (seconds) are streamed instead of decoded into
/// RAM (about 23 MB per minute at 48 kHz). Shorter ones stay completely in
/// memory, where any jump is instant and the disk plays no part.
pub const STREAM_AFTER: f64 = 30.0;

/// Loads `path` for playback at `target_rate`: long files as a stream,
/// others completely into RAM.
pub fn load(path: &Path, target_rate: u32) -> Result<AudioBuffer, MediaError> {
    match probe_duration(path)? {
        Some(secs) if secs >= STREAM_AFTER => stream::open_stream(path, target_rate),
        _ => decode_file(path, target_rate),
    }
}

/// Decodes `path` completely and returns a stereo buffer at `target_rate`.
pub fn decode_file(path: &Path, target_rate: u32) -> Result<AudioBuffer, MediaError> {
    let mut decoder = stream::StreamDecoder::open(path, Some(target_rate))?;
    // Reserved up front and trimmed at the end: grown by doubling, the
    // buffer could hold up to twice the memory it needs.
    let estimate = decoder.duration().map_or(0, |secs| (secs * target_rate as f64) as usize + 1024);
    let mut samples = Vec::with_capacity(estimate * 2);
    while decoder.read(&mut samples, 1 << 16)? > 0 {}
    samples.shrink_to_fit();
    Ok(AudioBuffer::from_interleaved_stereo(target_rate, samples))
}

/// Overview of a file for drawing its waveform.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Waveform {
    /// Seconds.
    pub duration: f64,
    /// Peak level (0…1, all channels) of each of the equally long slices.
    pub peaks: Vec<f32>,
}

/// Longest stretch [`waveform_range`] decodes, in seconds.
pub const RANGE_MAX_SECS: f64 = 120.0;

/// Like [`waveform`] for `from`–`to` seconds of the file only (at most
/// [`RANGE_MAX_SECS`]), from every frame instead of blocks: the detail of a
/// zoomed-in view. `duration` is the stretch covered, shorter where the file
/// ends before `to`.
pub fn waveform_range(path: &Path, from: f64, to: f64, buckets: usize) -> Result<Waveform, MediaError> {
    let mut decoder = stream::StreamDecoder::open(path, None)?;
    let rate = decoder.target_rate() as f64;
    let from = from.max(0.0);
    let frames = ((to.min(from + RANGE_MAX_SECS) - from).max(0.0) * rate).round() as usize;
    decoder.seek((from * rate).round() as usize)?;
    let mut samples = Vec::with_capacity(frames * 2);
    while samples.len() < frames * 2 {
        let want = (frames - samples.len() / 2).min(1 << 16);
        if decoder.read(&mut samples, want)? == 0 {
            break;
        }
    }
    samples.truncate(frames * 2);
    Ok(Waveform { duration: (samples.len() / 2) as f64 / rate, peaks: peaks(&samples, 2, buckets) })
}

fn peaks(samples: &[f32], channels: usize, buckets: usize) -> Vec<f32> {
    let frames = samples.len() / channels;
    if frames == 0 || buckets == 0 {
        return Vec::new();
    }
    let buckets = buckets.min(frames);
    (0..buckets)
        .map(|b| {
            let (from, to) = (b * frames / buckets, (b + 1) * frames / buckets);
            samples[from * channels..to * channels].iter().fold(0.0f32, |m, s| m.max(s.abs())).min(1.0)
        })
        .collect()
}

/// Reads the duration in seconds from the file header without decoding.
/// `None` if the container does not state it.
pub fn probe_duration(path: &Path) -> Result<Option<f64>, MediaError> {
    let file = File::open(path).map_err(|source| MediaError::Io { path: path.to_owned(), source })?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|source| MediaError::Decode { path: path.to_owned(), source })?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| MediaError::NoAudioTrack { path: path.to_owned() })?;
    let from_duration = track.time_base.zip(track.duration).and_then(|(tb, d)| tb.calc_duration(d));
    let from_frames = || {
        let rate = track.codec_params.as_ref()?.audio()?.sample_rate?;
        Some(track.num_frames? as f64 / rate as f64)
    };
    Ok(from_duration.map(|t| t.as_secs_f64()).or_else(from_frames))
}

/// Appends `samples` (interleaved, `channels` per frame) to `out` as stereo:
/// mono is duplicated to both sides; more than two channels keep the first two.
pub(crate) fn push_stereo(out: &mut Vec<f32>, samples: &[f32], channels: usize) {
    match channels {
        2 => out.extend_from_slice(samples),
        1 => out.extend(samples.iter().flat_map(|&s| [s, s])),
        n => out.extend(samples.chunks_exact(n).flat_map(|f| [f[0], f[1]])),
    }
}

#[cfg(test)]
mod tests {
    use rubato::audioadapter_buffers::direct::InterleavedSlice;
    use rubato::{
        Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    use super::*;

    #[test]
    fn peaks_cover_every_frame() {
        // Stereo, 6 frames; the loudest sample of each half wins.
        let samples = [0.1, -0.2, 0.3, 0.0, -0.9, 0.1, 0.0, 0.0, 0.5, -0.6, 0.2, 0.2];
        assert_eq!(peaks(&samples, 2, 2), [0.9, 0.6]);
        assert_eq!(peaks(&samples, 2, 100).len(), 6, "never more buckets than frames");
        assert!(peaks(&[], 2, 10).is_empty());
        assert_eq!(peaks(&[2.0, -3.0], 1, 1), [1.0], "clipped to full scale");
    }

    #[test]
    fn waveform_range_shows_the_stretch_in_detail() {
        let dir = test_dir("range");
        let path = dir.join("steps.wav");
        // 1 s at 1000 Hz: quiet, then loud from 0.5 s on.
        let mono: Vec<i16> = (0..1000).map(|i| if i < 500 { 1000 } else { 16_000 }).collect();
        write_test_wav(&path, 1000, 1, &mono);
        let range = waveform_range(&path, 0.4, 0.6, 4).unwrap();
        assert!((range.duration - 0.2).abs() < 1e-9, "{}", range.duration);
        assert!(range.peaks[..2].iter().all(|&peak| peak < 0.05), "{:?}", range.peaks);
        assert!(range.peaks[2..].iter().all(|&peak| peak > 0.45), "{:?}", range.peaks);
        let tail = waveform_range(&path, 0.9, 2.0, 10).unwrap();
        assert!((tail.duration - 0.1).abs() < 1e-9, "ends with the file: {}", tail.duration);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn stored_peaks_give_the_overview_of_the_samples() {
        let dir = test_dir("peaks");
        let path = dir.join("tone.wav");
        let left_right: Vec<i16> =
            tone(10_000).iter().enumerate().map(|(i, &s)| if i % 2 == 0 { s } else { s / 3 }).collect();
        write_test_wav(&path, 8_000, 2, &left_right);
        let built = peaks::Peaks::build(&path, &|| true).unwrap();
        assert_eq!(
            (built.rate, built.frames, built.blocks.len()),
            (8_000, 5_000, 5_000usize.div_ceil(peaks::BLOCK))
        );
        let samples = decode_file(&path, 8_000).unwrap().samples().to_vec();
        for buckets in [1, 7, 19, 20, 100] {
            let overview = built.overview(buckets);
            assert!((overview.duration - 0.625).abs() < 1e-9);
            // Per block of the samples, as before peaks were kept.
            let blocks: Vec<f32> = samples
                .chunks(peaks::BLOCK * 2)
                .map(|block| block.iter().fold(0.0f32, |max, sample| max.max(sample.abs())))
                .collect();
            let expected = peaks(&blocks, 1, buckets);
            assert_eq!(overview.peaks.len(), expected.len(), "{buckets} buckets");
            for (got, want) in overview.peaks.iter().zip(&expected) {
                assert!((got - want).abs() < 1e-4, "{buckets} buckets: {got} vs {want}");
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn stored_peaks_serve_ranges_down_to_a_block() {
        let dir = test_dir("peaks-range");
        let path = dir.join("steps.wav");
        // 4 s at 1024 Hz: quiet, then loud from 2 s on (a block boundary).
        let mono: Vec<i16> = (0..4096).map(|i| if i < 2048 { 1000 } else { 16_000 }).collect();
        write_test_wav(&path, 1024, 1, &mono);
        let built = peaks::Peaks::build(&path, &|| true).unwrap();
        assert!(built.range(0.0, 1.0, 10).is_none(), "finer than a block: decode");
        let range = built.range(0.0, 4.0, 4).unwrap();
        assert!((range.duration - 4.0).abs() < 1e-9);
        assert!(range.peaks[..2].iter().all(|&peak| peak < 0.05), "{:?}", range.peaks);
        assert!(range.peaks[2..].iter().all(|&peak| peak > 0.45), "{:?}", range.peaks);
        // Past the end: as dense as asked for, over what is there.
        let tail = built.range(3.0, 7.0, 4).unwrap();
        assert!((tail.duration - 1.0).abs() < 1e-9, "{}", tail.duration);
        assert_eq!(tail.peaks.len(), 1);
        assert!(built.range(9.0, 13.0, 4).unwrap().peaks.is_empty());
        assert!(matches!(peaks::Peaks::build(&path, &|| false), Err(MediaError::Cancelled)));
        std::fs::remove_dir_all(dir).ok();
    }

    fn to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
        let mut out = vec![9.0];
        push_stereo(&mut out, samples, channels);
        out.split_off(1)
    }

    #[test]
    fn mono_is_duplicated() {
        assert_eq!(to_stereo(&[0.1, 0.2], 1), [0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn surround_keeps_front_pair() {
        assert_eq!(to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3), [1.0, 2.0, 4.0, 5.0]);
    }

    /// Whole-clip resampling as a reference for the streaming resampler.
    fn resample_stereo(samples: &[f32], from: u32, to: u32) -> Result<Vec<f32>, String> {
        let frames = samples.len() / 2;
        let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
            .oversampling_factor(256)
            .interpolation(SincInterpolationType::Quadratic);
        let ratio = to as f64 / from as f64;
        let mut resampler = Async::<f32>::new_sinc(ratio, 1.1, &params, 1024, 2, FixedAsync::Input)
            .map_err(|e| e.to_string())?;
        let input = InterleavedSlice::new(samples, 2, frames).map_err(|e| e.to_string())?;
        let out = resampler.process_all(&input, frames, None).map_err(|e| e.to_string())?;
        Ok(out.take_data())
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jq-media-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 440 Hz-ish sweep of `frames` mono frames.
    fn tone(frames: usize) -> Vec<i16> {
        (0..frames).map(|i| ((i as f32 * 0.0627).sin() * 12_000.0 + (i % 997) as f32) as i16).collect()
    }

    #[test]
    fn streamed_resampling_matches_whole_clip_resampling() {
        let dir = test_dir("resample");
        let path = dir.join("tone.wav");
        let mono = tone(30_000);
        write_test_wav(&path, 44_100, 1, &mono);
        let stereo: Vec<f32> = mono.iter().flat_map(|&s| [s as f32 / 32_768.0; 2]).collect();
        let reference = resample_stereo(&stereo, 44_100, 48_000).unwrap();

        let buf = decode_file(&path, 48_000).unwrap();
        // rubato rounds the expected length up, so allow one frame either way.
        assert!(
            buf.frames().abs_diff(reference.len() / 2) <= 1,
            "{} vs {}",
            buf.frames(),
            reference.len() / 2
        );
        let worst = buf.samples().iter().zip(&reference).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-4, "streamed resampling differs by {worst}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn seeking_continues_at_the_frame() {
        let dir = test_dir("seek");
        let path = dir.join("tone.wav");
        write_test_wav(&path, 48_000, 2, &tone(40_000));
        let whole = decode_file(&path, 48_000).unwrap();
        let mut decoder = stream::StreamDecoder::open(&path, Some(48_000)).unwrap();
        decoder.seek(12_345).unwrap();
        let mut piece = Vec::new();
        assert_eq!(decoder.read(&mut piece, 1000).unwrap(), 1000);
        assert_eq!(piece[..], whole.samples()[12_345 * 2..13_345 * 2]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resampled_seeking_continues_in_phase() {
        use jq_engine::stream::CHUNK_FRAMES;
        let dir = test_dir("seek-resampled");
        let path = dir.join("tone.wav");
        write_test_wav(&path, 44_100, 1, &tone(CHUNK_FRAMES * 3));
        let whole = decode_file(&path, 48_000).unwrap();
        // Chunk starts as the stream reader seeks to them, plus odd frames.
        for frame in [CHUNK_FRAMES, CHUNK_FRAMES * 2, 100, 12_345] {
            let mut decoder = stream::StreamDecoder::open(&path, Some(48_000)).unwrap();
            decoder.seek(frame).unwrap();
            let mut piece = Vec::new();
            assert_eq!(decoder.read(&mut piece, 2000).unwrap(), 2000);
            let reference = &whole.samples()[frame * 2..(frame + 2000) * 2];
            let worst = piece.iter().zip(reference).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(worst < 1e-4, "seek to {frame} differs by {worst}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn seeking_to_chunk_starts_is_exact() {
        use jq_engine::stream::CHUNK_FRAMES;
        let dir = test_dir("seek-chunks");
        let path = dir.join("tone.wav");
        write_test_wav(&path, 48_000, 1, &tone(CHUNK_FRAMES * 8));
        let whole = decode_file(&path, 48_000).unwrap();
        // Frame / rate in seconds used to round some of these one frame early.
        for chunk in 1..8 {
            let frame = chunk * CHUNK_FRAMES;
            let mut decoder = stream::StreamDecoder::open(&path, Some(48_000)).unwrap();
            decoder.seek(frame).unwrap();
            let mut piece = Vec::new();
            assert_eq!(decoder.read(&mut piece, 100).unwrap(), 100);
            assert_eq!(piece[..], whole.samples()[frame * 2..(frame + 100) * 2], "chunk {chunk}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stream_reader_loads_what_playback_needs() {
        use jq_engine::stream::CHUNK_FRAMES;
        let dir = test_dir("reader");
        let path = dir.join("long.wav");
        let frames = CHUNK_FRAMES * 3 + 500;
        write_test_wav(&path, 8_000, 1, &tone(frames));
        let whole = decode_file(&path, 8_000).unwrap();
        let buf = stream::open_stream(&path, 8_000).unwrap();
        let data = buf.stream().unwrap().clone();
        assert_eq!(buf.frames(), frames);

        // Idle: the start is loaded for GO.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !buf.is_ready_at(0) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(buf.is_ready_at(0));
        // A voice far into the file: its chunk and the next follow.
        let slot = data.claim_slot(1).unwrap();
        data.report_position(slot, CHUNK_FRAMES * 2 + 10);
        while !buf.is_ready_at(CHUNK_FRAMES * 3) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        for f in [5, CHUNK_FRAMES * 2 + 77, frames - 1] {
            assert_eq!(buf.frame(f), whole.frame(f), "frame {f}");
        }
        assert_eq!(data.underruns(), 0);
        drop(buf);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stream_keeps_every_start_point_ready() {
        use jq_engine::stream::CHUNK_FRAMES;
        let dir = test_dir("hints");
        let path = dir.join("long.wav");
        let frames = CHUNK_FRAMES * 14;
        write_test_wav(&path, 8_000, 1, &tone(frames));
        let buf = stream::open_stream(&path, 8_000).unwrap();
        let data = buf.stream().unwrap().clone();
        let wait = |ready: &dyn Fn() -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !ready() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            ready()
        };

        // Two cues on the same file, one from the start, one far into it.
        let late = CHUNK_FRAMES * 12 + 100;
        data.set_hints(vec![0, late]);
        assert!(wait(&|| buf.is_ready_at(0) && buf.is_ready_at(late)));
        // Nothing else is loaded while idle.
        assert!(!data.is_loaded(5));

        // The first cue is gone: its start is released again.
        data.set_hints(vec![late]);
        assert!(wait(&|| !data.is_loaded(0)));
        assert!(buf.is_ready_at(late));
        drop(buf);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodes_wav() {
        let dir = std::env::temp_dir().join(format!("jq-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        write_test_wav(&path, 48_000, 1, &[0, 16_384, -16_384, 32_767]);

        let buf = decode_file(&path, 48_000).unwrap();
        let d = probe_duration(&path).unwrap().unwrap();
        assert!((d - 4.0 / 48_000.0).abs() < 1e-6, "{d}");
        assert_eq!(buf.frames(), 4);
        assert_eq!(buf.frame(1), (0.5, 0.5));
        assert_eq!(buf.frame(2), (-0.5, -0.5));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn write_test_wav(path: &Path, rate: u32, channels: u16, samples: &[i16]) {
        let data_len = (samples.len() * 2) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        b.extend_from_slice(&(channels * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        samples.iter().for_each(|s| b.extend_from_slice(&s.to_le_bytes()));
        std::fs::write(path, b).unwrap();
    }
}
