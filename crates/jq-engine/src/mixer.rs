use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jq_i18n::t;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::buffer::AudioBuffer;
use crate::pool::StretchUnit;
use crate::ramp::Curve;
use crate::resample::kernel_table;
use crate::stream::{NO_POSITION, StreamData};
use crate::voice::{Context, LoopExit, LoopPass, LoopRegion, Voice, VoiceId, VoiceParams};

/// Voice ids are unique across all engines (main and preview), so voices of
/// two engines on one streamed buffer never take each other's position slot.
static NEXT_VOICE: AtomicU64 = AtomicU64::new(1);

/// Room after a loop end that stays loaded too, for the crossfade tail and
/// a devamp jump.
const PIN_MARGIN: Duration = Duration::from_millis(500);

/// The frames of a loop a streamed buffer must keep loaded: where playback
/// lands (the start, a chunk beyond it, so the wrap never waits for the next
/// chunk) and where an exit lands (the end plus `margin` for the crossfade
/// tail). The body between is only needed while playback is in it, and the
/// reader follows that, so a long loop does not keep the whole file in memory.
fn loop_pins(l: &LoopRegion, margin: usize) -> impl Iterator<Item = std::ops::Range<usize>> {
    let head_end = l.start.saturating_add(crate::stream::CHUNK_FRAMES + margin);
    let (first, second) = if head_end >= l.end {
        (l.start..l.end + margin, None)
    } else {
        (l.start..head_end, Some(l.end..l.end + margin))
    };
    std::iter::once(first).chain(second)
}

/// How long live trim and pan changes take, so slider moves do not zipper.
pub const PARAM_SMOOTHING: Duration = Duration::from_millis(20);

/// Blend of a pause and a resume, against clicks.
pub const PAUSE_FADE: Duration = Duration::from_millis(10);

/// Blend of a hard stop (stop, STOP ALL, a fade of 0 s), against clicks.
pub const STOP_FADE: Duration = Duration::from_millis(10);

/// Blend at a voice's own end (end mark or end of file), against clicks
/// where the end lies in the middle of the audio. See
/// [`crate::VoiceParams::end_blend_frames`].
pub const END_BLEND: Duration = Duration::from_millis(10);

/// Blend from direct (or varispeed) playback over to the stretcher.
pub const SWITCH_FADE: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    pub sample_rate: u32,
    /// Channels per frame of the device stream.
    pub channels: usize,
    /// Upper bound of simultaneous voices; memory is reserved up front.
    pub max_voices: usize,
    /// How often position events are sent, per second.
    pub report_rate_hz: u32,
    /// Voices that can be stretched at once (pitch apart from the rate);
    /// made up front. See [`crate::stretcher_pool_size`].
    pub stretchers: usize,
    /// Output frames of a stretcher's pre-roll per output frame of a
    /// callback. See [`crate::prime_ratio`].
    pub prime_ratio: u32,
}

impl EngineConfig {
    /// Few stretchers and a modest pre-roll; the app measures the machine
    /// and sets both (see [`EngineConfig::measured`]).
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        Self { sample_rate, channels, max_voices: 64, report_rate_hz: 30, stretchers: 4, prime_ratio: 8 }
    }

    /// With stretchers and pre-roll sized for this machine (takes a few
    /// milliseconds to measure).
    pub fn measured(sample_rate: u32, channels: usize) -> Self {
        let (stretchers, prime_ratio) = crate::pool::measure(sample_rate);
        Self { stretchers, prime_ratio, ..Self::new(sample_rate, channels) }
    }
}

