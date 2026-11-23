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
    /// Largest difference after a seek from decoding straight through.
    seek: f32,
}

const fn lossless(file: &'static str, rate: u32, secs: f64, bits: i32) -> Fixture {
    // Half a step of the format, and some room for rounding on the way.
    let step = 1.0 / (1u64 << (bits - 1)) as f64;
    Fixture { file, rate, secs, exact: Some(step), mono: false, slack: 2, seek: 1e-4 }
}

const fn lossy(file: &'static str, rate: u32) -> Fixture {
    Fixture { file, rate, secs: 0.5, exact: None, mono: false, slack: 2048, seek: 1e-4 }
}

/// Opus predicts from the frames before, so a decoder started cold at a
/// seek converges on the straight decode only slowly: after the preroll it
/// is still off by about −56 dB, fading. The timing is exact.
const fn opus(file: &'static str) -> Fixture {
    Fixture { seek: 2e-3, ..lossy(file, 48_000) }
}

const FIXTURES: [Fixture; 26] = [
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
    lossless("pcm24.caf", 48_000, 0.25, 24),
    // Read by jq-media's `wav64`, as symphonia only knows RIFF.
    lossless("rf64.wav", 48_000, 0.25, 24),
    lossless("float32.w64", 44_100, 0.25, 24),
    lossless("flac16.flac", 44_100, 0.5, 16),
    lossless("flac24.flac", 96_000, 0.5, 24),
    lossless("alac.m4a", 44_100, 0.5, 16),
    lossless("flac.oga", 44_100, 0.5, 16),
    lossy("aac.m4a", 44_100),
    lossy("adts.aac", 44_100),
    lossy("vorbis.ogg", 44_100),
    // Decoded by jq-media's `opus` (libopus); the Ogg reader trims the pre-skip.
    opus("opus.ogg"),
    opus("opus.opus"),
    // 5.1 in Vorbis channel order (L C R …): front left and right must come first.
    opus("surround.opus"),
    lossy("cbr.mp3", 48_000),
    // ID3v2 with a title and a cover picture before the audio.
    lossy("tagged.mp3", 44_100),
    lossy("mp2.mp2", 48_000),
    // The sound of a video clip (H.264 picture, AAC sound).
    lossy("video.mp4", 48_000),
    lossy("video.mov", 48_000),
    // MP3 in WAV (format tag 0x0055): read by jq-media's `wav_mpeg`. Without
    // a LAME header nothing tells the delay and padding (two MP3 frames):
    // ffmpeg keeps them as well.
    Fixture { slack: 2 * 1152, ..lossy("mp3-in.wav", 44_100) },
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
            assert!(worst < f.seek, "{}: seek to {target} is off by {worst}", f.file);
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

/// Whether symphonia on its own (its probe and its decoders, without what
/// jq-media adds) opens `file` and makes a decoder for it.
fn symphonia_alone_reads(file: &str) -> bool {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let path = path(file);
    let source = std::fs::File::open(&path).unwrap();
    let mss = MediaSourceStream::new(Box::new(source), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .and_then(|format| {
            let params = format.default_track(TrackType::Audio).and_then(|t| t.codec_params.clone());
            let params = params
                .and_then(|p| p.audio().cloned())
                .ok_or(symphonia::core::errors::Error::Unsupported("no track"))?;
            symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default())
        })
        .is_ok()
}

// The tests below fail once symphonia reads by itself what jq-media reads
// in its own code: then that code is dead weight (or, for Opus, competes
// with symphonia's decoder). Each says what to remove; the files stay in
// `FIXTURES`, which keeps checking that they still play.

/// The tests below would pass with a helper that never reads anything.
#[test]
fn symphonia_alone_reads_what_it_supports() {
    for file in ["pcm16.wav", "vorbis.ogg", "aac.m4a", "cbr.mp3"] {
        assert!(symphonia_alone_reads(file), "{file}");
    }
}

/// `.wav` may hold MP3 (format tag 0x0055). symphonia's WAV reader only
/// knows PCM, float, A-law/µ-law and ADPCM, so jq-media reads the `data`
/// chunk itself (`src/wav_mpeg.rs`, entered only when symphonia refuses).
/// When this fails: delete `wav_mpeg.rs`, its use in `open_format` and this test.
#[test]
fn symphonia_still_refuses_mp3_in_wav() {
    assert!(
        !symphonia_alone_reads("mp3-in.wav"),
        "symphonia reads MP3 in WAV now: remove jq-media's own handling (see this test's comment)"
    );
}

/// symphonia's WAV reader only knows RIFF, so jq-media reads RF64/BW64 and
/// Wave64 itself (`src/wav64.rs`, entered only when symphonia refuses).
/// When this fails for both: delete `wav64.rs`, its use in `open_format` and
/// this test (for one of them: only that part of `wav64.rs`). Check that
/// symphonia takes the length of RF64 files over 4 GB from the `ds64` chunk.
#[test]
fn symphonia_still_refuses_rf64_and_wave64() {
    for file in ["rf64.wav", "float32.w64"] {
        assert!(
            !symphonia_alone_reads(file),
            "symphonia reads {file} now: remove jq-media's own handling (see this test's comment)"
        );
    }
}

/// symphonia reads Opus from Ogg but has no decoder for it, so jq-media
/// brings one (`src/opus.rs`, libopus via `opusic-sys`, registered in
/// `codecs`). When this fails: delete `opus.rs`, its registration, the
/// `opusic-sys` dependency (and the CMake requirement in README.md and
/// CLAUDE.md) and this test, after checking that symphonia's decoder passes
/// `FIXTURES` (5.1 included) and applies the output gain.
#[test]
fn symphonia_still_has_no_opus_decoder() {
    for file in ["opus.ogg", "surround.opus"] {
        assert!(
            !symphonia_alone_reads(file),
            "symphonia decodes {file} now: remove jq-media's Opus decoder (see this test's comment)"
        );
    }
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

/// Matroska (`.mka`) is not offered: symphonia's reader states no length and
/// seeks to the wrong samples, and MP3 in it plays about 2000 frames long.
/// When this fails, it may have learned the length: move `flac.mka` into
/// `FIXTURES` (and `mka` into `app/src/lib/media.ts`) if the rest passes.
#[test]
fn matroska_still_has_no_length() {
    assert_eq!(probe_duration(&path("flac.mka")).unwrap(), None);
}

/// Formats met in practice that jq-media cannot decode: refused by every
/// reader, already when the file is added (`probe_duration`), naming the
/// format, whether symphonia reads the container (AC-3 and 5.1 AAC in MP4)
/// or not. When one plays after an upgrade, symphonia learned it: move the
/// file into `FIXTURES` (its extension into `app/src/lib/media.ts`), and
/// drop its name from `codec_name` or `recognize` in `src/lib.rs` if no
/// other file needs it.
#[test]
fn unsupported_formats_are_refused_by_name() {
    for (file, codec) in [
        ("ac3.m4a", "AC-3 (Dolby Digital)"),
        // symphonia's AAC decoder takes only AAC-LC in mono or stereo.
        ("aac51.m4a", "HE-AAC or surround AAC"),
        ("ac3.ac3", "AC-3 (Dolby Digital)"),
        ("wma.wma", "WMA"),
        ("wavpack.wv", "WavPack"),
    ] {
        let file = path(file);
        let reason = jq_i18n::detail!("media_unsupported_codec", codec = codec);
        let reasons = [
            probe_duration(&file).err().map(|e| e.reason()),
            decode_file(&file, 48_000).err().map(|e| e.reason()),
            StreamDecoder::open(&file, Some(48_000)).err().map(|e| e.reason()),
            Peaks::build(&file, &|| true).err().map(|e| e.reason()),
            Loudness::measure(&file, &|| true).err().map(|e| e.reason()),
        ];
        for got in reasons {
            assert_eq!(got.as_ref(), Some(&reason), "{}", file.display());
        }
    }
}
