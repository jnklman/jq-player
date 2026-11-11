//! Piecewise decoding: [`StreamDecoder`] reads a file in order (or from a
//! seek position) as stereo at a target rate, and [`open_stream`] runs a
//! reader thread that keeps the chunks of a streamed buffer loaded.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use jq_engine::AudioBuffer;
use jq_engine::stream::{CHUNK_FRAMES, GRACE, Retired, StreamData};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use crate::{MediaError, push_stereo};

/// Chunks kept loaded ahead of a playing position (about 14 s at 48 kHz),
/// so a slow or busy disk has time to catch up.
const AHEAD: usize = 10;
/// How often the reader looks at the playing positions while a voice plays
/// and everything wanted is loaded. With no voice it sleeps until the
/// control thread changes something.
const FOLLOW: Duration = Duration::from_millis(10);
/// How long a chunk that failed to decode is left alone, so a broken spot
/// neither spins nor holds up the chunks other voices need.
const ERROR_PAUSE: Duration = Duration::from_millis(500);
/// Resampler input chunk (frames).
const RESAMPLE_CHUNK: usize = 1024;
/// Source frames a resampled seek starts early, so the resampler has its
/// history (half its sinc length and then some) once the target comes.
const RESAMPLE_MARGIN: usize = 256;

/// Decodes a file in order as stereo at `target_rate`, with seeking.
pub struct StreamDecoder {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    source_rate: u32,
    target_rate: u32,
    duration: Option<f64>,
    resampler: Option<Async<f32>>,
    /// A decoded packet with all its channels, reused between packets.
    packet: Vec<f32>,
    /// Stereo source frames waiting for the resampler.
    input: Vec<f32>,
    /// Stereo output frames ready to hand out, from `out_pos` on.
    output: Vec<f32>,
    out_pos: usize,
    /// Source frames to drop after an accurate seek (it lands before the target).
    skip_source: usize,
    /// Output frames to drop: the resampler's delay.
    skip_output: usize,
    /// Output frames to drop after the delay: a resampled seek starts early.
    skip_lead: usize,
    /// Source frames fed and output frames produced since the last (re)start,
    /// so the flush at the end yields exactly the resampled length.
    fed: usize,
    produced: usize,
    /// No more packets; `flushed` once the resampler is drained too.
    ended: bool,
    flushed: bool,
}

impl StreamDecoder {
    /// Opens `path`; `target_rate` `None` keeps the file's own rate.
    pub fn open(path: &Path, target_rate: Option<u32>) -> Result<Self, MediaError> {
        let decode_err = |source| MediaError::Decode { path: path.to_owned(), source };
        let file = File::open(path).map_err(|source| MediaError::Io { path: path.to_owned(), source })?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(decode_err)?;
        let no_track = || MediaError::NoAudioTrack { path: path.to_owned() };
        let track = format.default_track(TrackType::Audio).ok_or_else(no_track)?;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(no_track)?;
        let source_rate = params.sample_rate.ok_or_else(no_track)?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(decode_err)?;
        let from_duration = track.time_base.zip(track.duration).and_then(|(tb, d)| tb.calc_duration(d));
        let duration = from_duration
            .map(|t| t.as_secs_f64())
            .or_else(|| track.num_frames.map(|n| n as f64 / source_rate as f64));
        let (track_id, time_base) = (track.id, track.time_base);
        let target_rate = target_rate.unwrap_or(source_rate);
        let mut d = Self {
            path: path.to_owned(),
            format,
            decoder,
            track_id,
            time_base,
            source_rate,
            target_rate,
            duration,
            resampler: None,
            packet: Vec::new(),
            input: Vec::new(),
            output: Vec::new(),
            out_pos: 0,
            skip_source: 0,
            skip_output: 0,
            skip_lead: 0,
            fed: 0,
            produced: 0,
            ended: false,
            flushed: false,
        };
        if source_rate != target_rate {
            let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
                .oversampling_factor(256)
                .interpolation(SincInterpolationType::Quadratic);
            let ratio = target_rate as f64 / source_rate as f64;
            let resampler = Async::<f32>::new_sinc(ratio, 1.1, &params, RESAMPLE_CHUNK, 2, FixedAsync::Input)
                .map_err(|e| d.resample_err(e))?;
            d.skip_output = resampler.output_delay();
            d.resampler = Some(resampler);
        }
        Ok(d)
    }

