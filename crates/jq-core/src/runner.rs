//! Show controller: cue list, playhead and the cue logic on top of the engine.
//! Keyboard, MIDI, OSC and buttons all end up as an [`Action`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jq_engine::{
    AudioBuffer, Controller, Curve, END_BLEND, Event, Held, LoopExit, LoopPass, LoopRegion, PARAM_SMOOTHING,
    Shape, VoiceId, VoiceParams, db_to_gain, gain_to_db,
};
use jq_i18n::t;
use jq_show::{
    AdvanceAt, Cue, CueId, CueKind, DevampMode, ExitLoop, FadeCurve, GoToCue, GroupMode, LoopLevel, LoopMode,
    OthersAction, OthersScope, PanCurve, PauseAction, PositionTo, RATE_MAX, RATE_MIN, Repeat, SILENCE_DB,
    SecondTrigger, Show, SoundCue, Speed, StartStopAction, StopwatchAction, Targets, VolumeAction, VolumeCue,
};
use serde::Serialize;

use crate::loader::{BufferCache, MediaLoader};
use crate::midi::{self, MidiSink};
use crate::order::{Order, step_file};
use crate::osc::{self, OscOut};
use crate::settings::LocalSettings;
use crate::watch::Stamps;

/// Sound cues after the playhead that are kept decoded.
const ARM_AHEAD: usize = 3;
const LOOP_CROSSFADE: Duration = Duration::from_millis(5);
/// Longest a streamed cue waits for the part it starts with before it plays
/// (silence where that part is missing).
const START_WAIT: Duration = Duration::from_secs(2);
/// Crossfade of a devamp "immediate" from the current position to the exit.
const DEVAMP_CROSSFADE: Duration = Duration::from_millis(50);
/// Crossfade of rehearsal jumps within a running cue.
const JUMP_CROSSFADE: Duration = Duration::from_millis(20);
/// A position cue to the previous mark within this many seconds after a
/// mark goes to the one before it.
const PREVIOUS_MARK_GRACE: f64 = 0.5;
/// Longest time a show value is taken as (about 115 days): anything longer
/// is a typo, and must neither panic nor overflow `Instant`.
const MAX_SECS: f64 = 1e7;
/// Go-to cues that start their target, one inside the other, before a
/// chain counts as an endless loop and is broken off.
const GOTO_DEPTH: u32 = 16;
/// GOs whose first cue is remembered for [`FollowState::root`].
const ROOTS_KEPT: usize = 64;

/// Control commands shared by keyboard, MIDI and (later) OSC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "action", content = "cue", rename_all = "snake_case")]
pub enum Action {
    Go,
    StopAll,
    FadeAll,
    /// Esc: fades everything out over the panic time; a second panic while
    /// that fade runs (or a panic time of 0) stops at once.
    Panic,
    PlayheadNext,
    PlayheadPrev,
    SetPlayhead(CueId),
    /// Starts one cue (honouring its pre-wait) without moving the playhead,
    /// e.g. by its hotkey.
    StartCue(CueId),
    /// Stops one cue (the selected one).
    StopCue(CueId),
    /// Fades one cue out with its own fade-out time (FADE ALL time if 0) and stops it.
    FadeCue(CueId),
    /// Rehearsal jumps in a running cue by the jump times from the settings.
    JumpBack(CueId),
    JumpForward(CueId),
    /// To the "jump to end" lead time before the end.
    JumpEnd(CueId),
    /// A playlist goes on to its next item: the item playing fades out
    /// with its fade-out time (at once without one), then the gap runs as
    /// after an item that ended by itself; in the gap, the next item starts
    /// at once. Paused, the next item starts at once (no fade, no gap) and
    /// stays paused at its start. Not on the last item (without repeat).
    NextItem(CueId),
    /// Plays a sound cue on the preview output (or stops it if it is the
    /// one playing there); never touches the show.
    Preview(CueId),
    StopPreview,
    /// Holds a running cue (a group: everything in it), its pre-wait and
    /// playlist gap included, until it is resumed.
    PauseCue(CueId),
    ResumeCue(CueId),
    /// Pauses the cue if anything of it still plays, else resumes it.
    TogglePauseCue(CueId),
    PauseAll,
    ResumeAll,
    /// Pauses everything if anything still plays, else resumes everything.
    TogglePauseAll,
    /// Stops one cue and starts it again from the top.
    RestartCue(CueId),
    /// Leaves the innermost loop or vamp of one cue at its end.
    ExitLoopCue(CueId),
    /// Brings one cue's level and pan back to its own at once.
    ResetCue(CueId),
    /// The hotkey or MIDI control of a cue was released: fires its second
    /// trigger if it has [`jq_show::Cue::release_trigger`].
    ReleaseCue(CueId),
    /// Breaks off what one running cue (a group: anything in it) would
    /// still start by its advance: post-waits, auto-follows, cues at its
    /// end or before it, loop exits. The cue itself plays on.
    CancelFollow(CueId),
    /// Moves one cue's level, pan or rate by the step from the settings.
    VolumeUp(CueId),
    VolumeDown(CueId),
    PanLeft(CueId),
    PanRight(CueId),
    RateUp(CueId),
    RateDown(CueId),
}

/// What remote control does with values (OSC), the way control cues do;
/// not a cue in the list.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// To `db`, or by `db` from the current level if `relative`, over
    /// `fade` seconds.
    Volume {
        cue: CueId,
        db: f64,
        fade: f64,
        relative: bool,
    },
    /// To `pan`, or by `pan` from the current pan if `relative`.
    Pan {
        cue: CueId,
        pan: f64,
        fade: f64,
        relative: bool,
    },
    Position {
        cue: CueId,
        to: PositionTo,
    },
    Reset {
        cue: CueId,
        fade: f64,
    },
    /// Rate to `rate` percent (or by `rate` points from the current rate if
    /// `relative`) and transposition to `transpose` semitones, each `None`
    /// unchanged, over `fade` seconds.
    Rate {
        cue: CueId,
        rate: Option<f64>,
        transpose: Option<f64>,
        relative: bool,
        fade: f64,
    },
    /// The stopwatch by its name (ignoring case) or id.
    Stopwatch {
        watch: String,
        action: StopwatchAction,
    },
}

/// Something the operator should see.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "level", content = "message", rename_all = "snake_case")]
pub enum Notice {
    Warning(String),
    Error(String),
}

/// Snapshot for the UI, sent about 30 times per second.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct State {
    pub playhead: Option<CueId>,
    pub active: Vec<ActiveState>,
    /// Cues waiting for their pre-wait to elapse.
    pub waiting: Vec<WaitingState>,
    /// Fade cues whose fade, and wait cues whose wait, is still in progress.
    pub fading: Vec<FadingState>,
    /// Playlist cues in the gap between two items (nothing of them plays).
    pub gaps: Vec<GapState>,
    /// The cue playing on the preview output.
    pub preview: Option<PreviewState>,
    /// GOs dropped by the GO lockout so far; the UI flashes GO when it
    /// changes.
    pub go_rejected: u32,
    /// Something is paused (PAUSE ALL then offers to resume).
    pub any_paused: bool,
    /// GOs taken on the playhead cue so far, if it repeats.
    pub repeat: Option<RepeatState>,
    /// Cues waiting to start by a chain, the next one of each.
    pub following: Vec<FollowState>,
    /// The show's stopwatches, in its order.
    pub stopwatches: Vec<StopwatchState>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StopwatchState {
    pub id: String,
    /// Seconds it has run since it was last reset.
    pub elapsed: f64,
    /// A countdown's time left, seconds: its start time minus `elapsed`;
    /// negative once it ran out (overtime). `None` for a stopwatch that
    /// counts up.
    pub remaining: Option<f64>,
    pub running: bool,
}

/// A stopwatch's time: what it ran before plus the run since `since`.
#[derive(Clone, Copy, Debug, Default)]
struct Watch {
    since: Option<Instant>,
    total: Duration,
}

impl Watch {
    fn elapsed(&self, now: Instant) -> Duration {
        self.total + self.since.map_or(Duration::ZERO, |s| now.saturating_duration_since(s))
    }
}

/// What a control cue's targets resolve to.
enum Hit {
    /// The cues as chosen (`targets`), and with everything inside them (`ids`).
    Cues {
        targets: Vec<CueId>,
        ids: Vec<CueId>,
    },
    All,
    Output(String),
}