/// Control thread → audio thread.
#[derive(Debug)]
pub enum Command {
    Play {
        id: VoiceId,
        params: VoiceParams,
        /// Position slot claimed in a streamed buffer.
        slot: Option<usize>,
    },
    Ramp {
        id: VoiceId,
        gain: f32,
        frames: u32,
        curve: Curve,
        stop: bool,
    },
    Stop(VoiceId),
    StopAll,
    FadeAll {
        frames: u32,
        curve: Curve,
    },
    ExitLoop {
        id: VoiceId,
        exit: LoopExit,
        /// Release every loop after the one left, too.
        all: bool,
        /// End the voice at the exit instead of playing on.
        stop: bool,
    },
    Pause {
        id: VoiceId,
        frames: u32,
    },
    Resume {
        id: VoiceId,
        frames: u32,
    },
    /// Holds a running fade or pan move where it is (`held`), or lets it
    /// go on; the voice plays on. A new fade (pan move) releases it.
    Hold {
        id: VoiceId,
        ramp: Held,
        held: bool,
    },
    Seek {
        id: VoiceId,
        frame: usize,
        crossfade_frames: u32,
        /// Loops from the target on hold again, also those released before
        /// (a jump back before a vamp).
        rearm: bool,
    },
    /// Live level correction, independent of fades.
    SetTrim {
        id: VoiceId,
        gain: f32,
        frames: u32,
    },
    SetPan {
        id: VoiceId,
        pan: f32,
        frames: u32,
        curve: Curve,
    },
    /// Lowering by another cue (ducking), independent of fades and trim.
    SetDuck {
        id: VoiceId,
        gain: f32,
        frames: u32,
        curve: Curve,
    },
    /// Moves rate (file frames per output frame) and pitch (frequency
    /// factor) together; equal, the voice plays varispeed.
    SetSpeed {
        id: VoiceId,
        rate: f32,
        pitch: f32,
        frames: u32,
        curve: Curve,
    },
}

/// Which of a voice's moves [`Command::Hold`] holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    /// The fade envelope.
    Gain,
    Pan,
    /// Rate and pitch.
    Speed,
}

/// Audio thread → control thread.
#[derive(Debug)]
pub enum Event {
    Started(VoiceId),
    /// The buffer (and loops, marks) travel back so they are freed outside
    /// the audio thread.
    Finished {
        id: VoiceId,
        buffer: Arc<AudioBuffer>,
        loops: Arc<[LoopRegion]>,
        marks: Arc<[usize]>,
    },
    /// The voice limit was reached; the voice never played.
    Rejected {
        id: VoiceId,
        buffer: Arc<AudioBuffer>,
        loops: Arc<[LoopRegion]>,
        marks: Arc<[usize]>,
    },
    Position {
        id: VoiceId,
        frame: usize,
        gain: f32,
        looping: bool,
        /// The loop played through and the pass, inside one.
        pass: Option<LoopPass>,
        /// Balance as it is now (−1 … +1), also while a pan move runs.
        pan: f32,
        /// Rate and pitch as they are now (1 = original).
        rate: f32,
        pitch: f32,
    },
    /// The voice needed a stretcher (pitch apart from its rate) and none
    /// was free: it plays varispeed, its pitch following the rate.
    NoStretcher(VoiceId),
    /// The voice is within [`VoiceParams::notify_before_end`] of its end
    /// (sent once, before `Finished` of a voice that ends by itself).
    NearEnd(VoiceId),
    /// The voice left a loop released by [`Controller::exit_loop`]; `last`:
    /// no other released loop is still ahead.
    LoopExited {
        id: VoiceId,
        last: bool,
    },
}

#[derive(Debug)]
pub enum EngineError {
    QueueFull,
    SampleRateMismatch { buffer: u32, engine: u32 },
    InvalidRegion(RegionProblem),
    OutputChannel { channel: usize, channels: usize },
}

/// What is wrong with the playback region of a voice (see [`EngineError::InvalidRegion`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionProblem {
    StartAfterEnd,
    LoopBackwards,
    LoopPastEnd,
    LoopsUnordered,
    MarksUnsorted,
}

impl std::fmt::Display for RegionProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            Self::StartAfterEnd => t!("engine.region.start_after_end"),
            Self::LoopBackwards => t!("engine.region.loop_backwards"),
            Self::LoopPastEnd => t!("engine.region.loop_past_end"),
            Self::LoopsUnordered => t!("engine.region.loops_unordered"),
            Self::MarksUnsorted => t!("engine.region.marks_unsorted"),
        })
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            Self::QueueFull => t!("engine.queue_full"),
            Self::SampleRateMismatch { buffer, engine } => {
                t!("engine.sample_rate_mismatch", buffer = buffer, engine = engine)
            }
            Self::InvalidRegion(problem) => t!("engine.invalid_region", problem = problem),
            Self::OutputChannel { channel, channels } => {
                t!("engine.output_channel", first = channel + 1, second = channel + 2, channels = channels)
            }
        })
    }
}

