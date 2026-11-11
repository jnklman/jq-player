use std::sync::Arc;

use jq_stretch::CHANNELS;

use crate::buffer::AudioBuffer;
use crate::pool::{BLOCK, StretchUnit};
use crate::ramp::{Curve, Ramp};
use crate::resample::Resampler;

/// Upper bound for levels and the live trim (+24 dB), so a bad value cannot
/// blast the PA.
const MAX_GAIN: f32 = 16.0;

/// Range of rates and pitch factors the engine plays (the show allows less).
pub(crate) const MIN_RATE: f32 = 0.25;
pub(crate) const MAX_RATE: f32 = 4.0;

/// A linear gain the mixer can use: NaN is silence, the rest is clamped.
fn sane_gain(gain: f32) -> f32 {
    if gain.is_nan() { 0.0 } else { gain.clamp(0.0, MAX_GAIN) }
}

fn sane_pan(pan: f32) -> f32 {
    if pan.is_nan() { 0.0 } else { pan.clamp(-1.0, 1.0) }
}

/// A rate or pitch factor the engine can play: NaN and infinity are 1.
fn sane_rate(rate: f32) -> f32 {
    if rate.is_finite() { rate.clamp(MIN_RATE, MAX_RATE) } else { 1.0 }
}

/// Only linear and S make sense for a balance or a rate (no dB curves).
fn plain_curve(curve: Curve) -> Curve {
    if curve == Curve::SCurve { curve } else { Curve::Linear }
}

/// Handle of one playing instance of a buffer. Allocated by the controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(pub u64);

/// A region that playback jumps back through, sample-exactly. Frames are
/// buffer positions; `end` is exclusive. A voice can have several (vamp 1,
/// vamp 2, …), which are left one after the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopRegion {
    pub start: usize,
    pub end: usize,
    /// How often playback jumps back to `start`; `None` loops until the loop
    /// is exited via [`LoopExit`] (vamp).
    pub repeats: Option<u32>,
}

/// Where playback is within the loops of a voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopPass {
    /// Index of the loop in [`VoiceParams::loops`].
    pub index: usize,
    /// Pass through it, from 1.
    pub pass: u32,
}

/// How a loop is left (devamp). It applies to the first loop that is not
/// released yet; a second devamp releases the next one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopExit {
    /// Finish the current pass, then play on past the loop end.
    AtLoopEnd,
    /// At the next mark inside the loop, jump to the loop end (crossfading);
    /// without a mark ahead, like [`LoopExit::AtLoopEnd`].
    AtNextMark { crossfade_frames: u32 },
    /// Jump from the current position to the loop end right away, crossfading.
    Immediate { crossfade_frames: u32 },
}

/// Everything needed to start a voice.
#[derive(Clone, Debug)]
pub struct VoiceParams {
    pub buffer: Arc<AudioBuffer>,
    /// First frame to play.
    pub start: usize,
    /// Frame where playback ends (exclusive); `None` = end of buffer.
    pub end: Option<usize>,
    /// Sorted, non-overlapping loop regions.
    pub loops: Arc<[LoopRegion]>,
    /// Sorted frames where [`LoopExit::AtNextMark`] may leave a loop (e.g.
    /// bar starts).
    pub marks: Arc<[usize]>,
    /// Linear gain reached after the fade-in.
    pub gain: f32,
    pub fade_in_frames: u32,
    pub fade_curve: Curve,
    /// Crossfade length at loop jumps, to avoid clicks at marks that are not
    /// on a zero crossing. 0 = hard jump.
    pub loop_crossfade_frames: u32,
    /// Fade to silence over this many (output) frames before `end`, on the
    /// last pass of a loop. 0 = off.
    pub end_fade_frames: u32,
    /// Short linear blend to silence over this many (output) frames before
    /// `end`, on the last pass, on top of envelope, trim and duck: an end in
    /// the middle of the audio does not click. 0 = hard end.
    pub end_blend_frames: u32,
    /// Balance, -1 (left) … +1 (right).
    pub pan: f32,
    /// First device channel of the stereo pair this voice is mixed into.
    pub output_channel: usize,
    /// Sends [`crate::Event::NearEnd`] once this many output frames or fewer
    /// are left on the last pass (no loop ahead that still repeats), at the
    /// rate of the moment.
    pub notify_before_end: Option<usize>,
    /// A streamed buffer whose chunk at `start` is not loaded yet: wait up to
    /// this many frames for it, standing still, instead of playing silence;
    /// [`crate::Event::Started`] comes once playback really starts. 0 = no
    /// waiting.
    pub wait_frames: usize,
    /// File frames played per output frame (1 = original speed).
    pub rate: f32,
    /// Frequency factor of what is heard (1 = original pitch). Equal to
    /// `rate`, the voice is read faster or slower (varispeed); otherwise a
    /// stretcher keeps the pitch apart from the rate.
    pub pitch: f32,
    /// Starts paused at `start`, silent from the first frame (no pause
    /// blend), until [`crate::Controller::resume`].
    pub paused: bool,
}

impl VoiceParams {
    pub fn new(buffer: Arc<AudioBuffer>) -> Self {
        Self {
            buffer,
            start: 0,
            end: None,
            loops: Arc::from([]),
            marks: Arc::from([]),
            gain: 1.0,
            fade_in_frames: 0,
            fade_curve: Curve::Linear,
            loop_crossfade_frames: 0,
            end_fade_frames: 0,
            end_blend_frames: 0,
            pan: 0.0,
            output_channel: 0,
            notify_before_end: None,
            wait_frames: 0,
            rate: 1.0,
            pitch: 1.0,
            paused: false,
        }
    }

