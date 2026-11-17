//! Real files of every format the app offers (`app/src/lib/media.ts`), made
//! by ffmpeg with `tests/fixtures/formats/make.sh`: each one must decode to
//! the signal it holds (left a 441 Hz sine at 0.5, right an 882 Hz sine at
//! 0.25), at its own rate and resampled, from the start and after a seek,
//! and the waveform and loudness readers must read it too.

use std::path::{Path, PathBuf};

use jq_media::loudness::Loudness;
use jq_media::peaks::Peaks;
use jq_media::stream::StreamDecoder;
use jq_media::{decode_file, probe_duration};

const LEFT: (f64, f64) = (441.0, 0.5);
const RIGHT: (f64, f64) = (882.0, 0.25);

struct Fixture {
    file: &'static str,
    rate: u32,
    secs: f64,
    /// Lossless: the largest difference from the formula a sample may have
    /// (its quantization). `None`: lossy, compared by level and pitch only.
    exact: Option<f64>,
    mono: bool,
    /// Frames a lossy decode may run longer or shorter than the signal:
    /// encoder delay and padding the file does not let the decoder remove.
    slack: usize,
}

const fn lossless(file: &'static str, rate: u32, secs: f64, bits: i32) -> Fixture {
    // Half a step of the format, and some room for rounding on the way.
    let step = 1.0 / (1u64 << (bits - 1)) as f64;
    Fixture { file, rate, secs, exact: Some(step), mono: false, slack: 2 }
}

const fn lossy(file: &'static str, rate: u32) -> Fixture {
    Fixture { file, rate, secs: 0.5, exact: None, mono: false, slack: 2048 }
}

const FIXTURES: [Fixture; 15] = [
    lossless("pcm16.wav", 44_100, 0.25, 16),
    // 24 bit: WAVE_FORMAT_EXTENSIBLE.
    lossless("pcm24.wav", 44_100, 0.25, 24),
    lossless("float32.wav", 48_000, 0.25, 24),
    Fixture { mono: true, ..lossless("u8-mono.wav", 22_050, 0.25, 8) },
    // 5.1: the first two channels are left and right; the others (loud,
    // other pitches) must not get in.
    lossless("surround.wav", 44_100, 0.25, 16),
    lossless("pcm16.aiff", 44_100, 0.25, 16),
    lossless("pcm24.aif", 48_000, 0.25, 24),
    lossless("flac16.flac", 44_100, 0.5, 16),
    lossless("flac24.flac", 96_000, 0.5, 24),
    lossless("alac.m4a", 44_100, 0.5, 16),
    lossy("aac.m4a", 44_100),
    lossy("adts.aac", 44_100),
    lossy("vorbis.ogg", 44_100),
    lossy("cbr.mp3", 48_000),
    // ID3v2 with a title and a cover picture before the audio.
    lossy("tagged.mp3", 44_100),
];

fn path(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formats").join(file)
}

/// Level of the sine at `freq` in one channel of interleaved stereo
/// `samples`, over whole seconds' worth from `from` (phase does not matter:
/// lossy decoders may shift the signal by their delay).
fn level(samples: &[f32], channel: usize, rate: u32, freq: f64, from: usize, frames: usize) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for i in from..from + frames {
        let x = f64::from(samples[i * 2 + channel]);
        let phase = std::f64::consts::TAU * freq * i as f64 / f64::from(rate);
        re += x * phase.cos();
        im += x * phase.sin();
    }
    2.0 * re.hypot(im) / frames as f64
}