    /// Length from the file header in seconds, if it states one.
    pub fn duration(&self) -> Option<f64> {
        self.duration
    }

    pub fn target_rate(&self) -> u32 {
        self.target_rate
    }

    /// Appends up to `frames` stereo frames to `out`; fewer only at the end.
    pub fn read(&mut self, out: &mut Vec<f32>, frames: usize) -> Result<usize, MediaError> {
        while (self.output.len() - self.out_pos) / 2 < frames && !self.flushed {
            self.fill()?;
        }
        let take = ((self.output.len() - self.out_pos) / 2).min(frames);
        out.extend_from_slice(&self.output[self.out_pos..self.out_pos + take * 2]);
        self.out_pos += take * 2;
        if self.out_pos >= self.output.len() / 2 {
            self.output.drain(..self.out_pos);
            self.out_pos = 0;
        }
        Ok(take)
    }

    /// Continues at output frame `frame` (at the target rate), with the same
    /// samples decoding from the start would give there.
    pub fn seek(&mut self, frame: usize) -> Result<(), MediaError> {
        let (source_frame, lead) = self.seek_point(frame);
        let secs = source_frame as f64 / self.source_rate as f64;
        let time = Time::try_from_secs_f64(secs).unwrap_or(Time::ZERO);
        let seeked = self
            .format
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|source| MediaError::Decode { path: self.path.clone(), source })?;
        self.decoder.reset();
        let tick_secs = self
            .time_base
            .map_or(1.0 / self.source_rate as f64, |tb| tb.numer.get() as f64 / tb.denom.get() as f64);
        // Counted from the frame asked for: `required_ts` went through
        // seconds and may be a frame off.
        let landed =
            (seeked.actual_ts.get().max(0) as f64 * tick_secs * self.source_rate as f64).round() as usize;
        self.skip_source = source_frame.saturating_sub(landed);
        self.input.clear();
        self.output.clear();
        self.out_pos = 0;
        (self.fed, self.produced, self.ended, self.flushed) = (0, 0, false, false);
        if let Some(r) = &mut self.resampler {
            r.reset();
            self.skip_output = r.output_delay();
        }
        self.skip_lead = lead;
        Ok(())
    }

    /// Where to restart decoding for output frame `frame`: the source frame
    /// and the output frames to drop from there. With resampling, that is a
    /// source frame a little earlier on which the two sample grids meet, so
    /// the resampler continues in phase and with its history filled; starting
    /// right at `frame` would shift what follows by a fraction of a sample
    /// and click where it meets the chunk before.
    fn seek_point(&self, frame: usize) -> (usize, usize) {
        if self.resampler.is_none() {
            return (frame, 0);
        }
        let common = gcd(self.source_rate, self.target_rate);
        let source_period = (self.source_rate / common) as usize;
        let output_period = (self.target_rate / common) as usize;
        let source = (frame as u128 * self.source_rate as u128 / self.target_rate as u128) as usize;
        let periods = source.saturating_sub(RESAMPLE_MARGIN) / source_period;
        (periods * source_period, frame - periods * output_period)
    }

    /// Decodes the next packet (or flushes at the end).
    fn fill(&mut self) -> Result<(), MediaError> {
        if self.ended {
            self.resample(true)?;
            self.flushed = true;
            return Ok(());
        }
        let decode_err = |source| MediaError::Decode { path: self.path.clone(), source };
        let packet = match self.format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => {
                self.ended = true;
                return Ok(());
            }
            // Some formats signal the end of the stream with an EOF error.
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.ended = true;
                return Ok(());
            }
            Err(e) => return Err(decode_err(e)),
        };
        if packet.track_id != self.track_id {
            return Ok(());
        }
        let channels = match self.decoder.decode(&packet) {
            Ok(buf) => {
                self.packet.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved(&mut self.packet);
                buf.spec().channels().count().max(1)
            }
            // A corrupt packet is skipped, like every common player does.
            Err(SymError::DecodeError(_)) => return Ok(()),
            Err(e) => return Err(decode_err(e)),
        };
        let frames = self.packet.len() / channels;
        let skip = self.skip_source.min(frames);
        self.skip_source -= skip;
        self.fed += frames - skip;
        let source = &self.packet[skip * channels..frames * channels];
        if self.resampler.is_some() {
            push_stereo(&mut self.input, source, channels);
            self.resample(false)
        } else {
            push_stereo(&mut self.output, source, channels);
            Ok(())
        }
    }

    /// Runs the resampler over the waiting input; with `last`, also over the
    /// rest and its delay, until the output has the resampled length.
    fn resample(&mut self, last: bool) -> Result<(), MediaError> {
        let Some(r) = self.resampler.as_mut() else { return Ok(()) };
        let ratio = self.target_rate as f64 / self.source_rate as f64;
        let wanted = (self.fed as f64 * ratio).ceil() as usize;
        loop {
            let need = r.input_frames_next();
            let have = self.input.len() / 2;
            if have < need && !(last && self.produced < wanted) {
                return Ok(());
            }
            let partial = (have < need).then_some(have);
            self.input.resize(self.input.len().max(need * 2), 0.0);
            let out_max = r.output_frames_max();
            let start = self.output.len();
            self.output.resize(start + out_max * 2, 0.0);
            let input = InterleavedSlice::new(&self.input[..], 2, self.input.len() / 2)
                .map_err(|e| MediaError::Resample { path: self.path.clone(), message: e.to_string() })?;
            let mut output = InterleavedSlice::new_mut(&mut self.output[start..], 2, out_max)
                .map_err(|e| MediaError::Resample { path: self.path.clone(), message: e.to_string() })?;
            let indexing = Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: partial,
                active_channels_mask: None,
            };
            let (used, written) = r
                .process_into_buffer(&input, &mut output, Some(&indexing))
                .map_err(|e| MediaError::Resample { path: self.path.clone(), message: e.to_string() })?;
            self.output.truncate(start + written * 2);
            let delay = self.skip_output.min(written);
            self.output.drain(start..start + delay * 2);
            self.skip_output -= delay;
            self.produced += written - delay;
            let lead = self.skip_lead.min(written - delay);
            self.output.drain(start..start + lead * 2);
            self.skip_lead -= lead;
            self.input.drain(..used.min(have) * 2);
            if partial.is_some() {
                self.input.clear();
            }
            if last && self.produced >= wanted {
                // The flush overshoots by up to one chunk.
                let excess = self.produced - wanted;
                self.output.truncate(self.output.len().saturating_sub(excess * 2).max(start));
                self.produced = wanted;
                return Ok(());
            }
        }
    }

    fn resample_err(&self, e: impl std::fmt::Display) -> MediaError {
        MediaError::Resample { path: self.path.clone(), message: e.to_string() }
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

/// Opens `path` as a streamed buffer at `target_rate`: a reader thread loads
/// the chunks that playback needs and drops the others. Needs a file whose
/// header states its length.
pub fn open_stream(path: &Path, target_rate: u32) -> Result<AudioBuffer, MediaError> {
    let decoder = StreamDecoder::open(path, Some(target_rate))?;
    let secs = decoder.duration().ok_or_else(|| MediaError::NoAudioTrack { path: path.to_owned() })?;
    let data = Arc::new(StreamData::new((secs * target_rate as f64).ceil() as usize));
    let reader_data = data.clone();
    std::thread::Builder::new()
        .name("jq-stream".into())
        .spawn(move || read_chunks(&reader_data, decoder))
        .map_err(|source| MediaError::Io { path: path.to_owned(), source })?;
    Ok(AudioBuffer::streamed(target_rate, data))
}

/// The reader loop: load the most urgent missing chunk, drop unwanted ones,
/// free retired chunks after the grace period.
fn read_chunks(data: &StreamData, mut decoder: StreamDecoder) {
    let mut retired: VecDeque<(Instant, Retired)> = VecDeque::new();
    // Only this thread stores and evicts, so it knows what is loaded without
    // looking at every chunk of a long file.
    let mut loaded = BTreeSet::new();
    // Chunks that failed to decode, and until when they are skipped.
    let mut failed: HashMap<usize, Instant> = HashMap::new();
    // Chunk the decoder would deliver next without seeking.
    let mut next = Some(0);
    while !data.is_stopped() {
        let seen = data.changes();
        let now = Instant::now();
        while retired.front().is_some_and(|(at, _)| now.duration_since(*at) >= GRACE) {
            retired.pop_front();
        }
        failed.retain(|_, until| *until > now);
        let wanted = wanted_chunks(data);
        let unwanted: Vec<usize> = loaded.iter().copied().filter(|c| !wanted.contains(c)).collect();
        for c in unwanted {
            loaded.remove(&c);
            retired.extend(data.evict(c).map(|r| (now, r)));
        }
        let Some(&chunk) = wanted.iter().find(|c| !loaded.contains(*c) && !failed.contains_key(*c)) else {
            let timeout = if data.has_voices() {
                Some(FOLLOW)
            } else {
                // Only retired chunks to free and failed ones to retry come
                // due by themselves.
                let due =
                    retired.front().map(|(at, _)| *at + GRACE).into_iter().chain(failed.values().copied());
                due.min().map(|at| at.saturating_duration_since(now))
            };
            data.wait_for_change(seen, timeout);
            continue;
        };
        let mut samples = Vec::with_capacity(CHUNK_FRAMES * 2);
        let result = (|| {
            if next != Some(chunk) {
                decoder.seek(chunk * CHUNK_FRAMES)?;
            }
            decoder.read(&mut samples, CHUNK_FRAMES)
        })();
        match result {
            Ok(frames) => {
                if frames < CHUNK_FRAMES {
                    // The real end of the file; the header may have guessed.
                    data.set_frames(chunk * CHUNK_FRAMES + frames);
                }
                next = Some(chunk + 1);
                if frames > 0 {
                    samples.shrink_to_fit();
                    retired.extend(data.store(chunk, samples).map(|r| (now, r)));
                    loaded.insert(chunk);
                }
            }
            Err(_) => {
                next = None;
                failed.insert(chunk, now + ERROR_PAUSE);
            }
        }
    }
}

/// Chunks to keep, most urgent first: around each playing position (ahead
/// of it), the pinned loop regions, then where the next GO will start.
fn wanted_chunks(data: &StreamData) -> Vec<usize> {
    let count = data.chunk_count();
    let last = data.frames().div_ceil(CHUNK_FRAMES);
    let mut wanted = Vec::new();
    let add = |c: usize, wanted: &mut Vec<usize>| {
        if c < count && c < last.max(1) && !wanted.contains(&c) {
            wanted.push(c);
        }
    };
    let positions: Vec<usize> = data.positions().map(|p| p / CHUNK_FRAMES).collect();
    for step in 0..=AHEAD {
        for &c in &positions {
            add(c + step, &mut wanted);
        }
    }
    for pin in data.pins() {
        for c in pin.start / CHUNK_FRAMES..=pin.end.saturating_sub(1) / CHUNK_FRAMES {
            add(c, &mut wanted);
        }
    }
    for hint in data.hints() {
        add(hint / CHUNK_FRAMES, &mut wanted);
        add(hint / CHUNK_FRAMES + 1, &mut wanted);
    }
    // One chunk behind each position, for short jumps back.
    for &c in &positions {
        if c > 0 {
            add(c - 1, &mut wanted);
        }
    }
    wanted
}