    pub(crate) fn end_frame(&self) -> usize {
        self.end.unwrap_or(self.buffer.frames()).min(self.buffer.frames())
    }
}

/// Secondary read head that keeps playing the material after a jump point
/// while it fades out, so jumps crossfade instead of clicking.
#[derive(Clone, Copy, Debug)]
struct Tail {
    pos: usize,
    remaining: u32,
    total: u32,
}

/// What the stretcher and the per-frame mixing need from the mixer.
pub(crate) struct Context<'a> {
    /// Kernel of the varispeed interpolation.
    pub(crate) table: &'a [f32],
    /// Pre-roll a voice may compute per output frame of a callback.
    pub(crate) prime_ratio: u32,
}

/// The read head: walks through the file in file time, sample-exactly
/// through loops (vamps), marks, jumps and their crossfades. It knows
/// nothing about levels or the rate.
#[derive(Clone, Copy)]
struct Head {
    pos: usize,
    end: usize,
    /// Index of the first loop not yet passed.
    current: usize,
    /// Loops with an index below this are released (devamped).
    released: usize,
    /// Repeats left in the current loop; `None` = until released.
    loops_left: Option<u32>,
    /// Pending devamp at a mark: jump to the current loop's end here.
    jump_at: Option<(usize, u32)>,
    crossfade: u32,
    tail: Option<Tail>,
    /// Loops with an index below this were released by [`Head::exit_loop`]
    /// (not by running out of repeats); leaving one is reported.
    exit_released: usize,
    /// End at the exit of a released loop instead of playing on.
    stop_on_exit: bool,
    /// Released loops left so far, and whether the latest was the last
    /// released one (or the voice stops there).
    exit_total: u64,
    exit_last: bool,
    /// A loop was left with `stop_on_exit`; the voice stops when it hears it.
    stop_request: bool,
    /// Pass through the loop at `current`, from 1.
    pass: u32,
}

impl Head {
    fn new(p: &VoiceParams, end: usize) -> Self {
        // Loops that end before the start position are passed already.
        let current = p.loops.iter().position(|l| l.end > p.start).unwrap_or(p.loops.len());
        let mut head = Self {
            pos: p.start,
            end,
            current,
            released: current,
            loops_left: None,
            jump_at: None,
            crossfade: p.loop_crossfade_frames,
            tail: None,
            exit_released: 0,
            stop_on_exit: false,
            exit_total: 0,
            exit_last: false,
            stop_request: false,
            pass: 1,
        };
        head.enter_loop(&p.loops);
        head
    }

    /// Sets up the counters for the loop at `current`.
    fn enter_loop(&mut self, loops: &[LoopRegion]) {
        let Some(l) = loops.get(self.current) else { return };
        self.pass = 1;
        self.loops_left = l.repeats;
        if l.repeats == Some(0) {
            self.released = self.released.max(self.current + 1);
        }
    }

    fn current_loop(&self, loops: &[LoopRegion]) -> Option<LoopRegion> {
        loops.get(self.current).copied()
    }

    fn loop_pass(&self, loops: &[LoopRegion]) -> Option<LoopPass> {
        // At `end` the jump back is still to come.
        self.current_loop(loops)
            .filter(|l| (l.start..=l.end).contains(&self.pos))
            .map(|_| LoopPass { index: self.current, pass: self.pass })
    }

    fn is_looping(&self, loops: &[LoopRegion]) -> bool {
        // At `end` the jump back is still to come.
        self.current_loop(loops)
            .is_some_and(|l| self.released <= self.current && (l.start..=l.end).contains(&self.pos))
    }

    /// True while a loop that is not released lies ahead or around the position.
    fn loop_ahead(&self, loops: &[LoopRegion]) -> bool {
        self.released < loops.len()
    }

    fn exit_loop(&mut self, loops: &[LoopRegion], marks: &[usize], exit: LoopExit, all: bool, stop: bool) {
        let target = self.released;
        let Some(l) = loops.get(target).copied() else { return };
        self.released = if all { loops.len() } else { target + 1 };
        self.exit_released = self.released;
        self.stop_on_exit = stop;
        // Only a loop that playback is inside of can be left early.
        if target != self.current || !(l.start..l.end).contains(&self.pos) {
            return;
        }
        match exit {
            LoopExit::AtLoopEnd => {}
            LoopExit::Immediate { crossfade_frames } => self.jump(l.end, crossfade_frames),
            LoopExit::AtNextMark { crossfade_frames } => {
                let next = marks.partition_point(|&m| m <= self.pos);
                if let Some(&mark) = marks.get(next).filter(|&&m| m < l.end) {
                    self.jump_at = Some((mark, crossfade_frames));
                }
            }
        }
    }

    /// Moves to `to`. Loops before it count as passed; with `rearm`, those
    /// from it on hold again even if released before.
    fn seek(&mut self, loops: &[LoopRegion], to: usize, crossfade: u32, rearm: bool) {
        self.jump(to, crossfade);
        self.jump_at = None;
        let current = loops.iter().position(|l| l.end > to).unwrap_or(loops.len());
        if rearm && self.released > current {
            self.current = current;
            self.released = current;
            self.exit_released = self.exit_released.min(current);
            self.enter_loop(loops);
        } else if current != self.current {
            self.current = current;
            self.released = self.released.max(current);
            self.enter_loop(loops);
        }
    }

    fn jump(&mut self, to: usize, crossfade: u32) {
        self.tail = (crossfade > 0).then_some(Tail { pos: self.pos, remaining: crossfade, total: crossfade });
        self.pos = to;
    }