impl std::error::Error for EngineError {}

/// Creates the two halves of the engine: the [`Controller`] stays on the
/// control thread, the [`Mixer`] moves into the audio callback.
pub fn engine(config: EngineConfig) -> (Controller, Mixer) {
    let queue = 1024.max(config.max_voices * 4);
    let (cmd_tx, cmd_rx) = RingBuffer::new(queue);
    let (evt_tx, evt_rx) = RingBuffer::new(queue * 4);
    let report_every = (config.sample_rate / config.report_rate_hz.max(1)).max(1) as usize;
    let controller = Controller { commands: cmd_tx, events: evt_rx, config, claims: Vec::new() };
    let mixer = Mixer {
        voices: Vec::with_capacity(config.max_voices),
        commands: cmd_rx,
        events: evt_tx,
        // Every voice can finish, reach its end mark and leave a loop, and
        // every queued Play be answered in one callback; position reports
        // never take that room.
        reserve: config.max_voices * 4 + 16,
        channels: config.channels,
        report_every,
        since_report: 0,
        stop_frames: (STOP_FADE.as_secs_f64() * config.sample_rate as f64).round() as u32,
        switch_frames: (SWITCH_FADE.as_secs_f64() * config.sample_rate as f64).round() as u32,
        pool: (0..config.stretchers).map(|_| StretchUnit::new(config.sample_rate)).collect(),
        table: kernel_table(),
        prime_ratio: config.prime_ratio.max(1),
    };
    (controller, mixer)
}

pub struct Controller {
    commands: Producer<Command>,
    events: Consumer<Event>,
    config: EngineConfig,
    /// Streams this engine's voices hold a position slot in, until they end.
    claims: Vec<(VoiceId, Arc<StreamData>)>,
}

impl Drop for Controller {
    /// Voices whose end is never polled any more give their slots back.
    fn drop(&mut self) {
        for (id, stream) in &self.claims {
            stream.release_slot(id.0);
        }
    }
}

impl Controller {
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn frames(&self, d: Duration) -> u32 {
        (d.as_secs_f64() * self.config.sample_rate as f64).round() as u32
    }

    pub fn play(&mut self, params: VoiceParams) -> Result<VoiceId, EngineError> {
        self.validate(&params)?;
        let id = VoiceId(NEXT_VOICE.fetch_add(1, Ordering::Relaxed));
        let stream = params.buffer.stream().cloned();
        let slot = stream.as_ref().and_then(|s| s.claim_slot(id.0));
        if let (Some(stream), Some(slot)) = (&stream, slot) {
            // Loops jump back, so their regions stay loaded; the reader
            // starts loading at the start position right away.
            let margin = self.frames(PIN_MARGIN) as usize;
            stream.set_pins(slot, params.loops.iter().flat_map(|l| loop_pins(l, margin)).collect());
            stream.report_position(slot, params.start);
        }
        if let Err(e) = self.send(Command::Play { id, params, slot }) {
            if let Some(stream) = stream {
                stream.release_slot(id.0);
            }
            return Err(e);
        }
        if let (Some(stream), Some(_)) = (stream, slot) {
            self.claims.push((id, stream));
        }
        Ok(id)
    }

    /// Ramps a voice to `gain`; with `stop` it ends once the ramp is done.
    pub fn ramp(
        &mut self,
        id: VoiceId,
        gain: f32,
        d: Duration,
        curve: Curve,
        stop: bool,
    ) -> Result<(), EngineError> {
        let frames = self.frames(d);
        self.send(Command::Ramp { id, gain, frames, curve, stop })
    }

    pub fn fade_out(&mut self, id: VoiceId, d: Duration, curve: Curve) -> Result<(), EngineError> {
        self.ramp(id, 0.0, d, curve, true)
    }

    /// Sets a voice's live level correction (linear, multiplies the fade
    /// envelope), smoothed over [`PARAM_SMOOTHING`].
    pub fn set_trim(&mut self, id: VoiceId, gain: f32) -> Result<(), EngineError> {
        let frames = self.frames(PARAM_SMOOTHING);
        self.send(Command::SetTrim { id, gain, frames })
    }