/// Both channels carry their sine at the right level and nothing of the
/// other one, in the middle of the file (away from fades and encoder delay).
fn assert_signal(f: &Fixture, samples: &[f32], rate: u32, what: &str) {
    let frames = samples.len() / 2;
    let (from, len) = (frames / 5, frames * 3 / 5);
    let right = if f.mono { LEFT } else { RIGHT };
    let other = |(freq, _): (f64, f64)| if freq == LEFT.0 { RIGHT.0 } else { LEFT.0 };
    for (channel, expected) in [(0, LEFT), (1, right)] {
        let got = level(samples, channel, rate, expected.0, from, len);
        assert!(
            (got - expected.1).abs() < expected.1 * if f.exact.is_some() { 0.03 } else { 0.1 },
            "{} {what}, channel {channel}: {} Hz at {got:.4}, not {}",
            f.file,
            expected.0,
            expected.1
        );
        let leak = level(samples, channel, rate, other(expected), from, len);
        assert!(
            leak < 0.01,
            "{} {what}, channel {channel}: {} Hz leaks in at {leak:.4}",
            f.file,
            other(expected)
        );
    }
    // The ends ring (the sine stops mid-cycle; codecs and resampling smear that).
    let peak = samples[from * 2..(from + len) * 2].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak < f.max_peak(), "{} {what}: peak {peak}, something else got in", f.file);
}

impl Fixture {
    /// Highest sample level expected for a signal peaking at 0.5. Lossy
    /// encoders overshoot (ffmpeg's AAC encoder by up to 2 dB; ffmpeg's
    /// own decoder gives the same samples).
    fn max_peak(&self) -> f32 {
        if self.exact.is_some() { 0.53 } else { 0.7 }
    }
}

fn decode_all(file: &str, rate: u32) -> Vec<f32> {
    decode_file(&path(file), rate).unwrap_or_else(|e| panic!("{file}: {e}")).samples().to_vec()
}

#[test]
fn every_format_reports_its_length() {
    for f in &FIXTURES {
        let secs = probe_duration(&path(f.file)).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        let secs = secs.unwrap_or_else(|| panic!("{}: no length", f.file));
        let tolerance = if f.exact.is_some() { 0.001 } else { 0.05 };
        assert!((secs - f.secs).abs() < tolerance, "{}: {secs} s, not {} s", f.file, f.secs);
    }
}

#[test]
fn every_format_decodes_to_its_signal() {
    for f in &FIXTURES {
        let samples = decode_all(f.file, f.rate);
        let frames = samples.len() / 2;
        let expected = (f.secs * f64::from(f.rate)).round() as usize;
        match f.exact {
            Some(step) => {
                assert_eq!(frames, expected, "{}: every frame, no more", f.file);
                let right = if f.mono { LEFT } else { RIGHT };
                let worst = samples
                    .chunks(2)
                    .enumerate()
                    .flat_map(|(i, lr)| {
                        let t = i as f64 / f64::from(f.rate);
                        let want =
                            [LEFT, right].map(|(freq, amp)| amp * (std::f64::consts::TAU * freq * t).sin());
                        [(f64::from(lr[0]) - want[0]).abs(), (f64::from(lr[1]) - want[1]).abs()]
                    })
                    .fold(0.0, f64::max);
                assert!(worst <= step, "{}: off by {worst}, more than {step}", f.file);
            }
            // Encoders pad to whole codec frames; the decoder should take
            // the padding off again, but a few frames' worth may stay.
            None => {
                assert!(frames.abs_diff(expected) <= f.slack, "{}: {frames} frames, not {expected}", f.file)
            }
        }
        assert_signal(f, &samples, f.rate, "as it is");
    }
}

#[test]
fn every_format_keeps_its_pitch_at_the_device_rate() {
    for f in &FIXTURES {
        let device = if f.rate == 48_000 { 44_100 } else { 48_000 };
        let samples = decode_all(f.file, device);
        let expected = f.secs * f64::from(device);
        let tolerance = (f.slack as f64 * f64::from(device) / f64::from(f.rate)).max(2.0);
        assert!(
            ((samples.len() / 2) as f64 - expected).abs() <= tolerance,
            "{} at {device} Hz: {} frames, not {expected}",
            f.file,
            samples.len() / 2
        );
        assert_signal(f, &samples, device, &format!("at {device} Hz"));
    }
}