    /// Takes jumps at a mark and at a loop end; false once the end is reached.
    #[inline]
    fn prepare(&mut self, loops: &[LoopRegion], frames: usize) -> bool {
        if let Some((at, crossfade)) = self.jump_at
            && self.pos == at
        {
            self.jump_at = None;
            if let Some(l) = self.current_loop(loops) {
                self.jump(l.end, crossfade);
            }
        }
        if let Some(l) = self.current_loop(loops)
            && self.pos == l.end
        {
            if self.released > self.current {
                if self.current < self.exit_released {
                    self.exit_total += 1;
                    self.exit_last = self.stop_on_exit || self.current + 1 >= self.exit_released;
                    self.stop_request |= self.stop_on_exit;
                }
                // Passed: the next loop (if any) takes over.
                self.current += 1;
                self.enter_loop(loops);
            } else {
                self.jump(l.start, self.crossfade);
                self.pass = self.pass.saturating_add(1);
                if let Some(n) = self.loops_left.as_mut() {
                    *n -= 1;
                    if *n == 0 {
                        self.released = self.current + 1;
                    }
                }
            }
        }
        // A stream may turn out shorter than its header said.
        self.pos < self.end && self.pos < frames
    }

    /// The frame at the position (blended with a jump's tail), then one ahead.
    #[inline]
    fn read(&mut self, buffer: &AudioBuffer) -> (f32, f32) {
        let frames = buffer.frames();
        let (mut l, mut r) = buffer.frame(self.pos);
        if let Some(t) = self.tail.as_mut() {
            let (tl, tr) = if t.pos < frames { buffer.frame(t.pos) } else { (0.0, 0.0) };
            let w = t.remaining as f32 / t.total as f32;
            l = l * (1.0 - w) + tl * w;
            r = r * (1.0 - w) + tr * w;
            t.pos += 1;
            t.remaining -= 1;
            if t.remaining == 0 {
                self.tail = None;
            }
        }
        self.pos += 1;
        (l, r)
    }
}

/// The head, optionally read through the varispeed interpolation.
#[derive(Clone, Copy)]
struct Feed {
    head: Head,
    resampler: Resampler,
    resampled: bool,
}

impl Feed {
    /// The next frame of the head (resampled at `rate`); `None` once all of
    /// it was delivered.
    #[inline]
    fn pull(
        &mut self,
        buffer: &AudioBuffer,
        loops: &[LoopRegion],
        table: &[f32],
        rate: f64,
    ) -> Option<(f32, f32)> {
        if !self.resampled {
            return self.head.prepare(loops, buffer.frames()).then(|| self.head.read(buffer));
        }
        for _ in 0..self.resampler.wanted() {
            if self.head.prepare(loops, buffer.frames()) {
                let pos = self.head.pos;
                let frame = self.head.read(buffer);
                self.resampler.push(frame, pos);
            } else {
                self.resampler.push_end(self.head.pos);
            }
        }
        (!self.resampler.drained()).then(|| self.resampler.next(table, rate))
    }

    /// File position of what this feed delivers next.
    fn position(&self) -> usize {
        if self.resampled { self.resampler.position() } else { self.head.pos }
    }
}

/// How the voice is heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Path {
    /// Straight from the head, at rate 1 (bit-exact as before rates existed).
    Direct,
    /// Through the varispeed interpolation.
    Resampled,
    /// Through a stretcher (rate and pitch apart).
    Stretched,
}

/// A voice on its way to the stretcher: the stretcher is fed from a copy
/// of the feed and pre-rolled until its output lines up with what is heard,
/// then the voice blends over to it.
#[derive(Clone, Copy)]
struct Switch {
    feed: Feed,
    /// Output frames the stretcher still has to compute (and throw away)
    /// before its next one is the one to hear now.
    behind: i64,
    /// Blend frames left once lined up (`None` before).
    fade_left: Option<u32>,
    /// Nothing was heard yet (the voice starts on the stretcher).
    from_silence: bool,
}

/// A move of rate and pitch.
#[derive(Clone, Copy, Debug)]
struct SpeedMove {
    rate: f32,
    pitch: f32,
    frames: u32,
    curve: Curve,
}