    /// Moves a voice's balance (-1 … +1), smoothed over [`PARAM_SMOOTHING`].
    pub fn set_pan(&mut self, id: VoiceId, pan: f32) -> Result<(), EngineError> {
        let frames = self.frames(PARAM_SMOOTHING);
        self.send(Command::SetPan { id, pan, frames, curve: Curve::Linear })
    }

    /// Moves a voice's rate and pitch over `d` (a rate cue); with `pitch`
    /// equal to `rate` the voice plays varispeed.
    pub fn speed_to(
        &mut self,
        id: VoiceId,
        rate: f32,
        pitch: f32,
        d: Duration,
        curve: Curve,
    ) -> Result<(), EngineError> {
        let frames = self.frames(d);
        self.send(Command::SetSpeed { id, rate, pitch, frames, curve })
    }

    /// Changes rate and pitch smoothed over [`PARAM_SMOOTHING`] (live edits).
    pub fn set_speed(&mut self, id: VoiceId, rate: f32, pitch: f32) -> Result<(), EngineError> {
        self.speed_to(id, rate, pitch, PARAM_SMOOTHING, Curve::Linear)
    }

    /// Holds the voice's move of rate and pitch (a paused rate cue), or lets it go on.
    pub fn hold_speed(&mut self, id: VoiceId, held: bool) -> Result<(), EngineError> {
        self.send(Command::Hold { id, ramp: Held::Speed, held })
    }

    /// Moves a voice's balance to `pan` over `d` (a pan cue).
    pub fn pan_to(&mut self, id: VoiceId, pan: f32, d: Duration, curve: Curve) -> Result<(), EngineError> {
        let frames = self.frames(d);
        self.send(Command::SetPan { id, pan, frames, curve })
    }

    /// Ends a voice after [`STOP_FADE`] (a paused one at once).
    pub fn stop(&mut self, id: VoiceId) -> Result<(), EngineError> {
        self.send(Command::Stop(id))
    }

    pub fn stop_all(&mut self) -> Result<(), EngineError> {
        self.send(Command::StopAll)
    }

    pub fn fade_all(&mut self, d: Duration, curve: Curve) -> Result<(), EngineError> {
        let frames = self.frames(d);
        self.send(Command::FadeAll { frames, curve })
    }

    /// Leaves the voice's next loop; with `all` every loop after it as well.
    /// With `stop` the voice ends at the exit after [`STOP_FADE`].
    pub fn exit_loop(
        &mut self,
        id: VoiceId,
        exit: LoopExit,
        all: bool,
        stop: bool,
    ) -> Result<(), EngineError> {
        self.send(Command::ExitLoop { id, exit, all, stop })
    }

    /// Blends a voice out over [`PAUSE_FADE`] and holds it there; a stop,
    /// fade or change to a paused voice takes effect at once.
    pub fn pause(&mut self, id: VoiceId) -> Result<(), EngineError> {
        let frames = self.frames(PAUSE_FADE);
        self.send(Command::Pause { id, frames })
    }

    /// Plays a paused voice on, blending in over [`PAUSE_FADE`].
    pub fn resume(&mut self, id: VoiceId) -> Result<(), EngineError> {
        let frames = self.frames(PAUSE_FADE);
        self.send(Command::Resume { id, frames })
    }

    /// Holds the voice's running fade (a paused fade cue), or lets it go on.
    pub fn hold_fade(&mut self, id: VoiceId, held: bool) -> Result<(), EngineError> {
        self.send(Command::Hold { id, ramp: Held::Gain, held })
    }

    /// Holds the voice's running pan move (a paused pan cue), or lets it go on.
    pub fn hold_pan(&mut self, id: VoiceId, held: bool) -> Result<(), EngineError> {
        self.send(Command::Hold { id, ramp: Held::Pan, held })
    }

    /// Lowers a voice to `gain` (linear) on top of its fades and trim, over `d`.
    pub fn set_duck(&mut self, id: VoiceId, gain: f32, d: Duration, curve: Curve) -> Result<(), EngineError> {
        let frames = self.frames(d);
        self.send(Command::SetDuck { id, gain, frames, curve })
    }

    /// Moves a voice to `frame` (clamped to its end), crossfading.
    pub fn seek(&mut self, id: VoiceId, frame: usize, crossfade_frames: u32) -> Result<(), EngineError> {
        self.send(Command::Seek { id, frame, crossfade_frames, rearm: false })
    }