#[test]
fn every_format_seeks_to_the_same_samples() {
    for f in &FIXTURES {
        let straight = decode_all(f.file, f.rate);
        let frames = straight.len() / 2;
        for target in [frames / 3, frames / 2 + 77] {
            let mut decoder = StreamDecoder::open(&path(f.file), Some(f.rate)).unwrap();
            decoder.seek(target).unwrap_or_else(|e| panic!("{}: {e}", f.file));
            let mut seeked = Vec::new();
            let len = 1024.min(frames - target);
            decoder.read(&mut seeked, len).unwrap();
            let reference = &straight[target * 2..(target + len) * 2];
            let worst = reference.iter().zip(&seeked).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert_eq!(seeked.len(), reference.len(), "{} at {target}", f.file);
            assert!(worst < 1e-4, "{}: seek to {target} is off by {worst}", f.file);
        }
    }
}

#[test]
fn every_format_has_a_waveform_and_a_loudness() {
    for f in &FIXTURES {
        let peaks = Peaks::build(&path(f.file), &|| true).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        assert!((peaks.duration() - f.secs).abs() < 0.05, "{}: waveform of {} s", f.file, peaks.duration());
        let overview = peaks.overview(10);
        let loudest = overview.peaks.iter().fold(0.0f32, |m, &p| m.max(p));
        assert!((0.45..=f.max_peak()).contains(&loudest), "{}: waveform peaks at {loudest}", f.file);

        let loudness =
            Loudness::measure(&path(f.file), &|| true).unwrap_or_else(|e| panic!("{}: {e}", f.file));
        // 0.5 is −6 dBFS; the true peak may lie a little above the samples.
        let peak = loudness.peak_db.unwrap_or_else(|| panic!("{}: no peak", f.file));
        let highest = 20.0 * f64::from(f.max_peak()).log10() + 0.5;
        assert!((-6.5..=highest).contains(&peak), "{}: peak {peak} dB", f.file);
        // Integrated loudness needs 400 ms; a shorter file has none.
        assert_eq!(
            loudness.lufs.is_some_and(f64::is_finite),
            f.secs >= 0.4,
            "{}: {:?}",
            f.file,
            loudness.lufs
        );
    }
}

/// Refused by every reader, with a reason the operator understands.
fn assert_refused(file: &str) {
    let file = path(file);
    let unsupported = "not a supported audio format";
    assert_eq!(decode_file(&file, 48_000).unwrap_err().reason(), unsupported);
    assert_eq!(
        StreamDecoder::open(&file, Some(48_000)).err().map(|e| e.reason()).as_deref(),
        Some(unsupported)
    );
    assert!(Peaks::build(&file, &|| true).is_err());
    assert!(Loudness::measure(&file, &|| true).is_err());
}

/// `.ogg` may hold Opus instead of Vorbis; symphonia has no Opus decoder.
/// The Ogg container is read, so the length is known: only playing fails.
/// Once a decoder comes in, move the file into `FIXTURES` (`lossy`).
#[test]
fn opus_in_ogg_is_refused_with_a_reason() {
    assert_refused("opus.ogg");
    let secs = probe_duration(&path("opus.ogg")).unwrap().unwrap();
    assert!((secs - 0.5).abs() < 0.05, "{secs}");
}

/// `.wav` may hold MP3 (format tag 0x0055); symphonia's WAV reader only
/// knows PCM, float, A-law/µ-law and ADPCM. Once it is read, move the file
/// into `FIXTURES` (`lossy`).
#[test]
fn mp3_in_wav_is_refused_with_a_reason() {
    assert_refused("mp3-in.wav");
    let error = probe_duration(&path("mp3-in.wav")).unwrap_err();
    assert_eq!(error.reason(), "not a supported audio format", "not even the length");
}

/// symphonia's MP4 reader ignores edit lists, so AAC in MP4 keeps the
/// encoder's priming (1024 frames) and end padding: the cue starts about
/// 23 ms late and runs that much longer. ffmpeg decodes the same samples,
/// only trimmed. If this fails after an upgrade, the reader learned edit
/// lists: expect exactly 0.5 s here and in `every_format_decodes_to_its_signal`.
#[test]
fn aac_in_mp4_keeps_the_encoder_delay() {
    let samples = decode_all("aac.m4a", 44_100);
    assert_eq!(samples.len() / 2, 22_050 + 1024 + 478);
}