pub(crate) struct Voice {
    pub(crate) id: VoiceId,
    /// Position slot in a streamed buffer; `None` in memory or if all were taken.
    pub(crate) slot: Option<usize>,
    pub(crate) buffer: Arc<AudioBuffer>,
    // Travel back with the buffer, so nothing is freed in the audio thread.
    pub(crate) loops: Arc<[LoopRegion]>,
    pub(crate) marks: Arc<[usize]>,
    /// The stretcher lent from the pool; goes back when the voice ends.
    pub(crate) stretch: Option<StretchUnit>,
    feed: Feed,
    path: Path,
    switch: Option<Switch>,
    /// Rate and pitch heard (or wanted): file frames per output frame and
    /// the frequency factor.
    rate: Ramp,
    pitch: Ramp,
    /// A paused rate cue holds both where they are.
    speed_held: bool,
    /// The resampler's rate while stretched (fixed from the switch on); the
    /// stretcher does the rest.
    base_rate: f64,
    /// A move waiting for the switch to the stretcher.
    pending: Option<SpeedMove>,
    /// The pool was empty when a stretcher was needed (reported once).
    no_stretcher: bool,
    /// Output frames this voice has produced (pauses left out).
    clock: i64,
    /// Output frame at which the stretcher's output number 0 is (was) heard.
    origin: i64,
    /// Stretcher output frames computed since the switch began.
    produced: i64,
    /// Output frame (on `clock`) at which the stretcher has played out the
    /// end of the file.
    end_heard_at: Option<i64>,
    /// File position heard now while stretched.
    heard_pos: Option<usize>,
    /// Loop exits passed on to the mixer.
    delivered: u64,
    ready: u32,
    ready_last: bool,
    /// Fade envelope: fade-in, fades, end fade.
    gain: Ramp,
    /// Live level correction on top of the envelope, so edits never disturb a fade.
    trim: Ramp,
    /// Lowering while another cue ducks this one, independent of fades and trim.
    duck: Ramp,
    /// Pause blend: fades to 0 on pause, back to 1 on resume.
    hold: Ramp,
    /// Pause requested; once `hold` is down the voice stands still.
    paused: bool,
    /// Stop requested; the voice ends once `hold` is down.
    stopping: bool,
    /// Length of the blend of a stop, against clicks.
    stop_frames: u32,
    /// Length of the blend over to the stretcher.
    switch_frames: u32,
    /// The envelope stands still (a paused fade cue) until a new fade or
    /// the end fade takes over.
    envelope_held: bool,
    /// The pan move stands still (a paused pan cue) until a new one.
    pan_held: bool,
    stop_after_ramp: bool,
    curve: Curve,
    end_fade: u32,
    /// Envelope level the end fade started from; `None` until it starts.
    end_fade_from: Option<f32>,
    end_blend: u32,
    pan: Ramp,
    out_ch: usize,
    finished: bool,
    notify_before_end: Option<usize>,
    near_end_sent: bool,
    /// Waiting for the start chunk of a stream, at most `wait_left` frames more.
    waiting: bool,
    wait_left: usize,
    /// [`crate::Event::Started`] is sent.
    started_sent: bool,
}

/// Exits of the stretched feed waiting to be heard: when, how many in all
/// by then, the last flag and a stop at the exit.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Pending {
    pub(crate) at: i64,
    pub(crate) total: u64,
    pub(crate) last: bool,
    pub(crate) stop: bool,
}

impl Voice {
    /// Builds a voice from params already validated by the controller; a
    /// voice whose pitch does not follow its rate takes a stretcher from
    /// `pool` (without one it plays varispeed and reports that).
    pub(crate) fn new(
        id: VoiceId,
        p: VoiceParams,
        slot: Option<usize>,
        stop_frames: u32,
        switch_frames: u32,
        pool: &mut Vec<StretchUnit>,
    ) -> Self {
        let end = p.end_frame();
        let level = sane_gain(p.gain);
        let mut gain = Ramp::constant(if p.fade_in_frames > 0 { 0.0 } else { level });
        gain.start(level, p.fade_in_frames, p.fade_curve);
        let (rate, pitch) = (sane_rate(p.rate), sane_rate(p.pitch));
        let mut voice = Self {
            id,
            slot,
            stretch: None,
            feed: Feed { head: Head::new(&p, end), resampler: Resampler::new(), resampled: false },
            path: Path::Direct,
            switch: None,
            rate: Ramp::constant(rate),
            pitch: Ramp::constant(pitch),
            speed_held: false,
            base_rate: 1.0,
            pending: None,
            no_stretcher: false,
            clock: 0,
            origin: 0,
            produced: 0,
            end_heard_at: None,
            heard_pos: None,
            delivered: 0,
            ready: 0,
            ready_last: false,
            gain,
            trim: Ramp::constant(1.0),
            duck: Ramp::constant(1.0),
            hold: Ramp::constant(if p.paused { 0.0 } else { 1.0 }),
            paused: p.paused,
            stopping: false,
            stop_frames,
            switch_frames,
            envelope_held: false,
            pan_held: false,
            stop_after_ramp: false,
            curve: p.fade_curve,
            end_fade: p.end_fade_frames,
            end_fade_from: None,
            end_blend: p.end_blend_frames,
            pan: Ramp::constant(sane_pan(p.pan)),
            out_ch: p.output_channel,
            finished: p.start >= end,
            notify_before_end: p.notify_before_end,
            near_end_sent: false,
            waiting: p.wait_frames > 0 && !p.buffer.is_ready_at(p.start),
            wait_left: p.wait_frames,
            started_sent: false,
            buffer: p.buffer,
            loops: p.loops,
            marks: p.marks,
        };
        if pitch != rate {
            match pool.pop() {
                Some(unit) => voice.begin_switch(unit, true),
                None => {
                    voice.no_stretcher = true;
                    voice.pitch = Ramp::constant(rate);
                    voice.resample();
                }
            }
        } else if rate != 1.0 {
            voice.resample();
        }
        voice
    }

    /// From direct playback to the varispeed interpolation, seamlessly: the
    /// frames heard so far are in the resampler already.
    fn resample(&mut self) {
        if !self.feed.resampled {
            self.feed.resampler.engage();
            self.feed.resampled = true;
        }
        if self.path == Path::Direct {
            self.path = Path::Resampled;
        }
    }