    /// Like [`Controller::seek`], but loops from the target on hold again
    /// (a jump back before a vamp left already holds there again).
    pub fn seek_rearming(
        &mut self,
        id: VoiceId,
        frame: usize,
        crossfade_frames: u32,
    ) -> Result<(), EngineError> {
        self.send(Command::Seek { id, frame, crossfade_frames, rearm: true })
    }

    /// Drains all pending events.
    pub fn poll_events(&mut self) -> impl Iterator<Item = Event> + '_ {
        let (events, claims) = (&mut self.events, &mut self.claims);
        std::iter::from_fn(|| events.pop().ok()).inspect(|e| {
            // A voice that is gone no longer holds its stream's loops.
            if let Event::Finished { id, buffer, .. } | Event::Rejected { id, buffer, .. } = e
                && let Some(stream) = buffer.stream()
            {
                stream.release_slot(id.0);
                claims.retain(|(claim, _)| claim != id);
            }
        })
    }

    fn send(&mut self, cmd: Command) -> Result<(), EngineError> {
        self.commands.push(cmd).map_err(|_| EngineError::QueueFull)
    }

    fn validate(&self, p: &VoiceParams) -> Result<(), EngineError> {
        let (buffer, engine) = (p.buffer.sample_rate(), self.config.sample_rate);
        if buffer != engine {
            return Err(EngineError::SampleRateMismatch { buffer, engine });
        }
        if p.output_channel + 1 >= self.config.channels {
            return Err(EngineError::OutputChannel {
                channel: p.output_channel,
                channels: self.config.channels,
            });
        }
        let end = p.end_frame();
        if p.start > end {
            return Err(EngineError::InvalidRegion(RegionProblem::StartAfterEnd));
        }
        for l in p.loops.iter() {
            if l.start >= l.end {
                return Err(EngineError::InvalidRegion(RegionProblem::LoopBackwards));
            }
            if l.end > end {
                return Err(EngineError::InvalidRegion(RegionProblem::LoopPastEnd));
            }
        }
        if p.loops.windows(2).any(|w| w[0].end > w[1].start) {
            return Err(EngineError::InvalidRegion(RegionProblem::LoopsUnordered));
        }
        if !p.marks.is_sorted() {
            return Err(EngineError::InvalidRegion(RegionProblem::MarksUnsorted));
        }
        Ok(())
    }
}

/// Realtime half: sums all voices into the device buffer. Never allocates,
/// locks or blocks.
pub struct Mixer {
    voices: Vec<Voice>,
    commands: Consumer<Command>,
    events: Producer<Event>,
    /// Event slots kept free for `Started`, `Finished`, `Rejected`, `NearEnd`
    /// and `LoopExited`.
    reserve: usize,
    channels: usize,
    report_every: usize,
    since_report: usize,
    /// [`STOP_FADE`] in frames.
    stop_frames: u32,
    /// [`SWITCH_FADE`] in frames.
    switch_frames: u32,
    /// Stretchers not lent to a voice; never grows past its capacity.
    pool: Vec<StretchUnit>,
    /// Kernel of the varispeed interpolation.
    table: Arc<[f32]>,
    prime_ratio: u32,
}