impl Hit {
    /// True for a voice of cue `cue` playing on `output`.
    fn hits(&self, cue: CueId, output: &str) -> bool {
        match self {
            Hit::Cues { ids, .. } => ids.contains(&cue),
            Hit::All => true,
            Hit::Output(o) => o == output,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RepeatState {
    pub done: u32,
    /// `None` = until the playhead is moved on.
    pub count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FollowState {
    /// Starts once `after` reaches its point.
    pub cue: CueId,
    pub after: CueId,
    /// First cue of the chain (the one its GO started).
    pub root: CueId,
    /// `after` is paused, and the time left stands.
    pub paused: bool,
    /// Seconds; `None` while unknown (an open vamp, a loop exit).
    pub remaining: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PreviewState {
    pub cue: CueId,
    /// `false` while the file is still being loaded.
    pub playing: bool,
    pub elapsed: f64,
    /// 0…1 within start–end (from the position played from to the file's
    /// end for [`Runner::preview_at`]).
    pub progress: f32,
    /// Seconds in the file.
    pub position: f64,
}

struct Preview {
    cue: CueId,
    path: PathBuf,
    /// Set once the buffer is loaded and the voice started.
    voice: Option<VoiceId>,
    /// Plays from here to `until` or the file's end ([`Runner::preview_at`])
    /// instead of the cue's start–end.
    from: Option<usize>,
    until: Option<usize>,
    start_frame: usize,
    end_frame: usize,
    frame: usize,
    started: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActiveState {
    pub cue: CueId,
    pub elapsed: f64,
    /// `None` for endless loops.
    pub remaining: Option<f64>,
    /// Position within the start–end range, 0…1.
    pub progress: f32,
    pub level_db: f32,
    /// Balance as it plays now (−1 … +1), also while a pan cue moves it.
    pub pan: f32,
    /// Rate as it plays now, percent (100 = original), also while a rate
    /// cue moves it.
    pub rate: f32,
    /// Transposition as it plays now, semitones.
    pub transpose: f32,
    pub looping: bool,
    /// Position in a playlist cue.
    pub item: Option<ItemState>,
    /// A playlist item another one follows, not fading out to it yet
    /// (unless paused): [`Action::NextItem`] goes on.
    pub has_next: bool,
    /// Pass through the loop or vamp playback is in.
    pub pass: Option<PassState>,
    pub paused: bool,
    /// Waits for its file's start to be read; plays once it is (at most
    /// [`START_WAIT`] later).
    pub loading: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PassState {
    /// From 1.
    pub pass: u32,
    /// Passes in all; `None` = until devamped or stopped.
    pub count: Option<u32>,
    /// The vamp (from 1) for a cue with vamps; `None` for its loop.
    pub vamp: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ItemState {
    /// Position in the playing order (shuffled or not), 0-based.
    pub index: usize,
    /// The item's place in the list, 0-based (shuffled: not `index`).
    pub file: usize,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WaitingState {
    pub cue: CueId,
    pub remaining: f64,
    pub paused: bool,
    /// First cue of its chain (the cue itself when its GO started it).
    pub root: CueId,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GapState {
    pub cue: CueId,
    /// Seconds until the next item starts.
    pub remaining: f64,
    /// The item that starts then.
    pub next: ItemState,
    pub paused: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FadingState {
    pub cue: CueId,
    pub elapsed: f64,
    pub remaining: f64,
    /// 0…1.
    pub progress: f32,
    pub paused: bool,
}

struct Fading {
    cue: CueId,
    started: Instant,
    duration: Duration,
    paused_at: Option<Instant>,
    /// Voices a volume, pan or reset cue moves, and what of them; a pause
    /// holds those moves.
    voices: Vec<(VoiceId, Held)>,
}

struct Active {
    cue: CueId,
    voice: VoiceId,
    output: String,
    fade_out: f64,
    start_frame: usize,
    end_frame: usize,
    loop_mode: LoopMode,
    loops: Vec<LoopRegion>,
    /// Loops that repeat until devamped (vamps, or an endless loop).
    open_loops: usize,
    /// Devamps sent to this voice so far.
    devamped: usize,
    started: Option<Instant>,
    /// The file's part at the start is still being read (see
    /// [`VoiceParams::wait_frames`]); cleared once it plays.
    loading: bool,
    frame: usize,
    gain: f32,
    looping: bool,
    /// The loop played through and the pass, as the engine last reported.
    pass: Option<LoopPass>,
    /// The loops are the cue's vamps (else one loop over the range).
    vamps: bool,
    /// Cue to fire when this one ends by itself.
    follow: Option<CueId>,
    /// Cue to fire at the engine's `NearEnd`, and the lead in seconds.
    near_end: Option<(CueId, f64)>,
    /// Cues to fire when the voice leaves a loop a devamp cue released.
    on_exit: Vec<OnExit>,
    /// Playlist item this voice plays.
    item: Option<Item>,
    /// Start the next playlist item when this one ends by itself.
    continues: bool,
    /// Volume a "fade to" set (as a cue volume, before normalization and
    /// limits); the next playlist item starts there.
    level: Option<f64>,
    /// Pan a pan cue set; the next playlist item starts there.
    pan_set: Option<f64>,
    /// Balance as the engine last reported it.
    pan_now: f32,
    /// Rate and transposition a rate cue set (each `None` = the cue's own);
    /// the next playlist item starts there.
    speed_set: SpeedSet,
    /// The cue's own speed settings as last applied (live edits compare
    /// against it, like `pan`).
    speed: Speed,
    /// Rate and pitch as the engine last reported them.
    rate_now: f32,
    pitch_now: f32,
    /// The rate changed while playing: times are counted from the position
    /// instead of the clock.
    speed_moved: bool,
    /// When `frame` was reported.
    frame_at: Option<Instant>,
    /// Fading out to a stop (fade out, FADE ALL, panic, exclusive): a reset
    /// leaves it alone, or it would undo the stop.
    stopping: bool,
    /// Normalization offset of the file played.
    normalize_db: f64,
    /// Level the voice was started with (see [`Runner::level_db`]); live
    /// edits become a trim relative to it.
    base_db: f64,
    trim_db: f64,
    pan: f64,
    /// The GO (or direct start) this voice goes back to; what one GO starts
    /// is never "others" to itself (see [`Runner::affect_others`]).
    go: u64,
    paused_at: Option<Instant>,
    /// Fading out as one of the others of an exclusive cue.
    excluded: bool,
}

/// A playlist item playing.
#[derive(Clone, Copy, Debug)]
struct Item {
    /// Counted from the playlist's start, through all its passes.
    step: usize,
    /// The file played (index into the list).
    file: usize,
    /// Files in the list.
    count: usize,
}

impl Item {
    fn state(&self) -> ItemState {
        ItemState { index: self.step % self.count.max(1), file: self.file, count: self.count }
    }
}

/// A devamp cue's next cue, waiting for its target to leave the loop.
#[derive(Clone, Copy, Debug)]
struct OnExit {
    devamp: CueId,
    next: CueId,
    /// Waits for the last loop (devamp of all loops).
    all: bool,
    go: u64,
}

/// Another cue lowered by a starting one (ducking), while that one runs.
struct Duck {
    by: CueId,
    gain: f32,
    /// Seconds of the duck and of its return.
    fade: f64,
    /// Cues running when it started; they drop out once they end.
    targets: Vec<CueId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TimerKind {
    /// Pre-wait elapsed: fire the cue (`direct`: see [`Runner::trigger`];
    /// `from`: see [`Runner::trigger_from`]).
    Fire { direct: bool, from: Option<f64> },
    /// The owner reached its advance point: trigger the cue.
    Follow,
    /// The gap after a playlist item is over: start step `step` (see
    /// [`Item::step`]).
    Item { step: usize, carry: Carry },
}

/// What a playlist's next item takes over from the one before.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Carry {
    /// See [`Active::level`].
    level: Option<f64>,
    /// See [`Active::pan_set`].
    pan: Option<f64>,
    /// See [`Active::speed_set`].
    speed: SpeedSet,
}

/// Rate (percent) and transposition (semitones) set by a rate cue; `None`
/// keeps the cue's own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SpeedSet {
    rate: Option<f64>,
    transpose: Option<f64>,
}

impl SpeedSet {
    /// `speed` with what was set instead of its own values.
    fn over(self, speed: Speed) -> Speed {
        Speed {
            rate: self.rate.unwrap_or(speed.rate),
            transpose: self.transpose.unwrap_or(speed.transpose),
            ..speed
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Timer {
    at: Instant,
    cue: CueId,
    kind: TimerKind,
    /// The cue the timer belongs to (for a follow: the cue followed), which
    /// a pause holds it with.
    owner: CueId,
    go: u64,
    /// Time left while paused.
    paused: Option<Duration>,
    /// The go-to/start nesting it was set in, if it was due at once: it
    /// fires in the same tick, so a loop of cues with no time between them
    /// runs over the timers instead of recursion and is broken off the same
    /// way (see [`GOTO_DEPTH`]).
    depth: u32,
}

impl Timer {
    fn new(at: Instant, cue: CueId, kind: TimerKind, owner: CueId, go: u64) -> Self {
        Self { at, cue, kind, owner, go, paused: None, depth: 0 }
    }
}

pub struct Runner {
    show: Show,
    media_dir: PathBuf,
    settings: LocalSettings,
    engine: Controller,
    cache: BufferCache,
    /// Start frames of the armed files' coming cues, kept loaded in streams.
    starts: HashMap<PathBuf, Vec<usize>>,
    /// Index into the cue list; `None` = past the last cue.
    playhead: Option<usize>,
    active: Vec<Active>,
    timers: Vec<Timer>,
    fading: Vec<Fading>,
    notices: Vec<Notice>,
    preview: Option<Preview>,
    /// Engine of a separate preview device; `None` = the main engine.
    preview_engine: Option<Controller>,
    osc_out: Option<Box<dyn OscOut>>,
    midi_out: Option<Box<dyn MidiSink>>,
    /// Groups with auto-follow: (group, next cue, GO) once nothing in the
    /// group runs.
    group_follows: Vec<(CueId, CueId, u64)>,
    ducks: Vec<Duck>,
    /// Last GO number handed out (see [`Active::go`]).
    last_go_id: u64,
    /// Files still being copied into the media folder; not loaded until done.
    importing: HashSet<PathBuf>,
    /// The last GO taken, for the GO lockout.
    last_go: Option<Instant>,
    go_rejected: u32,
    /// End of the fade of the last panic.
    panic_until: Option<Instant>,
    /// GOs taken on the playhead cue (Repeat); any playhead move resets it.
    repeat_done: u32,
    /// The cue each recent GO started first, and whether it was a direct
    /// start (hotkey, OSC): such cues cannot all be armed, so loading one on
    /// the spot is no warning.
    roots: Vec<(u64, CueId, bool)>,
    /// Go-to and start cues starting their targets in the current call.
    goto_depth: u32,
    /// Cues being fired, nested (a group fires its cues); arming waits
    /// until the outermost one is done.
    firing: u32,
    /// Stopwatch times by id, for the session.
    stopwatches: HashMap<String, Watch>,
    /// Playing orders of the shuffled playlists running.
    orders: HashMap<CueId, Order>,
    /// Orders drawn for the next start of shuffled playlists to come, so
    /// that the file they begin with can be armed.
    drawn: HashMap<CueId, Order>,
    rng: fastrand::Rng,
}

impl Runner {
    pub fn new(
        show: Show,
        media_dir: PathBuf,
        settings: LocalSettings,
        engine: Controller,
        loader: Arc<dyn MediaLoader>,
    ) -> Self {
        let rate = engine.config().sample_rate;
        let mut runner = Self {
            playhead: show.playable_from(0),
            show,
            media_dir,
            settings,
            engine,
            cache: BufferCache::new(loader, rate),
            starts: HashMap::new(),
            active: Vec::new(),
            timers: Vec::new(),
            fading: Vec::new(),
            notices: Vec::new(),
            preview: None,
            preview_engine: None,
            osc_out: None,
            midi_out: None,
            group_follows: Vec::new(),
            ducks: Vec::new(),
            last_go_id: 0,
            importing: HashSet::new(),
            last_go: None,
            go_rejected: 0,
            panic_until: None,
            repeat_done: 0,
            roots: Vec::new(),
            goto_depth: 0,
            firing: 0,
            stopwatches: HashMap::new(),
            orders: HashMap::new(),
            drawn: HashMap::new(),
            rng: fastrand::Rng::new(),
        };
        runner.arm();
        runner
    }

    pub fn show(&self) -> &Show {
        &self.show
    }

    /// Replaces the show after an edit; the playhead stays on its cue.
    pub fn set_show(&mut self, show: Show) {
        let current = self.playhead_cue();
        self.show = show;
        let watches = &self.show.stopwatches;
        self.stopwatches.retain(|id, _| watches.iter().any(|w| &w.id == id));
        self.playhead = current
            .and_then(|id| self.show.index_of(id))
            .or(self.playhead)
            .and_then(|i| self.show.playable_from(i));
        if self.playhead_cue() != current {
            self.repeat_done = 0;
        }
        self.arm();
        self.apply_live_edits();
    }

    /// Takes the media watcher's snapshot: changed files are read again and
    /// armed anew. Returns the running cues (and the preview) whose file
    /// changed; they go on with what they read before (a streamed file is
    /// read on, so it may sound wrong once overwritten in place).
    pub fn media_changed(&mut self, stamps: Stamps) -> Vec<CueId> {
        let changed = self.cache.check(stamps);
        if changed.is_empty() {
            return Vec::new();
        }
        let (show, media_dir) = (&self.show, &self.media_dir);
        let path_of = |a: &Active| {
            let files = show.cue(a.cue)?.media_files();
            let index = a.item.map_or(0, |item| item.file);
            Some(jq_show::paths::resolve(media_dir, files.get(index)?))
        };
        let mut playing: Vec<CueId> = self
            .active
            .iter()
            .filter(|a| path_of(a).is_some_and(|p| changed.contains(&p)))
            .map(|a| a.cue)
            .collect();
        if let Some(p) = self.preview.as_ref().filter(|p| p.voice.is_some() && changed.contains(&p.path)) {
            playing.push(p.cue);
        }
        playing.dedup();
        self.arm();
        playing
    }

    /// Files that were read as something else than the media watcher's
    /// last snapshot says: it may have missed a change and should look again.
    /// They are read once its next snapshot comes.
    pub fn take_suspect_files(&mut self) -> Vec<PathBuf> {
        self.cache.take_suspects()
    }

    /// Lets running cues follow `cue` while it is still being edited (a
    /// slider being dragged), outside the editor's history; the edit's end
    /// comes in as [`Runner::set_show`]. Until then the next GO plays the
    /// cue as given here. Cues linked with it follow as well. Returns false
    /// if there is no such cue.
    pub fn live_edit(&mut self, mut cue: Cue) -> bool {
        let Some(c) = self.show.cue_mut(cue.id) else { return false };
        cue.link = c.link;
        let id = cue.id;
        *c = cue;
        self.show.spread_link(id);
        self.apply_live_edits();
        true
    }

    /// Lets running sound and playlist cues follow edits to their volume,
    /// normalization, pan and speed, so levels can be set while listening. Other fields
    /// apply on the next GO.
    fn apply_live_edits(&mut self) {
        let mut trims = Vec::new();
        let mut pans = Vec::new();
        let mut speeds = Vec::new();
        for a in &mut self.active {
            // Rate, pitch and transposition of sound and playlist cues; a
            // value changed on the cue replaces what a rate cue set.
            let own = match self.show.cue(a.cue).map(|c| &c.kind) {
                Some(CueKind::Sound(s)) => Some(s.speed),
                Some(CueKind::Playlist(p)) => {
                    p.items.get(item_file(a)).map(|item| item.overrides.speed_over(p.speed))
                }
                _ => None,
            };
            if let Some(own) = own
                && own != a.speed
            {
                if own.rate != a.speed.rate {
                    a.speed_set.rate = None;
                }
                if own.transpose != a.speed.transpose {
                    a.speed_set.transpose = None;
                }
                a.speed = own;
                a.speed_moved = true;
                let (rate, pitch) = a.speed_set.over(own).factors();
                speeds.push((a.voice, rate as f32, pitch as f32));
            }
            // Volume, normalization offset and pan of sound and playlist
            // cues (an item's: with its own gain and pan).
            let (volume_db, normalize_db, pan) = match self.show.cue(a.cue).map(|c| &c.kind) {
                Some(CueKind::Sound(s)) => (s.volume_db, s.normalize_db.unwrap_or(0.0), s.pan),
                Some(CueKind::Playlist(p)) => {
                    let (offset_db, pan) = item_offset_pan(p, item_file(a));
                    (p.volume_db, offset_db, pan)
                }
                _ => continue,
            };
            a.normalize_db = normalize_db;
            let level = self.show.level_limits.level(volume_db, a.normalize_db);
            let trim_db = level.unwrap_or(SILENCE_DB) - a.base_db;
            if trim_db != a.trim_db {
                a.trim_db = trim_db;
                let gain = if level.is_none() { 0.0 } else { db_to_gain(trim_db as f32) };
                trims.push((a.voice, gain));
            }
            if pan != a.pan {
                // An edited pan takes over from a pan cue's, also for the next item.
                a.pan = pan;
                a.pan_set = None;
                pans.push((a.voice, pan.clamp(-1.0, 1.0) as f32));
            }
        }
        for (v, gain) in trims {
            self.send(|e| e.set_trim(v, gain));
        }
        for (v, pan) in pans {
            self.send(|e| e.set_pan(v, pan));
        }
        for (v, rate, pitch) in speeds {
            self.send(|e| e.set_speed(v, rate, pitch));
        }
    }

    /// After the show folder moved; files are armed again from there.
    pub fn set_media_dir(&mut self, media_dir: PathBuf) {
        self.media_dir = media_dir;
        self.arm();
    }

    /// Files being copied into the media folder; once a file leaves the
    /// set, it is armed like any other.
    pub fn set_importing(&mut self, files: HashSet<PathBuf>) {
        if files != self.importing {
            self.importing = files;
            self.arm();
        }
    }

    /// Hands what outlives the audio device to `next`, a runner on the
    /// reopened output for the same show: the stopwatch times (a countdown
    /// keeps running), the GO count of Repeat, the GO lockout and the last
    /// panic, the orders drawn for shuffled playlists to come and the files
    /// being imported. Only while idle: running cues are not handed over.
    pub fn carry_over(&mut self, next: &mut Runner) {
        next.stopwatches = std::mem::take(&mut self.stopwatches);
        next.stopwatches.retain(|id, _| next.show.stopwatches.iter().any(|w| &w.id == id));
        next.repeat_done = if next.playhead == self.playhead { self.repeat_done } else { 0 };
        (next.last_go, next.go_rejected, next.panic_until) =
            (self.last_go, self.go_rejected, self.panic_until);
        next.drawn = std::mem::take(&mut self.drawn);
        next.importing = std::mem::take(&mut self.importing);
        next.arm();
    }

    pub fn settings(&self) -> &LocalSettings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: LocalSettings) {
        self.settings = settings;
    }

    /// True if no cue is running or waiting for its pre-wait.
    pub fn is_idle(&self) -> bool {
        self.active.is_empty() && self.timers.is_empty()
    }

    /// Like [`Runner::is_idle`], and no preview plays either.
    pub fn is_quiet(&self) -> bool {
        self.is_idle() && self.preview.is_none()
    }

    /// Whether the UI's state changes by itself although the show is quiet:
    /// a stopwatch runs, files are being loaded (armed), or a cue with a
    /// duration (wait, or a fade with nothing playing to move) runs.
    pub fn is_lively(&self) -> bool {
        self.stopwatches.values().any(|w| w.since.is_some())
            || self.cache.is_loading()
            || self.fading.iter().any(|f| f.paused_at.is_none())
    }

    /// Uses a separate device's engine for previews (`None` = main engine);
    /// a running preview stops.
    pub fn set_preview_engine(&mut self, engine: Option<Controller>) {
        self.stop_preview();
        self.preview_engine = engine;
    }

    /// Where OSC cues send to; without one they report an error.
    pub fn set_osc_out(&mut self, out: Box<dyn OscOut>) {
        self.osc_out = Some(out);
    }

    /// Where MIDI cues send to; `None` = no output (they report an error).
    pub fn set_midi_out(&mut self, out: Option<Box<dyn MidiSink>>) {
        self.midi_out = out;
    }

    pub fn playhead_cue(&self) -> Option<CueId> {
        self.playhead.map(|i| self.show.cues[i].id)
    }

    /// True if the playhead cue's file is decoded and GO starts without disk access.
    /// A streamed file counts once the chunk at the start position is loaded.
    pub fn is_armed(&self, id: CueId) -> bool {
        let ready = |file: &str, start: f64| {
            let path = jq_show::paths::resolve(&self.media_dir, file);
            self.cache.buffer(&path).is_some_and(|b| b.is_ready_at(self.frame_at(start)))
        };
        match self.show.cue(id).map(|c| &c.kind) {
            Some(CueKind::Sound(s)) => s.is_placeholder() || ready(&s.file, s.start),
            Some(CueKind::Playlist(p)) => {
                // Shuffled: the file the order drawn for its next start begins with.
                let order = self.drawn.get(&id).filter(|order| order.fits(p));
                if !p.shuffle.is_off() && order.is_none() && !p.items.is_empty() {
                    return false;
                }
                step_file(order, 0, p.items.len())
                    .and_then(|file| p.items.get(file))
                    .is_none_or(|item| ready(&item.file, item.overrides.start))
            }
            Some(CueKind::Group(g)) => g.cues.iter().all(|c| self.is_armed(c.id)),
            _ => true,
        }
    }

    fn frame_at(&self, secs: f64) -> usize {
        (secs.max(0.0) * self.engine.config().sample_rate as f64).round() as usize
    }

    /// Streamed files of the next cues keep their start position loaded.
    fn hint_streams(&self) {
        let preview = self.preview.as_ref().map(|p| (&p.path, p.start_frame));
        for (path, starts) in &self.starts {
            if let Some(stream) = self.cache.buffer(path).and_then(|b| b.stream()) {
                let mut hints = starts.clone();
                hints.extend(preview.filter(|(p, _)| *p == path).map(|(_, start)| start));
                stream.set_hints(hints);
            }
        }
        if let Some((path, start)) = preview.filter(|(p, _)| !self.starts.contains_key(*p))
            && let Some(stream) = self.cache.buffer(path).and_then(|b| b.stream())
        {
            stream.set_hints(vec![start]);
        }
    }

    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    pub fn handle(&mut self, action: Action, now: Instant) {
        match action {
            Action::Go => self.go(now),
            Action::StopAll => {
                self.cancel_all();
                self.send(|e| e.stop_all());
            }
            Action::FadeAll => {
                self.cancel_all();
                self.active.iter_mut().for_each(|a| a.stopping = true);
                let d = secs(self.settings.fade_all);
                let c = self.decibel(Shape::SlowTop);
                self.send(|e| e.fade_all(d, c));
            }
            Action::Panic => {
                self.cancel_all();
                let fade = secs(self.settings.panic_fade);
                if fade.is_zero() || self.panic_until.is_some_and(|until| now < until) {
                    self.panic_until = None;
                    self.send(|e| e.stop_all());
                } else {
                    self.panic_until = Some(now + fade);
                    self.active.iter_mut().for_each(|a| a.stopping = true);
                    let c = self.decibel(Shape::Linear);
                    self.send(|e| e.fade_all(fade, c));
                }
            }
            Action::PlayheadNext => self.set_playhead(self.playhead.map_or(self.show.cues.len(), |i| i + 1)),
            Action::PlayheadPrev => {
                let i = self.playhead.unwrap_or(self.show.cues.len());
                // Back past scene markers; with none but them before, on the first cue.
                let prev = (0..i).rev().find(|&k| !self.show.cues[k].is_scene()).unwrap_or(0);
                self.set_playhead(prev);
            }
            Action::SetPlayhead(id) => {
                if let Some(i) = self.show.index_of(id) {
                    self.set_playhead(i);
                }
            }
            Action::FadeCue(id) => {
                let ids = self.with_descendants(id);
                // Its post-wait ends with it: the operator broke off the chain.
                self.timers.retain(|t| !ids.contains(&t.cue) && !ids.contains(&t.owner));
                self.group_follows.retain(|(g, ..)| !ids.contains(g));
                let fade_all = self.settings.fade_all;
                // Voices already on their way out keep their fade: starting
                // it anew from the current level would only drag it out.
                let running = self.active.iter().any(|a| ids.contains(&a.cue));
                // A playlist on its way to its next item ends with that fade instead.
                let mut ended = false;
                for a in self.active.iter_mut().filter(|a| ids.contains(&a.cue) && a.stopping && a.continues)
                {
                    a.continues = false;
                    ended = true;
                }
                let voices: Vec<_> = self
                    .active
                    .iter_mut()
                    .filter(|a| ids.contains(&a.cue) && !a.stopping)
                    .map(|a| {
                        a.stopping = true;
                        (cancel_follow(a), if a.fade_out > 0.0 { a.fade_out } else { fade_all })
                    })
                    .collect();
                if running
                    && voices.is_empty()
                    && !ended
                    && let Some(cue) = self.show.cue(id)
                {
                    self.notices.push(Notice::Warning(t!("runner.already_fading", cue = label(cue))));
                }
                for (v, d) in voices {
                    let c = self.decibel(Shape::SlowTop);
                    self.send(|e| e.fade_out(v, secs(d), c));
                }
            }
            Action::JumpBack(id) => {
                self.jump(id, now, self.settings.jump_back, |a, step| a.frame.saturating_sub(step));
            }
            Action::JumpForward(id) => {
                self.jump(id, now, self.settings.jump_forward, |a, step| a.frame + step)
            }
            Action::JumpEnd(id) => {
                let lead = self.settings.jump_end;
                self.jump(id, now, lead, |a, step| a.end_frame.saturating_sub(step).max(a.start_frame));
            }
            Action::NextItem(id) => self.skip_item(id, now),
            Action::StartCue(id) => {
                let go = self.new_go(id, true);
                self.trigger(id, now, true, go);
            }
            Action::Preview(id) => {
                // A preview from a position switches to the cue's own.
                let same = self.preview.as_ref().is_some_and(|p| p.cue == id && p.from.is_none());
                self.stop_preview();
                if !same {
                    self.start_preview(id, None, None);
                }
            }
            Action::StopPreview => self.stop_preview(),
            // Its post-wait ends with it: the operator broke off the chain.
            Action::StopCue(id) => self.stop_hit(&self.cue_hit(id)),
            Action::PauseCue(id) => self.set_paused(Some(&self.with_descendants(id)), true, now),
            Action::ResumeCue(id) => self.set_paused(Some(&self.with_descendants(id)), false, now),
            Action::TogglePauseCue(id) => {
                let ids = self.with_descendants(id);
                self.set_paused(Some(&ids), self.is_playing(Some(&ids)), now);
            }
            Action::PauseAll => self.set_paused(None, true, now),
            Action::ResumeAll => self.set_paused(None, false, now),
            Action::TogglePauseAll => self.set_paused(None, self.is_playing(None), now),
            Action::RestartCue(id) => {
                self.remove_now(&self.with_descendants(id));
                let go = self.new_go(id, true);
                self.trigger(id, now, true, go);
            }
            Action::ExitLoopCue(id) => {
                self.exit_loops(&self.with_descendants(id), ExitLoop::default());
            }
            Action::ResetCue(id) => {
                self.apply_reset(&self.cue_hit(id), Duration::ZERO);
            }
            Action::ReleaseCue(id) => {
                if self.show.cue(id).is_some_and(|c| c.release_trigger) && self.is_running_tree(id) {
                    let go = self.new_go(id, true);
                    self.second_trigger(id, now, go);
                }
            }
            Action::CancelFollow(id) => {
                let ids = self.with_descendants(id);
                self.timers.retain(|t| t.kind != TimerKind::Follow || !ids.contains(&t.owner));
                self.group_follows.retain(|(g, ..)| !ids.contains(g));
                for a in self.active.iter_mut().filter(|a| ids.contains(&a.cue)) {
                    // A playlist goes on with its items: they are not a chain.
                    (a.follow, a.near_end) = (None, None);
                    a.on_exit.clear();
                }
            }
            Action::VolumeUp(cue) | Action::VolumeDown(cue) => {
                let step = self.settings.volume_step.abs();
                let db = if matches!(action, Action::VolumeUp(_)) { step } else { -step };
                self.control(Control::Volume { cue, db, fade: 0.0, relative: true }, now);
            }
            Action::PanLeft(cue) | Action::PanRight(cue) => {
                let step = self.settings.pan_step.abs();
                let pan = if matches!(action, Action::PanRight(_)) { step } else { -step };
                self.control(Control::Pan { cue, pan, fade: 0.0, relative: true }, now);
            }
            Action::RateUp(cue) | Action::RateDown(cue) => {
                let step = self.settings.rate_step.abs();
                let rate = if matches!(action, Action::RateUp(_)) { step } else { -step };
                let rate = Some(rate);
                self.control(Control::Rate { cue, rate, transpose: None, relative: true, fade: 0.0 }, now);
            }
        }
    }

    /// Applies a remote control's value to a cue or a stopwatch.
    pub fn control(&mut self, control: Control, now: Instant) {
        match control {
            Control::Volume { cue, db, fade, relative } => {
                let f = VolumeCue {
                    targets: Targets::Cues(vec![cue]),
                    action: VolumeAction::FadeTo,
                    volume_db: Some(db),
                    duration: Some(fade),
                    curve: FadeCurve::default(),
                };
                self.apply_volume(&self.cue_hit(cue), &f, relative);
            }
            Control::Pan { cue, pan, fade, relative } => {
                self.apply_pan(&self.cue_hit(cue), pan, relative, secs(fade), PanCurve::SCurve);
            }
            Control::Position { cue, to } => {
                let crossfade = self.engine.frames(JUMP_CROSSFADE);
                self.apply_position(&self.cue_hit(cue), to, crossfade, now);
            }
            Control::Reset { cue, fade } => {
                self.apply_reset(&self.cue_hit(cue), secs(fade));
            }
            Control::Rate { cue, rate, transpose, relative, fade } => {
                let set = SpeedSet { rate, transpose };
                self.apply_rate(&self.cue_hit(cue), set, relative, secs(fade), PanCurve::SCurve);
            }
            Control::Stopwatch { watch, action } => {
                let id = self
                    .show
                    .stopwatches
                    .iter()
                    .find(|w| w.id == watch || w.name.to_lowercase() == watch.to_lowercase())
                    .map(|w| w.id.clone());
                if let Some(id) = id {
                    self.stopwatch(&id, action, now);
                }
            }
        }
    }

    /// Starts, stops or resets one of the show's stopwatches (by id).
    pub fn stopwatch(&mut self, id: &str, action: StopwatchAction, now: Instant) {
        if !self.show.stopwatches.iter().any(|w| w.id == id) {
            return;
        }
        let w = self.stopwatches.entry(id.to_owned()).or_default();
        match action {
            StopwatchAction::Start => {
                w.since.get_or_insert(now);
            }
            StopwatchAction::Stop => *w = Watch { since: None, total: w.elapsed(now) },
            StopwatchAction::Reset => *w = Watch::default(),
            StopwatchAction::Restart => *w = Watch { since: Some(now), total: Duration::ZERO },
        }
    }

    /// Pauses or resumes the voices, pre-waits, playlist gaps, follow
    /// timers and fade/wait progress of the cues `ids` (`None` = all);
    /// their times stand still meanwhile.
    fn set_paused(&mut self, ids: Option<&[CueId]>, pause: bool, now: Instant) {
        let hit = |id: CueId| ids.is_none_or(|ids| ids.contains(&id));
        let mut voices = Vec::new();
        for a in self.active.iter_mut().filter(|a| hit(a.cue)) {
            match (pause, a.paused_at) {
                (true, None) => a.paused_at = Some(now),
                (false, Some(at)) => {
                    a.paused_at = None;
                    if let Some(started) = a.started.as_mut() {
                        *started += now.saturating_duration_since(at);
                    }
                }
                _ => continue,
            }
            voices.push(a.voice);
        }
        for t in self.timers.iter_mut().filter(|t| hit(t.owner)) {
            match (pause, t.paused) {
                (true, None) => t.paused = Some(t.at.saturating_duration_since(now)),
                (false, Some(left)) => (t.at, t.paused) = (now + left, None),
                _ => {}
            }
        }
        let mut held = Vec::new();
        for f in self.fading.iter_mut().filter(|f| hit(f.cue)) {
            match (pause, f.paused_at) {
                (true, None) => f.paused_at = Some(now),
                (false, Some(at)) => {
                    f.started += now.saturating_duration_since(at);
                    f.paused_at = None;
                }
                _ => continue,
            }
            held.extend(f.voices.iter().copied());
        }
        for v in voices {
            self.send(|e| if pause { e.pause(v) } else { e.resume(v) });
        }
        // A paused volume or pan cue holds its move; the cues it moves play on.
        for (v, ramp) in held {
            self.send(|e| match ramp {
                Held::Gain => e.hold_fade(v, pause),
                Held::Pan => e.hold_pan(v, pause),
                Held::Speed => e.hold_speed(v, pause),
            });
        }
    }

    /// True if anything of the cues `ids` (`None` = all) runs and is not paused.
    fn is_playing(&self, ids: Option<&[CueId]>) -> bool {
        let hit = |id: CueId| ids.is_none_or(|ids| ids.contains(&id));
        self.active.iter().any(|a| hit(a.cue) && a.paused_at.is_none())
            || self.timers.iter().any(|t| hit(t.owner) && t.paused.is_none())
            || self.fading.iter().any(|f| hit(f.cue) && f.paused_at.is_none())
    }

    /// A new GO number, starting with cue `id`.
    fn new_go_for(&mut self, id: CueId) -> u64 {
        self.new_go(id, false)
    }

    fn new_go(&mut self, id: CueId, direct: bool) -> u64 {
        self.last_go_id += 1;
        if self.roots.len() == ROOTS_KEPT {
            self.roots.remove(0);
        }
        self.roots.push((self.last_go_id, id, direct));
        self.last_go_id
    }

    /// GO `go` was a direct start (see [`Runner::roots`]).
    fn is_direct_go(&self, go: u64) -> bool {
        self.roots.iter().any(|&(g, _, direct)| g == go && direct)
    }

    /// A GO or direct start of a cue that is already running.
    fn second_trigger(&mut self, id: CueId, now: Instant, go: u64) {
        let Some(mode) = self.show.cue(id).map(|c| c.second_trigger) else { return };
        match mode {
            SecondTrigger::Ignore => {}
            SecondTrigger::Pause => self.handle(Action::TogglePauseCue(id), now),
            SecondTrigger::Stop => self.handle(Action::StopCue(id), now),
            SecondTrigger::FadeOut => self.handle(Action::FadeCue(id), now),
            SecondTrigger::Restart => {
                self.remove_now(&self.with_descendants(id));
                self.trigger(id, now, true, go);
            }
            SecondTrigger::ExitLoop { exit } => {
                self.exit_loops(&self.with_descendants(id), exit);
            }
            SecondTrigger::NextItem => self.skip_item(id, now),
        }
    }

    /// Stops the cues `ids` at once and forgets them, so nothing of them
    /// follows any more and they can start again right away.
    fn remove_now(&mut self, ids: &[CueId]) {
        self.timers.retain(|t| !ids.contains(&t.owner));
        self.fading.retain(|f| !ids.contains(&f.cue));
        self.group_follows.retain(|(g, ..)| !ids.contains(g));
        let voices: Vec<VoiceId> =
            self.active.iter().filter(|a| ids.contains(&a.cue)).map(|a| a.voice).collect();
        self.active.retain(|a| !ids.contains(&a.cue));
        for v in voices {
            self.send(|e| e.stop(v));
        }
        let (ended, kept) = std::mem::take(&mut self.ducks).into_iter().partition(|d| ids.contains(&d.by));
        self.ducks = kept;
        self.unduck(ended);
    }

    /// Processes engine events and due timers. Call regularly (every few ms).
    pub fn tick(&mut self, now: Instant) {
        if self.cache.poll() {
            // A file that failed to load may move arming on to the next item.
            self.arm();
        }
        for (path, frames) in self.cache.take_underruns(now) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let ms = frames * 1000 / self.engine.config().sample_rate.max(1) as u64;
            self.notices.push(Notice::Warning(t!("runner.underrun", name = name, ms = ms)));
        }
        let events: Vec<Event> = self.engine.poll_events().collect();
        for event in events {
            match event {
                Event::Started(v) => {
                    let rate = self.engine.config().sample_rate as f64;
                    if let Some(a) = self.active.iter_mut().find(|a| a.voice == v) {
                        // Paused before it played, it starts when the pause
                        // began. Started from a position, the part before it
                        // counts as played, as after a jump (see `seek_voices`),
                        // so the time left and the follows are right.
                        let at = a.paused_at.unwrap_or(now);
                        let played = a.frame.saturating_sub(a.start_frame) as f64
                            / rate
                            / (a.rate_now as f64).max(0.01);
                        a.started = Some(at.checked_sub(secs(played)).unwrap_or(at));
                    }
                    self.preview_event(&Event::Started(v), now);
                }
                Event::Position { id, frame, gain, looping, pass, pan, rate, pitch } => {
                    if let Some(a) = self.active.iter_mut().find(|a| a.voice == id) {
                        (a.frame, a.gain, a.looping, a.pass, a.pan_now) = (frame, gain, looping, pass, pan);
                        (a.rate_now, a.pitch_now, a.frame_at) = (rate, pitch, Some(now));
                    }
                    self.preview_event(&event, now);
                }
                Event::NoStretcher(v) => {
                    if let Some(cue) =
                        self.active.iter().find(|a| a.voice == v).and_then(|a| self.show.cue(a.cue))
                    {
                        self.notices
                            .push(Notice::Warning(t!("runner.too_many_stretchers", cue = label(cue))));
                    }
                }
                Event::Finished { id, .. } => {
                    self.preview_event(&event, now);
                    if let Some(i) = self.active.iter().position(|a| a.voice == id) {
                        let a = self.active.remove(i);
                        self.follow_ended(&a, now);
                        if let (true, Some(item)) = (a.continues, a.item) {
                            let carry = Carry { level: a.level, pan: a.pan_set, speed: a.speed_set };
                            self.next_item(a.cue, item.step + 1, carry, now, a.go);
                        }
                    }
                }
                Event::Rejected { id, .. } => {
                    self.preview_event(&event, now);
                    self.notices.push(Notice::Error(t!("runner.too_many_voices")));
                    // Like a broken cue, a rejected one must not break the chain.
                    if let Some(i) = self.active.iter().position(|a| a.voice == id) {
                        let a = self.active.remove(i);
                        self.follow_ended(&a, now);
                    }
                }
                Event::NearEnd(v) => {
                    let Some(a) = self.active.iter_mut().find(|a| a.voice == v) else { continue };
                    if let Some((next, _)) = a.near_end.take() {
                        let go = a.go;
                        self.trigger(next, now, false, go);
                    }
                }
                Event::LoopExited { id, last } => {
                    let Some(a) = self.active.iter_mut().find(|a| a.voice == id) else { continue };
                    let Some(i) = a.on_exit.iter().position(|e| !e.all || last) else { continue };
                    let e = a.on_exit.remove(i);
                    self.fire_on_exit(e, now);
                }
            }
        }

        let preview_events: Vec<Event> =
            self.preview_engine.as_mut().map(|e| e.poll_events().collect()).unwrap_or_default();
        for event in &preview_events {
            self.preview_event(event, now);
        }
        self.start_preview_when_loaded();

        while let Some(i) = self.timers.iter().position(|t| t.paused.is_none() && t.at <= now) {
            let t = self.timers.remove(i);
            let outer = std::mem::replace(&mut self.goto_depth, t.depth);
            match t.kind {
                TimerKind::Fire { direct, from } => self.fire(t.cue, now, t.go, direct, from),
                TimerKind::Follow => self.trigger(t.cue, now, false, t.go),
                TimerKind::Item { step, carry } => {
                    self.next_items_from(t.cue, step, carry, t.go, None, now);
                }
            }
            self.goto_depth = outer;
        }
        // Orders of the playlists that have ended (or go on with no next item).
        let (active, timers) = (&self.active, &self.timers);
        self.orders.retain(|id, _| {
            active.iter().any(|a| a.cue == *id && a.continues)
                || timers.iter().any(|t| t.cue == *id && matches!(t.kind, TimerKind::Item { .. }))
        });
        // A volume, pan, rate or reset cue whose voices have all ended has
        // nothing left to move; wait cues (no voices) run their time.
        let active = &self.active;
        self.fading.retain(|f| {
            (f.paused_at.is_some() || now < f.started + f.duration)
                && (f.voices.is_empty() || f.voices.iter().any(|(v, _)| active.iter().any(|a| a.voice == *v)))
        });

        // Groups with auto-follow whose cues are all done.
        let done: Vec<(CueId, CueId, u64)> = self
            .group_follows
            .iter()
            .copied()
            .filter(|(g, ..)| self.with_descendants(*g).iter().skip(1).all(|d| !self.is_running(*d)))
            .collect();
        if !done.is_empty() {
            self.group_follows.retain(|f| !done.contains(f));
            for (_, next, go) in done {
                self.trigger(next, now, false, go);
            }
        }
        self.end_ducks();
    }

    /// A voice that ended by itself (or never started) starts what still
    /// waits for it: its follow, and a near end or loop exit it never
    /// reported (it was jumped past), so the chain does not break.
    fn follow_ended(&mut self, a: &Active, now: Instant) {
        if let Some(next) = a.follow {
            self.trigger(next, now, false, a.go);
        }
        if let Some((next, _)) = a.near_end {
            self.trigger(next, now, false, a.go);
        }
        for &e in &a.on_exit {
            self.fire_on_exit(e, now);
        }
    }

    /// Starts a devamp cue's next cue once one of its targets left the
    /// loop; the other targets no longer wait for it.
    fn fire_on_exit(&mut self, e: OnExit, now: Instant) {
        for a in &mut self.active {
            a.on_exit.retain(|o| o.devamp != e.devamp || o.go != e.go);
        }
        self.trigger(e.next, now, false, e.go);
    }

    /// Ducks whose cue no longer runs end; cues that ended leave them.
    fn end_ducks(&mut self) {
        if self.ducks.is_empty() {
            return;
        }
        let mut ended = Vec::new();
        for mut d in std::mem::take(&mut self.ducks) {
            d.targets.retain(|c| self.is_running(*c));
            if self.is_running_tree(d.by) && !d.targets.is_empty() {
                self.ducks.push(d);
            } else {
                ended.push(d);
            }
        }
        self.unduck(ended);
    }

    /// Brings the targets of ended ducks back over each duck's fade, to
    /// what the remaining ducks still ask for.
    fn unduck(&mut self, ended: Vec<Duck>) {
        for d in ended {
            let voices: Vec<(VoiceId, f32)> = self
                .active
                .iter()
                .filter(|a| d.targets.contains(&a.cue))
                .map(|a| (a.voice, self.duck_gain(a.cue)))
                .collect();
            for (v, gain) in voices {
                let c = self.decibel(Shape::S);
                self.send(|e| e.set_duck(v, gain, secs(d.fade), c));
            }
        }
    }

    /// The strongest lowering on cue `id` (linear; 1 = none).
    fn duck_gain(&self, id: CueId) -> f32 {
        self.ducks.iter().filter(|d| d.targets.contains(&id)).map(|d| d.gain).fold(1.0, f32::min)
    }

    /// Seconds a voice has played (pauses left out).
    fn elapsed(&self, a: &Active, now: Instant) -> f64 {
        let until = a.paused_at.unwrap_or(now);
        a.started.map_or(0.0, |s| until.saturating_duration_since(s).as_secs_f64())
    }

    /// Seconds (real time, at the rate of now) until a voice ends; `None`
    /// while a vamp or endless loop is open.
    fn remaining(&self, a: &Active, now: Instant) -> Option<f64> {
        let rate = self.engine.config().sample_rate as f64;
        let speed = (a.rate_now as f64).max(0.01);
        let range_frames = a.end_frame.saturating_sub(a.start_frame).max(1);
        let range = range_frames as f64 / rate / speed;
        let remaining = if a.devamped < a.open_loops {
            None
        } else if a.open_loops > 0 {
            // Devamped: the rest of the file from the current position.
            Some(a.end_frame.saturating_sub(a.frame) as f64 / rate / speed)
        } else if a.speed_moved {
            // The rate changed on the way: from the position, the passes
            // still to come, at the rate of now.
            let passes_left = match a.loop_mode {
                LoopMode::Times(n) => n.max(1).saturating_sub(a.pass.map_or(1, |p| p.pass)) as usize,
                _ => 0,
            };
            let left = a.end_frame.saturating_sub(a.frame) + passes_left * range_frames;
            let since = match (a.paused_at, a.frame_at) {
                (None, Some(at)) => now.saturating_duration_since(at).as_secs_f64(),
                _ => 0.0,
            };
            Some(left as f64 / rate / speed - since)
        } else {
            match a.loop_mode {
                LoopMode::Times(n) => Some(range * n.max(1) as f64 - self.elapsed(a, now)),
                _ => Some(range - self.elapsed(a, now)),
            }
        };
        remaining.map(|r| r.max(0.0))
    }

    /// The next cue of each running chain, with what it waits for.
    /// The cue GO `go` started first; `or` if it is no longer known.
    fn root(&self, go: u64, or: CueId) -> CueId {
        self.roots.iter().find(|(g, ..)| *g == go).map_or(or, |r| r.1)
    }

    fn following(&self, now: Instant) -> Vec<FollowState> {
        let root = |go: u64, after: CueId| self.root(go, after);
        let mut out: Vec<FollowState> = Vec::new();
        for t in self.timers.iter().filter(|t| t.kind == TimerKind::Follow) {
            out.push(FollowState {
                cue: t.cue,
                after: t.owner,
                root: root(t.go, t.owner),
                paused: t.paused.is_some(),
                remaining: Some(
                    t.paused.unwrap_or_else(|| t.at.saturating_duration_since(now)).as_secs_f64(),
                ),
            });
        }
        for a in &self.active {
            let state = |cue: CueId, remaining: Option<f64>| FollowState {
                cue,
                after: a.cue,
                root: root(a.go, a.cue),
                paused: a.paused_at.is_some(),
                remaining,
            };
            if let Some(next) = a.follow {
                out.push(state(next, self.remaining(a, now)));
            }
            if let Some((next, lead)) = a.near_end {
                out.push(state(next, self.remaining(a, now).map(|r| (r - lead).max(0.0))));
            }
            out.extend(
                a.on_exit.iter().map(|e| FollowState { root: root(e.go, e.devamp), ..state(e.next, None) }),
            );
        }
        for &(group, next, go) in &self.group_follows {
            // The group ends with the last of its voices.
            let inside = self.with_descendants(group);
            let voices: Vec<&Active> = self.active.iter().filter(|a| inside.contains(&a.cue)).collect();
            let remaining = voices
                .iter()
                .map(|a| self.remaining(a, now))
                .try_fold(0.0, |most: f64, r| Some(most.max(r?)));
            out.push(FollowState {
                cue: next,
                after: group,
                root: root(go, group),
                paused: !voices.is_empty() && voices.iter().all(|a| a.paused_at.is_some()),
                remaining,
            });
        }
        let mut seen = HashSet::new();
        out.retain(|f| seen.insert(f.cue));
        out
    }

    pub fn state(&self, now: Instant) -> State {
        let rate = self.engine.config().sample_rate as f64;
        let active = self
            .active
            .iter()
            .map(|a| {
                let elapsed = self.elapsed(a, now);
                let range = a.end_frame.saturating_sub(a.start_frame).max(1) as f64 / rate;
                let progress = a.frame.saturating_sub(a.start_frame) as f64 / (range * rate);
                ActiveState {
                    cue: a.cue,
                    elapsed,
                    remaining: self.remaining(a, now),
                    progress: progress.clamp(0.0, 1.0) as f32,
                    level_db: gain_to_db(a.gain),
                    pan: a.pan_now,
                    rate: a.rate_now * 100.0,
                    transpose: {
                        // The pitch beyond what the rate does to it.
                        let follows = !a.speed_set.over(a.speed).preserve_pitch;
                        let shift = if follows { a.pitch_now / a.rate_now.max(0.01) } else { a.pitch_now };
                        let semitones = 12.0 * shift.max(0.01).log2();
                        if semitones.abs() < 1e-3 { 0.0 } else { semitones }
                    },
                    looping: a.looping,
                    item: a.item.map(|item| item.state()),
                    has_next: a.item.is_some() && a.continues && (!a.stopping || a.paused_at.is_some()),
                    pass: a.pass.and_then(|p| {
                        let region = a.loops.get(p.index)?;
                        Some(PassState {
                            pass: p.pass,
                            count: region.repeats.map(|r| r.saturating_add(1)),
                            vamp: a.vamps.then_some(p.index + 1),
                        })
                    }),
                    paused: a.paused_at.is_some(),
                    loading: a.loading && a.started.is_none(),
                }
            })
            .collect();
        let waiting = self
            .timers
            .iter()
            .filter(|t| matches!(t.kind, TimerKind::Fire { .. }))
            .map(|t| WaitingState {
                cue: t.cue,
                remaining: t.paused.unwrap_or_else(|| t.at.saturating_duration_since(now)).as_secs_f64(),
                paused: t.paused.is_some(),
                root: self.root(t.go, t.cue),
            })
            .collect();
        let gaps = self
            .timers
            .iter()
            .filter_map(|t| match (t.kind, self.show.cue(t.cue).map(|c| &c.kind)) {
                (TimerKind::Item { step, .. }, Some(CueKind::Playlist(p))) => {
                    let count = p.items.len();
                    Some(GapState {
                        cue: t.cue,
                        remaining: t
                            .paused
                            .unwrap_or_else(|| t.at.saturating_duration_since(now))
                            .as_secs_f64(),
                        next: Item { step, file: step_file(self.orders.get(&t.cue), step, count)?, count }
                            .state(),
                        paused: t.paused.is_some(),
                    })
                }
                _ => None,
            })
            .collect();
        let fading = self
            .fading
            .iter()
            .map(|f| {
                let elapsed = f.paused_at.unwrap_or(now).saturating_duration_since(f.started).min(f.duration);
                FadingState {
                    cue: f.cue,
                    elapsed: elapsed.as_secs_f64(),
                    remaining: (f.duration - elapsed).as_secs_f64(),
                    progress: (elapsed.as_secs_f64() / f.duration.as_secs_f64().max(f64::MIN_POSITIVE))
                        as f32,
                    paused: f.paused_at.is_some(),
                }
            })
            .collect();
        let preview = self.preview.as_ref().map(|p| {
            let range = p.end_frame.saturating_sub(p.start_frame).max(1) as f64;
            PreviewState {
                cue: p.cue,
                playing: p.voice.is_some(),
                elapsed: p.started.map_or(0.0, |s| now.saturating_duration_since(s).as_secs_f64()),
                progress: (p.frame.saturating_sub(p.start_frame) as f64 / range).clamp(0.0, 1.0) as f32,
                position: p.frame as f64 / rate,
            }
        });
        State {
            playhead: self.playhead_cue(),
            active,
            waiting,
            fading,
            gaps,
            preview,
            go_rejected: self.go_rejected,
            any_paused: self.active.iter().any(|a| a.paused_at.is_some())
                || self.timers.iter().any(|t| t.paused.is_some())
                || self.fading.iter().any(|f| f.paused_at.is_some()),
            repeat: self.playhead.and_then(|i| match self.show.cues[i].passes {
                Repeat::Off => None,
                Repeat::Times(n) => Some(RepeatState { done: self.repeat_done, count: Some(n) }),
                Repeat::Forever => Some(RepeatState { done: self.repeat_done, count: None }),
            }),
            following: self.following(now),
            stopwatches: self
                .show
                .stopwatches
                .iter()
                .map(|w| {
                    let watch = self.stopwatches.get(&w.id).copied().unwrap_or_default();
                    let elapsed = watch.elapsed(now).as_secs_f64();
                    StopwatchState {
                        id: w.id.clone(),
                        elapsed,
                        remaining: w.countdown.map(|from| from - elapsed),
                        running: watch.since.is_some(),
                    }
                })
                .collect(),
        }
    }

    fn go(&mut self, now: Instant) {
        let lockout = secs(self.settings.go_lockout);
        if self.last_go.is_some_and(|last| now.saturating_duration_since(last) < lockout) {
            self.go_rejected = self.go_rejected.wrapping_add(1);
            return;
        }
        self.last_go = Some(now);
        // A panic after this GO fades again, also what it starts.
        self.panic_until = None;
        let Some(i) = self.playhead else { return };
        let cue = &self.show.cues[i];
        let (id, passes, restarts) = (cue.id, cue.passes, cue.second_trigger == SecondTrigger::Restart);
        if passes != Repeat::Off {
            // Only a GO that starts the cue anew is a pass; on the running
            // cue the others (fade, stop, pause, ignore) are its second
            // trigger, and the playhead stays. A cue fading out to a stop
            // starts anew.
            if !restarts && self.is_running_tree(id) && !self.is_fading_out(id) {
                self.second_trigger(id, now, 0);
                return;
            }
            self.repeat_done += 1;
            if passes == Repeat::Forever || matches!(passes, Repeat::Times(n) if self.repeat_done < n) {
                let go = self.new_go_for(id);
                self.trigger(id, now, true, go);
                self.arm();
                return;
            }
        }
        let go = self.new_go_for(id);
        self.go_on(i, now, go, true);
    }

    /// Starts top-level cue `index` as GO `go` does, with the playhead set
    /// beforehand to the cue after the chain this starts (so a go-to cue in
    /// the chain can move it elsewhere).
    fn go_on(&mut self, index: usize, now: Instant, go: u64, direct: bool) {
        let Some(index) = self.show.playable_from(index) else { return };
        // A chain runs on past scene markers.
        let mut last = index;
        while self.show.cues[last].advance.start_next
            && let Some(next) = self.show.playable_from(last + 1)
        {
            last = next;
        }
        // Not `set_playhead`: arming from there now would drop the files
        // about to start.
        self.move_playhead(last + 1);
        self.trigger(self.show.cues[index].id, now, direct, go);
        self.arm();
    }

    /// Starts a cue, honouring its pre-wait, as part of GO `go`. `direct`:
    /// by a GO or a direct start (hotkey, OSC, MIDI), not by a chain or a
    /// group.
    fn trigger(&mut self, id: CueId, now: Instant, direct: bool, go: u64) {
        self.trigger_from(id, now, direct, go, None);
    }

    /// Like [`Runner::trigger`]; a sound cue plays from `from` (seconds in
    /// the file) instead of its start position.
    fn trigger_from(&mut self, id: CueId, now: Instant, direct: bool, go: u64, from: Option<f64>) {
        // A GO on a cue fading out to a stop breaks the fade off and starts
        // the cue again from the top, whatever its second trigger.
        if direct && self.is_fading_out(id) {
            self.remove_now(&self.with_descendants(id));
        }
        let Some(cue) = self.show.cue(id) else { return };
        // A scene marker is only a heading: starting it does nothing.
        if cue.is_scene() {
            return;
        }
        // A cue runs at most once at a time; the running cue list can show it only once.
        if self.is_running_tree(id) {
            if direct {
                self.second_trigger(id, now, go);
                return;
            }
            self.notices.push(Notice::Warning(t!("runner.already_running", cue = label(cue))));
            // Like a broken cue, a blocked one must not break the chain; at
            // later points the running instance carries it on.
            if let Some((next, AdvanceAt::Start)) = self.advance_of(cue) {
                self.trigger(next, now, false, go);
            }
            return;
        }
        if cue.pre_wait > 0.0 {
            let kind = TimerKind::Fire { direct, from };
            self.add_timer(now, Timer::new(now + secs(cue.pre_wait), id, kind, id, go));
        } else {
            self.fire(id, now, go, direct, from);
        }
    }

    /// Sets timer `t`; one due at once keeps the go-to/start nesting.
    fn add_timer(&mut self, now: Instant, mut t: Timer) {
        if t.at <= now {
            t.depth = self.goto_depth;
        }
        self.timers.push(t);
    }

    /// The cue `cue` starts by itself, and when (see [`effective_at`]).
    fn advance_of(&self, cue: &Cue) -> Option<(CueId, AdvanceAt)> {
        if !cue.advance.start_next {
            return None;
        }
        Some((self.show.next_after(cue.id)?, effective_at(cue)))
    }

    /// Whether the cue is playing, waiting for its pre-wait or fading.
    fn is_running(&self, id: CueId) -> bool {
        self.active.iter().any(|a| a.cue == id)
            || self.fading.iter().any(|f| f.cue == id)
            || self.timers.iter().any(|t| t.cue == id && t.kind != TimerKind::Follow)
    }

    /// Running itself or (a group) anything inside it, or waiting to follow.
    fn is_running_tree(&self, id: CueId) -> bool {
        self.is_running(id)
            || self.group_follows.iter().any(|(g, ..)| *g == id)
            || self.show.cue(id).is_some_and(|c| c.descendants().into_iter().any(|d| self.is_running(d)))
    }

    /// Everything of the cue (a group: of its cues) is fading out to a stop
    /// (its fade out, FADE ALL, panic, an exclusive cue): voices only, all
    /// of them stopping, nothing left to start.
    fn is_fading_out(&self, id: CueId) -> bool {
        let ids = self.with_descendants(id);
        let mut voices = self.active.iter().filter(|a| ids.contains(&a.cue)).peekable();
        voices.peek().is_some()
            // A playlist item fading out for the next one is no end.
            && voices.all(|a| a.stopping && !a.continues)
            && !self.fading.iter().any(|f| ids.contains(&f.cue))
            && !self.timers.iter().any(|t| ids.contains(&t.cue) && t.kind != TimerKind::Follow)
            && !self.group_follows.iter().any(|(g, ..)| ids.contains(g))
    }

    /// The cue and everything inside it (a group's cues).
    fn with_descendants(&self, id: CueId) -> Vec<CueId> {
        std::iter::once(id).chain(self.show.cue(id).map(Cue::descendants).unwrap_or_default()).collect()
    }

    fn fire(&mut self, id: CueId, now: Instant, go: u64, direct: bool, from: Option<f64>) {
        self.firing += 1;
        self.fire_cue(id, now, go, direct, from);
        self.firing -= 1;
        // What this cue continues to is loaded while it plays. Not before
        // the whole group is fired: a later cue in it may not have set the
        // timer yet that keeps its successors armed.
        self.arm();
    }

    fn fire_cue(&mut self, id: CueId, now: Instant, go: u64, direct: bool, from: Option<f64>) {
        let Some(cue) = self.show.cue(id).cloned() else { return };
        let advance = self.advance_of(&cue);
        // The point the next cue waits for, if the cue's own mechanics measure it.
        let (end_next, near_end) = match advance {
            Some((next, AdvanceAt::End)) => (Some(next), None),
            Some((next, AdvanceAt::BeforeEnd { secs })) => (None, Some((next, secs.clamp(0.0, MAX_SECS)))),
            _ => (None, None),
        };
        // The cue is over at once (or broke): what waits for its end starts now.
        let mut ended = false;
        self.affect_others(&cue, go);

        match &cue.kind {
            CueKind::Playlist(_) => {
                self.begin_order(id);
                if !self.start_items_from(id, 0, Carry::default(), go, None) {
                    ended = true;
                }
            }
            CueKind::Group(g) => {
                match g.mode {
                    GroupMode::All => g.cues.iter().for_each(|c| self.trigger(c.id, now, false, go)),
                    GroupMode::Sequence => {
                        if let Some(first) = g.cues.first() {
                            self.trigger(first.id, now, false, go);
                        }
                    }
                }
                // The end waits until nothing in the group runs (see `tick`).
                if let Some(next) = end_next {
                    self.group_follows.push((id, next, go));
                }
            }
            CueKind::Sound(s) if s.is_placeholder() => {
                self.notices.push(Notice::Warning(t!("runner.placeholder", cue = label(&cue))));
                // Like a broken cue: the chain goes on.
                ended = true;
            }
            CueKind::Sound(s) => {
                let lead = near_end.map(|(_, lead)| lead);
                match self.start_sound(id, s, &label(&cue), None, SpeedSet::default(), go, lead, from, None) {
                    Ok(()) => {
                        if let Some(a) = self.active.last_mut() {
                            (a.follow, a.near_end) = (end_next, near_end);
                        }
                    }
                    Err(e) => {
                        self.notices.push(Notice::Error(t!(
                            "runner.cue_problem",
                            cue = label(&cue),
                            problem = e
                        )));
                        // A broken cue must not break the chain.
                        ended = true;
                    }
                }
            }
            CueKind::Volume(f) => {
                let hit = self.hit(id, &f.targets);
                let (duration, voices) = self.apply_volume(&hit, f, false);
                self.fading_for(id, now, duration, voices);
                self.follow_at_end(id, now, duration, end_next, near_end, go);
            }
            CueKind::Pan(p) => {
                let hit = self.hit(id, &p.targets);
                let voices = self.apply_pan(&hit, p.pan, false, secs(p.duration), p.curve);
                let duration = self.moving(&voices, p.duration);
                self.fading_for(id, now, duration, voices.into_iter().map(|v| (v, Held::Pan)).collect());
                self.follow_at_end(id, now, duration, end_next, near_end, go);
            }
            CueKind::Rate(r) => {
                let hit = self.hit(id, &r.targets);
                let set = SpeedSet { rate: r.rate, transpose: r.transpose };
                let voices = self.apply_rate(&hit, set, false, secs(r.duration), r.curve);
                let duration = self.moving(&voices, r.duration);
                self.fading_for(id, now, duration, voices.into_iter().map(|v| (v, Held::Speed)).collect());
                self.follow_at_end(id, now, duration, end_next, near_end, go);
            }
            CueKind::Reset(r) => {
                let hit = self.hit(id, &r.targets);
                let voices = self.apply_reset(&hit, secs(r.duration));
                let moved: Vec<VoiceId> = voices.iter().map(|(v, _)| *v).collect();
                let duration = self.moving(&moved, r.duration);
                self.fading_for(id, now, duration, voices);
                self.follow_at_end(id, now, duration, end_next, near_end, go);
            }
            CueKind::StartStop(c) => {
                let hit = self.hit(id, &c.targets);
                match c.action {
                    StartStopAction::Start => self.start_hit(&cue, &hit, c.from, now, go),
                    StartStopAction::Stop => self.stop_hit(&hit),
                }
                ended = true;
            }
            CueKind::Pause(c) => {
                let hit = self.hit(id, &c.targets);
                self.pause_hit(&hit, c.action, now);
                ended = true;
            }
            CueKind::Position(c) => {
                let hit = self.hit(id, &c.targets);
                let crossfade =
                    self.engine.frames(Duration::from_secs_f64(c.crossfade_ms.clamp(0.0, 1e4) / 1e3));
                self.apply_position(&hit, c.to, crossfade, now);
                ended = true;
            }
            CueKind::Stopwatch(w) => {
                match &w.watch {
                    None => self
                        .notices
                        .push(Notice::Warning(t!("runner.no_stopwatch_chosen", cue = label(&cue)))),
                    Some(watch) if !self.show.stopwatches.iter().any(|s| &s.id == watch) => {
                        self.notices.push(Notice::Error(t!("runner.stopwatch_missing", cue = label(&cue))));
                    }
                    Some(watch) => self.stopwatch(&watch.clone(), w.action, now),
                }
                ended = true;
            }
            CueKind::ExitLoop(d) => {
                let hit = self.hit(id, &d.targets);
                let ids = self.hit_cues(&hit);
                let voices = self.exit_loops(&ids, d.exit);
                match advance {
                    Some((next, AdvanceAt::LoopExit { of })) => {
                        // A cue no longer in the target counts as any target.
                        let of = of.filter(|of| ids.contains(of));
                        let exit = OnExit { devamp: id, next, all: d.exit.level == LoopLevel::All, go };
                        let mut waiting = false;
                        for a in self.active.iter_mut().filter(|a| voices.contains(&a.voice)) {
                            if of.is_none_or(|of| a.cue == of) {
                                a.on_exit.push(exit);
                                waiting = true;
                            }
                        }
                        // Nothing to leave: the chain goes on at once.
                        if !waiting {
                            self.trigger(next, now, false, go);
                        }
                    }
                    _ => ended = true,
                }
            }
            CueKind::Osc(o) => {
                if let Err(e) = self.send_osc(o) {
                    self.notices.push(Notice::Error(t!(
                        "runner.cue_problem",
                        cue = label(&cue),
                        problem = e
                    )));
                }
                ended = true;
            }
            CueKind::Midi(m) => {
                if let Err(e) = self.send_midi(m) {
                    self.notices.push(Notice::Error(t!(
                        "runner.cue_problem",
                        cue = label(&cue),
                        problem = e
                    )));
                }
                ended = true;
            }
            CueKind::Wait(w) => {
                let duration = secs(w.duration);
                if duration.is_zero() {
                    ended = true;
                } else {
                    self.fading_for(id, now, duration, Vec::new());
                    self.follow_at_end(id, now, duration, end_next, near_end, go);
                }
            }
            CueKind::GoTo(g) => {
                self.go_to(&cue, g, now, go, direct);
                ended = true;
            }
            // Never fired (see `trigger_from`).
            CueKind::Scene(_) => ended = true,
        }

        self.schedule_advance(&cue, advance, ended, now, go);
    }

    /// Shows a cue with a duration (volume, pan, reset, wait) as running
    /// for it; a pause holds the moves of `voices`.
    /// How long a pan, rate or reset cue of `duration` seconds runs: not at
    /// all if none of the voices it moves plays on (none at all, or all
    /// paused: the engine applies the end value to those at once).
    fn moving(&self, voices: &[VoiceId], duration: f64) -> Duration {
        let playing = self.active.iter().any(|a| a.paused_at.is_none() && voices.contains(&a.voice));
        if playing { secs(duration) } else { Duration::ZERO }
    }

    fn fading_for(&mut self, cue: CueId, now: Instant, duration: Duration, voices: Vec<(VoiceId, Held)>) {
        if !duration.is_zero() {
            self.fading.push(Fading { cue, started: now, duration, paused_at: None, voices });
        }
    }

    /// Starts the next cue together with this one, or sets a timer for the
    /// points measured from its start; `ended`: the cue is already over,
    /// so the points at its end are now too. The rest is up to the cue's
    /// own mechanics (see [`Runner::fire`]).
    fn schedule_advance(
        &mut self,
        cue: &Cue,
        advance: Option<(CueId, AdvanceAt)>,
        ended: bool,
        now: Instant,
        go: u64,
    ) {
        let Some((next, at)) = advance else { return };
        let delay = match at {
            AdvanceAt::Start => None,
            AdvanceAt::After { secs: after } => Some(after),
            AdvanceAt::AfterFadeIn => Some(match &cue.kind {
                CueKind::Sound(s) => s.fade_in,
                CueKind::Playlist(p) => p.fade_in,
                _ => 0.0,
            }),
            AdvanceAt::LoopExit { .. } => return,
            _ if ended => None,
            _ => return,
        };
        match delay {
            // The cue is the timer's owner, so pausing it holds the time.
            Some(delay) => {
                self.add_timer(now, Timer::new(now + secs(delay), next, TimerKind::Follow, cue.id, go))
            }
            None => self.trigger(next, now, false, go),
        }
    }

    /// The next cue of a fade or wait cue, at its end or `near_end`
    /// seconds before.
    fn follow_at_end(
        &mut self,
        id: CueId,
        now: Instant,
        duration: Duration,
        end_next: Option<CueId>,
        near_end: Option<(CueId, f64)>,
        go: u64,
    ) {
        let (next, lead) = match (end_next, near_end) {
            (Some(next), _) => (next, Duration::ZERO),
            (_, Some((next, lead))) => (next, secs(lead)),
            _ => return,
        };
        self.add_timer(now, Timer::new(now + duration.saturating_sub(lead), next, TimerKind::Follow, id, go));
    }

    /// Moves the playhead to a go-to cue's target, or starts the target as
    /// a GO on it would (with this GO, so an exclusive target spares the
    /// chain that led here).
    fn go_to(&mut self, cue: &Cue, g: &GoToCue, now: Instant, go: u64, direct: bool) {
        let Some(target) = g.target else {
            self.notices.push(Notice::Warning(t!("runner.no_target", cue = label(cue))));
            return;
        };
        let Some(index) = self.show.index_of(target) else {
            // Only top-level cues can be jumped to; a cue inside a group is not.
            let why = match self.show.cue(target) {
                Some(inner) => t!("runner.target_in_group", target = label(inner)),
                None => t!("runner.target_missing"),
            };
            self.notices.push(Notice::Error(t!("runner.cue_problem", cue = label(cue), problem = why)));
            return;
        };
        if !g.start {
            self.move_playhead(index);
            return;
        }
        if self.goto_depth >= GOTO_DEPTH {
            self.notices.push(Notice::Error(t!("runner.goto_loop", cue = label(cue))));
            return;
        }
        self.goto_depth += 1;
        self.go_on(index, now, go, direct);
        self.goto_depth -= 1;
    }

    /// Applies a starting cue's effect on the others: the cues running from
    /// another GO, outside its own tree (on its outputs only, if so set).
    /// The show's setting stands in only for top-level cues; a group's
    /// cues act on the others by a setting of their own.
    fn affect_others(&mut self, cue: &Cue, go: u64) {
        let top = self.show.index_of(cue.id).is_some();
        let Some(effect) = cue.others.or(top.then_some(self.show.others)) else { return };
        if effect.effect == OthersAction::None || cue.media_files().is_empty() {
            return;
        }
        let own = self.with_descendants(cue.id);
        let outputs: Vec<String> = own.iter().filter_map(|id| self.output_of(*id)).collect();
        let hit = |output: &str, other_go: u64, other: CueId| {
            other_go != go
                && !own.contains(&other)
                && (effect.scope == OthersScope::All || outputs.iter().any(|o| o == output))
        };
        match effect.effect {
            OthersAction::None => {}
            OthersAction::FadeStop => {
                let (voices, cues): (Vec<VoiceId>, Vec<CueId>) = self
                    .active
                    .iter_mut()
                    .filter(|a| !a.excluded && hit(&a.output, a.go, a.cue))
                    .map(|a| {
                        a.excluded = true;
                        (cancel_follow(a), a.cue)
                    })
                    .unzip();
                // Their chains end with them, post-waits included.
                self.timers.retain(|t| t.kind != TimerKind::Follow || !cues.contains(&t.owner));
                // Playlists waiting in a gap stop as well.
                let show = &self.show;
                self.timers.retain(|t| {
                    let output = match show.cue(t.cue).map(|c| &c.kind) {
                        Some(CueKind::Playlist(p)) => &p.output,
                        _ => return true,
                    };
                    !matches!(t.kind, TimerKind::Item { .. }) || !hit(output, t.go, t.cue)
                });
                for v in voices {
                    let c = self.decibel(Shape::SlowTop);
                    self.send(|e| e.fade_out(v, secs(effect.fade), c));
                }
            }
            OthersAction::Duck => {
                let mut targets: Vec<CueId> =
                    self.active.iter().filter(|a| hit(&a.output, a.go, a.cue)).map(|a| a.cue).collect();
                targets.sort_unstable();
                targets.dedup();
                if targets.is_empty() {
                    return;
                }
                self.ducks.push(Duck {
                    by: cue.id,
                    gain: db_to_gain(effect.duck_db.min(0.0) as f32),
                    fade: effect.fade,
                    targets,
                });
                let voices: Vec<(VoiceId, f32)> = self
                    .active
                    .iter()
                    .map(|a| (a.voice, self.duck_gain(a.cue)))
                    .filter(|(_, g)| *g < 1.0)
                    .collect();
                for (v, gain) in voices {
                    let c = self.decibel(Shape::S);
                    self.send(|e| e.set_duck(v, gain, secs(effect.fade), c));
                }
            }
        }
    }

    /// The output a sound or playlist cue plays on.
    fn output_of(&self, id: CueId) -> Option<String> {
        match &self.show.cue(id)?.kind {
            CueKind::Sound(s) => Some(s.output.clone()),
            CueKind::Playlist(p) => Some(p.output.clone()),
            _ => None,
        }
    }

    /// Plays a sound cue (or playlist item) as part of GO `go`, at `volume`
    /// and with `speed_set` (a rate cue's, for the next playlist item)
    /// instead of its own if given, from `from` (seconds in the file, within
    /// start–end) instead of its start; the engine reports when `near_end`
    /// seconds (real time) are left.
    #[allow(clippy::too_many_arguments)]
    fn start_sound(
        &mut self,
        id: CueId,
        s: &SoundCue,
        name: &str,
        volume: Option<f64>,
        speed_set: SpeedSet,
        go: u64,
        near_end: Option<f64>,
        from: Option<f64>,
        paused: Option<Instant>,
    ) -> Result<(), String> {
        let path = jq_show::paths::resolve(&self.media_dir, &s.file);
        if self.importing.contains(&path) {
            return Err(t!("runner.still_copying"));
        }
        let (buffer, loaded_now) =
            self.cache.get_now(&path).map_err(|e| t!("runner.file_not_loadable", error = e))?;
        // Cues started directly (hotkey, OSC) cannot all be armed.
        if loaded_now && !self.is_direct_go(go) {
            self.notices.push(Notice::Warning(t!("runner.not_preloaded", cue = name)));
        }

        let rate = self.engine.config().sample_rate as f64;
        let frames = |secs: f64| (secs.max(0.0) * rate).round() as usize;
        let start = frames(s.start);
        let end = s.end.map_or(buffer.frames(), frames).min(buffer.frames());
        let loops = loop_regions(s, start, end, frames);
        let open_loops = loops.iter().filter(|l| l.repeats.is_none()).count();
        let kept_loops = loops.clone();
        let mut marks: Vec<usize> = s.marks.iter().map(|&m| frames(m)).collect();
        marks.sort_unstable();
        // Vamps before it count as passed.
        let play_from = from.map_or(start, |f| frames(f).clamp(start, end));
        let passed = loops.iter().filter(|l| l.repeats.is_none() && l.end <= play_from).count();
        let mut p = VoiceParams::new(buffer);
        p.start = play_from;
        p.end = Some(end);
        p.loops = loops.into();
        p.marks = marks.into();
        let normalize_db = s.normalize_db.unwrap_or(0.0);
        let base_db = self.level_db(s.volume_db, normalize_db);
        p.gain = db_to_gain(volume.map_or(base_db, |v| self.level_db(v, normalize_db)) as f32);
        p.fade_in_frames = self.engine.frames(secs(s.fade_in));
        p.fade_curve = self.curve(s.curve);
        p.end_fade_frames = if s.end_fade { self.engine.frames(secs(s.fade_out)) } else { 0 };
        p.end_blend_frames = self.engine.frames(END_BLEND);
        p.pan = s.pan.clamp(-1.0, 1.0) as f32;
        p.loop_crossfade_frames = self.engine.frames(LOOP_CROSSFADE);
        p.output_channel = self.settings.output_channel(&s.output);
        p.notify_before_end = near_end.map(frames);
        p.wait_frames = self.engine.frames(START_WAIT) as usize;
        let speed = speed_set.over(s.speed);
        let (rate_factor, pitch_factor) = speed.factors();
        (p.rate, p.pitch) = (rate_factor as f32, pitch_factor as f32);
        p.paused = paused.is_some();
        let loading = !ready_at(&p.buffer, play_from);

        let voice = self.engine.play(p).map_err(|e| e.to_string())?;
        // The next item of a ducked playlist starts ducked.
        let duck = self.duck_gain(id);
        if duck < 1.0 {
            let c = self.decibel(Shape::S);
            self.send(|e| e.set_duck(voice, duck, Duration::ZERO, c));
        }
        self.active.push(Active {
            cue: id,
            voice,
            output: s.output.clone(),
            fade_out: s.fade_out,
            start_frame: start,
            end_frame: end,
            loop_mode: s.loop_mode,
            loops: kept_loops,
            open_loops,
            devamped: passed,
            started: None,
            loading,
            frame: play_from,
            gain: 0.0,
            // A loop over the whole range loops from the first frame; vamps
            // report it once playback reaches them.
            looping: s.vamps.is_empty() && s.loop_mode.repeats() != Some(0),
            pass: None,
            vamps: !s.vamps.is_empty(),
            follow: None,
            near_end: None,
            on_exit: Vec::new(),
            item: None,
            continues: false,
            level: None,
            pan_set: None,
            pan_now: s.pan.clamp(-1.0, 1.0) as f32,
            speed_set,
            speed: s.speed,
            rate_now: rate_factor as f32,
            pitch_now: pitch_factor as f32,
            speed_moved: false,
            frame_at: None,
            stopping: false,
            normalize_db,
            base_db,
            trim_db: 0.0,
            pan: s.pan,
            go,
            paused_at: paused,
            excluded: false,
        });
        Ok(())
    }

    /// After a playlist item: step `step` (after the gap), the next pass's
    /// first when repeating; past the last item the playlist is done.
    fn next_item(&mut self, id: CueId, step: usize, carry: Carry, now: Instant, go: u64) {
        let Some(CueKind::Playlist(p)) = self.show.cue(id).map(|c| &c.kind) else { return };
        if step >= p.items.len() && !p.repeat {
            return;
        }
        if p.gap > 0.0 {
            let at = now + secs(p.gap);
            self.add_timer(now, Timer::new(at, id, TimerKind::Item { step, carry }, id, go));
        } else {
            self.next_items_from(id, step, carry, go, None, now);
        }
    }

    /// [`Action::NextItem`]: fades out the item playlist `id` plays (its
    /// end starts the next one after the gap), or ends its gap now. Paused,
    /// the next item starts at once, paused at its start.
    fn skip_item(&mut self, id: CueId, now: Instant) {
        let gap = self.timers.iter().position(|t| t.cue == id && matches!(t.kind, TimerKind::Item { .. }));
        if let Some(i) = gap
            && let TimerKind::Item { step, carry } = self.timers[i].kind
        {
            let t = self.timers.remove(i);
            self.next_items_from(id, step, carry, t.go, t.paused.map(|_| now), now);
            return;
        }
        let Some(i) = self.active.iter().position(|a| {
            a.cue == id && a.item.is_some() && a.continues && (!a.stopping || a.paused_at.is_some())
        }) else {
            return;
        };
        if self.active[i].paused_at.is_some() {
            // Its voice is silent already: it ends at once, and so does the gap.
            let a = self.active.remove(i);
            self.send(|e| e.stop(a.voice));
            let Some(item) = a.item else { return };
            let carry = Carry { level: a.level, pan: a.pan_set, speed: a.speed_set };
            self.next_items_from(id, item.step + 1, carry, a.go, Some(now), now);
            return;
        }
        let a = &mut self.active[i];
        // Stopping, so that a reset or "fade to" leaves the fade alone.
        a.stopping = true;
        let (voice, fade) = (a.voice, a.fade_out);
        if fade > 0.0 {
            let c = self.decibel(Shape::SlowTop);
            self.send(|e| e.fade_out(voice, secs(fade), c));
        } else {
            self.send(|e| e.stop(voice));
        }
    }

    /// Makes the shuffled orders reproducible.
    #[cfg(test)]
    pub(crate) fn seed_shuffle(&mut self, seed: u64) {
        self.rng = fastrand::Rng::with_seed(seed);
        self.drawn.clear();
        self.arm();
    }

    /// A playlist starts: shuffled, in the order drawn for it (or a new one).
    fn begin_order(&mut self, id: CueId) {
        self.orders.remove(&id);
        let drawn = self.drawn.remove(&id);
        let Some(CueKind::Playlist(p)) = self.show.cue(id).map(|c| &c.kind) else { return };
        let order = drawn.filter(|order| order.fits(p)).or_else(|| Order::draw(p, &mut self.rng));
        if let Some(order) = order {
            self.orders.insert(id, order);
        }
    }

    /// The engine curve of a show curve.
    fn curve(&self, c: FadeCurve) -> Curve {
        match c {
            FadeCurve::Linear => Curve::Linear,
            FadeCurve::SCurve => Curve::SCurve,
            FadeCurve::Decibel => self.decibel(Shape::Linear),
            FadeCurve::DecibelS => self.decibel(Shape::S),
            FadeCurve::DecibelSlowTop => self.decibel(Shape::SlowTop),
            FadeCurve::DecibelSlowBottom => self.decibel(Shape::SlowBottom),
        }
    }

    /// In dB down to the show's minimum level, which is silence anyway.
    /// Fades without a curve of their own: FADE ALL, fading one cue and
    /// exclusive cues use the default (slow at the top), panic even steps
    /// (quiet soon), ducking S.
    fn decibel(&self, shape: Shape) -> Curve {
        Curve::decibel(self.show.level_limits.min_db as f32, shape)
    }

    /// The level a cue volume plays at with a normalization offset, within
    /// the show's limits; [`SILENCE_DB`] for silence.
    fn level_db(&self, volume_db: f64, normalize_db: f64) -> f64 {
        self.show.level_limits.level(volume_db, normalize_db).unwrap_or(SILENCE_DB)
    }

    /// Goes on with a playlist's items from step `from` on, as
    /// [`Runner::start_items_from`]; if none can start, the playlist is over
    /// and its next cue waiting for the end starts, which a later item
    /// would have carried (a broken cue must not break the chain).
    fn next_items_from(
        &mut self,
        id: CueId,
        from: usize,
        carry: Carry,
        go: u64,
        paused: Option<Instant>,
        now: Instant,
    ) {
        if self.start_items_from(id, from, carry, go, paused) {
            return;
        }
        let advance = self.show.cue(id).and_then(|cue| self.advance_of(cue));
        if let Some((next, AdvanceAt::End | AdvanceAt::BeforeEnd { .. })) = advance {
            self.trigger(next, now, false, go);
        }
    }

    /// Starts the first playable item from step `from` on; a broken file is
    /// reported and skipped. Returns false if none could start. `paused`:
    /// it starts paused (since then), silent at its start.
    fn start_items_from(
        &mut self,
        id: CueId,
        from: usize,
        carry: Carry,
        go: u64,
        paused: Option<Instant>,
    ) -> bool {
        let Some(cue) = self.show.cue(id).cloned() else { return false };
        let CueKind::Playlist(p) = &cue.kind else { return false };
        let count = p.items.len();
        // Far enough for the items tried here and the next one armed.
        if let Some(order) = self.orders.get_mut(&id) {
            order.reach(from + 2 * count, &mut self.rng);
        }
        let tries = if p.repeat { count } else { count.saturating_sub(from) };
        for step in from..from + tries {
            let Some(file) = step_file(self.orders.get(&id), step, count) else { break };
            let Some(mut item) = p.item(file) else { continue };
            // The item's gain plays like a normalization offset, below the
            // playlist's volume: it stays through fades and volume edits.
            let (offset_db, own_pan) = item_offset_pan(p, file);
            (item.volume_db, item.normalize_db) = (p.volume_db, Some(offset_db));
            item.pan = carry.pan.unwrap_or(own_pan);
            // The points at the end wait for the playlist's last item.
            let last = step + 1 == count && !p.repeat;
            let advance = self.advance_of(&cue).filter(|_| last);
            let near_end = match advance {
                Some((next, AdvanceAt::BeforeEnd { secs })) => Some((next, secs.clamp(0.0, MAX_SECS))),
                _ => None,
            };
            let lead = near_end.map(|(_, lead)| lead);
            match self.start_sound(id, &item, &label(&cue), carry.level, carry.speed, go, lead, None, paused)
            {
                Ok(()) => {
                    if let Some(a) = self.active.last_mut() {
                        (a.item, a.continues) = (Some(Item { step, file, count }), !last);
                        // Live edits compare against the item's own pan.
                        (a.level, a.pan_set, a.pan) = (carry.level, carry.pan, own_pan);
                        a.near_end = near_end;
                        if let Some((next, AdvanceAt::End)) = advance {
                            a.follow = Some(next);
                        }
                    }
                    self.arm();
                    return true;
                }
                Err(e) => self.notices.push(Notice::Error(t!(
                    "runner.item_error",
                    cue = label(&cue),
                    item = file + 1,
                    error = e
                ))),
            }
        }
        false
    }

    /// What the targets of control cue `id` resolve to.
    fn hit(&self, id: CueId, targets: &Targets) -> Hit {
        let chosen = match targets {
            Targets::Previous(_) => self.show.previous_control_target_before(id).into_iter().collect(),
            Targets::Cues(ids) => ids.clone(),
            Targets::All(_) => return Hit::All,
            Targets::Output { output } => return Hit::Output(output.clone()),
        };
        self.cues_hit(chosen)
    }

    /// One cue and everything inside it.
    fn cue_hit(&self, id: CueId) -> Hit {
        self.cues_hit(vec![id])
    }

    fn cues_hit(&self, targets: Vec<CueId>) -> Hit {
        // A group as target means everything inside it.
        let ids = targets.iter().flat_map(|id| self.with_descendants(*id)).collect();
        Hit::Cues { targets, ids }
    }

    /// The cues a hit means: the chosen ones with what is inside them, or
    /// the running cues on all outputs or one.
    fn hit_cues(&self, hit: &Hit) -> Vec<CueId> {
        match hit {
            Hit::Cues { ids, .. } => ids.clone(),
            _ => {
                let mut ids: Vec<CueId> =
                    self.active.iter().filter(|a| hit.hits(a.cue, &a.output)).map(|a| a.cue).collect();
                ids.dedup();
                ids
            }
        }
    }

    /// Ends what the hit cues have not started yet (pre-waits, a group's
    /// later cues, playlist gaps), like a stopped cue.
    fn stop_pending(&mut self, hit: &Hit) {
        match hit {
            Hit::Cues { ids, .. } => {
                self.timers.retain(|t| !ids.contains(&t.cue) && !ids.contains(&t.owner));
                self.group_follows.retain(|(g, ..)| !ids.contains(g));
            }
            _ => {
                // Playlists waiting in a gap stop as well.
                let show = &self.show;
                self.timers.retain(|t| {
                    let output = match show.cue(t.cue).map(|c| &c.kind) {
                        Some(CueKind::Playlist(p)) => &p.output,
                        _ => return true,
                    };
                    !matches!(t.kind, TimerKind::Item { .. }) || !hit.hits(t.cue, output)
                });
            }
        }
    }

    /// Their chains end with the voices of these cues, post-waits included.
    fn end_chains(&mut self, voices: &[VoiceId]) {
        let cues: Vec<CueId> =
            self.active.iter().filter(|a| voices.contains(&a.voice)).map(|a| a.cue).collect();
        self.timers.retain(|t| t.kind != TimerKind::Follow || !cues.contains(&t.owner));
    }

    /// Stops the hit cues at once (with the short stop blend), and what they
    /// would still start; all: like STOP ALL.
    fn stop_hit(&mut self, hit: &Hit) {
        if let Hit::All = hit {
            self.cancel_all();
            self.send(|e| e.stop_all());
            return;
        }
        self.stop_pending(hit);
        let voices: Vec<VoiceId> = self
            .active
            .iter_mut()
            .filter(|a| hit.hits(a.cue, &a.output))
            .map(|a| {
                a.stopping = true;
                cancel_follow(a)
            })
            .collect();
        self.end_chains(&voices);
        for v in voices {
            self.send(|e| e.stop(v));
        }
    }

    /// Starts the chosen cues as part of GO `go` (a sound cue from `from`,
    /// seconds in the file), without moving the playhead; paused ones go
    /// on. A running cue is no second trigger: it plays on, with a warning.
    fn start_hit(&mut self, cue: &Cue, hit: &Hit, from: Option<f64>, now: Instant, go: u64) {
        let Hit::Cues { targets, .. } = hit else { return };
        for &target in targets {
            if !self.is_running_tree(target) {
                // A start cue starting itself (or one that starts it) would never end.
                if self.goto_depth >= GOTO_DEPTH {
                    self.notices.push(Notice::Error(t!("runner.start_loop", cue = label(cue))));
                    return;
                }
                self.goto_depth += 1;
                self.trigger_from(target, now, false, go, from);
                self.goto_depth -= 1;
                continue;
            }
            let ids = self.with_descendants(target);
            if self.is_paused(&ids) && !self.is_playing(Some(&ids)) {
                self.set_paused(Some(&ids), false, now);
            } else if let Some(cue) = self.show.cue(target) {
                self.notices.push(Notice::Warning(t!("runner.already_running", cue = label(cue))));
            }
        }
    }

    /// True if anything of the cues `ids` is paused.
    fn is_paused(&self, ids: &[CueId]) -> bool {
        self.active.iter().any(|a| ids.contains(&a.cue) && a.paused_at.is_some())
            || self.timers.iter().any(|t| ids.contains(&t.owner) && t.paused.is_some())
            || self.fading.iter().any(|f| ids.contains(&f.cue) && f.paused_at.is_some())
    }

    fn pause_hit(&mut self, hit: &Hit, action: PauseAction, now: Instant) {
        let ids: Option<Vec<CueId>> = match hit {
            Hit::All => None,
            Hit::Cues { ids, .. } => Some(ids.clone()),
            Hit::Output(_) => {
                let mut ids = self.hit_cues(hit);
                // Playlists waiting in a gap on the output too.
                let show = &self.show;
                ids.extend(self.timers.iter().filter_map(|t| match show.cue(t.cue).map(|c| &c.kind) {
                    Some(CueKind::Playlist(p)) if hit.hits(t.cue, &p.output) => Some(t.cue),
                    _ => None,
                }));
                Some(ids)
            }
        };
        let pause = match action {
            PauseAction::Pause => true,
            PauseAction::Resume => false,
            PauseAction::Toggle => self.is_playing(ids.as_deref()),
        };
        self.set_paused(ids.as_deref(), pause, now);
    }

    /// Applies a volume cue (`relative`: its level is added to each target's
    /// current one) and returns how long it takes and the voices it fades.
    /// No running target counts as success.
    fn apply_volume(&mut self, hit: &Hit, f: &VolumeCue, relative: bool) -> (Duration, Vec<(VoiceId, Held)>) {
        let mut longest = Duration::ZERO;
        let stop = f.action == VolumeAction::FadeStop;
        if !stop {
            self.warn_stopping(hit, &t!("runner.what.fade_to_level"));
        }
        let show = &self.show;
        let targets: Vec<(VoiceId, f64, f64, f64, f64, bool)> = self
            .active
            .iter_mut()
            // "Fade to" would undo a fade to a stop.
            .filter(|a| hit.hits(a.cue, &a.output) && (stop || !(a.stopping || a.excluded)))
            // "Fade to" leaves the cue running, so what follows it stays.
            .map(|a| {
                let own = match show.cue(a.cue).map(|c| &c.kind) {
                    Some(CueKind::Sound(s)) => s.volume_db,
                    Some(CueKind::Playlist(p)) => p.volume_db,
                    _ => 0.0,
                };
                let current = a.level.unwrap_or(own);
                a.stopping |= stop;
                (
                    if stop { cancel_follow(a) } else { a.voice },
                    a.fade_out,
                    // A cue edited to silence has no trim to make up for (it
                    // would take +100 dB): its fade target plays once its
                    // volume is raised again, not that much over it.
                    if a.trim_db == SILENCE_DB - a.base_db { 0.0 } else { a.trim_db },
                    a.normalize_db,
                    current,
                    a.paused_at.is_some(),
                )
            })
            .collect();
        if stop {
            self.stop_pending(hit);
        }
        let voices: Vec<VoiceId> = targets.iter().map(|(voice, ..)| *voice).collect();
        if stop {
            self.end_chains(&voices);
        }
        for (voice, own_fade, trim_db, normalize_db, current, paused) in targets {
            let d = secs(f.duration.unwrap_or(own_fade));
            // The engine applies a fade to a paused voice at once.
            if !paused {
                longest = longest.max(d);
            }
            let c = self.curve(f.curve);
            match f.action {
                VolumeAction::FadeTo => {
                    // The target is absolute, so the envelope compensates the trim.
                    let volume = f.volume_db.unwrap_or(f64::NEG_INFINITY);
                    let volume = if relative { current + volume } else { volume };
                    let volume = volume.min(self.show.level_limits.max_db);
                    let level = self.show.level_limits.level(volume, normalize_db);
                    let gain = level.map_or(0.0, |db| db_to_gain((db - trim_db) as f32));
                    if let Some(a) = self.active.iter_mut().find(|a| a.voice == voice) {
                        a.level = Some(volume);
                    }
                    self.send(|e| e.ramp(voice, gain, d, c, false));
                }
                VolumeAction::FadeStop => self.send(|e| e.fade_out(voice, d, c)),
            }
        }
        (longest, voices.into_iter().map(|v| (v, Held::Gain)).collect())
    }

    /// Warns about chosen targets fading out to a stop, which `what` leaves
    /// alone (it would undo the stop); like starting a running cue. Targets
    /// "all" and an output take such cues in by nature and stay quiet.
    fn warn_stopping(&mut self, hit: &Hit, what: &str) {
        let Hit::Cues { ids, .. } = hit else { return };
        let mut fading: Vec<CueId> = Vec::new();
        for a in self.active.iter().filter(|a| (a.stopping || a.excluded) && ids.contains(&a.cue)) {
            if !fading.contains(&a.cue) {
                fading.push(a.cue);
            }
        }
        for id in fading {
            if let Some(cue) = self.show.cue(id) {
                let message = t!("runner.fading_no_effect", cue = label(cue), what = what);
                self.notices.push(Notice::Warning(message));
            }
        }
    }

    /// Moves the balance of the hit cues to `pan` over `d`; returns the
    /// voices it moves.
    fn apply_pan(
        &mut self,
        hit: &Hit,
        pan: f64,
        relative: bool,
        d: Duration,
        curve: PanCurve,
    ) -> Vec<VoiceId> {
        let pan = if pan.is_nan() { 0.0 } else { pan };
        let moves: Vec<(VoiceId, f64)> = self
            .active
            .iter_mut()
            .filter(|a| hit.hits(a.cue, &a.output))
            .map(|a| {
                let to = if relative { a.pan_set.unwrap_or(a.pan) + pan } else { pan };
                let to = to.clamp(-1.0, 1.0);
                a.pan_set = Some(to);
                (a.voice, to)
            })
            .collect();
        let curve = match curve {
            PanCurve::Linear => Curve::Linear,
            PanCurve::SCurve => Curve::SCurve,
        };
        // A jump would click.
        let d = d.max(PARAM_SMOOTHING);
        for &(v, to) in &moves {
            self.send(|e| e.pan_to(v, to as f32, d, curve));
        }
        moves.into_iter().map(|(v, _)| v).collect()
    }

    /// Moves rate and transposition of the hit cues (each `None` part stays;
    /// with `relative` the rate is added to each cue's current one) over
    /// `d`; whether the pitch follows the rate is each cue's own setting.
    /// Returns the voices it moves.
    fn apply_rate(
        &mut self,
        hit: &Hit,
        set: SpeedSet,
        relative: bool,
        d: Duration,
        curve: PanCurve,
    ) -> Vec<VoiceId> {
        let set = SpeedSet {
            rate: set.rate.filter(|r| !r.is_nan()),
            transpose: set.transpose.filter(|t| !t.is_nan()),
        };
        let moves: Vec<(VoiceId, f32, f32)> = self
            .active
            .iter_mut()
            .filter(|a| hit.hits(a.cue, &a.output))
            .map(|a| {
                let current = a.speed_set.over(a.speed).clamped().rate;
                let rate = set.rate.map(|r| if relative { current + r } else { r });
                a.speed_set = SpeedSet {
                    rate: rate.map(|r| r.clamp(RATE_MIN, RATE_MAX)).or(a.speed_set.rate),
                    transpose: set.transpose.or(a.speed_set.transpose),
                };
                a.speed_moved = true;
                let (rate, pitch) = a.speed_set.over(a.speed).factors();
                (a.voice, rate as f32, pitch as f32)
            })
            .collect();
        let curve = match curve {
            PanCurve::Linear => Curve::Linear,
            PanCurve::SCurve => Curve::SCurve,
        };
        // A jump would click.
        let d = d.max(PARAM_SMOOTHING);
        for &(v, rate, pitch) in &moves {
            self.send(|e| e.speed_to(v, rate, pitch, d, curve));
        }
        moves.into_iter().map(|(v, ..)| v).collect()
    }

    /// Brings the level and pan of the hit cues back to their cues' own
    /// over `d`; cues fading out to a stop are left alone. Returns the
    /// voices it moves.
    fn apply_reset(&mut self, hit: &Hit, d: Duration) -> Vec<(VoiceId, Held)> {
        self.warn_stopping(hit, &t!("runner.what.reset"));
        let show = &self.show;
        /// Where a voice goes back to: gain, pan, and rate and pitch if a
        /// rate cue moved them.
        type Back = (VoiceId, f32, f32, Option<(f32, f32)>);
        let targets: Vec<Back> = self
            .active
            .iter_mut()
            .filter(|a| !a.stopping && !a.excluded && hit.hits(a.cue, &a.output))
            .map(|a| {
                a.level = None;
                a.pan_set = None;
                let pan = match show.cue(a.cue).map(|c| &c.kind) {
                    Some(CueKind::Sound(s)) => s.pan,
                    Some(CueKind::Playlist(p)) => item_offset_pan(p, item_file(a)).1,
                    _ => 0.0,
                };
                // The envelope of the cue's own level (live edits are in the trim).
                let gain = if a.base_db <= SILENCE_DB { 0.0 } else { db_to_gain(a.base_db as f32) };
                // Rate and transposition back to the cue's own, if a rate cue moved them.
                let speed = (a.speed_set != SpeedSet::default()).then(|| {
                    a.speed_set = SpeedSet::default();
                    let (rate, pitch) = a.speed.factors();
                    (rate as f32, pitch as f32)
                });
                (a.voice, gain, pan.clamp(-1.0, 1.0) as f32, speed)
            })
            .collect();
        let d = d.max(PARAM_SMOOTHING);
        let c = self.decibel(Shape::S);
        let mut moved = Vec::new();
        for (v, gain, pan, speed) in targets {
            self.send(|e| e.ramp(v, gain, d, c, false));
            self.send(|e| e.pan_to(v, pan, d, Curve::SCurve));
            moved.extend([(v, Held::Gain), (v, Held::Pan)]);
            if let Some((rate, pitch)) = speed {
                self.send(|e| e.speed_to(v, rate, pitch, d, Curve::SCurve));
                moved.push((v, Held::Speed));
            }
        }
        moved
    }

    /// Jumps within the hit cues' voices; a playlist only relative, within
    /// its current item.
    fn apply_position(&mut self, hit: &Hit, to: PositionTo, crossfade: u32, now: Instant) {
        let rate = self.engine.config().sample_rate as f64;
        let frames = |secs: f64| (secs.clamp(0.0, MAX_SECS) * rate).round() as usize;
        let mut seeks = Vec::new();
        let mut missing = Vec::new();
        for a in self.active.iter().filter(|a| hit.hits(a.cue, &a.output)) {
            let marks: Vec<usize> = match self.show.cue(a.cue).map(|c| &c.kind) {
                Some(CueKind::Sound(s)) if a.item.is_none() => {
                    let mut marks: Vec<usize> = s.marks.iter().map(|&m| frames(m)).collect();
                    marks.sort_unstable();
                    marks
                }
                _ if matches!(to, PositionTo::Relative { .. }) => Vec::new(),
                _ => continue,
            };
            let target = match to {
                PositionTo::Absolute { secs } => Some(frames(secs)),
                PositionTo::Relative { secs } if secs >= 0.0 => Some(a.frame + frames(secs)),
                PositionTo::Relative { secs } => Some(a.frame.saturating_sub(frames(-secs))),
                PositionTo::Mark { index } => marks.get(index.wrapping_sub(1)).copied(),
                PositionTo::NextMark => marks.iter().copied().find(|&m| m > a.frame),
                PositionTo::PreviousMark => {
                    // Right after a mark, the one before it (like a player's "back").
                    let before = a.frame.saturating_sub(frames(PREVIOUS_MARK_GRACE));
                    marks.iter().copied().rev().find(|&m| m < before)
                }
            };
            match target {
                Some(t) => seeks.push((a.voice, t.clamp(a.start_frame, a.end_frame))),
                None => missing.push(a.cue),
            }
        }
        for cue in missing {
            if let Some(c) = self.show.cue(cue) {
                self.notices.push(Notice::Warning(t!("runner.mark_missing", cue = label(c))));
            }
        }
        self.seek_voices(&seeks, now, crossfade);
    }

    /// Moves a running cue to the frame `to` returns (given `by` seconds in
    /// frames).
    fn jump(&mut self, id: CueId, now: Instant, by: f64, to: impl Fn(&Active, usize) -> usize) {
        let rate = self.engine.config().sample_rate as f64;
        let step = (by.clamp(0.0, MAX_SECS) * rate).round() as usize;
        let seeks: Vec<(VoiceId, usize)> =
            self.active.iter().filter(|a| a.cue == id).map(|a| (a.voice, to(a, step))).collect();
        let crossfade = self.engine.frames(JUMP_CROSSFADE);
        self.seek_voices(&seeks, now, crossfade);
    }

    /// Moves voices to frames; vamps jumped over count as devamped, and a
    /// jump back before vamps left already holds there again.
    fn seek_voices(&mut self, seeks: &[(VoiceId, usize)], now: Instant, crossfade: u32) {
        let rate = self.engine.config().sample_rate as f64;
        let mut sends = Vec::new();
        for &(voice, to) in seeks {
            let Some(a) = self.active.iter_mut().find(|a| a.voice == voice) else { continue };
            let target = to.min(a.end_frame);
            // Elapsed time follows the jump (in real time at the cue's rate).
            let delta = (target as f64 - a.frame as f64) / rate / (a.rate_now as f64).max(0.01);
            if let Some(started) = a.started {
                a.started = Some(if delta >= 0.0 {
                    started.checked_sub(secs(delta)).unwrap_or(started)
                } else {
                    (started + secs(-delta)).min(a.paused_at.unwrap_or(now))
                });
            }
            let back = target < a.frame;
            let passed = a.loops.iter().filter(|l| l.repeats.is_none() && l.end <= target).count();
            a.devamped = if back { passed } else { a.devamped.max(passed) };
            a.frame = target;
            sends.push((a.voice, target, back));
        }
        for (v, frame, back) in sends {
            self.send(
                |e| if back { e.seek_rearming(v, frame, crossfade) } else { e.seek(v, frame, crossfade) },
            );
        }
    }

    /// Sends all messages of an OSC cue; stops at the first failure.
    fn send_osc(&self, cue: &jq_show::OscCue) -> Result<(), String> {
        let device = cue.device.as_deref().ok_or_else(|| t!("runner.no_osc_device"))?;
        let target = osc::device_target(&self.show, &self.settings, device)
            .ok_or_else(|| t!("runner.osc_device_missing", device = device))?;
        let out = self.osc_out.as_ref().ok_or_else(|| t!("runner.osc_unavailable"))?;
        for message in &cue.messages {
            out.send(&target, &osc::encode(message)?)?;
        }
        Ok(())
    }

    /// Sends all messages of a MIDI cue; stops at the first failure.
    fn send_midi(&mut self, cue: &jq_show::MidiCue) -> Result<(), String> {
        let out = self.midi_out.as_mut().ok_or_else(|| t!("runner.no_midi_output"))?;
        for message in &cue.messages {
            out.send(&midi::encode(message)?)?;
        }
        Ok(())
    }

    /// Plays a sound cue's file on the preview output from `at` seconds to
    /// `until` (default: the file's end), at the cue's level, to find
    /// positions in it (waveform editor). A preview of the same cue already
    /// playing this way to the same end moves there instead. Never touches
    /// the show.
    pub fn preview_at(&mut self, id: CueId, at: f64, until: Option<f64>) {
        let frame = self.frame_at(at);
        let until = until.map(|secs| self.frame_at(secs));
        if let Some(p) = self.preview.as_mut().filter(|p| p.cue == id && p.from.is_some() && p.until == until)
            && let Some(voice) = p.voice
            && self.cache.buffer(&p.path).is_some_and(|b| ready_at(b, frame))
        {
            (p.from, p.start_frame, p.frame) = (Some(frame), frame, frame);
            let crossfade = self.engine.frames(JUMP_CROSSFADE);
            let engine = self.preview_engine.as_mut().unwrap_or(&mut self.engine);
            if let Err(e) = engine.seek(voice, frame, crossfade) {
                self.notices.push(Notice::Error(t!("runner.preview_error", error = e)));
            }
            return;
        }
        self.stop_preview();
        self.start_preview(id, Some(frame), until);
    }

    /// Queues a sound cue for preview (from frame `from` to `until` or the
    /// file's end, or else its start–end); it starts once its file is loaded,
    /// so the show thread never waits for the disk.
    fn start_preview(&mut self, id: CueId, from: Option<usize>, until: Option<usize>) {
        let Some(CueKind::Sound(s)) = self.show.cue(id).map(|c| &c.kind) else { return };
        if s.is_placeholder() {
            self.notices.push(Notice::Warning(t!("runner.preview_placeholder")));
            return;
        }
        if !self.settings.preview_enabled() {
            self.notices.push(Notice::Warning(t!("runner.no_preview_output")));
            return;
        }
        let path = jq_show::paths::resolve(&self.media_dir, &s.file);
        let start = from.unwrap_or_else(|| self.frame_at(s.start));
        self.cache.request(&path);
        self.preview = Some(Preview {
            cue: id,
            path,
            voice: None,
            from,
            until,
            start_frame: start,
            end_frame: 0,
            frame: start,
            started: None,
        });
        self.hint_streams();
        self.start_preview_when_loaded();
    }

    fn start_preview_when_loaded(&mut self) {
        let Some(p) = self.preview.as_ref().filter(|p| p.voice.is_none()) else { return };
        if let Some(e) = self.cache.error(&p.path) {
            self.notices.push(Notice::Error(t!("runner.preview_not_loadable", error = e)));
            self.preview = None;
            return;
        }
        // A streamed file also needs the part played first, or it would
        // start with silence.
        if !self.cache.buffer(&p.path).is_some_and(|b| ready_at(b, p.start_frame)) {
            return;
        }
        let (Some(CueKind::Sound(s)), Ok((buffer, _))) =
            (self.show.cue(p.cue).map(|c| c.kind.clone()), self.cache.get_now(&p.path.clone()))
        else {
            self.preview = None;
            return;
        };
        let rate = self.engine.config().sample_rate as f64;
        let frames = |secs: f64| (secs.max(0.0) * rate).round() as usize;
        let start = p.start_frame.min(buffer.frames());
        let end = match p.from {
            Some(_) => p.until.map_or(buffer.frames(), |until| until.min(buffer.frames())).max(start),
            None => s.end.map_or(buffer.frames(), frames).min(buffer.frames()).max(start),
        };
        let mut params = VoiceParams::new(buffer);
        params.start = start;
        params.end = Some(end);
        params.gain = db_to_gain(self.level_db(s.volume_db, s.normalize_db.unwrap_or(0.0)) as f32);
        params.pan = s.pan.clamp(-1.0, 1.0) as f32;
        params.output_channel = self.settings.preview_first_channel();
        params.end_blend_frames = self.engine.frames(END_BLEND);
        // Heard as in the show: at the cue's rate and pitch.
        let (rate_factor, pitch_factor) = s.speed.factors();
        (params.rate, params.pitch) = (rate_factor as f32, pitch_factor as f32);
        let engine = self.preview_engine.as_mut().unwrap_or(&mut self.engine);
        match engine.play(params) {
            Ok(voice) => {
                if let Some(p) = self.preview.as_mut() {
                    (p.voice, p.start_frame, p.end_frame, p.frame) = (Some(voice), start, end, start);
                }
            }
            Err(e) => {
                self.notices.push(Notice::Error(t!("runner.preview_error", error = e)));
                self.preview = None;
            }
        }
    }

    /// Follows the preview voice's events (from either engine).
    fn preview_event(&mut self, event: &Event, now: Instant) {
        let Some(p) = self.preview.as_mut() else { return };
        match *event {
            Event::Started(id) if Some(id) == p.voice => p.started = Some(now),
            Event::Position { id, frame, .. } if Some(id) == p.voice => p.frame = frame,
            Event::Finished { id, .. } | Event::Rejected { id, .. } if Some(id) == p.voice => {
                self.preview = None
            }
            _ => {}
        }
    }

    fn stop_preview(&mut self) {
        let Some(voice) = self.preview.take().and_then(|p| p.voice) else { return };
        let engine = self.preview_engine.as_mut().unwrap_or(&mut self.engine);
        if let Err(e) = engine.stop(voice) {
            self.notices.push(Notice::Error(t!("runner.preview_error", error = e)));
        }
    }

    /// Leaves the next vamp (or all loops) of the running cues `ids`; none
    /// running counts as success. Devamp cues and second triggers share it.
    /// Returns the voices that leave a loop.
    fn exit_loops(&mut self, ids: &[CueId], exit: ExitLoop) -> Vec<VoiceId> {
        let (all, stop) = (exit.level == LoopLevel::All, exit.stop);
        let exit = match exit.mode {
            DevampMode::LoopEnd => LoopExit::AtLoopEnd,
            DevampMode::NextMark => {
                LoopExit::AtNextMark { crossfade_frames: self.engine.frames(LOOP_CROSSFADE) }
            }
            DevampMode::Immediate => {
                LoopExit::Immediate { crossfade_frames: self.engine.frames(DEVAMP_CROSSFADE) }
            }
        };
        let voices: Vec<VoiceId> = self
            .active
            .iter_mut()
            .filter(|a| ids.contains(&a.cue) && a.devamped < a.open_loops)
            .map(|a| {
                a.devamped = if all { a.open_loops } else { a.devamped + 1 };
                a.voice
            })
            .collect();
        for &v in &voices {
            self.send(|e| e.exit_loop(v, exit, all, stop));
        }
        voices
    }

    /// Clears pending pre-waits and auto-follows (STOP/FADE ALL).
    fn cancel_all(&mut self) {
        self.timers.clear();
        self.fading.clear();
        self.group_follows.clear();
        self.active.iter_mut().for_each(|a| {
            cancel_follow(a);
        });
    }

    fn set_playhead(&mut self, index: usize) {
        self.move_playhead(index);
        self.arm();
    }

    /// Without arming; every move (also onto the same cue) starts its
    /// Repeat count anew. A scene marker passes the playhead on to the
    /// first cue after it.
    fn move_playhead(&mut self, index: usize) {
        self.playhead = self.show.playable_from(index);
        self.repeat_done = 0;
    }

    /// Requests the next sound cues' files and drops buffers no longer needed.
    fn arm(&mut self) {
        if self.firing > 0 {
            return;
        }
        let (show, media_dir) = (&self.show, &self.media_dir);
        let rate = self.engine.config().sample_rate as f64;
        // File `index` of a cue (a sound cue has one, a playlist several),
        // with the frame it starts at.
        let file = |id: CueId, index: usize| {
            let cue = show.cue(id)?;
            let files = cue.media_files();
            let path =
                files.get(index % files.len().max(1)).map(|f| jq_show::paths::resolve(media_dir, f))?;
            let start = match &cue.kind {
                CueKind::Sound(s) => s.start,
                CueKind::Playlist(p) => {
                    p.items.get(index % p.items.len().max(1)).map_or(0.0, |i| i.overrides.start)
                }
                _ => 0.0,
            };
            let start = (start.max(0.0) * rate).round() as usize;
            Some((path, start))
        };
        // A running playlist's step `from` and, past items whose file failed
        // to load (missing, no audio), the next one after them: that is the
        // one that plays.
        let (cache, orders) = (&self.cache, &self.orders);
        let items_from = |id: CueId, from: usize| {
            let count = show.cue(id).map_or(0, |c| c.media_files().len());
            let mut out = Vec::new();
            for step in from..from + count {
                let Some(item) = step_file(orders.get(&id), step, count).and_then(|index| file(id, index))
                else {
                    break;
                };
                let failed = cache.error(&item.0).is_some();
                out.push(item);
                if !failed {
                    break;
                }
            }
            out
        };
        let mut wanted = Vec::new();
        // Cues whose (first) file is wanted: what the next GOs start.
        let mut cues = ahead(show, self.playhead_cue(), true);
        // Cues still to come from pre-waits, chains and playlists stay armed
        // even though the playhead has moved past them, together with the
        // cues they continue to in turn; a pause frees nothing.
        for t in &self.timers {
            match t.kind {
                TimerKind::Item { step, .. } => wanted.extend(items_from(t.cue, step)),
                TimerKind::Fire { .. } | TimerKind::Follow => cues.extend(ahead(show, Some(t.cue), false)),
            }
        }
        for a in &self.active {
            let waiting = a.follow.into_iter().chain(a.near_end.map(|(next, _)| next));
            for next in waiting.chain(a.on_exit.iter().map(|e| e.next)) {
                cues.extend(ahead(show, Some(next), false));
            }
            if let Some(item) = a.item {
                wanted.extend(items_from(a.cue, item.step + 1));
            }
        }
        for &(_, next, _) in &self.group_follows {
            cues.extend(ahead(show, Some(next), false));
        }
        // What start cues to come start, from where they start it.
        let starting: Vec<(CueId, Option<f64>)> = cues
            .iter()
            .filter_map(|id| match &show.cue(*id)?.kind {
                CueKind::StartStop(c) if c.action == StartStopAction::Start => {
                    let chosen: Vec<CueId> = match &c.targets {
                        Targets::Cues(ids) => ids.clone(),
                        Targets::Previous(_) => {
                            show.previous_control_target_before(*id).into_iter().collect()
                        }
                        _ => Vec::new(),
                    };
                    Some(chosen.into_iter().map(move |t| (t, c.from)))
                }
                _ => None,
            })
            .flatten()
            .collect();
        for (target, from) in starting {
            let started = ahead(show, Some(target), false);
            if let (Some(from), Some(CueKind::Sound(s))) = (from, show.cue(target).map(|c| &c.kind))
                && !s.is_placeholder()
            {
                let path = jq_show::paths::resolve(media_dir, &s.file);
                wanted.push((path, (from.clamp(s.start.max(0.0), MAX_SECS) * rate).round() as usize));
                cues.extend(started.into_iter().skip(1));
            } else {
                cues.extend(started);
            }
        }
        // Shuffled playlists to come are drawn the order they start in.
        self.drawn.retain(|id, _| cues.contains(id));
        for &id in &cues {
            match show.cue(id).map(|c| &c.kind) {
                Some(CueKind::Group(_)) => {}
                Some(CueKind::Playlist(p)) => {
                    let order = match self.drawn.get(&id).filter(|order| order.fits(p)) {
                        Some(order) => Some(order),
                        None => match Order::draw(p, &mut self.rng) {
                            Some(order) => Some(&*self.drawn.entry(id).insert_entry(order).into_mut()),
                            None => {
                                self.drawn.remove(&id);
                                None
                            }
                        },
                    };
                    wanted.extend(step_file(order, 0, p.items.len()).and_then(|index| file(id, index)));
                }
                _ => wanted.extend(file(id, 0)),
            }
        }
        let mut starts: HashMap<PathBuf, Vec<usize>> = HashMap::new();
        for (path, start) in wanted {
            let list = starts.entry(path).or_default();
            if !list.contains(&start) {
                list.push(start);
            }
        }
        for path in starts.keys().filter(|p| !self.importing.contains(*p)) {
            self.cache.request(path);
        }
        let mut keep: HashSet<PathBuf> = starts.keys().cloned().collect();
        if let Some(p) = &self.preview {
            keep.insert(p.path.clone());
        }
        for a in &self.active {
            keep.extend(file(a.cue, a.item.map_or(0, |item| item.file)).map(|(path, _)| path));
        }
        self.cache.retain(&keep);
        self.starts = starts;
        self.hint_streams();
    }

    fn send(&mut self, f: impl FnOnce(&mut Controller) -> Result<(), jq_engine::EngineError>) {
        if let Err(e) = f(&mut self.engine) {
            self.notices.push(Notice::Error(t!("runner.engine_error", error = e)));
        }
    }
}

/// Like [`AudioBuffer::is_ready_at`], but a frame past the end (nothing to
/// play) counts as ready too.
fn ready_at(buffer: &AudioBuffer, frame: usize) -> bool {
    frame >= buffer.frames() || buffer.is_ready_at(frame)
}

/// The cue's vamps in frames (sorted, inside start–end, overlaps dropped),
/// or else its loop over the whole range.
fn loop_regions(s: &SoundCue, start: usize, end: usize, frames: impl Fn(f64) -> usize) -> Vec<LoopRegion> {
    if s.vamps.is_empty() {
        return match s.loop_mode.repeats() {
            Some(0) => vec![],
            repeats => vec![LoopRegion { start, end, repeats }],
        };
    }
    let mut vamps: Vec<LoopRegion> = s
        .vamps
        .iter()
        .map(|v| LoopRegion { start: frames(v.start), end: frames(v.end).min(end), repeats: None })
        // A vamp that ends before the start position is never reached.
        .filter(|l| l.start < l.end && l.end > start)
        .collect();
    vamps.sort_by_key(|l| l.start);
    let mut kept: Vec<LoopRegion> = Vec::with_capacity(vamps.len());
    for l in vamps {
        if kept.last().is_none_or(|k| k.end <= l.start) {
            kept.push(l);
        }
    }
    kept
}

/// `id` and the cues that start after it in the order they start (a group
/// with everything inside it), up to [`ARM_AHEAD`] steps with media. One
/// step is what starts at once: cues chained "with the start" count as one.
/// With `list`, the walk goes the way the GOs do (on to the next top-level
/// cue after a chain, to a go-to cue's target), else it ends with the
/// chain. Cues seen already end it too (a go-to back to the top of a loop).
fn ahead(show: &Show, id: Option<CueId>, list: bool) -> Vec<CueId> {
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
    let (mut steps, mut media) = (0, false);
    let mut next = id;
    while let Some(id) = next
        && seen.insert(id)
        && let Some(cue) = show.cue(id)
    {
        out.push(id);
        out.extend(cue.descendants());
        media |= !cue.media_files().is_empty();
        let top_level = |target: Option<CueId>| target.filter(|t| show.index_of(*t).is_some());
        let (to, together) = match &cue.kind {
            CueKind::GoTo(g) if g.start || list => (top_level(g.target), g.start),
            _ if cue.advance.start_next => (show.next_after(id), effective_at(cue) == AdvanceAt::Start),
            _ if list => (show.index_of(id).and_then(|i| show.cues.get(i + 1)).map(|c| c.id), false),
            _ => (None, false),
        };
        if !together && media {
            (steps, media) = (steps + 1, false);
            if steps == ARM_AHEAD {
                break;
            }
        }
        next = to;
    }
    out
}

/// What `advance.at` means for the cue's type: a point it does not have is
/// read as the nearest one it has, so a show edited by hand breaks no chain.
fn effective_at(cue: &Cue) -> AdvanceAt {
    let at = cue.advance.at;
    let instant = matches!(
        cue.kind,
        CueKind::Osc(_)
            | CueKind::Midi(_)
            | CueKind::ExitLoop(_)
            | CueKind::GoTo(_)
            | CueKind::StartStop(_)
            | CueKind::Pause(_)
            | CueKind::Position(_)
            | CueKind::Stopwatch(_)
    );
    match (at, &cue.kind) {
        (AdvanceAt::Start | AdvanceAt::After { .. }, _) => at,
        (AdvanceAt::AfterFadeIn, CueKind::Sound(SoundCue { fade_in, .. }))
        | (AdvanceAt::AfterFadeIn, CueKind::Playlist(jq_show::PlaylistCue { fade_in, .. }))
            if *fade_in > 0.0 =>
        {
            at
        }
        (AdvanceAt::AfterFadeIn, _) => AdvanceAt::Start,
        (AdvanceAt::LoopExit { .. }, CueKind::ExitLoop(_)) => at,
        _ if instant => AdvanceAt::Start,
        (AdvanceAt::BeforeFadeOut, CueKind::Sound(s)) if s.end_fade && s.fade_out > 0.0 => {
            AdvanceAt::BeforeEnd { secs: s.fade_out }
        }
        (
            AdvanceAt::BeforeEnd { .. },
            CueKind::Sound(_)
            | CueKind::Playlist(_)
            | CueKind::Wait(_)
            | CueKind::Volume(_)
            | CueKind::Pan(_)
            | CueKind::Rate(_)
            | CueKind::Reset(_),
        ) => at,
        _ => AdvanceAt::End,
    }
}

/// The playlist file a voice plays (0 for a sound cue's).
fn item_file(a: &Active) -> usize {
    a.item.map_or(0, |item| item.file)
}

/// Offset below the playlist's volume (normalization plus the item's gain)
/// and pan of the playlist's item `file`.
fn item_offset_pan(p: &jq_show::PlaylistCue, file: usize) -> (f64, f64) {
    let Some(item) = p.items.get(file) else { return (0.0, p.pan) };
    let o = &item.overrides;
    (item.normalize_db.unwrap_or(0.0) + o.gain_db, o.pan.unwrap_or(p.pan))
}

/// Nothing follows the voice any more: neither the next cue nor the next
/// playlist item.
fn cancel_follow(a: &mut Active) -> VoiceId {
    a.follow = None;
    a.near_end = None;
    a.on_exit.clear();
    a.continues = false;
    a.voice
}

fn label(cue: &Cue) -> String {
    match (cue.number.is_empty(), cue.title.is_empty()) {
        (false, false) => t!("cue.label_both", number = cue.number, title = cue.title),
        (false, true) => cue.number.clone(),
        _ => t!("cue.label_title", title = cue.title),
    }
}

/// Seconds from the show or settings as a duration; negative and NaN are 0.
// Not `clamp`: it keeps NaN, which `from_secs_f64` panics on.
#[allow(clippy::manual_clamp)]
fn secs(s: f64) -> Duration {
    Duration::from_secs_f64(s.max(0.0).min(MAX_SECS))
}