    /// Starts the way over to the stretcher from where the voice is now.
    fn begin_switch(&mut self, mut unit: StretchUnit, from_silence: bool) {
        // The resampler (if any) keeps its rate; the stretcher does the rest.
        self.base_rate = if self.feed.resampled { self.rate.value() as f64 } else { 1.0 };
        let freeze = |r: &mut Ramp| *r = Ramp::constant(r.value());
        if !from_silence {
            freeze(&mut self.rate);
            freeze(&mut self.pitch);
        }
        let stretch_rate = self.rate.value() as f64 / self.base_rate;
        unit.stretcher.reset();
        unit.stretcher.set_pitch((self.pitch.value() as f64 / self.base_rate) as f32);
        unit.carry = 0.0;
        unit.used = BLOCK;
        unit.clear(self.feed.head.exit_total);
        if !from_silence {
            // What was heard before, straight from the file, as pre-roll.
            let at = self.feed.position();
            let n = unit.history_frames();
            for i in 0..n {
                let frame = (at + i).checked_sub(n).filter(|&f| f < self.buffer.frames());
                let (l, r) = frame.map_or((0.0, 0.0), |f| self.buffer.frame(f));
                unit.history[i * CHANNELS] = l;
                unit.history[i * CHANNELS + 1] = r;
            }
            unit.stretcher.seek(&unit.history, stretch_rate);
        }
        let behind = unit.latency(stretch_rate) as i64;
        self.origin = self.clock - behind;
        self.produced = 0;
        self.end_heard_at = None;
        self.heard_pos = None;
        self.switch = Some(Switch { feed: self.feed, behind, fade_left: None, from_silence });
        self.stretch = Some(unit);
    }

    /// A seek or loop exit during the switch: start it again from there.
    fn restart_switch(&mut self) {
        if let Some(sw) = self.switch.take()
            && let Some(unit) = self.stretch.take()
        {
            self.begin_switch(unit, sw.from_silence);
        }
    }

    /// File position heard now.
    pub(crate) fn position(&self) -> usize {
        match (self.path, self.heard_pos) {
            (Path::Stretched, Some(pos)) => pos,
            _ => self.feed.position(),
        }
    }

    /// The feed playback is heard from (the stretcher's while stretched).
    fn heard_feed(&self) -> &Feed {
        &self.feed
    }

    /// Level of the voice: envelope times trim and duck. The pause blend is
    /// left out, so a paused voice still reports the level it plays on at.
    pub(crate) fn gain(&self) -> f32 {
        self.gain.value() * self.trim.value() * self.duck.value()
    }

    /// Balance as it is now (−1 … +1).
    pub(crate) fn pan(&self) -> f32 {
        self.pan.value()
    }

    /// Rate and pitch as they are now.
    pub(crate) fn speed(&self) -> (f32, f32) {
        (self.rate.value(), self.pitch.value())
    }

    /// True once if the voice needed a stretcher and the pool had none.
    pub(crate) fn take_no_stretcher(&mut self) -> bool {
        std::mem::take(&mut self.no_stretcher)
    }

    /// Frames from a paused voice's settings: changes to a voice that is
    /// paused apply at once, so nothing is left pending for the resume.
    fn now_if_paused(&self, frames: u32) -> u32 {
        if self.paused { 0 } else { frames }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }

    /// Pre-rolling a voice that has not been heard yet.
    fn starting(&self) -> bool {
        self.switch.is_some_and(|s| s.from_silence)
    }

    /// True once when playback starts (after waiting for a stream's first
    /// chunk and the stretcher's pre-roll), or when the voice ends without
    /// ever playing.
    pub(crate) fn take_started(&mut self) -> bool {
        let due = !self.started_sent && ((!self.waiting && !self.starting()) || self.finished);
        self.started_sent |= due;
        due
    }

    /// Waits for the chunk at the position while `wait_left` lasts; true
    /// while it still does. Paused, the wait stands still too.
    fn wait_for_data(&mut self, frames: usize) -> bool {
        if !self.waiting {
            return false;
        }
        // Nothing was heard yet: a stop or fade to a stop ends it at once.
        if self.stopping || self.stop_after_ramp {
            self.finished = true;
            return true;
        }
        if self.buffer.is_ready_at(self.feed.head.pos) || self.wait_left == 0 {
            self.waiting = false;
            return false;
        }
        if !self.paused {
            self.wait_left = self.wait_left.saturating_sub(frames);
        }
        true
    }

    /// The loop playback is inside and the pass through it, also on the
    /// last pass (released or out of repeats); `None` between loops.
    pub(crate) fn loop_pass(&self) -> Option<LoopPass> {
        self.heard_feed().head.loop_pass(&self.loops)
    }

    /// True while playback is inside a loop that will still jump back.
    pub(crate) fn is_looping(&self) -> bool {
        self.heard_feed().head.is_looping(&self.loops)
    }

    /// A fade on a paused voice would never be heard: with `stop` it ends
    /// at once, otherwise the target is set at once.
    pub(crate) fn ramp(&mut self, gain: f32, frames: u32, curve: Curve, stop: bool) {
        let frames = self.now_if_paused(frames);
        // A fade of 0 s to a stop is a stop, with its short blend.
        if stop && frames == 0 {
            self.stop();
            return;
        }
        self.gain.start(sane_gain(gain), frames, curve);
        self.envelope_held = false;
        self.stop_after_ramp = stop;
    }

    pub(crate) fn set_trim(&mut self, gain: f32, frames: u32) {
        self.trim.start(sane_gain(gain), self.now_if_paused(frames), Curve::Linear);
    }

    pub(crate) fn set_pan(&mut self, pan: f32, frames: u32, curve: Curve) {
        self.pan.start(sane_pan(pan), self.now_if_paused(frames), plain_curve(curve));
        self.pan_held = false;
    }

    pub(crate) fn set_duck(&mut self, gain: f32, frames: u32, curve: Curve) {
        self.duck.start(sane_gain(gain), self.now_if_paused(frames), curve);
    }