impl Mixer {
    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// Fills `out` (interleaved, `channels` per frame). All commands queued
    /// before this call take effect at the first frame, so voices started
    /// together run sample-synchronously.
    pub fn process(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        while let Ok(cmd) = self.commands.pop() {
            self.apply(cmd);
        }

        let ctx = Context { table: &self.table, prime_ratio: self.prime_ratio };
        for v in &mut self.voices {
            v.render(out, self.channels, &ctx);
            if v.take_no_stretcher() {
                let _ = self.events.push(Event::NoStretcher(v.id));
            }
            if v.take_started() {
                let _ = self.events.push(Event::Started(v.id));
            }
            let (exits, last) = v.take_loop_exits();
            for i in 0..exits {
                let _ = self.events.push(Event::LoopExited { id: v.id, last: last && i + 1 == exits });
            }
            if v.take_near_end() {
                let _ = self.events.push(Event::NearEnd(v.id));
            }
        }
        // NaN (a bad float file) would pass the clamp: silence instead.
        for s in out.iter_mut() {
            *s = if s.is_nan() { 0.0 } else { s.clamp(-1.0, 1.0) };
        }

        let mut i = 0;
        while i < self.voices.len() {
            // With the event queue full (a control thread that has not polled
            // for very long), a finished voice waits silently for the next
            // callback, so its buffer is never freed here and its stream
            // slot comes back.
            if self.voices[i].is_finished() && self.events.slots() > 0 {
                let mut v = self.voices.swap_remove(i);
                if let Some(unit) = v.stretch.take() {
                    self.pool.push(unit);
                }
                if let Some(slot) = v.slot {
                    v.buffer.report_position(slot, NO_POSITION);
                }
                let _ = self.events.push(Event::Finished {
                    id: v.id,
                    buffer: v.buffer,
                    loops: v.loops,
                    marks: v.marks,
                });
            } else {
                i += 1;
            }
        }

        self.since_report += out.len() / self.channels.max(1);
        // A control thread that has not polled for a while must not lose a
        // `Finished` (whose buffer would then be freed here) to positions.
        if self.since_report >= self.report_every && self.events.slots() > self.reserve + self.voices.len() {
            self.since_report = 0;
            for v in &self.voices {
                let _ = self.events.push(Event::Position {
                    id: v.id,
                    frame: v.position(),
                    gain: v.gain(),
                    looping: v.is_looping(),
                    pass: v.loop_pass(),
                    pan: v.pan(),
                    rate: v.speed().0,
                    pitch: v.speed().1,
                });
            }
        }
    }

    fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::Play { id, params, slot } => {
                if self.voices.len() == self.voices.capacity() {
                    let _ = self.events.push(Event::Rejected {
                        id,
                        buffer: params.buffer,
                        loops: params.loops,
                        marks: params.marks,
                    });
                } else {
                    let v =
                        Voice::new(id, params, slot, self.stop_frames, self.switch_frames, &mut self.pool);
                    self.voices.push(v);
                }
            }
            Command::Ramp { id, gain, frames, curve, stop } => {
                if let Some(v) = self.voice(id) {
                    v.ramp(gain, frames, curve, stop);
                }
            }
            Command::Stop(id) => {
                if let Some(v) = self.voice(id) {
                    v.stop();
                }
            }
            Command::StopAll => self.voices.iter_mut().for_each(Voice::stop),
            // Paused voices end at once (see `Voice::ramp`).
            Command::FadeAll { frames, curve } => {
                self.voices.iter_mut().for_each(|v| v.ramp(0.0, frames, curve, true));
            }
            Command::ExitLoop { id, exit, all, stop } => {
                if let Some(v) = self.voice(id) {
                    v.exit_loop(exit, all, stop);
                }
            }
            Command::Pause { id, frames } => {
                if let Some(v) = self.voice(id) {
                    v.pause(frames);
                }
            }
            Command::Resume { id, frames } => {
                if let Some(v) = self.voice(id) {
                    v.resume(frames);
                }
            }
            Command::Hold { id, ramp, held } => {
                if let Some(v) = self.voice(id) {
                    match ramp {
                        Held::Gain => v.hold_envelope(held),
                        Held::Pan => v.hold_pan(held),
                        Held::Speed => v.hold_speed(held),
                    }
                }
            }
            Command::SetDuck { id, gain, frames, curve } => {
                if let Some(v) = self.voice(id) {
                    v.set_duck(gain, frames, curve);
                }
            }
            Command::Seek { id, frame, crossfade_frames, rearm } => {
                if let Some(v) = self.voice(id) {
                    v.seek(frame, crossfade_frames, rearm);
                }
            }
            Command::SetTrim { id, gain, frames } => {
                if let Some(v) = self.voice(id) {
                    v.set_trim(gain, frames);
                }
            }
            Command::SetPan { id, pan, frames, curve } => {
                if let Some(v) = self.voice(id) {
                    v.set_pan(pan, frames, curve);
                }
            }
            Command::SetSpeed { id, rate, pitch, frames, curve } => {
                if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
                    v.set_speed(rate, pitch, frames, curve, &mut self.pool);
                }
            }
        }
    }

    fn voice(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.voices.iter_mut().find(|v| v.id == id)
    }
}