    /// Moves rate and pitch over `frames`. A pitch apart from the rate needs
    /// a stretcher from `pool`; the move then starts once the voice is heard
    /// through it. Without one left, rate and pitch move together.
    pub(crate) fn set_speed(
        &mut self,
        rate: f32,
        pitch: f32,
        frames: u32,
        curve: Curve,
        pool: &mut Vec<StretchUnit>,
    ) {
        let mut m = SpeedMove {
            rate: sane_rate(rate),
            pitch: sane_rate(pitch),
            frames: self.now_if_paused(frames),
            curve: plain_curve(curve),
        };
        if self.switch.is_some() {
            self.pending = Some(m);
            return;
        }
        if self.path != Path::Stretched && m.pitch != m.rate {
            if let Some(unit) = pool.pop() {
                self.begin_switch(unit, false);
                self.pending = Some(m);
                return;
            }
            self.no_stretcher = true;
            m.pitch = m.rate;
        }
        if self.path == Path::Direct && (m.rate != 1.0 || self.rate.value() != 1.0) {
            self.resample();
        }
        self.start_speed(m);
    }

    fn start_speed(&mut self, m: SpeedMove) {
        self.rate.start(m.rate, m.frames, m.curve);
        self.pitch.start(m.pitch, m.frames, m.curve);
        self.speed_held = false;
    }

    /// Holds the move of rate and pitch where it is, or lets it go on.
    pub(crate) fn hold_speed(&mut self, held: bool) {
        self.speed_held = held;
    }

    /// Holds the envelope (a running fade) where it is, or lets it go on.
    pub(crate) fn hold_envelope(&mut self, held: bool) {
        self.envelope_held = held;
    }

    /// Holds the pan move where it is, or lets it go on.
    pub(crate) fn hold_pan(&mut self, held: bool) {
        self.pan_held = held;
    }

    /// Blends out over `frames`, then stands still: position, loops, fades,
    /// trim and pan keep their state until [`Voice::resume`].
    pub(crate) fn pause(&mut self, frames: u32) {
        if self.stopping {
            return;
        }
        self.paused = true;
        self.hold.start(0.0, frames, Curve::Linear);
    }

    /// Plays on where the pause stopped, blending in from the current pause
    /// level (a resume during the pause blend turns it around).
    pub(crate) fn resume(&mut self, frames: u32) {
        if self.stopping {
            return;
        }
        self.paused = false;
        self.hold.start(1.0, frames, Curve::Linear);
    }

    /// Ends the voice after a short blend (`stop_frames`), so a hard stop
    /// does not click; a paused voice is silent already and ends at once.
    pub(crate) fn stop(&mut self) {
        if self.paused || self.stop_frames == 0 || self.starting() {
            self.finished = true;
            self.stopping = true;
        } else if !self.stopping {
            self.stopping = true;
            self.hold.start(0.0, self.stop_frames, Curve::Linear);
        }
    }

    /// Output frames left until the end is heard, at the rate of now; `None`
    /// while a loop that still repeats lies ahead.
    fn remaining(&self) -> Option<usize> {
        if self.heard_feed().head.loop_ahead(&self.loops) {
            return None;
        }
        let file = self.feed.head.end.saturating_sub(self.position());
        Some(match self.path {
            Path::Direct => file,
            _ => (file as f64 / self.rate.value() as f64).round() as usize,
        })
    }

    /// True once when the voice is within `notify_before_end` of its end on
    /// the last pass, or ends by itself before that; never while paused or
    /// after a stop. Checked once per block.
    pub(crate) fn take_near_end(&mut self) -> bool {
        let Some(lead) = self.notify_before_end else { return false };
        if self.near_end_sent || self.stopping || self.stop_after_ramp || self.paused || self.starting() {
            return false;
        }
        let due = self.finished || self.remaining().is_some_and(|left| left <= lead);
        self.near_end_sent = due;
        due
    }

    /// Released loops left (as heard) since the last call, and whether the
    /// last of them was the last one released (or the voice stops there).
    pub(crate) fn take_loop_exits(&mut self) -> (u32, bool) {
        let taken = (self.ready, self.ready_last);
        self.ready = 0;
        taken
    }

    /// Passes on exits the heard head made (direct and resampled playback
    /// hear them at once).
    fn collect_exits(&mut self, head: Head) {
        if head.exit_total > self.delivered {
            self.ready += (head.exit_total - self.delivered) as u32;
            self.ready_last = head.exit_last;
            self.delivered = head.exit_total;
        }
    }

    /// Passes on the stretched feed's exits whose time has come.
    fn deliver_pending(&mut self) {
        let Some(unit) = self.stretch.as_mut() else { return };
        let mut stop = false;
        while let Some(p) = unit.pending.first().copied().filter(|p| p.at <= self.clock) {
            unit.pending.remove(0);
            if p.total > self.delivered {
                self.ready += (p.total - self.delivered) as u32;
                self.ready_last = p.last;
                self.delivered = p.total;
            }
            stop |= p.stop;
        }
        if stop {
            self.stop();
        }
    }

    /// With `all`, every loop after the one left is released as well; with
    /// `stop` the voice ends at the exit (after the stop blend).
    pub(crate) fn exit_loop(&mut self, exit: LoopExit, all: bool, stop: bool) {
        self.feed.head.exit_loop(&self.loops, &self.marks, exit, all, stop);
        self.restart_switch();
    }

    /// Moves playback to `to` (rehearsal jumps, position cues). Loops
    /// before it count as passed; with `rearm`, those from it on hold again
    /// even if released before. Going back before a started end fade undoes
    /// the fade.
    pub(crate) fn seek(&mut self, to: usize, crossfade: u32, rearm: bool) {
        let to = to.min(self.feed.head.end);
        // A paused voice has nothing to crossfade from.
        let crossfade = self.now_if_paused(crossfade);
        self.feed.head.seek(&self.loops, to, crossfade, rearm);
        self.restart_switch();
        let fade_span = match self.path {
            Path::Direct => self.end_fade as usize,
            _ => (self.end_fade as f64 * self.rate.value() as f64) as usize,
        };
        if let Some(level) = self.end_fade_from
            && to + fade_span < self.feed.head.end
            && !self.stop_after_ramp
        {
            self.end_fade_from = None;
            self.gain.start(level, crossfade, Curve::Linear);
        }
    }

    /// Starts the end fade when the end is that close (as heard).
    fn check_end_fade(&mut self) {
        if self.end_fade == 0 || self.end_fade_from.is_some() || self.stop_after_ramp {
            return;
        }
        if let Some(left) = self.remaining()
            && left <= self.end_fade as usize
        {
            self.end_fade_from = Some(self.gain.target());
            self.gain.start(0.0, left as u32, self.curve);
            self.envelope_held = false;
        }
    }

    /// Level of the end blend with `left` output frames to the end (`None`
    /// while a loop that still repeats lies ahead). It follows the position
    /// alone, so a seek back or a loop jump needs nothing undone.
    #[inline]
    fn end_blend(&self, left: Option<usize>) -> f32 {
        match left {
            Some(left) if left < self.end_blend as usize => left as f32 / self.end_blend as f32,
            _ => 1.0,
        }
    }

    /// Computes the stretcher's next block from `feed` (the switch's while
    /// it runs), noting the heard position and the exits it passes.
    fn produce(&mut self, table: &[f32], switching: bool) {
        let rate = self.rate.value() as f64;
        let (base, origin, produced) = (self.base_rate, self.origin, self.produced);
        let pitch = self.pitch.value() as f64 / base;
        let (Some(unit), feed) = (
            self.stretch.as_mut(),
            match (switching, self.switch.as_mut()) {
                (true, Some(sw)) => &mut sw.feed,
                _ => &mut self.feed,
            },
        ) else {
            return;
        };
        let stretch_rate = rate / base;
        unit.carry += BLOCK as f64 * stretch_rate;
        let take = (unit.carry as usize).min(unit.input_frames());
        unit.carry -= take as f64;
        let from = feed.position();
        let mut ended = false;
        for i in 0..take {
            let (l, r) = match feed.pull(&self.buffer, &self.loops, table, base) {
                Some(frame) => frame,
                None => {
                    ended = true;
                    (0.0, 0.0)
                }
            };
            unit.input[i * CHANNELS] = l;
            unit.input[i * CHANNELS + 1] = r;
        }
        unit.stretcher.set_pitch(pitch as f32);
        let (input, output) = (&unit.input[..take * CHANNELS], &mut unit.output);
        unit.stretcher.process(input, output);
        unit.used = 0;
        let heard_at = origin + produced + unit.latency(stretch_rate) as i64;
        unit.note(heard_at, from);
        let mut overflow = None;
        if feed.head.exit_total > unit.queued || feed.head.stop_request {
            overflow = unit.queue(Pending {
                at: heard_at,
                total: feed.head.exit_total,
                last: feed.head.exit_last,
                stop: feed.head.stop_request,
            });
            unit.queued = feed.head.exit_total;
            feed.head.stop_request = false;
        }
        if let Some(p) = overflow {
            // Better early than never.
            self.ready += p.total.saturating_sub(self.delivered) as u32;
            self.ready_last = p.last;
            self.delivered = self.delivered.max(p.total);
            if p.stop {
                self.stop();
            }
        }
        if ended && self.end_heard_at.is_none() {
            self.end_heard_at = Some(heard_at);
        }
        self.produced += BLOCK as i64;
    }

    /// Pre-rolls the stretcher by up to `budget` output frames.
    fn prime(&mut self, table: &[f32], mut budget: usize) {
        while let Some(sw) = self.switch
            && sw.behind > 0
            && budget > 0
        {
            self.produce(table, true);
            let discard = sw.behind.min(BLOCK as i64);
            if let Some(unit) = self.stretch.as_mut() {
                unit.used = discard as usize;
            }
            if let Some(sw) = self.switch.as_mut() {
                sw.behind -= discard;
            }
            budget = budget.saturating_sub(BLOCK);
        }
        if let Some(sw) = self.switch.as_mut()
            && sw.behind <= 0
            && sw.fade_left.is_none()
        {
            sw.fade_left = Some(if sw.from_silence { 0 } else { self.switch_frames });
        }
        self.finish_switch();
    }

    /// Hands over to the stretcher once the blend is done.
    fn finish_switch(&mut self) {
        if let Some(sw) = self.switch
            && sw.fade_left == Some(0)
        {
            self.feed = sw.feed;
            self.switch = None;
            self.path = Path::Stretched;
            if let Some(m) = self.pending.take() {
                self.start_speed(m);
            }
        }
    }

    /// The stretcher's next output frame.
    #[inline]
    fn stretched_frame(&mut self, table: &[f32], switching: bool) -> (f32, f32) {
        if self.stretch.as_ref().is_some_and(|u| u.used >= BLOCK) {
            self.produce(table, switching);
        }
        let Some(unit) = self.stretch.as_mut() else { return (0.0, 0.0) };
        let i = unit.used * CHANNELS;
        unit.used += 1;
        (unit.output[i], unit.output[i + 1])
    }

    /// Adds this voice into `out` (interleaved, `channels` per frame).
    pub(crate) fn render(&mut self, out: &mut [f32], channels: usize, ctx: &Context) {
        let frames = out.len() / channels.max(1);
        if self.wait_for_data(frames) {
            return;
        }
        if self.starting() {
            if self.stopping || self.stop_after_ramp {
                self.finished = true;
                return;
            }
            self.prime(ctx.table, frames * ctx.prime_ratio as usize);
            if self.starting() {
                return;
            }
        }
        match self.path {
            Path::Direct if self.switch.is_none() => self.render_direct(out, channels),
            _ => self.render_through(out, channels, ctx),
        }
        if let Some(slot) = self.slot {
            self.buffer.report_position(slot, self.position());
        }
    }

    /// Direct playback: per frame exactly what the engine did before rates
    /// existed (the bit-identical path), keeping the resampler's frames up
    /// to date for a later switch.
    fn render_direct(&mut self, out: &mut [f32], channels: usize) {
        let frames = self.buffer.frames();
        for frame in out.chunks_exact_mut(channels) {
            if self.finished {
                break;
            }
            if self.paused && self.hold.is_done() {
                break;
            }
            let head = &mut self.feed.head;
            if !head.prepare(&self.loops, frames) {
                self.finished = true;
                break;
            }
            if std::mem::take(&mut self.feed.head.stop_request) {
                self.stop();
            }
            if self.end_fade > 0
                && self.end_fade_from.is_none()
                && !self.stop_after_ramp
                && self.feed.head.pos + self.end_fade as usize >= self.feed.head.end
                && !self.feed.head.loop_ahead(&self.loops)
            {
                self.end_fade_from = Some(self.gain.target());
                self.gain.start(0.0, (self.feed.head.end - self.feed.head.pos) as u32, self.curve);
                self.envelope_held = false;
            }
            let head = &self.feed.head;
            let left = (!head.loop_ahead(&self.loops)).then(|| head.end.saturating_sub(head.pos));
            let blend = self.end_blend(left);
            let pos = self.feed.head.pos;
            let (l, r) = self.feed.head.read(&self.buffer);
            self.feed.resampler.push((l, r), pos);
            self.mix(frame, l * blend, r * blend);
            self.clock += 1;
        }
        self.collect_exits(self.feed.head);
    }

    /// Resampled or stretched playback, and the way over to the stretcher.
    fn render_through(&mut self, out: &mut [f32], channels: usize, ctx: &Context) {
        let frames = out.len() / channels.max(1);
        let mut played = 0;
        for frame in out.chunks_exact_mut(channels) {
            if self.finished {
                break;
            }
            if self.paused && self.hold.is_done() {
                break;
            }
            let blend = if self.end_blend > 0 { self.end_blend(self.remaining()) } else { 1.0 };
            let (mut l, mut r) = match self.path {
                Path::Stretched => {
                    if self.end_heard_at.is_some_and(|at| self.clock >= at) {
                        self.finished = true;
                        break;
                    }
                    let frame = self.stretched_frame(ctx.table, false);
                    if let Some(unit) = self.stretch.as_mut() {
                        self.heard_pos = unit.heard_at(self.clock).or(self.heard_pos);
                    }
                    frame
                }
                Path::Direct => {
                    // Direct while switching: as above, frame by frame.
                    let head = &mut self.feed.head;
                    if !head.prepare(&self.loops, self.buffer.frames()) {
                        self.finished = true;
                        break;
                    }
                    if std::mem::take(&mut self.feed.head.stop_request) {
                        self.stop();
                    }
                    let pos = self.feed.head.pos;
                    let frame = self.feed.head.read(&self.buffer);
                    self.feed.resampler.push(frame, pos);
                    frame
                }
                Path::Resampled => {
                    let rate = self.rate.value() as f64;
                    match self.feed.pull(&self.buffer, &self.loops, ctx.table, rate) {
                        Some(frame) => {
                            if std::mem::take(&mut self.feed.head.stop_request) {
                                self.stop();
                            }
                            frame
                        }
                        None => {
                            self.finished = true;
                            break;
                        }
                    }
                }
            };
            // The blend over to the stretcher, once it lines up.
            if let Some(sw) = self.switch
                && let Some(left) = sw.fade_left
                && left > 0
            {
                let (sl, sr) = self.stretched_frame(ctx.table, true);
                let w = 1.0 - left as f32 / self.switch_frames.max(1) as f32;
                l = l * (1.0 - w) + sl * w;
                r = r * (1.0 - w) + sr * w;
                if let Some(sw) = self.switch.as_mut() {
                    sw.fade_left = Some(left - 1);
                }
            }
            self.check_end_fade();
            self.mix(frame, l * blend, r * blend);
            self.clock += 1;
            played += 1;
            self.finish_switch();
        }
        if self.path != Path::Stretched {
            self.collect_exits(self.feed.head);
        }
        if let Some(sw) = self.switch.as_mut()
            && sw.fade_left.is_none()
        {
            sw.behind += played;
            let budget = frames * ctx.prime_ratio as usize;
            self.prime(ctx.table, budget);
        }
        self.deliver_pending();
    }

    /// The output stage: envelope, trim, duck, pause blend and pan, then
    /// one frame added into `frame`.
    #[inline]
    fn mix(&mut self, frame: &mut [f32], l: f32, r: f32) {
        if !self.speed_held {
            self.rate.tick();
            self.pitch.tick();
        }
        let envelope = if self.envelope_held { self.gain.value() } else { self.gain.tick() };
        let g = envelope * self.trim.tick() * self.duck.tick() * self.hold.tick();
        let pan = if self.pan_held { self.pan.value() } else { self.pan.tick() };
        frame[self.out_ch] += l * g * (1.0 - pan).clamp(0.0, 1.0);
        frame[self.out_ch + 1] += r * g * (1.0 + pan).clamp(0.0, 1.0);
        if (self.stop_after_ramp && self.gain.is_done()) || (self.stopping && self.hold.is_done()) {
            self.finished = true;
        }
    }
}
