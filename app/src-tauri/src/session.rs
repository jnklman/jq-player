//! One open show: file, editor, runner, audio output and keep-alive.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jq_audio::{AudioError, AudioOutput, DeviceProbe, OutputInfo, OutputSpec};
use jq_core::keys::{self, ControlAction};
use jq_core::midi::{self, MidiDecoder, MidiSink};
use jq_core::osc::{self, OscListener, UdpOut};
use jq_core::{
    FileLoader, KeepAlive, KeepAliveState, LocalSettings, MediaLock, MediaWatcher, Notice, PeakCache,
    Prewarmer, Runner, SelectionMode, Stamps,
};
use jq_engine::Controller;
use jq_i18n::t;
use jq_media::loudness::Loudness;
use jq_midi::{MidiIn, MidiOut};
use jq_show::paths::{self, FileStamp, MediaIssue, NameCheck};
use jq_show::{
    Advance, AdvanceAt, Chord, ControlKind, Cue, CueId, CueKind, Editor, GoToCue, GroupCue, LevelChanges,
    LevelLimits, LoopMode, MidiCue, MidiMessage, NetDevice, OscCue, OscMessage, OthersEffect, Output,
    PlaylistCue, PositionTo, PreviousKeyword, Repeat, SceneCue, SecondTrigger, Show, ShowFile, SoundCue,
    StartStopAction, Stopwatch, Targets, TimingIssue, WaitCue, check_timing, check_values, new_numbers,
};
use serde::{Deserialize, Serialize};

use crate::backend::{CloseCheck, Request};
use crate::jobs::{self, Copier, Copy, Found, Prober};

const MAX_VOICES: usize = 64;
/// One preview at a time, plus room for one that is still stopping.
const PREVIEW_VOICES: usize = 2;
/// Quiet time after the last edit before the autosave is written.
const AUTOSAVE_DELAY: Duration = Duration::from_secs(1);
/// How often media files are checked for changes on disk when polling
/// ([`jq_core::WatchMode::Poll`]).
const WATCH_INTERVAL: Duration = Duration::from_secs(1);
/// How long the show must be quiet before peaks are built in the background.
const PREWARM_QUIET: Duration = Duration::from_secs(3);
/// Callback buffer sizes offered (frames, powers of two).
const MIN_BUFFER_FRAMES: u32 = 32;
const MAX_BUFFER_FRAMES: u32 = 4096;
/// Shortest time between two views sent for loudness measured.
const MEASURED_INTERVAL: Duration = Duration::from_secs(1);
/// True peak a normalized file stays at or below, dBTP.
const NORMALIZE_CEILING: f64 = -1.0;
/// A stored offset this far (dB) from the current suggestion is stale.
const STALE_OFFSET: f64 = 0.5;
/// Range of the loudness target, LUFS.
const LOUDNESS_TARGETS: std::ops::RangeInclusive<f64> = -40.0..=-6.0;
/// Range the level limits may be set in, dB.
const MAX_LEVEL_DB: f64 = 24.0;
const MIN_LEVEL_DB: f64 = -100.0;
/// Longest fade a cue's effect on others takes, s.
const MAX_FADE: f64 = 600.0;

/// Edits sent by the UI; each one is a single undo step.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    /// Replaces the cue with the same id.
    UpdateCue {
        cue: Box<Cue>,
    },
    /// Replaces several cues at once (inspector with a multi-selection).
    UpdateCues {
        cues: Vec<Cue>,
    },
    /// Adds sound cues for files at `index` (default: end), stored as
    /// `import` says.
    AddSounds {
        paths: Vec<PathBuf>,
        index: Option<usize>,
        #[serde(flatten)]
        import: Import,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    /// Adds a control cue (start/stop, pause, volume, pan, position, loop
    /// exit, reset, stopwatch); without targets it acts on the entry before it.
    AddControl {
        kind: ControlKind,
        index: Option<usize>,
        targets: Option<Vec<CueId>>,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    /// Turns control cues into another control type; targets, duration and
    /// curve stay where both types have them.
    SetCueType {
        ids: Vec<CueId>,
        kind: ControlKind,
    },
    /// Replaces the show's stopwatches.
    SetStopwatches {
        stopwatches: Vec<Stopwatch>,
    },
    AddOsc {
        index: Option<usize>,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    AddMidi {
        index: Option<usize>,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    /// Adds one playlist cue with these files (copied in like `AddSounds`).
    AddPlaylist {
        paths: Vec<PathBuf>,
        index: Option<usize>,
        #[serde(flatten)]
        import: Import,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    /// Appends files to a playlist cue.
    AddToPlaylist {
        id: CueId,
        paths: Vec<PathBuf>,
        #[serde(flatten)]
        import: Import,
    },
    /// Gives one sound cue (or one item of a playlist) another file (stored
    /// like `AddSounds`); all other settings stay.
    SetFile {
        id: CueId,
        path: PathBuf,
        /// The playlist item to replace; `None` for a sound cue.
        #[serde(default)]
        item: Option<usize>,
        /// Title the cue after the new file, as `AddSounds` does (sound cues).
        #[serde(default)]
        retitle: bool,
        #[serde(flatten)]
        import: Import,
    },
    /// Replaces every use of one stored media path with the selected file.
    RelinkFile {
        old_path: String,
        path: PathBuf,
        #[serde(flatten)]
        import: Import,
    },
    /// Replaces the show's network devices (targets of OSC cues).
    SetDevices {
        devices: Vec<NetDevice>,
    },
    /// Replaces the show's logical outputs (add, rename, remove, reorder).
    SetOutputs {
        outputs: Vec<Output>,
    },
    AddWait {
        index: Option<usize>,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    /// Adds a scene marker (a heading in the main list, without a number).
    AddScene {
        index: Option<usize>,
    },
    /// Adds a sound cue without a file (a placeholder, given one later with
    /// `SetFile`).
    AddPlaceholder {
        index: Option<usize>,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    /// Adds a go-to cue without a target yet.
    AddGoto {
        index: Option<usize>,

        /// Into this group instead of the main list.
        group: Option<CueId>,
    },
    AddGroup {
        index: Option<usize>,
    },
    /// What cues do to the others when they start, unless they have a
    /// setting of their own.
    SetOthers {
        others: OthersEffect,
    },
    /// Sets the normalization offsets of these cues (and a group's cues)
    /// from their files' measurements; files not measured yet are left out.
    Normalize {
        ids: Vec<CueId>,
    },
    /// Loudness target (LUFS) and whether new cues are normalized as soon as
    /// their files are measured. Offsets already set stay.
    SetNormalization {
        target: f64,
        on_import: bool,
    },
    /// Loudest and quietest cue level; volumes outside are brought in (see
    /// [`Show::clamp_levels`]) in the same undo step.
    SetLevelLimits {
        limits: LevelLimits,
    },
    /// Locks the show in show mode (entering it) or unlocks it. Allowed in
    /// show mode, unlike every other edit.
    LockShowMode {
        locked: bool,
    },
    Delete {
        ids: Vec<CueId>,
    },
    /// Moves `ids` as a block; `to` is the index among the cues that stay.
    Move {
        ids: Vec<CueId>,
        to: usize,
    },
    /// A `linked` copy is an instance of the original (not for groups).
    Duplicate {
        id: CueId,
        #[serde(default)]
        linked: bool,
    },
    /// Links `ids` with `source`: they take its settings and keep them in
    /// sync from then on (all but number, title, script reference, notes,
    /// hotkey, continue mode, second trigger and effect on others). Only
    /// cues of one type, no groups.
    LinkCues {
        source: CueId,
        ids: Vec<CueId>,
    },
    /// Takes cues out of their links; they keep their settings.
    UnlinkCues {
        ids: Vec<CueId>,
    },
    /// Puts copies of the cues on the clipboard; changes nothing in the show.
    Copy {
        ids: Vec<CueId>,
    },
    /// Copies the cues, then deletes them.
    Cut {
        ids: Vec<CueId>,
    },
    /// Inserts the clipboard after `after` (in its list, also a group's),
    /// or at the end of the main list. Copies get new ids and no hotkey;
    /// they keep their numbers if those are free (not inside a group).
    /// `linked` makes each copy an instance of its original, if that is
    /// still in the show.
    Paste {
        after: Option<CueId>,
        #[serde(default)]
        linked: bool,
    },
    /// Deletes `ids` and inserts the clipboard where the first of them was
    /// (in list order), as one undo step; copies as with [`EditOp::Paste`].
    /// `swap`: the deleted cues are on the clipboard afterwards (as by copy).
    PasteReplace {
        ids: Vec<CueId>,
        #[serde(default)]
        swap: bool,
    },
    /// Repairs missing files with same-named files found in `dir`; files
    /// outside the media folder are copied into it if `copy_outside` is set.
    Relink {
        dir: PathBuf,
        copy_outside: bool,
    },
    Renumber,
    /// Puts cues of the main list into a new group in sequence, at the place
    /// of the first (see [`jq_show::Editor::group`]).
    Group {
        ids: Vec<CueId>,
    },
    /// Dissolves the groups among `ids`: their cues take their place (see
    /// [`jq_show::Editor::ungroup`]).
    Ungroup {
        ids: Vec<CueId>,
    },
    Undo,
    Redo,
}

/// How picked files are stored.
#[derive(Debug, Default, Deserialize)]
pub struct Import {
    /// Copy files outside the media folder into it; otherwise they are
    /// linked by their absolute path.
    pub copy_outside: bool,
    /// For copied files whose name is taken by a different file, by picked
    /// path (see `check_import`). Without a choice the copy is renamed.
    #[serde(default)]
    pub on_clash: HashMap<PathBuf, OnClash>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnClash {
    /// Copy under the next free name (`donner-2.wav`).
    Rename,
    /// Link to the file that already has the name; nothing is copied.
    UseExisting,
    /// Leave the file out.
    Skip,
}

/// Files of a pick that need a question before they are added.
#[derive(Debug, Default, Serialize)]
pub struct ImportCheck {
    /// Picked paths outside the media folder.
    pub outside: Vec<PathBuf>,
    /// Of those, the ones whose name is taken when copied in.
    pub clashes: Vec<ImportClash>,
    /// Of those, the ones whose name holds the same file already (in the
    /// media folder or picked before); send them as `UseExisting`.
    pub identical: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct ImportClash {
    pub path: PathBuf,
    /// Stored path of the file that has the name.
    pub existing: String,
    /// Taken by an earlier file of the same pick (`UseExisting` makes no
    /// sense then), not by one in the media folder.
    pub picked: bool,
}

/// Which of `paths` lie outside `media_dir` and which of those clash when
/// copied in. Reads whole files to compare them, so it must not run on the
/// show thread.
pub fn check_import(media_dir: &Path, paths: &[PathBuf]) -> Result<ImportCheck, String> {
    let media_canonical = real_path(media_dir).unwrap_or(media_dir.to_path_buf());
    let mut check = ImportCheck::default();
    let mut canonical = Vec::new();
    for path in paths {
        let c = real_path(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if paths::relativize(&media_canonical, &c).is_none() {
            check.outside.push(path.clone());
            canonical.push(c);
        }
    }
    let names = paths::name_clashes(media_dir, &canonical).map_err(|e| e.to_string())?;
    for (path, name) in check.outside.iter().zip(names) {
        match name {
            NameCheck::Free => {}
            NameCheck::Identical => check.identical.push(path.clone()),
            NameCheck::Clash(c) => {
                check.clashes.push(ImportClash { path: path.clone(), existing: c.existing, picked: c.picked })
            }
        }
    }
    Ok(check)
}

/// Everything the UI needs to show the cue list; sent after every change.
#[derive(Debug, Serialize)]
pub struct ShowView<'a> {
    pub path: &'a Path,
    pub name: String,
    pub media_dir: PathBuf,
    pub media_dir_exists: bool,
    pub show: &'a Show,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The clipboard holds cues.
    pub can_paste: bool,
    pub audio: &'a OutputInfo,
    /// Frames per callback as the device runs it; `None` before the first.
    pub audio_frames: Option<u32>,
    pub audio_error: Option<String>,
    /// Why the open device stopped playing; cues run silently meanwhile and
    /// the device is opened again as soon as it can be.
    pub audio_lost: Option<String>,
    pub keep_alive: &'a KeepAliveState,
    pub settings: &'a LocalSettings,
    pub cue_info: HashMap<CueId, CueInfo>,
    /// Time (ms since 1970) of an autosave with changes lost in a crash;
    /// the UI offers to restore or discard it.
    pub recovery: Option<u64>,
    pub midi: MidiView<'a>,
    pub preview: PreviewView<'a>,
    /// Keys the app keeps for itself (see [`keys::fixed_keys`]).
    pub fixed_keys: Vec<Chord>,
    /// Global shortcuts of a new machine.
    pub default_keys: Vec<keys::KeyBinding>,
    pub osc: OscView<'a>,
}

#[derive(Debug, Serialize)]
pub struct OscView<'a> {
    /// UDP port incoming OSC is received on.
    pub listening: Option<u16>,
    pub error: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct PreviewView<'a> {
    /// Name of the separate preview device, if one is open.
    pub device: Option<&'a str>,
    pub error: Option<&'a str>,
    /// Like `ShowView::audio_lost`, for the preview device.
    pub lost: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MidiView<'a> {
    /// Name of the open input port.
    pub port: Option<&'a str>,
    pub error: Option<&'a str>,
    /// What waits for a MIDI control to be learned.
    pub learning: Option<LearnTarget>,
    /// Name of the open output port (for MIDI cues).
    pub out: Option<&'a str>,
    pub out_error: Option<&'a str>,
}

/// What the next MIDI control is learned for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LearnTarget {
    Action { action: ControlAction },
    Cue { cue: CueId },
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CueInfo {
    /// Length of the file in seconds.
    pub file_duration: Option<f64>,
    /// Playing time including loops, in real time at the cue's rate; `None`
    /// if unknown or endless.
    pub duration: Option<f64>,
    pub missing: bool,
    /// Per playlist item, in order: its file is missing.
    pub missing_items: Vec<bool>,
    /// Actual spelling if the file only matches ignoring case.
    pub case_mismatch: Option<String>,
    /// Counts changes of the file on disk, so views of it (the waveform)
    /// know to read it again.
    pub file_version: u32,
    /// Positions and fade times that do not fit the file.
    pub timing: Vec<TimingWarning>,
    /// Per playlist item, in order: its start and end positions that do
    /// not fit its file.
    pub item_timing: Vec<Vec<TimingWarning>>,
    /// The file's length is still being read; `file_duration` and
    /// `duration` follow.
    pub probing: bool,
    /// The file is still being copied into the media folder (progress in
    /// the state's `imports`); it plays once that is done.
    pub importing: bool,
    /// Volume plus normalization is above the show's maximum, so the cue
    /// (or some playlist item) plays at the maximum instead.
    pub level_capped: bool,
    /// The volume is below the show's minimum (but not set to silence), so
    /// the cue is silent; e.g. after editing the show file by hand.
    pub level_silenced: bool,
    /// Further problems, as messages: values only a hand-edited show file
    /// has, a hotkey that never fires, a file that is no audio.
    pub problems: Vec<String>,
    /// Per file (a playlist's in order); `None` until measured.
    pub loudness: Vec<Option<FileLoudness>>,
    /// Some file is still being measured.
    pub measuring: bool,
    /// A stored normalization offset is more than [`STALE_OFFSET`] dB off
    /// what the file needs now (it changed, or the target did).
    pub normalize_stale: bool,
    /// A go-to cue's target was deleted or is no longer at the top level;
    /// a devamp cue waits for the loop exit of a cue no longer in its target.
    pub target_missing: bool,
    /// Repeat can never apply: the cue ignores a GO while running, and never
    /// ends by itself.
    pub repeat_blocked: bool,
    /// Other cues with the same hotkey (it starts them all).
    pub hotkey_shared: Vec<CueId>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct FileLoudness {
    /// Integrated loudness, LUFS; `None` for a silent file.
    pub lufs: Option<f64>,
    /// True peak, dBTP.
    pub peak_db: Option<f64>,
    /// Offset that brings it to the show's target; `None` for a silent file.
    pub suggested_db: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TimingWarning {
    #[serde(flatten)]
    pub issue: TimingIssue,
    pub message: String,
}

/// A media file as last read from disk; reading happens in the background.
struct MediaFile {
    /// What the file looked like when last seen, so a change is noticed.
    stamp: Option<FileStamp>,
    /// Seconds.
    duration: Option<f64>,
    /// Why the file cannot be read, though it is there.
    unreadable: Option<String>,
    version: u32,
    /// Version the latest read under way brings; older results are dropped.
    requested: u32,
    probing: bool,
    /// Report timing issues once the read is done.
    report: bool,
    /// As last measured; `None` until then (or if it could not be read).
    loudness: Option<Loudness>,
    /// Peaks and loudness are being built (or checked) in the background.
    measuring: bool,
}

impl MediaFile {
    /// A file seen for the first time; its length is about to be read.
    fn new(stamp: Option<FileStamp>, report: bool) -> Self {
        Self {
            stamp,
            duration: None,
            unreadable: None,
            version: 0,
            requested: 0,
            probing: true,
            report,
            loudness: None,
            measuring: false,
        }
    }

    /// The file changed: it is read again. Returns the version to request.
    fn reread(&mut self, stamp: Option<FileStamp>) -> u32 {
        self.stamp = stamp;
        self.probing = true;
        self.requested += 1;
        self.requested
    }
}

pub struct Session {
    file: ShowFile,
    editor: Editor,
    pub runner: Runner,
    pub output: AudioOutput,
    audio_error: Option<AudioError>,
    /// Separate preview device; `None` = off or a pair of the main device.
    pub preview_output: Option<AudioOutput>,
    preview_error: Option<String>,
    /// `lost` of the main and preview output as last reported.
    lost_seen: (Option<String>, Option<String>),
    /// `callback_frames` of the output as last published; the first callback
    /// comes after the view of a (re)opened device went out.
    frames_seen: Option<u32>,
    keep_alive: KeepAlive,
    settings: LocalSettings,
    /// The machine's settings file.
    settings_path: PathBuf,
    /// Media files by stored path.
    media: HashMap<String, MediaFile>,
    /// Cues added with `normalize_on_import`, normalized as soon as their
    /// files are measured.
    auto_normalize: HashSet<CueId>,
    /// Loudness measured since the view was last sent for it, and when that
    /// was: while a show's files are measured one by one, the view goes out
    /// at most every [`MEASURED_INTERVAL`].
    measured: (bool, Instant),
    issues: Vec<MediaIssue>,
    watcher: MediaWatcher,
    /// The files the watcher was given last.
    watched: Vec<PathBuf>,
    /// Held in show mode; see [`MediaLock`].
    lock: Option<MediaLock>,
    /// Autosave found on opening, until the operator decides; meanwhile
    /// nothing is autosaved, so it cannot be overwritten.
    recovery: Option<(Show, SystemTime)>,
    /// Editor revision the autosave file matches.
    autosaved: u64,
    /// Last editor revision seen and when, for the quiet time.
    seen: (u64, Instant),
    midi: Option<MidiIn>,
    midi_error: Option<String>,
    /// Where the MIDI callback sends messages (the show thread).
    midi_tx: Sender<Request>,
    midi_decoder: MidiDecoder,
    learning: Option<LearnTarget>,
    osc_in: Option<OscListener>,
    osc_error: Option<String>,
    /// The port itself belongs to the runner.
    midi_out_name: Option<String>,
    midi_out_error: Option<String>,
    /// The `midi_out` setting the current port was opened for.
    midi_out_setting: Option<String>,
    /// Cues copied or cut, as they were then; kept until the show is closed.
    clipboard: Vec<Cue>,
    /// Reads media file lengths in the background.
    prober: Prober,
    /// Copies files into the media folder in the background.
    copier: Copier,
    /// Copies queued or under way; their cues exist already.
    copies: Vec<Copy>,
    /// Copies an edit plans (source, target, stored path); queued once the
    /// edit succeeds.
    planned: Vec<(PathBuf, PathBuf, String)>,
    /// A search for missing files under way, and whether to copy what is
    /// found outside the media folder.
    search: Option<(Receiver<Found>, bool)>,
    /// Builds the waveform peaks of the show's files while it is quiet.
    prewarmer: Prewarmer,
}

impl Session {
    /// Opens a loaded show, in show mode if `show_mode` is set. Audio
    /// problems never prevent opening: the show then runs on a silent output
    /// and the error is shown.
    pub fn open(
        mut file: ShowFile,
        show_mode: bool,
        settings_path: PathBuf,
        peaks: Arc<PeakCache>,
        midi_tx: Sender<Request>,
        notices: &mut Vec<Notice>,
    ) -> Self {
        let mut settings =
            LocalSettings::load(&settings_path, &file.show.id.to_string()).unwrap_or_else(|e| {
                notices.push(Notice::Warning(t!("session.settings_ignored", error = e)));
                LocalSettings::default()
            });
        settings.show_mode = show_mode || file.show.show_mode_locked;
        for id in file.show.sync_links(None) {
            let label = file.show.cue(id).map_or(String::new(), |c| {
                if c.number.is_empty() { t!("cue.label_title", title = c.title) } else { c.number.clone() }
            });
            notices.push(Notice::Warning(t!("session.link_synced", cue = label)));
        }
        let (output, controller, audio_error) = open_output(&settings, notices);
        let mut keep_alive = KeepAlive::new();
        if let jq_core::KeepAliveState::Failed(e) = keep_alive.set_enabled(settings.keep_awake) {
            notices.push(Notice::Warning(t!("session.keep_awake_failed", error = e)));
        }
        let mut runner = Runner::new(
            file.show.clone(),
            file.media_dir(),
            settings.clone(),
            controller,
            Arc::new(FileLoader),
        );
        attach_osc_out(&mut runner, notices);
        let watcher = MediaWatcher::new(settings.media_watch, WATCH_INTERVAL);
        let mut session = Self {
            editor: Editor::new(file.show.clone()),
            file,
            runner,
            output,
            audio_error,
            keep_alive,
            settings,
            settings_path,
            media: HashMap::new(),
            auto_normalize: HashSet::new(),
            measured: (false, Instant::now()),
            issues: Vec::new(),
            watcher,
            watched: Vec::new(),
            lock: None,
            recovery: None,
            autosaved: 0,
            seen: (0, Instant::now()),
            preview_output: None,
            preview_error: None,
            lost_seen: (None, None),
            frames_seen: None,
            midi: None,
            midi_error: None,
            midi_tx,
            midi_decoder: MidiDecoder::default(),
            learning: None,
            osc_in: None,
            osc_error: None,
            midi_out_name: None,
            midi_out_error: None,
            midi_out_setting: None,
            clipboard: Vec::new(),
            prober: Prober::new(),
            copier: Copier::new(),
            copies: Vec::new(),
            planned: Vec::new(),
            search: None,
            prewarmer: Prewarmer::new(peaks, PREWARM_QUIET),
        };
        session.listen_osc(notices);
        session.connect_midi_out(notices);
        session.connect_midi(notices);
        session.open_preview(notices);
        session.recovery = session.file.load_autosave().unwrap_or_else(|e| {
            notices.push(Notice::Warning(t!("session.autosave_unreadable", error = e)));
            None
        });
        session.refresh_media(true);
        for issue in &session.issues {
            notices.push(Notice::Warning(match issue {
                MediaIssue::Missing { stored, .. } => t!("session.file_missing", file = stored),
                MediaIssue::CaseMismatch { stored, actual, .. } => {
                    t!("session.case_mismatch", file = stored, actual = actual)
                }
            }));
        }
        session.update_lock(notices);
        session
    }

    /// Reports stream errors, a device lost or back again and a changed
    /// callback size. Returns true if the show view changed.
    pub fn audio_tick(&mut self, notices: &mut Vec<Notice>) -> bool {
        notices.extend(
            self.output
                .take_errors()
                .into_iter()
                .map(|e| Notice::Warning(t!("session.audio_error", error = e))),
        );
        if let Some(preview) = &self.preview_output {
            notices.extend(
                preview
                    .take_errors()
                    .into_iter()
                    .map(|e| Notice::Warning(t!("session.preview_error", error = e))),
            );
        }
        let frames = self.output.callback_frames();
        let frames_changed = frames != self.frames_seen;
        self.frames_seen = frames;
        let lost = (self.output.lost(), self.preview_output.as_ref().and_then(AudioOutput::lost));
        if lost == self.lost_seen {
            return frames_changed;
        }
        let preview = self.preview_output.as_ref().map_or("", |o| o.info().device.as_str());
        for (label, device, before, now) in [
            (t!("session.audio_output"), self.output.info().device.as_str(), &self.lost_seen.0, &lost.0),
            (t!("session.preview_output"), preview, &self.lost_seen.1, &lost.1),
        ] {
            match (before, now) {
                (None, Some(reason)) => notices.push(Notice::Error(t!(
                    "session.device_lost",
                    label = label,
                    device = device,
                    reason = reason
                ))),
                (Some(_), None) => {
                    notices.push(Notice::Warning(t!("session.device_back", label = label, device = device)))
                }
                _ => {}
            }
        }
        self.lost_seen = lost;
        true
    }

    /// Collects the background work on media files (lengths read, copies
    /// done, missing files found) and takes changes of files on disk: the
    /// runner reads them again, durations and waveforms follow, and what now
    /// fails or no longer fits is reported. Returns true if the show view
    /// changed.
    pub fn media_tick(&mut self, notices: &mut Vec<Notice>) -> bool {
        let mut changed = self.collect_probes(notices);
        changed |= self.collect_measured();
        changed |= self.collect_copies(notices);
        changed |= self.collect_search(notices);
        // A file read as something the watcher did not see: it may have
        // missed a change, so it looks again.
        if !self.runner.take_suspect_files().is_empty() {
            self.watcher.rescan();
        }
        notices.extend(self.watcher.take_warnings().into_iter().map(Notice::Warning));
        if let Some(stamps) = self.watcher.poll() {
            changed |= self.files_changed(stamps, notices);
        }
        changed
    }

    /// Looks at the media files again (for changes the watcher may have
    /// missed, e.g. on a network share): missing files are searched anew,
    /// changed ones read again. The result comes in with [`Self::media_tick`].
    pub fn rescan_media(&mut self) {
        self.refresh_media(false);
        self.watcher.rescan();
    }

    /// Selects and immediately saves an existing media folder.
    pub fn set_media_dir(&mut self, dir: PathBuf) -> Result<(), String> {
        if self.settings.show_mode {
            return Err(t!("session.show_mode_locked"));
        }
        if !self.runner.is_idle() {
            return Err(t!("session.media_dir_busy_cue"));
        }
        if !self.copies.is_empty() || self.search.is_some() {
            return Err(t!("session.media_dir_busy_files"));
        }
        if self.recovery.is_some() {
            return Err(t!("session.decide_recovery"));
        }
        let dir = real_path(&dir).map_err(|e| t!("session.media_dir_unreachable", error = e))?;
        if !dir.is_dir() {
            return Err(t!("session.not_a_folder", path = dir.display()));
        }
        let show_dir =
            real_path(self.file.dir()).map_err(|e| t!("session.show_dir_unreachable", error = e))?;
        let media_root = paths::media_root(&show_dir, &dir)
            .ok_or_else(|| t!("session.media_dir_not_storable", path = dir.display()))?;
        if self.editor.show().media_root == media_root {
            return Ok(());
        }

        self.editor.change(|show| show.media_root = media_root);
        self.file.show.media_root = self.editor.show().media_root.clone();
        self.runner.set_media_dir(self.file.media_dir());
        self.media.clear();
        self.refresh_media(false);
        self.watcher.rescan();
        Ok(())
    }

    /// Lets background reading (peaks) know whether it must wait. Cheap;
    /// meant for every tick, so a GO stops the reading at once.
    pub fn update_busy(&self) {
        self.prewarmer.set_busy(self.busy());
    }

    /// Whether background reading (peaks) must wait: something plays or
    /// waits, or the next GO's files are still being loaded. Show mode
    /// alone does not count: a GO then cancels the reading at once.
    fn busy(&self) -> bool {
        !self.runner.is_quiet() || self.runner.playhead_cue().is_some_and(|id| !self.runner.is_armed(id))
    }

    fn files_changed(&mut self, stamps: Stamps, notices: &mut Vec<Notice>) -> bool {
        let playing = self.runner.media_changed(stamps.clone());
        let changed = self.reread_changed(&stamps);
        if changed.is_empty() {
            return false;
        }
        let show = self.editor.show();
        for id in playing {
            if let Some(cue) = show.cue(id) {
                notices.push(Notice::Warning(t!("session.changed_while_playing", cue = cue_label(cue))));
            }
        }
        let before = std::mem::take(&mut self.issues);
        self.issues = paths::check_media(show, &self.file.media_dir());
        for file in &changed {
            let missing = |issues: &[MediaIssue]| {
                issues.iter().any(|i| matches!(i, MediaIssue::Missing { stored, .. } if stored == file))
            };
            notices.push(Notice::Warning(match (missing(&before), missing(&self.issues)) {
                (false, true) => t!("session.file_now_missing", file = file),
                (true, false) => t!("session.file_back", file = file),
                _ => t!("session.file_changed", file = file),
            }));
        }
        true
    }

    /// Has the files read again whose stamp in `stamps` differs from the one
    /// last seen; returns their stored paths. Timing issues are reported
    /// once they are read.
    fn reread_changed(&mut self, stamps: &Stamps) -> Vec<String> {
        let media_dir = self.file.media_dir();
        let mut changed = Vec::new();
        for (stored, file) in &mut self.media {
            let path = paths::resolve(&media_dir, stored);
            if let Some(&now) = stamps.get(&path)
                && now != file.stamp
            {
                let version = file.reread(now);
                file.report = true;
                self.prober.request(stored, path, version);
                changed.push(stored.clone());
            }
        }
        changed.sort();
        changed
    }

    /// Takes the file lengths read meanwhile. Returns true if any arrived.
    fn collect_probes(&mut self, notices: &mut Vec<Notice>) -> bool {
        let mut report = Vec::new();
        let mut any = false;
        let media_dir = self.file.media_dir();
        for p in self.prober.poll() {
            let Some(file) = self.media.get_mut(&p.stored).filter(|f| f.requested == p.version) else {
                continue;
            };
            // New, changed or copied in: its peaks and loudness are built (or
            // checked) next.
            if p.stamp.is_some() {
                self.prewarmer.request(paths::resolve(&media_dir, &p.stored));
                file.measuring = true;
            }
            (file.stamp, file.duration, file.unreadable, file.version, file.probing) =
                (p.stamp, p.duration, p.unreadable, p.version, false);
            if std::mem::take(&mut file.report) {
                report.push(p.stored);
            }
            any = true;
        }
        self.timing_notices(|f| report.contains(f), notices);
        any
    }

    /// Takes the loudness measured meanwhile; cues added with automatic
    /// normalization get their offsets. Returns true if the view is to be
    /// sent: something arrived, at most every [`MEASURED_INTERVAL`].
    fn collect_measured(&mut self) -> bool {
        let measured = self.prewarmer.poll();
        if measured.is_empty() {
            let (pending, sent) = self.measured;
            if pending && sent.elapsed() >= MEASURED_INTERVAL {
                self.measured = (false, Instant::now());
                return true;
            }
            return false;
        }
        let media_dir = self.file.media_dir();
        for (path, loudness) in measured {
            for (stored, file) in &mut self.media {
                if paths::resolve(&media_dir, stored) == path {
                    (file.loudness, file.measuring) = (loudness, false);
                }
            }
        }
        if self.run_auto_normalize() {
            self.runner.set_show(self.editor.show().clone());
        }
        if self.measured.1.elapsed() < MEASURED_INTERVAL {
            self.measured.0 = true;
            return false;
        }
        self.measured = (false, Instant::now());
        true
    }

    /// Normalizes the cues waiting for automatic normalization as far as
    /// their files are measured. The offsets come as if the cues had been
    /// added with them: in the undo and redo history too, never as a step of
    /// their own (that would clear redo, or an undo would bring the cue back
    /// without them). Not in show mode: levels stay as they are during a
    /// performance, the cues wait until it ends. Returns true if the show
    /// changed.
    fn run_auto_normalize(&mut self) -> bool {
        if self.auto_normalize.is_empty() || self.settings.show_mode {
            return false;
        }
        let ids: Vec<CueId> = self.auto_normalize.iter().copied().collect();
        let revision = self.editor.revision();
        let (changes, _) = normalization(self.editor.show(), &ids, true, |f| self.file_offset(f));
        if !changes.is_empty() {
            self.editor.amend(|show| set_offsets(show, &changes, true));
        }
        // Done once none of its files is still to be read or measured.
        let (media, show) = (&self.media, self.editor.show());
        self.auto_normalize.retain(|&id| {
            show.cue(id).is_some_and(|c| {
                c.media_files().iter().any(|f| media.get(*f).is_none_or(|m| m.probing || m.measuring))
            })
        });
        self.editor.revision() != revision
    }

    /// Marks cues for automatic normalization if the show asks for it.
    fn normalize_when_measured(&mut self, ids: impl IntoIterator<Item = CueId>) {
        if self.editor.show().normalize_on_import {
            self.auto_normalize.extend(ids);
        }
    }

    /// Takes the copies done meanwhile: their files are read and armed like
    /// any other. Returns true if any finished.
    fn collect_copies(&mut self, notices: &mut Vec<Notice>) -> bool {
        let done = self.copier.poll();
        if done.is_empty() {
            return false;
        }
        for (target, result) in done {
            let Some(i) = self.copies.iter().position(|c| c.target == target) else { continue };
            let copy = self.copies.remove(i);
            if let Err(e) = result {
                notices.push(Notice::Error(t!("session.not_copied", file = copy.stored, error = e)));
                continue;
            }
            // Seen as it is now, so the watcher reports no change.
            if let Some(file) = self.media.get_mut(&copy.stored) {
                let version = file.reread(paths::stamp(&target));
                self.prober.request(&copy.stored, target, version);
            }
        }
        self.issues = paths::check_media(self.editor.show(), &self.file.media_dir());
        self.runner.set_importing(self.importing_paths());
        true
    }

    /// Takes the result of a search for missing files: all repairs are one
    /// undo step; reports what is still missing.
    fn collect_search(&mut self, notices: &mut Vec<Notice>) -> bool {
        let Some((results, copy_outside)) = &self.search else { return false };
        let copy_outside = *copy_outside;
        let found = match results.try_recv() {
            Ok(found) => found,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                self.search = None;
                notices.push(Notice::Error(t!("session.search_aborted")));
                return true;
            }
        };
        self.search = None;
        self.planned.clear();
        let mut changes = found.inside;
        let on_clash = found
            .outside
            .iter()
            .filter(|(_, identical)| *identical)
            .map(|((_, _, path), _)| (path.clone(), OnClash::UseExisting))
            .collect();
        let import = Import { copy_outside, on_clash };
        for ((id, old, path), _) in found.outside {
            match self.store_media(&path, &import, notices) {
                Ok(Some(stored)) => changes.push((id, old, stored)),
                Ok(None) => {}
                Err(e) => notices.push(Notice::Warning(e)),
            }
        }
        let mut relinked = 0;
        if !changes.is_empty() {
            // Applied to the show as it is now, which may have changed meanwhile.
            self.editor.change(|show| {
                for (id, old, new) in &changes {
                    let Some(cue) = show.cue_mut(*id) else { continue };
                    for file in cue.media_files_mut().into_iter().filter(|f| *f == old) {
                        *file = new.clone();
                        relinked += 1;
                    }
                }
            });
        }
        self.start_copies();
        self.runner.set_show(self.editor.show().clone());
        self.refresh_media(false);
        let importing = self.importing_paths();
        let media = self.file.media_dir();
        let missing = self
            .issues
            .iter()
            .filter(|i| {
                matches!(i, MediaIssue::Missing { stored, .. } if !importing.contains(&paths::resolve(&media, stored)))
            })
            .count();
        if missing > 0 {
            notices.push(Notice::Warning(t!(
                "session.relinked_some",
                relinked = relinked,
                missing = missing
            )));
        }
        true
    }

    /// Queues the copies the last edit planned.
    fn start_copies(&mut self) {
        for (from, to, stored) in self.planned.drain(..) {
            self.copies.push(self.copier.copy(from, to, stored));
        }
        self.runner.set_importing(self.importing_paths());
    }

    fn importing_paths(&self) -> HashSet<PathBuf> {
        self.copies.iter().map(|c| c.target.clone()).collect()
    }

    /// Files being copied into the media folder, for the state event.
    pub fn imports(&self) -> Vec<ImportState<'_>> {
        self.copies.iter().map(|c| ImportState { file: &c.stored, progress: c.progress() }).collect()
    }

    /// The media folder and the stored paths the show may still need: as
    /// edited and as saved (leaving without saving brings that back), in
    /// its undo and redo history (undo brings a deleted cue back), on the
    /// clipboard and in an undecided recovery. Copies under way are hidden
    /// files and their cues exist already. Refused in show mode and for a
    /// media folder holding the show file.
    pub fn used_media(&self) -> Result<(PathBuf, Vec<String>), String> {
        if self.settings.show_mode {
            return Err(t!("session.show_mode_locked"));
        }
        let media = self.file.media_dir();
        let real = |p: &Path| real_path(p).unwrap_or_else(|_| p.to_path_buf());
        if real(self.file.dir()).starts_with(real(&media)) {
            return Err(t!("session.media_dir_has_show"));
        }
        let shows = self
            .editor
            .history()
            .chain(std::iter::once(&self.file.show))
            .chain(self.recovery.as_ref().map(|(s, _)| s));
        Ok((media, media_in(shows, &self.clipboard)))
    }

    /// A search for missing files is under way.
    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// Reports the timing issues of sound cues and playlist items whose
    /// file passes `files`.
    fn timing_notices(&self, files: impl Fn(&String) -> bool, notices: &mut Vec<Notice>) {
        for cue in self.editor.show().all() {
            match &cue.kind {
                CueKind::Sound(s) if files(&s.file) => {
                    for issue in self.timing(s) {
                        notices.push(Notice::Warning(t!(
                            "session.timing_cue",
                            cue = cue_label(cue),
                            issue = issue
                        )));
                    }
                }
                CueKind::Playlist(p) => {
                    for (i, item) in (0..p.items.len()).filter_map(|i| Some((i, p.item(i)?))) {
                        if !files(&item.file) {
                            continue;
                        }
                        for issue in self.timing(&item) {
                            let label = cue_label(cue);
                            notices.push(Notice::Warning(t!(
                                "session.timing_item",
                                cue = label,
                                item = i + 1,
                                issue = issue
                            )));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Timing issues of `s` against its file; none while its length is unknown.
    fn timing(&self, s: &SoundCue) -> Vec<TimingIssue> {
        let duration = self.media.get(&s.file).and_then(|f| f.duration);
        duration.map(|d| check_timing(s, d)).unwrap_or_default()
    }

    /// In show mode, locks the media files against changes (where the
    /// platform can); outside of it releases them.
    fn update_lock(&mut self, notices: &mut Vec<Notice>) {
        if !self.settings.show_mode {
            self.lock = None;
            return;
        }
        if !MediaLock::SUPPORTED {
            return;
        }
        let (lock, failed) = MediaLock::new(self.watched.iter().cloned());
        self.lock = Some(lock);
        if let Some((path, e)) = failed.first() {
            notices.push(Notice::Warning(t!(
                "session.lock_failed",
                count = failed.len(),
                file = path.display(),
                error = e
            )));
        }
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.file.show = self.editor.show().clone();
        self.file.save().map_err(|e| e.to_string())?;
        self.editor.mark_saved();
        self.autosaved = self.editor.revision();
        if self.recovery.is_none() {
            self.file.remove_autosave().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Writes the autosave once edits have been quiet for a moment. Edits are
    /// locked in show mode, so this never writes during a performance.
    pub fn autosave_tick(&mut self, now: Instant, notices: &mut Vec<Notice>) {
        let revision = self.editor.revision();
        if revision == self.autosaved || self.recovery.is_some() {
            return;
        }
        if revision != self.seen.0 {
            self.seen = (revision, now);
            return;
        }
        if now.duration_since(self.seen.1) < AUTOSAVE_DELAY {
            return;
        }
        self.autosaved = revision;
        let result = if self.editor.is_dirty() {
            self.file.write_autosave(self.editor.show())
        } else {
            self.file.remove_autosave()
        };
        if let Err(e) = result {
            notices.push(Notice::Warning(t!("session.autosave_failed", error = e)));
        }
    }

    /// Restores (one undo step) or discards the autosave found on opening.
    /// Allowed in show mode too: it is a deliberate decision after a crash.
    pub fn recover(&mut self, apply: bool, notices: &mut Vec<Notice>) -> Result<(), String> {
        let Some((show, _)) = self.recovery.take() else { return Ok(()) };
        if apply {
            self.editor.change(|s| *s = show);
            self.runner.set_show(self.editor.show().clone());
            self.refresh_media(false);
            self.update_lock(notices);
            self.enforce_show_mode_lock(notices);
        } else {
            self.file.remove_autosave().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The operator left without saving: the autosave is no longer wanted.
    /// An undecided recovery is kept for the next opening.
    pub fn discard_autosave(&self) -> Result<(), String> {
        if self.recovery.is_some() {
            return Ok(());
        }
        self.file.remove_autosave().map_err(|e| e.to_string())
    }

    /// Applies and saves machine settings. A changed audio device is opened
    /// again, which is refused while cues run; a failed or lost one is tried
    /// again only while none run (see [`audio_reopen`]). The layout is left
    /// as it is: only `set_layout` changes it.
    pub fn set_settings(
        &mut self,
        mut settings: LocalSettings,
        notices: &mut Vec<Notice>,
    ) -> Result<(), String> {
        settings.layout = self.settings.layout.clone();
        if !settings.fade_all.is_finite() || settings.fade_all < 0.0 {
            return Err(t!("session.setting.not_negative", what = t!("session.setting.fade_all")));
        }
        if [settings.jump_back, settings.jump_forward, settings.jump_end]
            .iter()
            .any(|j| !j.is_finite() || *j <= 0.0)
        {
            return Err(t!("session.setting.jump_times"));
        }
        for (secs, what) in [
            (settings.end_warning, t!("session.setting.end_warning")),
            (settings.panic_fade, t!("session.setting.panic_fade")),
            (settings.go_lockout, t!("session.setting.go_lockout")),
        ] {
            if !secs.is_finite() || secs < 0.0 {
                return Err(t!("session.setting.not_negative", what = what));
            }
        }
        keys::check_keys(&settings.keys)?;
        for (step, what) in [
            (settings.volume_step, t!("session.setting.volume_step")),
            (settings.pan_step, t!("session.setting.pan_step")),
            (settings.rate_step, t!("session.setting.rate_step")),
        ] {
            if !step.is_finite() || step <= 0.0 {
                return Err(t!("session.setting.positive", what = what));
            }
        }
        if settings.msc_device.is_some_and(|d| d > 126) {
            return Err(t!("session.setting.msc_device"));
        }
        if let Some(frames) = settings.buffer_frames
            && !(frames.is_power_of_two() && (MIN_BUFFER_FRAMES..=MAX_BUFFER_FRAMES).contains(&frames))
        {
            return Err(t!("session.setting.buffer", min = MIN_BUFFER_FRAMES, max = MAX_BUFFER_FRAMES));
        }
        // Set by the backend on opening a show.
        settings.last_show = self.settings.last_show.clone();
        if settings.osc_listen == Some(0) {
            return Err(t!("session.setting.osc_port"));
        }
        if settings.outputs.values().any(|&c| c == 0) || settings.preview_channel == Some(0) {
            return Err(t!("session.setting.channels_from_one"));
        }
        if settings.channels.is_some_and(|c| c < 2) {
            return Err(t!("session.setting.min_channels"));
        }
        if !settings.show_mode && self.editor.show().show_mode_locked {
            return Err(t!("session.setting.show_mode_locked"));
        }
        self.check_channels(&settings)?;
        let show_mode_changed = settings.show_mode != self.settings.show_mode;
        let reopen = audio_reopen(
            settings.audio_differs(&self.settings) || self.output.needs_channels(settings.needed_channels()),
            self.audio_error.is_some() || self.output.lost().is_some(),
            self.runner.is_idle(),
        )?;
        if settings.media_watch != self.settings.media_watch {
            self.watcher.set_mode(settings.media_watch);
        }
        let reconnect = self.midi_error.is_some() || settings.midi_port != self.settings.midi_port;
        let preview_changed = self.preview_error.is_some()
            || self.preview_output.as_ref().is_some_and(|o| o.lost().is_some())
            || (&settings.preview_device, settings.preview_channel)
                != (&self.settings.preview_device, self.settings.preview_channel);
        if settings.keep_awake != self.settings.keep_awake
            && let KeepAliveState::Failed(e) = self.keep_alive.set_enabled(settings.keep_awake)
        {
            notices.push(Notice::Warning(t!("session.keep_awake_failed", error = e)));
        }
        self.settings = settings;
        self.save_settings(notices);
        if reopen {
            self.reopen_audio(notices);
        } else {
            self.runner.set_settings(self.settings.clone());
        }
        if reconnect {
            self.connect_midi(notices);
        }
        // After reopening, the new runner was connected already.
        if !reopen && (self.midi_out_error.is_some() || self.settings.midi_out != self.midi_out_setting) {
            self.connect_midi_out(notices);
        }
        if self.osc_error.is_some() || self.settings.osc_listen != self.osc_in.as_ref().map(OscListener::port)
        {
            self.listen_osc(notices);
        }
        // A new runner (after reopening) needs its preview engine again.
        if reopen || preview_changed {
            self.open_preview(notices);
        }
        if show_mode_changed {
            self.update_lock(notices);
            // Cues measured during the performance get their offsets now.
            if self.run_auto_normalize() {
                self.runner.set_show(self.editor.show().clone());
            }
        }
        let channels = self.output.info().channels as usize;
        for (output, &first) in &self.settings.outputs {
            if first + 1 > channels {
                notices.push(Notice::Warning(t!(
                    "session.output_channels",
                    output = output,
                    first = first,
                    second = first + 1,
                    channels = channels
                )));
            }
        }
        Ok(())
    }

    /// Remembers where the audio file picker was last used. Unlike
    /// `set_settings` it touches nothing else, so it works while cues run
    /// or the audio device has failed.
    pub fn set_audio_dir(&mut self, dir: Option<PathBuf>, notices: &mut Vec<Notice>) {
        if self.settings.audio_dir != dir {
            self.settings.audio_dir = dir;
            self.save_settings(notices);
        }
    }

    /// Stores the UI's panel layout; like `set_audio_dir` it touches nothing
    /// else.
    pub fn set_layout(&mut self, layout: Option<serde_json::Value>, notices: &mut Vec<Notice>) {
        if self.settings.layout != layout {
            self.settings.layout = layout;
            self.save_settings(notices);
        }
    }

    /// Stores the cue list's selection mode; like `set_audio_dir` it
    /// touches nothing else.
    pub fn set_selection_mode(&mut self, mode: SelectionMode, notices: &mut Vec<Notice>) {
        if self.settings.selection_mode != mode {
            self.settings.selection_mode = mode;
            self.save_settings(notices);
        }
    }

    /// Which cues new level limits would change (see
    /// [`Show::clamp_levels`]), so the UI can warn first.
    pub fn check_level_limits(&self, limits: LevelLimits) -> Result<LevelChanges, String> {
        check_limits(limits)?;
        let mut show = self.editor.show().clone();
        Ok(show.clamp_levels(show.level_limits, limits))
    }

    /// Notes this show as the one opened last (for opening it on startup).
    pub fn remember_path(&mut self, notices: &mut Vec<Notice>) {
        if self.settings.last_show.as_ref() != Some(&self.file.path) {
            self.settings.last_show = Some(self.file.path.clone());
            self.save_settings(notices);
        }
    }

    fn save_settings(&self, notices: &mut Vec<Notice>) {
        if let Err(e) = self.settings.save(&self.settings_path, &self.file.show.id.to_string()) {
            notices.push(Notice::Warning(t!("session.settings_not_saved", error = e)));
        }
    }

    /// Changes the display name; refused in show mode.
    pub fn rename(&mut self, name: &str) -> Result<(), String> {
        if self.settings.show_mode {
            return Err(t!("session.show_mode_locked"));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(t!("session.name_empty"));
        }
        if self.editor.show().name != name {
            self.editor.change(|show| show.name = name.to_owned());
        }
        Ok(())
    }

    /// The media folder and every stored path the show may need after
    /// saving it elsewhere: in its undo history too, and on the clipboard.
    /// Refused where renaming is.
    pub fn save_as_plan(&self) -> Result<(PathBuf, Vec<String>), String> {
        self.check_save_as()?;
        Ok((self.file.media_dir(), media_in(self.editor.history(), &self.clipboard)))
    }

    fn check_save_as(&self) -> Result<(), String> {
        if self.settings.show_mode {
            return Err(t!("session.show_mode_locked"));
        }
        if !self.runner.is_idle() {
            return Err(t!("session.save_as_busy_cue"));
        }
        if !self.copies.is_empty() || self.search.is_some() {
            return Err(t!("session.save_as_busy_files"));
        }
        if self.recovery.is_some() {
            return Err(t!("session.decide_recovery"));
        }
        Ok(())
    }

    /// Saves the show as `path` in a new show folder (made by
    /// [`ShowFile::create_folder`], media already copied if wanted) with the
    /// media folder `media_root`, and goes on there. The old show stays as
    /// last saved; its autosave goes, its changes are in the new show.
    /// `copied`: the files copied into its media folder, from
    /// [`Session::save_as_plan`]; refused if the show has started using
    /// another one meanwhile (the new folder would lack it). On failure the
    /// new folder is removed again.
    pub fn save_as(
        &mut self,
        path: PathBuf,
        media_root: String,
        copied: Option<&[String]>,
    ) -> Result<(), String> {
        let folder = path.parent().map(Path::to_path_buf);
        let result = self.check_save_as().and_then(|()| {
            if let Some(copied) = copied {
                let copied: HashSet<&String> = copied.iter().collect();
                if media_in(self.editor.history(), &self.clipboard).iter().any(|f| !copied.contains(f)) {
                    return Err(t!("session.save_as_changed"));
                }
            }
            let name = path.file_stem().and_then(|name| name.to_str()).unwrap_or_default().to_owned();
            let show = Show { name, media_root: media_root.clone(), ..self.editor.show().clone() };
            let file = ShowFile { path, show };
            file.save().map_err(|e| e.to_string())?;
            Ok(file)
        });
        let file = match result {
            Ok(file) => file,
            Err(e) => {
                if let Some(folder) = folder {
                    let _ = std::fs::remove_dir_all(folder);
                }
                return Err(e);
            }
        };
        // Lost, the old autosave would only offer changes saved here.
        let _ = self.file.remove_autosave();
        self.file = file;
        let show_name = self.file.show.name.clone();
        self.editor.change_history(|s| {
            s.name = show_name.clone();
            s.media_root = media_root.clone();
        });
        self.editor.mark_saved();
        self.autosaved = self.editor.revision();
        self.runner.set_media_dir(self.file.media_dir());
        self.media.clear();
        self.refresh_media(false);
        Ok(())
    }

    pub fn set_keep_awake(&mut self, enabled: bool, notices: &mut Vec<Notice>) -> Result<(), String> {
        self.set_settings(LocalSettings { keep_awake: enabled, ..self.settings.clone() }, notices)
    }

    pub fn set_show_mode(&mut self, enabled: bool, notices: &mut Vec<Notice>) -> Result<(), String> {
        self.set_settings(LocalSettings { show_mode: enabled, ..self.settings.clone() }, notices)
    }

    /// A locked show stays in show mode, also when an undo or a recovery
    /// brings the lock back.
    fn enforce_show_mode_lock(&mut self, notices: &mut Vec<Notice>) {
        if self.editor.show().show_mode_locked && !self.settings.show_mode {
            self.settings.show_mode = true;
            self.runner.set_settings(self.settings.clone());
            self.update_lock(notices);
        }
    }

    /// Rejects output and preview channels the devices do not have. The
    /// devices are only probed when they change; one that cannot be found
    /// passes, opening it reports the error.
    fn check_channels(&self, settings: &LocalSettings) -> Result<(), String> {
        let main = if self.audio_error.is_some() || settings.audio_differs(&self.settings) {
            AudioOutput::probe(&main_spec(settings)).ok()
        } else {
            Some(probe_of(&self.output))
        };
        if let Some(main) = main {
            let pairs =
                self.editor.show().outputs.iter().map(|o| (o.name.clone(), settings.output_channel(&o.id)));
            let preview = settings.preview_device.is_none().then_some(settings.preview_channel).flatten();
            let preview = preview.map(|_| (t!("session.preview"), settings.preview_first_channel()));
            check_pairs(&main, settings.channels, pairs.chain(preview))?;
        }
        let preview_changed =
            self.preview_error.is_some() || settings.preview_device != self.settings.preview_device;
        let preview = match (&settings.preview_device, &self.preview_output) {
            (None, _) => None,
            (Some(_), Some(output)) if !preview_changed => Some(probe_of(output)),
            (Some(_), _) => preview_spec(settings, self.output.info().sample_rate)
                .and_then(|spec| AudioOutput::probe(&spec).ok()),
        };
        if let Some(preview) = preview {
            check_pairs(
                &preview,
                None,
                [(t!("session.preview"), settings.preview_first_channel())].into_iter(),
            )?;
        }
        Ok(())
    }

    /// Opens the separate preview device, at the main output's sample rate
    /// so decoded buffers fit both.
    fn open_preview(&mut self, notices: &mut Vec<Notice>) {
        self.runner.set_preview_engine(None);
        self.preview_output = None;
        self.preview_error = None;
        let Some(spec) = preview_spec(&self.settings, self.output.info().sample_rate) else { return };
        match AudioOutput::open(&spec, PREVIEW_VOICES) {
            Ok((output, controller)) => {
                self.preview_output = Some(output);
                self.runner.set_preview_engine(Some(controller));
            }
            Err(e) => {
                let e = t!("session.preview_unavailable", error = e);
                notices.push(Notice::Warning(e.clone()));
                self.preview_error = Some(e);
            }
        }
    }

    /// Listens for OSC on the configured port (closing the old one first).
    fn listen_osc(&mut self, notices: &mut Vec<Notice>) {
        self.osc_in = None;
        self.osc_error = None;
        let Some(port) = self.settings.osc_listen else { return };
        let tx = self.midi_tx.clone();
        match OscListener::start(port, move |packet| {
            let _ = tx.send(Request::Osc(packet.to_vec()));
        }) {
            Ok(listener) => self.osc_in = Some(listener),
            Err(e) => {
                let e = t!("session.osc_listen_failed", port = port, error = e);
                notices.push(Notice::Warning(e.clone()));
                self.osc_error = Some(e);
            }
        }
    }

    /// Opens the MIDI output for MIDI cues and hands it to the runner.
    fn connect_midi_out(&mut self, notices: &mut Vec<Notice>) {
        self.runner.set_midi_out(None);
        (self.midi_out_name, self.midi_out_error) = (None, None);
        self.midi_out_setting = self.settings.midi_out.clone();
        let Some(port) = self.settings.midi_out.clone() else { return };
        match MidiOut::connect(&port) {
            Ok(out) => {
                self.midi_out_name = Some(out.name().to_string());
                self.runner.set_midi_out(Some(Box::new(MidiOutSink(out))));
            }
            Err(e) => {
                notices.push(Notice::Warning(e.clone()));
                self.midi_out_error = Some(e);
            }
        }
    }

    /// Runs the actions of an incoming OSC packet.
    pub fn handle_osc(&mut self, packet: &[u8]) {
        let now = Instant::now();
        for (address, args) in osc::decode(packet) {
            match osc::incoming(self.runner.show(), &address, &args) {
                Some(osc::Incoming::Action(action)) => self.runner.handle(action, now),
                Some(osc::Incoming::Control(control)) => self.runner.control(control, now),
                None => {}
            }
        }
    }

    /// Opens the configured MIDI input (closing the old one first).
    fn connect_midi(&mut self, notices: &mut Vec<Notice>) {
        self.midi = None;
        self.midi_error = None;
        let Some(port) = self.settings.midi_port.clone() else { return };
        let tx = self.midi_tx.clone();
        match MidiIn::connect(&port, move |message| {
            let _ = tx.send(Request::Midi(message.to_vec()));
        }) {
            Ok(input) => self.midi = Some(input),
            Err(e) => {
                notices.push(Notice::Warning(e.clone()));
                self.midi_error = Some(e);
            }
        }
    }

    /// Runs what a MIDI message does (MSC, a global binding, cue triggers),
    /// or learns it. Returns true if the show view changed (something was
    /// learned).
    pub fn handle_midi(&mut self, message: &[u8], notices: &mut Vec<Notice>) -> bool {
        let now = Instant::now();
        if let Some(device) = self.settings.msc_device
            && let Some(command) = midi::msc(message, device)
        {
            match midi::msc_actions(self.editor.show(), &command) {
                Ok(actions) => actions.into_iter().for_each(|a| self.runner.handle(a, now)),
                Err(e) => notices.push(Notice::Warning(e)),
            }
            return false;
        }
        let Some(event) = self.midi_decoder.event(message) else { return false };
        if event.pressed
            && let Some(target) = self.learning.take()
        {
            match target {
                LearnTarget::Action { action } => {
                    midi::learn(&mut self.settings.midi, action, event.trigger);
                    self.save_settings(notices);
                    self.runner.set_settings(self.settings.clone());
                }
                LearnTarget::Cue { cue } => {
                    let Some(mut learned) = self.editor.show().cue(cue).cloned() else { return true };
                    learned.midi_trigger = Some(event.trigger);
                    if let Err(e) = self.edit(EditOp::UpdateCue { cue: Box::new(learned) }, notices) {
                        notices.push(Notice::Warning(e));
                    }
                }
            }
            return true;
        }
        for action in midi::actions(self.editor.show(), &self.settings.midi, event) {
            self.runner.handle(action, now);
        }
        false
    }

    /// Waits for the next MIDI control to bind it to a global action or a
    /// cue; `None` cancels.
    pub fn midi_learn(&mut self, target: Option<LearnTarget>) -> Result<(), String> {
        if target.is_some() && self.midi.is_none() {
            return Err(t!("session.no_midi_input"));
        }
        match target {
            Some(LearnTarget::Action { action }) if action.on_selected() => {
                return Err(t!("session.keyboard_only", action = action.name()));
            }
            Some(LearnTarget::Cue { cue }) => {
                if self.settings.show_mode {
                    return Err(t!("session.show_mode_locked"));
                }
                if self.editor.show().index_of(cue).is_none() {
                    return Err(t!("session.midi_main_only"));
                }
            }
            _ => {}
        }
        self.learning = target;
        Ok(())
    }

    /// Opens the output from the current settings and starts a new runner
    /// on it; the playhead stays on its cue, and what outlives the device
    /// (stopwatches, GO lockout, …, see [`Runner::carry_over`]) goes with
    /// it. Only call while idle.
    fn reopen_audio(&mut self, notices: &mut Vec<Notice>) {
        let playhead = self.runner.playhead_cue();
        // Release the old device first; some drivers allow only one stream.
        self.output = AudioOutput::null(48_000, 2, 1).0;
        let (output, controller, audio_error) = open_output(&self.settings, notices);
        (self.output, self.audio_error) = (output, audio_error);
        // The view sent now may precede the first callback; the next tick
        // publishes the size once it is known.
        self.frames_seen = None;
        let mut runner = self.new_runner(controller);
        if let Some(id) = playhead {
            runner.handle(jq_core::Action::SetPlayhead(id), Instant::now());
        }
        self.runner.carry_over(&mut runner);
        self.runner = runner;
        attach_osc_out(&mut self.runner, notices);
        self.connect_midi_out(notices);
    }

    fn new_runner(&self, controller: Controller) -> Runner {
        Runner::new(
            self.editor.show().clone(),
            self.file.media_dir(),
            self.settings.clone(),
            controller,
            Arc::new(FileLoader),
        )
    }

    pub fn edit(&mut self, op: EditOp, notices: &mut Vec<Notice>) -> Result<(), String> {
        if self.settings.show_mode && !matches!(op, EditOp::LockShowMode { .. }) {
            return Err(t!("session.show_mode_locked"));
        }
        self.planned.clear();
        match op {
            EditOp::UpdateCue { mut cue } => {
                check_scene(self.editor.show(), &mut cue)?;
                check_go_to(self.editor.show(), &cue)?;
                check_triggers(self.editor.show(), &self.settings, &mut cue)?;
                cue.others.as_ref().map(check_others).transpose()?;
                let show = self.editor.show();
                cue.cap_changed_levels(show.cue(cue.id), show.level_limits.max_db);
                if !self.editor.update(*cue) {
                    return Err(t!("edit.cue_not_found"));
                }
            }
            EditOp::UpdateCues { mut cues } => {
                for cue in &mut cues {
                    check_scene(self.editor.show(), cue)?;
                    check_go_to(self.editor.show(), cue)?;
                    check_triggers(self.editor.show(), &self.settings, cue)?;
                    cue.others.as_ref().map(check_others).transpose()?;
                }
                let show = self.editor.show();
                cues.iter_mut().for_each(|c| c.cap_changed_levels(show.cue(c.id), show.level_limits.max_db));
                if !self.editor.update_all(cues) {
                    return Err(t!("edit.cue_not_found"));
                }
            }
            EditOp::AddSounds { paths, index, import, group } => {
                let mut new = Vec::new();
                for path in paths {
                    let Some(stored) = self.store_media(&path, &import, notices)? else { continue };
                    let mut cue = Cue::new(CueKind::Sound(SoundCue::new(stored)));
                    cue.title = file_stem(&path);
                    new.push(cue);
                }
                // All files of one pick are one undo step.
                if !new.is_empty() {
                    let ids: Vec<CueId> = new.iter().map(|c| c.id).collect();
                    self.place(group, index, new)?;
                    self.normalize_when_measured(ids);
                }
            }
            EditOp::AddControl { kind, index, targets, group } => {
                let show = self.editor.show();
                // One chosen target names the cue, e.g. "Fade-Out Musik".
                let named = match targets.as_deref() {
                    Some([one]) => show.cue(*one).map(|c| c.title.clone()).filter(|t| !t.is_empty()),
                    _ => None,
                };
                let targets = targets.map_or(Targets::Previous(PreviousKeyword::Previous), Targets::Cues);
                let mut kind_value = CueKind::control(kind, targets);
                match &mut kind_value {
                    CueKind::Volume(v) => v.duration = Some(3.0),
                    CueKind::Stopwatch(w) => w.watch = show.stopwatches.first().map(|w| w.id.clone()),
                    _ => {}
                }
                let mut cue = Cue::new(kind_value);
                let base = control_title(kind);
                cue.title = named.map_or_else(|| base.clone(), |t| format!("{base} {t}"));
                self.place(group, index, vec![cue])?;
            }
            EditOp::SetCueType { ids, kind } => {
                let mut cues = Vec::new();
                for id in ids {
                    let mut cue =
                        self.editor.show().cue(id).cloned().ok_or_else(|| t!("edit.cue_not_found"))?;
                    cue.kind = cue.kind.convert(kind).ok_or_else(|| t!("session.convert_control_only"))?;
                    cues.push(cue);
                }
                if !self.editor.update_all(cues) {
                    return Err(t!("edit.cue_not_found"));
                }
            }
            EditOp::SetStopwatches { stopwatches } => {
                let mut ids = std::collections::HashSet::new();
                for w in &stopwatches {
                    if w.countdown.is_some_and(|secs| !(secs > 0.0 && secs <= MAX_COUNTDOWN)) {
                        return Err(t!("session.countdown_range", name = w.name));
                    }
                    if w.id.trim().is_empty() || !ids.insert(w.id.as_str()) {
                        return Err(t!("session.stopwatch_id", id = w.id));
                    }
                    if w.name.trim().is_empty() {
                        return Err(t!("session.stopwatch_name"));
                    }
                }
                self.editor.change(|s| s.stopwatches = stopwatches);
            }
            EditOp::AddOsc { index, group } => {
                let device = self.editor.show().devices.first().map(|d| d.id.clone());
                let mut cue = Cue::new(CueKind::Osc(OscCue {
                    device,
                    messages: vec![OscMessage { address: "/pb/1/go".into(), args: vec![] }],
                }));
                cue.title = "OSC".into();
                self.place(group, index, vec![cue])?;
            }
            EditOp::SetDevices { devices } => {
                let mut ids = std::collections::HashSet::new();
                for d in &devices {
                    if d.id.trim().is_empty() || !ids.insert(d.id.as_str()) {
                        return Err(t!("session.device_id", id = d.id));
                    }
                    if d.host.trim().is_empty() || d.port == 0 {
                        return Err(t!("session.device_address", name = d.name));
                    }
                }
                self.editor.change(|s| s.devices = devices);
            }
            EditOp::SetOutputs { outputs } => {
                check_outputs(self.editor.show(), &outputs)?;
                self.editor.change(|s| s.outputs = outputs);
            }
            EditOp::AddPlaylist { paths, index, import, group } => {
                let files = self.store_all(&paths, &import, notices)?;
                if files.is_empty() {
                    return Ok(());
                }
                let mut cue = Cue::new(CueKind::Playlist(PlaylistCue::new(files)));
                cue.title = t!("session.title.playlist");
                let id = cue.id;
                self.place(group, index, vec![cue])?;
                self.normalize_when_measured([id]);
            }
            EditOp::AddToPlaylist { id, paths, import } => {
                self.editor.show().cue(id).ok_or_else(|| t!("edit.cue_not_found"))?;
                let files = self.store_all(&paths, &import, notices)?;
                if files.is_empty() {
                    return Ok(());
                }
                self.editor.change(|s| {
                    if let Some(Cue { kind: CueKind::Playlist(p), .. }) = s.cue_mut(id) {
                        p.items.extend(files.into_iter().map(jq_show::PlaylistItem::new));
                    }
                });
                self.normalize_when_measured([id]);
            }
            EditOp::SetFile { id, path, item: Some(item), import, .. } => {
                let Some(Cue { kind: CueKind::Playlist(p), .. }) = self.editor.show().cue(id) else {
                    return Err(t!("session.playlist_not_found"));
                };
                if item >= p.items.len() {
                    return Err(t!("session.item_not_found"));
                }
                let Some(stored) = self.store_media(&path, &import, notices)? else { return Ok(()) };
                self.editor.change(|s| {
                    if let Some(Cue { kind: CueKind::Playlist(p), .. }) = s.cue_mut(id) {
                        // The entry keeps its overrides; the old file's offset does not fit.
                        let entry = &mut p.items[item];
                        (entry.file, entry.normalize_db) = (stored, None);
                    }
                });
                self.normalize_when_measured([id]);
            }
            EditOp::SetFile { id, path, retitle, import, item: None } => {
                let Some(Cue { kind: CueKind::Sound(_), .. }) = self.editor.show().cue(id) else {
                    return Err(t!("session.sound_not_found"));
                };
                let Some(stored) = self.store_media(&path, &import, notices)? else { return Ok(()) };
                let new_title = file_stem(&path);
                // The old file's offset does not fit; with automatic
                // normalization the new one gets its own.
                let auto = self.editor.show().normalize_on_import;
                self.editor.change(|s| {
                    if let Some(Cue { kind: CueKind::Sound(sound), title, .. }) = s.cue_mut(id) {
                        sound.file = stored;
                        if auto {
                            sound.normalize_db = None;
                        }
                        if retitle {
                            *title = new_title;
                        }
                    }
                });
                self.normalize_when_measured([id]);
            }
            EditOp::RelinkFile { old_path, path, import } => {
                let mut ids = Vec::new();
                for cue in &self.editor.show().cues {
                    collect_media_users(cue, &old_path, &mut ids);
                }
                if ids.is_empty() {
                    return Err(t!("session.file_unused"));
                }
                let Some(stored) = self.store_media(&path, &import, notices)? else { return Ok(()) };
                let auto = self.editor.show().normalize_on_import;
                self.editor.change(|show| {
                    for cue in &mut show.cues {
                        replace_media_path(cue, &old_path, &stored, auto);
                    }
                });
                self.normalize_when_measured(ids);
            }
            EditOp::AddGroup { index } => {
                let mut cue = Cue::new(CueKind::Group(GroupCue::default()));
                cue.title = t!("session.title.group");
                self.place(None, index, vec![cue])?;
            }
            EditOp::AddMidi { index, group } => {
                let mut cue = Cue::new(CueKind::Midi(MidiCue {
                    messages: vec![MidiMessage::ProgramChange { channel: 0, program: 0 }],
                }));
                cue.title = "MIDI".into();
                self.place(group, index, vec![cue])?;
            }
            EditOp::AddWait { index, group } => {
                let mut cue = Cue::new(CueKind::Wait(WaitCue { duration: 5.0 }));
                cue.title = t!("session.title.wait");
                cue.advance = Advance::new(AdvanceAt::End);
                self.place(group, index, vec![cue])?;
            }
            EditOp::AddScene { index } => {
                let mut cue = Cue::new(CueKind::Scene(SceneCue {}));
                cue.title = t!("session.title.scene");
                self.place(None, index, vec![cue])?;
            }
            EditOp::AddPlaceholder { index, group } => {
                let mut cue = Cue::new(CueKind::Sound(SoundCue::placeholder()));
                cue.title = t!("session.title.placeholder");
                self.place(group, index, vec![cue])?;
            }
            EditOp::AddGoto { index, group } => {
                let mut cue = Cue::new(CueKind::GoTo(GoToCue::default()));
                cue.title = t!("session.title.goto");
                self.place(group, index, vec![cue])?;
            }
            EditOp::SetOthers { others } => {
                check_others(&others)?;
                self.editor.change(|s| s.others = others);
            }
            EditOp::Normalize { ids } => {
                let waiting = self.normalize(&ids, false);
                if waiting > 0 {
                    notices.push(Notice::Warning(t!("session.normalize_waiting", count = waiting)));
                }
            }
            EditOp::SetNormalization { target, on_import } => {
                if !LOUDNESS_TARGETS.contains(&target) {
                    return Err(t!(
                        "session.loudness_range",
                        min = LOUDNESS_TARGETS.start(),
                        max = LOUDNESS_TARGETS.end()
                    ));
                }
                self.editor.change(|s| (s.loudness_target, s.normalize_on_import) = (target, on_import));
            }
            EditOp::SetLevelLimits { limits } => {
                check_limits(limits)?;
                // Volumes outside are brought in at once (the UI warned with
                // `check_level_limits`), in the same undo step.
                let old = self.editor.show().level_limits;
                self.editor.change(|s| {
                    s.clamp_levels(old, limits);
                    s.level_limits = limits;
                });
            }
            EditOp::LockShowMode { locked } => self.editor.change(|s| s.show_mode_locked = locked),
            EditOp::Delete { ids } => self.editor.delete(&ids),
            EditOp::Move { ids, to } => {
                self.editor.move_to(&ids, to);
            }
            EditOp::Duplicate { id, linked } => {
                self.editor.duplicate(id, linked);
            }
            EditOp::LinkCues { source, ids } => self.editor.link(source, &ids)?,
            EditOp::UnlinkCues { ids } => self.editor.unlink(&ids),
            EditOp::Copy { ids } => {
                self.copy(&ids)?;
                return Ok(());
            }
            EditOp::Cut { ids } => {
                self.copy(&ids)?;
                self.editor.delete(&ids);
            }
            EditOp::Paste { after, linked } => self.paste(after, linked)?,
            EditOp::PasteReplace { ids, swap } => self.paste_replace(&ids, swap)?,
            EditOp::Relink { dir, copy_outside } => {
                if self.search.is_some() {
                    return Err(t!("session.search_running"));
                }
                let search = jobs::search(self.editor.show().clone(), self.file.media_dir(), dir);
                self.search = Some((search, copy_outside));
                return Ok(());
            }
            EditOp::Renumber => self.editor.renumber(),
            EditOp::Group { ids } => {
                self.editor.group(&ids)?;
            }
            EditOp::Ungroup { ids } => {
                self.editor.ungroup(&ids);
            }
            EditOp::Undo => {
                self.editor.undo();
            }
            EditOp::Redo => {
                self.editor.redo();
            }
        }
        // Files already measured (used by other cues too) at once.
        self.run_auto_normalize();
        // The runner learns about copies before it sees their cues.
        self.start_copies();
        self.runner.set_show(self.editor.show().clone());
        if self.file.show.media_root != self.editor.show().media_root {
            self.file.show.media_root = self.editor.show().media_root.clone();
            self.runner.set_media_dir(self.file.media_dir());
            self.media.clear();
        }
        self.refresh_media(false);
        self.enforce_show_mode_lock(notices);
        Ok(())
    }

    /// A cue while it is being edited (a slider being dragged): running
    /// cues follow it, but the show and its history only change with the
    /// `UpdateCue` that ends the edit.
    pub fn live_cue(&mut self, cue: Cue) {
        if !self.settings.show_mode {
            self.runner.live_edit(cue);
        }
    }

    pub fn close_check(&self, now: Instant) -> CloseCheck {
        let state = self.runner.state(now);
        CloseCheck {
            show_mode: self.settings.show_mode,
            dirty: self.editor.is_dirty(),
            running: state.active.len() + state.waiting.len() + state.gaps.len(),
            imports: self.copies.len(),
        }
    }

    /// The language changed: texts the session holds (of the audio output) are put into it. True if the view
    /// changed.
    pub fn language_changed(&mut self) -> bool {
        let mut changed = self.output.relocalize();
        if let Some(preview) = &mut self.preview_output {
            changed |= preview.relocalize();
        }
        changed
    }

    pub fn view(&self) -> ShowView<'_> {
        let show = self.editor.show();
        let cue_info = show.all().into_iter().filter_map(|c| Some((c.id, self.cue_info(c)?))).collect();
        let media_dir = self.file.media_dir();
        ShowView {
            path: &self.file.path,
            name: show.name.clone(),
            media_dir_exists: media_dir.is_dir(),
            media_dir,
            show,
            dirty: self.editor.is_dirty(),
            can_undo: self.editor.can_undo(),
            can_redo: self.editor.can_redo(),
            can_paste: !self.clipboard.is_empty(),
            audio: self.output.info(),
            audio_frames: self.output.callback_frames(),
            audio_error: self.audio_error.as_ref().map(ToString::to_string),
            audio_lost: self.output.lost(),
            keep_alive: self.keep_alive.state(),
            settings: &self.settings,
            cue_info,
            recovery: self
                .recovery
                .as_ref()
                .map(|(_, at)| at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)),
            osc: OscView {
                listening: self.osc_in.as_ref().map(OscListener::port),
                error: self.osc_error.as_deref(),
            },
            preview: PreviewView {
                device: self.preview_output.as_ref().map(|o| o.info().device.as_str()),
                error: self.preview_error.as_deref(),
                lost: self.preview_output.as_ref().and_then(AudioOutput::lost),
            },
            midi: MidiView {
                port: self.midi.as_ref().map(MidiIn::name),
                error: self.midi_error.as_deref(),
                learning: self.learning,
                out: self.midi_out_name.as_deref(),
                out_error: self.midi_out_error.as_deref(),
            },
            fixed_keys: keys::fixed_keys(),
            default_keys: keys::default_keys(),
        }
    }

    fn cue_info(&self, cue: &Cue) -> Option<CueInfo> {
        let (target_missing, repeat_blocked) = (target_missing(self.editor.show(), cue), repeat_blocked(cue));
        let mut info = self.media_info(cue);
        let problems = self.problems(cue);
        let show = self.editor.show();
        let hotkey_shared: Vec<CueId> = match &cue.hotkey {
            Some(key) if show.index_of(cue.id).is_some() => show
                .cues
                .iter()
                .filter(|c| c.id != cue.id && c.hotkey.as_ref().is_some_and(|k| k.same(key)))
                .map(|c| c.id)
                .collect(),
            _ => Vec::new(),
        };
        if target_missing || repeat_blocked || !problems.is_empty() || !hotkey_shared.is_empty() {
            let info = info.get_or_insert_default();
            (info.target_missing, info.repeat_blocked, info.problems, info.hotkey_shared) =
                (target_missing, repeat_blocked, problems, hotkey_shared);
        }
        info
    }

    fn problems(&self, cue: &Cue) -> Vec<String> {
        let show = self.editor.show();
        let mut problems = check_values(cue, show);
        problems.extend(trigger_problems(show, &self.settings, cue));
        problems.extend(target_problems(show, cue));
        let duration = |file: &String| self.media.get(file).and_then(|m| m.duration);
        problems.extend(start_from_problems(show, cue, duration));
        problems.extend(position_problems(show, cue, duration));
        // A group's files are its cues' (which report them).
        let files = if matches!(cue.kind, CueKind::Group(_)) { Vec::new() } else { cue.media_files() };
        for (i, file) in files.iter().enumerate() {
            if let Some(reason) = self.media.get(*file).and_then(|m| m.unreadable.as_deref()) {
                problems.push(match cue.kind {
                    CueKind::Playlist(_) => t!("session.item_unreadable", item = i + 1, reason = reason),
                    _ => t!("session.file_unreadable", reason = reason),
                });
            }
        }
        problems
    }

    fn media_info(&self, cue: &Cue) -> Option<CueInfo> {
        let issue = self.issues.iter().find(|i| match i {
            MediaIssue::Missing { cue: id, .. } | MediaIssue::CaseMismatch { cue: id, .. } => *id == cue.id,
        });
        // A file being copied in is not missing, only not there yet.
        let importing = |f: &String| self.copies.iter().any(|c| &c.stored == f);
        let probing = |f: &String| self.media.get(f).is_some_and(|m| m.probing);
        let missing_in = |ids: &[CueId]| {
            self.issues.iter().any(
                |i| matches!(i, MediaIssue::Missing { cue: id, stored } if ids.contains(id) && !importing(stored)),
            )
        };
        let missing = missing_in(&[cue.id]);
        let case_mismatch = match issue {
            Some(MediaIssue::CaseMismatch { actual, .. }) => Some(actual.clone()),
            _ => None,
        };
        if let CueKind::Group(_) = &cue.kind {
            // Shows whether anything inside lacks its file.
            return Some(CueInfo { missing: missing_in(&cue.descendants()), ..CueInfo::default() });
        }
        if let CueKind::Playlist(p) = &cue.kind {
            let limits = self.editor.show().level_limits;
            let items: Vec<SoundCue> = (0..p.items.len()).filter_map(|i| p.item(i)).collect();
            // All items, each from its start to its end at its rate, plus
            // the gaps between them (real time); endless when repeating.
            let total = items
                .iter()
                .map(|item| {
                    let d = self.media.get(&item.file).and_then(|m| m.duration)?;
                    Some(
                        (item.end.unwrap_or(d).min(d) - item.start.max(0.0)).max(0.0)
                            / item.speed.factors().0,
                    )
                })
                .sum::<Option<f64>>();
            let item_timing = items.iter().map(|item| timing_warnings(self.timing(item))).collect();
            let gaps = p.gap.max(0.0) * p.items.len().saturating_sub(1) as f64;
            let duration = total.filter(|_| !p.repeat).map(|t| t + gaps);
            let missing_items = items
                .iter()
                .map(|SoundCue { file: f, .. }| {
                    !importing(f)
                        && self.issues.iter().any(
                            |i| matches!(i, MediaIssue::Missing { cue: id, stored } if *id == cue.id && stored == f),
                        )
                })
                .collect();
            return Some(CueInfo {
                duration,
                missing,
                missing_items,
                item_timing,
                case_mismatch,
                probing: items.iter().any(|item| probing(&item.file)),
                importing: items.iter().any(|item| importing(&item.file)),
                level_capped: items
                    .iter()
                    .any(|item| limits.caps(item.volume_db, item.normalize_db.unwrap_or(0.0))),
                level_silenced: limits.silences(p.volume_db),
                loudness: items.iter().map(|item| self.file_loudness(&item.file)).collect(),
                measuring: items.iter().any(|item| self.media.get(&item.file).is_some_and(|m| m.measuring)),
                normalize_stale: items.iter().any(|item| self.normalize_stale(item)),
                ..CueInfo::default()
            });
        }
        let CueKind::Sound(s) = &cue.kind else { return None };
        let file = self.media.get(&s.file);
        let file_duration = file.and_then(|f| f.duration);
        let duration = file_duration.and_then(|d| {
            // Real time at the cue's rate.
            let range = (s.end.unwrap_or(d).min(d) - s.start).max(0.0) / s.speed.factors().0;
            match s.loop_mode {
                // Vamps last until devamped.
                _ if !s.vamps.is_empty() => None,
                LoopMode::Off => Some(range),
                LoopMode::Times(n) => Some(range * n.max(1) as f64),
                LoopMode::Forever => None,
            }
        });
        let timing = timing_warnings(self.timing(s));
        Some(CueInfo {
            file_duration,
            duration,
            missing,
            case_mismatch,
            file_version: file.map_or(0, |f| f.version),
            timing,
            probing: probing(&s.file),
            importing: importing(&s.file),
            level_capped: self.editor.show().level_limits.caps(s.volume_db, s.normalize_db.unwrap_or(0.0)),
            level_silenced: self.editor.show().level_limits.silences(s.volume_db),
            loudness: vec![self.file_loudness(&s.file)],
            measuring: file.is_some_and(|f| f.measuring),
            normalize_stale: self.normalize_stale(s),
            ..CueInfo::default()
        })
    }

    fn file_loudness(&self, stored: &str) -> Option<FileLoudness> {
        let loudness = self.media.get(stored)?.loudness?;
        let suggested_db = loudness.offset(self.editor.show().loudness_target, NORMALIZE_CEILING);
        Some(FileLoudness { lufs: loudness.lufs, peak_db: loudness.peak_db, suggested_db })
    }

    /// The stored offset of a sound cue (or playlist item) is off what its
    /// file needs now.
    fn normalize_stale(&self, s: &SoundCue) -> bool {
        let suggested = self.file_loudness(&s.file).and_then(|l| l.suggested_db);
        matches!((s.normalize_db, suggested), (Some(stored), Some(now)) if (stored - now).abs() > STALE_OFFSET)
    }

    /// Normalizes the cues `ids` (and a group's cues) from their files'
    /// measurements; with `only_new`, only files without an offset. Returns
    /// the number of files left out because they are not measured yet.
    fn normalize(&mut self, ids: &[CueId], only_new: bool) -> usize {
        let (changes, waiting) = normalization(self.editor.show(), ids, only_new, |f| self.file_offset(f));
        if !changes.is_empty() {
            self.editor.change(|show| set_offsets(show, &changes, only_new));
        }
        waiting
    }

    /// The offset normalizing a file needs (see [`normalization`]).
    fn file_offset(&self, stored: &str) -> Option<Option<f64>> {
        let loudness = self.media.get(stored)?.loudness?;
        Some(loudness.offset(self.editor.show().loudness_target, NORMALIZE_CEILING))
    }

    /// Re-checks files, has the lengths of files not seen yet read (with
    /// `report`, their timing issues reported then) and has the watcher
    /// follow the show's files.
    fn refresh_media(&mut self, report: bool) {
        let media = self.file.media_dir();
        self.issues = paths::check_media(self.editor.show(), &media);
        let mut watched = Vec::new();
        for file in self.editor.show().cues.iter().flat_map(Cue::media_files) {
            let path = paths::resolve(&media, file);
            match self.media.get_mut(file) {
                None => {
                    self.media.insert(file.clone(), MediaFile::new(paths::stamp(&path), report));
                    self.prober.request(file, path.clone(), 0);
                }
                // Back in the show after a while unwatched: it may have changed.
                Some(m) if self.watched.binary_search(&path).is_err() => {
                    let now = paths::stamp(&path);
                    if now != m.stamp {
                        let version = m.reread(now);
                        self.prober.request(file, path.clone(), version);
                    }
                }
                Some(_) => {}
            }
            watched.push(path);
        }
        watched.sort();
        watched.dedup();
        if watched != self.watched {
            self.watcher.watch(watched.clone());
            self.watched = watched;
        }
    }

    /// Turns a picked file into a stored path. A file outside the media
    /// folder that is to be copied gets its name there right away; the copy
    /// is planned (see `start_copies`). Never reads a file. `None`: skipped.
    fn store_media(
        &mut self,
        path: &Path,
        import: &Import,
        notices: &mut Vec<Notice>,
    ) -> Result<Option<String>, String> {
        let media = self.file.media_dir();
        let canonical = real_path(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let media_canonical = real_path(&media).unwrap_or(media.clone());
        if let Some(rel) = paths::relativize(&media_canonical, &canonical) {
            return Ok(Some(rel));
        }
        if !import.copy_outside {
            notices.push(Notice::Warning(t!("session.outside_show_folder", path = path.display())));
            return Ok(Some(canonical.to_string_lossy().replace('\\', "/")));
        }
        let choice = import.on_clash.get(path).copied();
        let name =
            canonical.file_name().and_then(|n| n.to_str()).ok_or_else(|| t!("session.invalid_file_name"))?;
        // Names of copies under way or planned are taken, too.
        let taken = |n: &str| {
            self.copies.iter().any(|c| c.stored == n) || self.planned.iter().any(|(_, _, s)| s == n)
        };
        match choice {
            Some(OnClash::Skip) => return Ok(None),
            Some(OnClash::UseExisting) if media.join(name).is_file() || taken(name) => {
                return Ok(Some(name.to_owned()));
            }
            _ => {}
        }
        let stored = paths::free_name(&media, name, taken);
        // A rename the operator chose needs no warning.
        if stored != name && choice != Some(OnClash::Rename) {
            notices.push(Notice::Warning(t!("session.name_taken", name = name, stored = stored)));
        }
        self.planned.push((canonical, paths::resolve(&media, &stored), stored.clone()));
        Ok(Some(stored))
    }

    /// `store_media` for every path, leaving out skipped ones.
    fn store_all(
        &mut self,
        paths: &[PathBuf],
        import: &Import,
        notices: &mut Vec<Notice>,
    ) -> Result<Vec<String>, String> {
        let mut files = Vec::new();
        for path in paths {
            files.extend(self.store_media(path, import, notices)?);
        }
        Ok(files)
    }

    /// Copies of the cues, in list order, replace the clipboard.
    fn copy(&mut self, ids: &[CueId]) -> Result<(), String> {
        self.clipboard = self.copies(ids)?;
        Ok(())
    }

    /// Copies of the cues, in list order, as the clipboard holds them.
    fn copies(&self, ids: &[CueId]) -> Result<Vec<Cue>, String> {
        let cues: Vec<Cue> =
            self.editor.show().all().into_iter().filter(|c| ids.contains(&c.id)).cloned().collect();
        if cues.is_empty() {
            return Err(t!("edit.cue_not_found"));
        }
        Ok(cues)
    }

    fn paste(&mut self, after: Option<CueId>, linked: bool) -> Result<(), String> {
        if self.clipboard.is_empty() {
            return Ok(());
        }
        let show = self.editor.show();
        let (group, index) = match after {
            None => (None, None),
            Some(id) => {
                let list = show.siblings(id).ok_or_else(|| t!("edit.cue_not_found"))?;
                let at = list.iter().position(|c| c.id == id).map(|i| i + 1);
                (show.parent(id).map(|g| g.id), at)
            }
        };
        self.paste_at(group, index, linked)
    }

    /// Deletes `ids` and pastes at the place of the first of them, one undo
    /// step; `swap` puts the deleted cues on the clipboard.
    fn paste_replace(&mut self, ids: &[CueId], swap: bool) -> Result<(), String> {
        if self.clipboard.is_empty() || ids.is_empty() {
            return Ok(());
        }
        let show = self.editor.show();
        // The first in list order: a group comes before its cues, so the
        // place is never inside a group that goes too.
        let first =
            show.all().into_iter().find(|c| ids.contains(&c.id)).ok_or_else(|| t!("edit.cue_not_found"))?.id;
        let index = show.siblings(first).and_then(|list| list.iter().position(|c| c.id == first));
        let group = show.parent(first).map(|g| g.id);
        let removed = self.copies(ids)?;
        self.editor.delete(ids);
        if let Err(e) = self.paste_at(group, index, false) {
            self.editor.revert();
            return Err(e);
        }
        self.editor.join_last();
        if swap {
            self.clipboard = removed;
        }
        Ok(())
    }

    /// Inserts the clipboard into the main list or `group`, at `index` (default: end).
    fn paste_at(&mut self, group: Option<CueId>, index: Option<usize>, linked: bool) -> Result<(), String> {
        if self.clipboard.is_empty() {
            return Ok(());
        }
        let show = self.editor.show();
        if group.is_some() && self.clipboard.iter().any(|c| matches!(c.kind, CueKind::Group(_))) {
            return Err(t!("session.no_group_in_group"));
        }
        if group.is_some() && self.clipboard.iter().any(Cue::is_scene) {
            return Err(t!("session.scene_main_only"));
        }
        let taken: std::collections::HashSet<&str> =
            show.all().into_iter().map(|c| c.number.as_str()).collect();
        let mut new = self.clipboard.clone();
        // Originals that get a link for their copies.
        let mut originals = Vec::new();
        for cue in &mut new {
            let original = show.cue(cue.id).filter(|o| linked && o.can_link());
            cue.renew_ids();
            cue.hotkey = None;
            cue.midi_trigger = None;
            if let Some(o) = original {
                let link = o.link.unwrap_or_default();
                cue.link = Some(link);
                originals.push((o.id, link));
            }
            if group.is_some() || taken.contains(cue.number.as_str()) {
                cue.number.clear();
            }
        }
        self.place_with(group, index, new, |s| {
            for (id, link) in originals {
                if let Some(c) = s.cue_mut(id) {
                    c.link = Some(link);
                }
            }
        })
    }

    /// Inserts new cues (one undo step) into the main list or a group, at
    /// `index` (default: end), numbering those without a number by
    /// [`new_numbers`]: between 2 and 3 as 2.5, at the end of group 7 as 7.3 …
    fn place(&mut self, group: Option<CueId>, index: Option<usize>, new: Vec<Cue>) -> Result<(), String> {
        self.place_with(group, index, new, |_| {})
    }

    /// [`Session::place`], with `also` applied in the same undo step.
    fn place_with(
        &mut self,
        group: Option<CueId>,
        index: Option<usize>,
        mut new: Vec<Cue>,
        also: impl FnOnce(&mut Show),
    ) -> Result<(), String> {
        let show = self.editor.show();
        if group.is_some() && new.iter().any(Cue::is_scene) {
            return Err(t!("session.scene_main_only"));
        }
        let (list, prefix) = match group {
            None => (&show.cues, String::new()),
            Some(g) => match show.cue(g) {
                Some(Cue { kind: CueKind::Group(gr), number, .. }) => {
                    (&gr.cues, if number.is_empty() { String::new() } else { format!("{number}.") })
                }
                _ => return Err(t!("session.group_not_found")),
            },
        };
        let at = index.unwrap_or(list.len()).min(list.len());
        let mut taken: HashSet<&str> = show.all().into_iter().map(|c| c.number.as_str()).collect();
        taken.extend(new.iter().map(|c| c.number.as_str()));
        // Scene markers get no number.
        let unnumbered = |c: &Cue| c.number.is_empty() && !c.is_scene();
        let count = new.iter().filter(|c| unnumbered(c)).count();
        let numbers = new_numbers(list, &prefix, at, count, &taken);
        for (cue, n) in new.iter_mut().filter(|c| unnumbered(c)).zip(numbers) {
            cue.number = n;
        }
        self.editor.change(|s| {
            let list = match group {
                None => &mut s.cues,
                Some(g) => match s.cue_mut(g) {
                    Some(Cue { kind: CueKind::Group(gr), .. }) => &mut gr.cues,
                    _ => return,
                },
            };
            list.splice(at..at, new);
            also(s);
        });
        Ok(())
    }
}

/// The stored paths the cues of `shows` and `clipboard` use, each once and
/// sorted (the undo history holds many copies of the same show).
fn media_in<'a>(shows: impl Iterator<Item = &'a Show>, clipboard: &[Cue]) -> Vec<String> {
    let cues = shows.flat_map(|s| &s.cues).chain(clipboard);
    let files: std::collections::BTreeSet<&String> = cues.flat_map(Cue::media_files).collect();
    files.into_iter().cloned().collect()
}

/// `Path::canonicalize` without Windows' verbatim prefix (`\\?\C:\…`
/// becomes `C:\…`, `\\?\UNC\server\share\…` becomes `\\server\share\…`), so
/// paths stored in the show (a media folder or file outside it) stay
/// readable and usable; std adds the prefix itself where a path needs it.
pub(crate) fn real_path(path: &Path) -> std::io::Result<PathBuf> {
    let real = path.canonicalize()?;
    if cfg!(windows)
        && let Some(plain) = real.to_str().and_then(without_verbatim)
    {
        return Ok(PathBuf::from(plain));
    }
    Ok(real)
}

/// A verbatim Windows path as a plain one, where that names the same file:
/// a drive or UNC path without components the plain form treats differently
/// (`.`/`..`, trailing dots or spaces, device names like `CON`).
fn without_verbatim(path: &str) -> Option<String> {
    let (plain, rest) = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        (format!(r"\\{rest}"), rest)
    } else {
        let rest = path.strip_prefix(r"\\?\")?;
        let drive = rest.as_bytes();
        if !(drive.len() >= 3 && drive[0].is_ascii_alphabetic() && drive[1] == b':' && drive[2] == b'\\') {
            return None;
        }
        (rest.to_owned(), &rest[3..])
    };
    const DEVICES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    let plain_safe = |part: &str| {
        let stem = part.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
        let numbered = (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit();
        part != "."
            && part != ".."
            && !part.ends_with(['.', ' '])
            && !part.contains('/')
            && !DEVICES.contains(&stem.as_str())
            && !numbered
    };
    rest.split('\\').filter(|p| !p.is_empty()).all(plain_safe).then_some(plain)
}

/// Output ids must be unique and not empty, names given. The main output
/// (the default of new cues) stays, and an output still used by a cue
/// cannot go.
fn check_outputs(show: &Show, outputs: &[Output]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for o in outputs {
        if o.id.trim().is_empty() || !ids.insert(o.id.as_str()) {
            return Err(t!("session.output_id", id = o.id));
        }
        if o.name.trim().is_empty() {
            return Err(t!("session.output_name"));
        }
    }
    if !ids.contains(Output::MAIN) {
        return Err(t!("session.main_output"));
    }
    for cue in show.all() {
        let used = match &cue.kind {
            CueKind::Sound(s) => Some(&s.output),
            CueKind::Playlist(p) => Some(&p.output),
            kind => match kind.targets() {
                Some(Targets::Output { output }) => Some(output),
                _ => None,
            },
        };
        if let Some(id) = used.filter(|id| !ids.contains(id.as_str())) {
            let name = show.outputs.iter().find(|o| &o.id == id).map_or(id.as_str(), |o| o.name.as_str());
            return Err(t!("session.output_in_use", output = name, cue = cue_label(cue)));
        }
    }
    Ok(())
}

/// A start cue's "start from" that its targets cannot play from: past a
/// sound cue's end (it ends at once), before its start (it plays from
/// there), or a target that is no sound cue (it starts from the top).
fn start_from_problems(show: &Show, cue: &Cue, duration: impl Fn(&String) -> Option<f64>) -> Vec<String> {
    let CueKind::StartStop(c) = &cue.kind else { return Vec::new() };
    let Some(from) = c.from.filter(|_| c.action == StartStopAction::Start) else { return Vec::new() };
    let targets = chosen_targets(show, cue);
    let mut problems = Vec::new();
    for target in targets.iter().filter_map(|t| show.cue(*t)) {
        let name = cue_label(target);
        let CueKind::Sound(s) = &target.kind else {
            problems.push(t!("problem.start_from_sound_only", cue = name));
            continue;
        };
        if let Some(end) = sound_end(s, &duration).filter(|end| from >= *end) {
            problems.push(t!(
                "problem.start_from_after_end",
                time = secs_text(from),
                cue = name,
                end = secs_text(end)
            ));
        } else if from < s.start {
            problems.push(t!(
                "problem.start_from_before_start",
                time = secs_text(from),
                cue = name,
                start = secs_text(s.start)
            ));
        }
    }
    problems
}

/// The cues a control cue's targets name: the chosen ones (also deleted
/// ones), or the nearest one before it that it can act on, as playback
/// picks it (a fade cue in between is passed over).
fn chosen_targets(show: &Show, cue: &Cue) -> Vec<CueId> {
    match cue.kind.targets() {
        Some(Targets::Cues(ids)) => ids.clone(),
        Some(Targets::Previous(_)) => show.previous_control_target_before(cue.id).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Targets a control cue cannot act on: none chosen, deleted ones, no entry
/// before it, a stopwatch that is gone, "all" or an output for a start cue
/// (which starts only chosen cues), or start cues starting themselves.
fn target_problems(show: &Show, cue: &Cue) -> Vec<String> {
    let mut problems = Vec::new();
    let starts = matches!(&cue.kind, CueKind::StartStop(c) if c.action == StartStopAction::Start);
    match &cue.kind {
        CueKind::Stopwatch(w) => match &w.watch {
            None => problems.push(t!("problem.no_stopwatch")),
            Some(id) if !show.stopwatches.iter().any(|s| &s.id == id) => {
                problems.push(t!("problem.stopwatch_deleted"));
            }
            _ => {}
        },
        kind => match kind.targets() {
            None => {}
            Some(Targets::Cues(ids)) if !ids.is_empty() => {
                let gone = ids.iter().filter(|id| show.cue(**id).is_none()).count();
                if gone == ids.len() {
                    problems.push(t!("problem.target_deleted"));
                } else if gone > 0 {
                    problems.push(t!("problem.targets_deleted", gone = gone, total = ids.len()));
                }
            }
            Some(Targets::Previous(_)) if show.previous_control_target_before(cue.id).is_none() => {
                problems.push(t!("problem.no_previous"));
            }
            Some(Targets::All(_) | Targets::Output { .. }) if starts => {
                problems.push(t!("problem.start_needs_chosen"));
            }
            Some(Targets::Output { output }) if !show.outputs.iter().any(|o| &o.id == output) => {
                problems.push(t!("problem.output_missing", output = output));
            }
            _ => {}
        },
    }
    if starts_itself(show, cue) {
        problems.push(t!("problem.starts_itself"));
    }
    problems
}

/// True if starting the start cue `cue` starts it again at once, directly
/// or through other start cues (none of them with a pre-wait in between).
fn starts_itself(show: &Show, cue: &Cue) -> bool {
    let is_start = |c: &Cue| matches!(&c.kind, CueKind::StartStop(s) if s.action == StartStopAction::Start);
    if !is_start(cue) || cue.pre_wait > 0.0 {
        return false;
    }
    let mut seen = HashSet::new();
    let mut stack = vec![cue.id];
    while let Some(id) = stack.pop() {
        let Some(current) = show.cue(id) else { continue };
        for target in chosen_targets(show, current) {
            if target == cue.id {
                return true;
            }
            if show.cue(target).is_some_and(|t| is_start(t) && t.pre_wait <= 0.0) && seen.insert(target) {
                stack.push(target);
            }
        }
    }
    false
}

fn timing_warnings(issues: Vec<TimingIssue>) -> Vec<TimingWarning> {
    issues.into_iter().map(|issue| TimingWarning { issue, message: issue.to_string() }).collect()
}

/// Where a sound cue ends: its end position, else its file's end (`None`
/// while the file's length is not known).
fn sound_end(s: &SoundCue, duration: impl Fn(&String) -> Option<f64>) -> Option<f64> {
    match (s.end, duration(&s.file)) {
        (Some(end), Some(file)) => Some(end.min(file)),
        (end, file) => end.or(file),
    }
}

/// A position cue's jump its targets cannot make: past a sound cue's end
/// (it ends at once), a mark it does not have, or anything but a relative
/// jump on a playlist (which is ignored). A group as target: its cues.
fn position_problems(show: &Show, cue: &Cue, duration: impl Fn(&String) -> Option<f64>) -> Vec<String> {
    let CueKind::Position(p) = &cue.kind else { return Vec::new() };
    let mut targets: Vec<&Cue> = Vec::new();
    for target in chosen_targets(show, cue).into_iter().filter_map(|t| show.cue(t)) {
        match &target.kind {
            CueKind::Group(_) => targets.extend(target.descendants().into_iter().filter_map(|d| show.cue(d))),
            _ => targets.push(target),
        }
    }
    let mut problems = Vec::new();
    for target in targets {
        let name = cue_label(target);
        match (&target.kind, p.to) {
            (CueKind::Playlist(_), PositionTo::Relative { .. }) => {}
            (CueKind::Playlist(_), _) => problems.push(t!("problem.position_playlist", cue = name)),
            (CueKind::Sound(s), PositionTo::Absolute { secs }) => {
                if let Some(end) = sound_end(s, &duration).filter(|end| secs >= *end) {
                    problems.push(t!(
                        "problem.position_after_end",
                        time = secs_text(secs),
                        cue = name,
                        end = secs_text(end)
                    ));
                } else if secs < s.start {
                    problems.push(t!(
                        "problem.position_before_start",
                        time = secs_text(secs),
                        cue = name,
                        start = secs_text(s.start)
                    ));
                }
            }
            (
                CueKind::Sound(s),
                PositionTo::Mark { .. } | PositionTo::NextMark | PositionTo::PreviousMark,
            ) if s.marks.is_empty() => {
                problems.push(t!("problem.no_marks", cue = name));
            }
            (CueKind::Sound(s), PositionTo::Mark { index }) if index == 0 || index > s.marks.len() => {
                problems.push(t!("problem.mark_missing", index = index, cue = name, count = s.marks.len()));
            }
            _ => {}
        }
    }
    problems
}

/// Seconds as shown in messages: "2.2 s" (with a decimal comma in German).
fn secs_text(secs: f64) -> String {
    format!("{secs:.1} s").replace('.', &t!("format.decimal"))
}

/// File name without extension, the title of a new sound cue.
fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn collect_media_users(cue: &Cue, path: &str, ids: &mut Vec<CueId>) {
    if let CueKind::Group(group) = &cue.kind {
        for child in &group.cues {
            collect_media_users(child, path, ids);
        }
    } else if cue.media_files().iter().any(|file| file.as_str() == path) {
        ids.push(cue.id);
    }
}

fn replace_media_path(cue: &mut Cue, old: &str, new: &str, auto_normalize: bool) {
    match &mut cue.kind {
        CueKind::Sound(sound) if sound.file == old => {
            sound.file = new.to_owned();
            if auto_normalize {
                sound.normalize_db = None;
            }
        }
        CueKind::Playlist(playlist) => {
            // The entries keep their overrides; the old file's offset does not fit.
            for item in playlist.items.iter_mut().filter(|item| item.file == old) {
                (item.file, item.normalize_db) = (new.to_owned(), None);
            }
        }
        CueKind::Group(group) => {
            for child in &mut group.cues {
                replace_media_path(child, old, new, auto_normalize);
            }
        }
        _ => {}
    }
}

fn cue_label(cue: &Cue) -> String {
    match (cue.number.is_empty(), cue.title.is_empty()) {
        (false, false) => t!("cue.label_both", number = cue.number, title = cue.title),
        (false, true) => cue.number.clone(),
        (true, _) => t!("cue.label_title", title = cue.title),
    }
}

/// A MIDI output port as the runner's sink.
struct MidiOutSink(MidiOut);

impl MidiSink for MidiOutSink {
    fn send(&mut self, message: &[u8]) -> Result<(), String> {
        self.0.send(message)
    }
}

/// Gives the runner a UDP socket for OSC cues.
fn attach_osc_out(runner: &mut Runner, notices: &mut Vec<Notice>) {
    match UdpOut::new() {
        Ok(out) => runner.set_osc_out(Box::new(out)),
        Err(e) => notices.push(Notice::Warning(t!("session.osc_send_failed", error = e))),
    }
}

/// Whether applying settings opens the audio output again: always when the
/// audio settings `changed` (refused while cues run, `idle` false), and to
/// try a `failed` or lost device again only while nothing runs. Otherwise a
/// device that drops mid-show would block every other setting (show mode,
/// keep awake) until all cues are stopped; a lost one comes back by itself.
fn audio_reopen(changed: bool, failed: bool, idle: bool) -> Result<bool, String> {
    if changed && !idle {
        return Err(t!("session.audio_device_busy"));
    }
    Ok(changed || (failed && idle))
}

/// Opens the configured device; on failure a silent output takes its place
/// and the error is returned for display.
fn open_output(
    settings: &LocalSettings,
    notices: &mut Vec<Notice>,
) -> (AudioOutput, Controller, Option<AudioError>) {
    match AudioOutput::open(&main_spec(settings), MAX_VOICES) {
        Ok((output, controller)) => (output, controller, None),
        Err(e) => {
            notices.push(Notice::Error(t!("session.audio_unavailable", error = e)));
            let (output, controller) = AudioOutput::null(48_000, 2, MAX_VOICES);
            (output, controller, Some(e))
        }
    }
}

fn main_spec(settings: &LocalSettings) -> OutputSpec {
    OutputSpec {
        host: settings.host.clone(),
        device: settings.device.clone(),
        channels: settings.channels,
        min_channels: settings.needed_channels(),
        buffer_frames: settings.buffer_frames,
        ..Default::default()
    }
}

/// The separate preview device, if one is set.
fn preview_spec(settings: &LocalSettings, sample_rate: u32) -> Option<OutputSpec> {
    Some(OutputSpec {
        host: settings.host.clone(),
        device: Some(settings.preview_device.clone()?),
        sample_rate: Some(sample_rate),
        min_channels: settings.preview_first_channel() as u16 + 2,
        buffer_frames: settings.buffer_frames,
        ..Default::default()
    })
}

fn probe_of(output: &AudioOutput) -> DeviceProbe {
    DeviceProbe { name: output.info().device.clone(), max_channels: output.info().max_channels }
}

/// Level limits must lie in the range offered and leave room between them.
fn check_limits(LevelLimits { max_db, min_db }: LevelLimits) -> Result<(), String> {
    if !(max_db.is_finite() && min_db.is_finite()) || max_db > MAX_LEVEL_DB || min_db < MIN_LEVEL_DB {
        return Err(t!("session.level_limits", min = MIN_LEVEL_DB, max = MAX_LEVEL_DB));
    }
    if min_db >= max_db - 6.0 {
        return Err(t!("session.level_gap"));
    }
    Ok(())
}

/// The offsets to set when normalizing the cues `ids` (and a group's
/// cues): (cue, stored file, dB) where it differs from the stored one, and
/// how many files are not measured yet. `offset` gives a file's offset;
/// `None` = not measured, `Some(None)` = silent (left as it is). With
/// `only_new`, files that have an offset are left as they are.
fn normalization(
    show: &Show,
    ids: &[CueId],
    only_new: bool,
    offset: impl Fn(&str) -> Option<Option<f64>>,
) -> (Vec<(CueId, String, f64)>, usize) {
    let mut cues: Vec<&Cue> = Vec::new();
    for cue in ids.iter().filter_map(|&id| show.cue(id)) {
        for c in std::iter::once(cue).chain(cue.descendants().into_iter().filter_map(|d| show.cue(d))) {
            if !cues.iter().any(|known| known.id == c.id) {
                cues.push(c);
            }
        }
    }
    let (mut changes, mut waiting) = (Vec::new(), 0);
    for cue in cues {
        let files: Vec<(&String, Option<f64>)> = match &cue.kind {
            CueKind::Sound(s) => vec![(&s.file, s.normalize_db)],
            CueKind::Playlist(p) => p.items.iter().map(|item| (&item.file, item.normalize_db)).collect(),
            _ => continue,
        };
        for (file, stored) in files {
            if only_new && stored.is_some() {
                continue;
            }
            match offset(file) {
                None => waiting += 1,
                Some(Some(db)) if Some(db) != stored => changes.push((cue.id, file.clone(), db)),
                Some(_) => {}
            }
        }
    }
    (changes, waiting)
}

/// Sets the offsets `changes` (from [`normalization`]) where the cue still
/// uses that file; with `only_new`, only where it has none yet. Linked
/// cues follow.
fn set_offsets(show: &mut Show, changes: &[(CueId, String, f64)], only_new: bool) {
    let wanted = |stored: Option<f64>| !only_new || stored.is_none();
    for (id, file, db) in changes {
        let changed = match show.cue_mut(*id).map(|c| &mut c.kind) {
            Some(CueKind::Sound(s)) if &s.file == file && wanted(s.normalize_db) => {
                s.normalize_db = Some(*db);
                true
            }
            Some(CueKind::Playlist(p)) => {
                let mut changed = false;
                for item in p.items.iter_mut().filter(|item| &item.file == file && wanted(item.normalize_db))
                {
                    item.normalize_db = Some(*db);
                    changed = true;
                }
                changed
            }
            _ => false,
        };
        if changed {
            show.spread_link(*id);
        }
    }
}

/// Each `(label, zero-based first channel)` pair must fit the set channel
/// count, or else what the device takes.
fn check_pairs(
    device: &DeviceProbe,
    channels: Option<u16>,
    mut pairs: impl Iterator<Item = (String, usize)>,
) -> Result<(), String> {
    let name = &device.name;
    if let (Some(channels), Some(max)) = (channels, device.max_channels)
        && channels > max
    {
        return Err(t!("session.device_too_few_channels", name = name, max = max, channels = channels));
    }
    let Some(limit) = channels.or(device.max_channels) else { return Ok(()) };
    match pairs.find(|&(_, first)| first + 2 > limit as usize) {
        Some((label, first)) => Err(if channels.is_some() {
            t!(
                "session.pair_missing_set",
                label = label,
                first = first + 1,
                second = first + 2,
                name = name,
                limit = limit
            )
        } else {
            t!(
                "session.pair_missing_has",
                label = label,
                first = first + 1,
                second = first + 2,
                name = name,
                limit = limit
            )
        }),
        None => Ok(()),
    }
}

/// Refuses a hotkey or MIDI trigger outside the main list or on what the
/// app or a global shortcut uses; an empty hotkey becomes none.
fn check_triggers(show: &Show, settings: &LocalSettings, cue: &mut Cue) -> Result<(), String> {
    if cue.hotkey.as_ref().is_some_and(|k| k.code.trim().is_empty()) {
        cue.hotkey = None;
    }
    if cue.second_trigger == SecondTrigger::NextItem && !matches!(cue.kind, CueKind::Playlist(_)) {
        return Err(t!("session.next_item_playlist_only"));
    }
    let main = show.index_of(cue.id).is_some();
    if let Some(key) = &cue.hotkey {
        if !main {
            return Err(t!("session.hotkey_main_only"));
        }
        if keys::is_fixed(key) {
            return Err(t!("keys.fixed", key = key.text()));
        }
        if let Some(action) = keys::bound(&settings.keys, key) {
            return Err(t!("keys.global", key = key.text(), action = action.name()));
        }
    }
    if let Some(trigger) = cue.midi_trigger {
        if !main {
            return Err(t!("session.midi_main_only"));
        }
        if let Some(action) = midi::bound(&settings.midi, trigger) {
            return Err(t!("session.midi_global", action = action.name()));
        }
    }
    Ok(())
}

/// Hotkeys and MIDI triggers that never fire, e.g. after this machine's
/// global shortcuts changed or on a cue moved into a group.
fn trigger_problems(show: &Show, settings: &LocalSettings, cue: &Cue) -> Vec<String> {
    let mut problems = Vec::new();
    // Asked for every cue of the view: no list walk for the many without one.
    if cue.hotkey.is_none() && cue.midi_trigger.is_none() {
        return problems;
    }
    let main = show.index_of(cue.id).is_some();
    if let Some(key) = &cue.hotkey {
        let shown = key.text();
        if key.code.trim().is_empty() {
            problems.push(t!("problem.hotkey_no_key"));
        } else if !main {
            problems.push(t!("problem.hotkey_main_only", key = shown));
        } else if keys::is_fixed(key) {
            problems.push(t!("problem.hotkey_fixed", key = shown));
        } else if let Some(action) = keys::bound(&settings.keys, key) {
            problems.push(t!("problem.hotkey_global", key = shown, action = action.name()));
        }
    }
    if let Some(trigger) = cue.midi_trigger {
        if !main {
            problems.push(t!("problem.midi_main_only"));
        } else if let Some(action) = midi::bound(&settings.midi, trigger) {
            problems.push(t!("problem.midi_global", action = action.name()));
        }
    }
    if cue.release_trigger
        && cue.second_trigger == SecondTrigger::Ignore
        && (cue.hotkey.is_some() || cue.midi_trigger.is_some())
    {
        problems.push(t!("problem.release_ignored"));
    }
    problems
}

/// A go-to cue's target was deleted or is no longer at the top level, a
/// control cue has no target left (or a stopwatch cue's stopwatch is gone),
/// or a loop exit cue waits for the exit of a cue no longer in its target.
fn target_missing(show: &Show, cue: &Cue) -> bool {
    if let Some(Targets::Cues(ids)) = cue.kind.targets()
        && (ids.is_empty() || ids.iter().any(|id| show.cue(*id).is_none()))
    {
        return true;
    }
    match (&cue.kind, cue.advance.at) {
        (CueKind::GoTo(g), _) => g.target.is_some_and(|t| show.index_of(t).is_none()),
        (CueKind::Stopwatch(w), _) => {
            w.watch.as_ref().is_some_and(|id| !show.stopwatches.iter().any(|s| &s.id == id))
        }
        (CueKind::ExitLoop(d), AdvanceAt::LoopExit { of: Some(of) }) => {
            let targets: Vec<&Cue> = match &d.targets {
                Targets::Cues(ids) => ids.iter().filter_map(|t| show.cue(*t)).collect(),
                Targets::Previous(_) => show
                    .previous_control_target_before(cue.id)
                    .and_then(|p| show.cue(p))
                    .into_iter()
                    .collect(),
                // Any running cue may be the one.
                _ => return false,
            };
            cue.advance.start_next && !targets.iter().any(|t| t.id == of || t.descendants().contains(&of))
        }
        _ => false,
    }
}

/// Title of a new control cue.
fn control_title(kind: ControlKind) -> String {
    match kind {
        ControlKind::StartStop => t!("session.control.start"),
        ControlKind::Pause => t!("session.control.pause"),
        ControlKind::Volume => t!("session.control.fade_out"),
        ControlKind::Pan => t!("session.control.pan"),
        ControlKind::Rate => t!("session.control.rate"),
        ControlKind::Position => t!("session.control.position"),
        ControlKind::ExitLoop => t!("session.control.exit_loop"),
        ControlKind::Reset => t!("session.control.reset"),
        ControlKind::Stopwatch => t!("session.control.stopwatch"),
    }
}

/// Repeat can never apply: a GO while the cue runs is ignored, and it never
/// ends by itself.
fn repeat_blocked(cue: &Cue) -> bool {
    cue.passes != Repeat::Off && cue.second_trigger == SecondTrigger::Ignore && !cue.ends_by_itself()
}

/// Longest countdown, seconds (100 hours).
const MAX_COUNTDOWN: f64 = 360_000.0;

/// A scene marker stays in the main list and has nothing that plays: what
/// a cue does when started or what starts it is dropped (it never runs).
fn check_scene(show: &Show, cue: &mut Cue) -> Result<(), String> {
    if !cue.is_scene() {
        return Ok(());
    }
    if show.parent(cue.id).is_some() {
        return Err(t!("session.scene_main_only"));
    }
    let blank = Cue::new(CueKind::Scene(SceneCue {}));
    cue.advance = blank.advance;
    cue.passes = blank.passes;
    cue.pre_wait = blank.pre_wait;
    cue.hotkey = None;
    cue.midi_trigger = None;
    cue.second_trigger = blank.second_trigger;
    cue.release_trigger = false;
    cue.others = None;
    cue.link = None;
    Ok(())
}

/// A go-to cue's target is a top-level cue; one deleted meanwhile is left
/// as it is (it counts as missing).
fn check_go_to(show: &Show, cue: &Cue) -> Result<(), String> {
    if let CueKind::GoTo(GoToCue { target: Some(t), .. }) = &cue.kind
        && show.cue(*t).is_some()
        && show.index_of(*t).is_none()
    {
        return Err(t!("session.goto_top_level"));
    }
    Ok(())
}

fn check_others(others: &OthersEffect) -> Result<(), String> {
    if !others.fade.is_finite() || !(0.0..=MAX_FADE).contains(&others.fade) {
        return Err(t!("session.others_fade", max = MAX_FADE));
    }
    if !others.duck_db.is_finite() || !(-60.0..=0.0).contains(&others.duck_db) {
        return Err(t!("session.others_duck"));
    }
    Ok(())
}

/// A file being copied into the media folder.
#[derive(Clone, Debug, Serialize)]
pub struct ImportState<'a> {
    /// Stored path, as the cues use it.
    pub file: &'a str,
    /// 0…1; 0 while waiting for the copies before it.
    pub progress: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(max_channels: Option<u16>) -> DeviceProbe {
        DeviceProbe { name: "AG08".into(), max_channels }
    }

    #[test]
    fn scene_markers_stay_in_the_main_list_and_drop_what_plays() {
        let mut marker = Cue::new(CueKind::Scene(SceneCue {}));
        marker.advance = Advance::new(AdvanceAt::Start);
        marker.pre_wait = 2.0;
        marker.hotkey = Some(Chord::new("KeyA", "a"));
        marker.passes = Repeat::Forever;
        let group = Cue::new(CueKind::Group(GroupCue { cues: vec![marker.clone()], ..GroupCue::default() }));
        let show = Show { cues: vec![marker.clone()], ..Show::default() };
        let mut cue = marker.clone();
        check_scene(&show, &mut cue).unwrap();
        assert_eq!(
            (cue.advance, cue.pre_wait, cue.hotkey, cue.passes),
            (Advance::default(), 0.0, None, Repeat::Off)
        );
        let show = Show { cues: vec![group], ..Show::default() };
        assert_eq!(check_scene(&show, &mut marker.clone()), Err(t!("session.scene_main_only")));
    }

    #[test]
    fn relinking_media_path_replaces_all_cue_uses() {
        let quieter = jq_show::ItemOverrides { gain_db: -6.0, ..Default::default() };
        let sound =
            Cue::new(CueKind::Sound(SoundCue { normalize_db: Some(1.0), ..SoundCue::new("missing.wav") }));
        let mut list = PlaylistCue::new(vec!["missing.wav".into(), "missing.wav".into(), "other.wav".into()]);
        list.items.iter_mut().zip([2.0, 2.0, 3.0]).for_each(|(item, db)| item.normalize_db = Some(db));
        list.items[1].overrides = quieter;
        let playlist = Cue::new(CueKind::Playlist(list));
        let mut group = Cue::new(CueKind::Group(GroupCue {
            cues: vec![sound.clone(), playlist.clone()],
            ..GroupCue::default()
        }));

        let mut users = Vec::new();
        collect_media_users(&group, "missing.wav", &mut users);
        assert_eq!(users, [sound.id, playlist.id]);

        replace_media_path(&mut group, "missing.wav", "replacement.wav", true);
        let CueKind::Group(group) = group.kind else { panic!("expected group") };
        let CueKind::Sound(sound) = &group.cues[0].kind else { panic!("expected sound cue") };
        assert_eq!(sound.file, "replacement.wav");
        assert_eq!(sound.normalize_db, None);
        let CueKind::Playlist(playlist) = &group.cues[1].kind else { panic!("expected playlist cue") };
        let items: Vec<_> = playlist
            .items
            .iter()
            .map(|item| (item.file.as_str(), item.normalize_db, item.overrides))
            .collect();
        let none = jq_show::ItemOverrides::default();
        assert_eq!(
            items,
            [
                ("replacement.wav", None, none),
                ("replacement.wav", None, quieter),
                ("other.wav", Some(3.0), none)
            ],
            "the entries keep their overrides"
        );
    }

    #[test]
    fn triggers_are_checked_against_fixed_and_global_keys() {
        let settings = LocalSettings::default();
        let mut cue = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        let inner = Cue::new(CueKind::Sound(SoundCue::new("b.wav")));
        let group = Cue::new(CueKind::Group(GroupCue { cues: vec![inner.clone()], ..GroupCue::default() }));
        let show = Show { cues: vec![cue.clone(), group], ..Show::default() };
        let check = |mut cue: Cue| check_triggers(&show, &settings, &mut cue).map(|()| cue);

        cue.hotkey = Some(Chord::new("Space", " ").ctrl());
        assert!(check(cue.clone()).is_ok(), "Ctrl+Space is free");
        cue.hotkey = Some(Chord::new("Space", " "));
        assert_eq!(check(cue.clone()).unwrap_err(), "Space is assigned globally to “GO”");
        cue.hotkey = Some(Chord::new("KeyY", "z").ctrl());
        assert!(check(cue.clone()).is_err(), "Ctrl+Z (QWERTZ) undoes");
        cue.hotkey = Some(Chord::new("", ""));
        assert_eq!(check(cue.clone()).unwrap().hotkey, None, "an empty key is none");
        let mut nested = inner.clone();
        nested.hotkey = Some(Chord::new("F5", ""));
        assert!(check(nested).is_err(), "main list only");

        // A trigger bound globally is refused, and a problem once the binding comes later.
        let note = jq_core::midi::MidiTrigger::Note { channel: 0, note: 60 };
        cue.hotkey = Some(Chord::new("F5", ""));
        cue.midi_trigger = Some(note);
        assert!(check(cue.clone()).is_ok());
        assert_eq!(trigger_problems(&show, &settings, &cue), Vec::<String>::new());
        let mut bound = settings.clone();
        midi::learn(&mut bound.midi, ControlAction::Go, note);
        bound.keys.push(keys::KeyBinding { chord: Chord::new("F5", ""), action: ControlAction::StopAll });
        assert_eq!(
            trigger_problems(&show, &bound, &cue),
            [
                "Hotkey “F5” is assigned globally to “STOP ALL” – the global shortcut takes precedence",
                "The MIDI trigger is assigned globally to “GO” – the global assignment takes precedence",
            ]
        );
        cue.release_trigger = true;
        assert!(trigger_problems(&show, &settings, &cue)[0].starts_with("Release has no effect"));
        cue.hotkey = Some(Chord::new("", "ab"));
        assert_eq!(trigger_problems(&show, &settings, &cue)[0], "Hotkey without a key – never triggered");
    }

    #[test]
    fn normalization_sets_measured_offsets() {
        let sound = |file: &str, db: Option<f64>| {
            Cue::new(CueKind::Sound(SoundCue { normalize_db: db, ..SoundCue::new(file) }))
        };
        let (fresh, done, silent, pending) =
            (sound("a.wav", None), sound("a.wav", Some(-2.0)), sound("s.wav", None), sound("p.wav", None));
        let mut playlist = PlaylistCue::new(vec!["a.wav".into(), "b.wav".into()]);
        playlist.items[1].normalize_db = Some(1.0);
        let list = Cue::new(CueKind::Playlist(playlist));
        let group = Cue::new(CueKind::Group(GroupCue { cues: vec![pending], ..GroupCue::default() }));
        let ids = [fresh.id, done.id, silent.id, list.id, group.id];
        let show = Show { cues: vec![fresh, done, silent, list, group], ..Show::default() };
        let offset = |file: &str| match file {
            "a.wav" => Some(Some(-4.0)),
            "b.wav" => Some(Some(3.0)),
            "s.wav" => Some(None),
            _ => None,
        };

        let (changes, waiting) = normalization(&show, &ids, false, offset);
        let changed: Vec<(CueId, &str, f64)> =
            changes.iter().map(|(id, f, db)| (*id, f.as_str(), *db)).collect();
        assert_eq!(
            changed,
            [
                (ids[0], "a.wav", -4.0),
                (ids[1], "a.wav", -4.0),
                (ids[3], "a.wav", -4.0),
                (ids[3], "b.wav", 3.0)
            ]
        );
        assert_eq!(waiting, 1, "the group's cue is not measured yet");

        // Automatic: only what has no offset yet.
        let (changes, _) = normalization(&show, &ids, true, offset);
        let changed: Vec<(CueId, &str)> = changes.iter().map(|(id, f, _)| (*id, f.as_str())).collect();
        assert_eq!(changed, [(ids[0], "a.wav"), (ids[3], "a.wav")]);
    }

    #[test]
    fn go_to_targets_stay_at_the_top_level_and_may_go_missing() {
        let inner = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        let inner_id = inner.id;
        let group = Cue::new(CueKind::Group(GroupCue { cues: vec![inner], ..GroupCue::default() }));
        let group_id = group.id;
        let show = Show { cues: vec![group], ..Show::default() };
        let go_to = |target| Cue::new(CueKind::GoTo(GoToCue { target: Some(target), start: false }));
        assert!(check_go_to(&show, &go_to(group_id)).is_ok());
        assert!(check_go_to(&show, &go_to(inner_id)).is_err());
        let gone = CueId::new();
        assert!(check_go_to(&show, &go_to(gone)).is_ok(), "deleted meanwhile");
        assert!(!target_missing(&show, &go_to(group_id)));
        assert!(target_missing(&show, &go_to(inner_id)) && target_missing(&show, &go_to(gone)));

        let mut devamp = Cue::new(CueKind::control(ControlKind::ExitLoop, Targets::Cues(vec![group_id])));
        devamp.advance = Advance::new(AdvanceAt::LoopExit { of: Some(inner_id) });
        assert!(!target_missing(&show, &devamp));
        devamp.advance.at = AdvanceAt::LoopExit { of: Some(gone) };
        assert!(target_missing(&show, &devamp));
    }

    #[test]
    fn control_cues_without_a_target_count_as_missing() {
        let mut show = Show::default();
        let empty = Cue::new(CueKind::control(ControlKind::Pan, Targets::Cues(vec![])));
        assert!(target_missing(&show, &empty));
        let previous =
            Cue::new(CueKind::control(ControlKind::Pan, Targets::Previous(PreviousKeyword::Previous)));
        assert!(!target_missing(&show, &previous));
        let mut watch = Cue::new(CueKind::control(ControlKind::Stopwatch, Targets::Cues(vec![])));
        assert!(!target_missing(&show, &watch), "none chosen yet is no missing one");
        if let CueKind::Stopwatch(w) = &mut watch.kind {
            w.watch = Some("w".into());
        }
        assert!(target_missing(&show, &watch));
        show.stopwatches.push(Stopwatch { id: "w".into(), name: "Vorstellung".into(), countdown: None });
        assert!(!target_missing(&show, &watch));
    }

    #[test]
    fn a_start_from_past_the_end_is_a_problem() {
        let mut sound = Cue::new(CueKind::Sound(SoundCue { start: 1.0, ..SoundCue::new("kurz.wav") }));
        sound.number = "1".into();
        let mut cut = Cue::new(CueKind::Sound(SoundCue { end: Some(4.0), ..SoundCue::new("lang.wav") }));
        cut.number = "2".into();
        let mut wait = Cue::new(CueKind::Wait(WaitCue { duration: 1.0 }));
        wait.number = "3".into();
        let ids = vec![sound.id, cut.id, wait.id];
        let start = |from: f64, targets: Targets| {
            let mut c = Cue::new(CueKind::control(ControlKind::StartStop, targets));
            if let CueKind::StartStop(s) = &mut c.kind {
                s.from = Some(from);
            }
            c
        };
        let show = Show { cues: vec![sound, cut, wait], ..Show::default() };
        let length = |file: &String| Some(if file == "kurz.wav" { 2.2 } else { 300.0 });
        assert_eq!(
            start_from_problems(&show, &start(3.0, Targets::Cues(ids.clone())), length),
            [
                "Start from 3.0 s is behind the end of cue 1 (2.2 s) – the cue ends at once",
                "“Start from” only applies to sound cues – cue 3 starts from the beginning",
            ]
        );
        assert_eq!(
            start_from_problems(&show, &start(5.0, Targets::Cues(ids.clone())), length)[1],
            "Start from 5.0 s is behind the end of cue 2 (4.0 s) – the cue ends at once",
            "its own end, not the file's"
        );
        assert_eq!(
            start_from_problems(&show, &start(0.5, Targets::Cues(vec![ids[0]])), length),
            ["Start from 0.5 s is before the start point of cue 1 (1.0 s) – plays from the start point"]
        );
        assert!(start_from_problems(&show, &start(1.5, Targets::Cues(vec![ids[0]])), length).is_empty());
        assert!(
            start_from_problems(&show, &start(9.0, Targets::Cues(vec![ids[0]])), |_| None).is_empty(),
            "length not known yet"
        );
    }

    #[test]
    fn unusable_targets_are_problems() {
        let sound = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        let sound_id = sound.id;
        let control = |kind, targets| Cue::new(CueKind::control(kind, targets));
        let mut show = Show { cues: vec![sound], ..Show::default() };
        let check = |show: &Show, cue: &Cue| target_problems(show, cue);
        let first = control(ControlKind::Pan, Targets::Previous(PreviousKeyword::Previous));
        assert_eq!(check(&show, &first), ["No sound, playlist or group before it – the cue acts on nothing"]);
        assert!(check(&show, &control(ControlKind::Pan, Targets::Cues(vec![]))).is_empty());
        let gone = CueId::new();
        assert_eq!(
            check(&show, &control(ControlKind::Pan, Targets::Cues(vec![gone]))),
            ["Target missing (deleted)"]
        );
        assert_eq!(
            check(&show, &control(ControlKind::Pan, Targets::Cues(vec![sound_id, gone]))),
            ["1 of 2 targets missing (deleted)"]
        );
        assert!(check(&show, &control(ControlKind::Pan, Targets::All(jq_show::AllKeyword::All))).is_empty());
        assert_eq!(
            check(&show, &control(ControlKind::Volume, Targets::Output { output: "weg".into() })),
            ["Output “weg” does not exist in the show – the cue acts on nothing"]
        );
        assert!(
            check(&show, &control(ControlKind::Volume, Targets::Output { output: "main".into() })).is_empty()
        );
        let mut start_all = control(ControlKind::StartStop, Targets::All(jq_show::AllKeyword::All));
        assert_eq!(
            check(&show, &start_all),
            ["A start cue only starts chosen cues – with this target it starts nothing"]
        );
        if let CueKind::StartStop(s) = &mut start_all.kind {
            s.action = jq_show::StartStopAction::Stop;
        }
        assert!(check(&show, &start_all).is_empty(), "stopping all is fine");

        // Start cues starting themselves, directly or in a ring; a pre-wait breaks the ring.
        let mut own = control(ControlKind::StartStop, Targets::Cues(vec![]));
        if let CueKind::StartStop(s) = &mut own.kind {
            s.targets = Targets::Cues(vec![own.id]);
        }
        let mut a = control(ControlKind::StartStop, Targets::Cues(vec![]));
        let b = control(ControlKind::StartStop, Targets::Cues(vec![a.id]));
        if let CueKind::StartStop(s) = &mut a.kind {
            s.targets = Targets::Cues(vec![b.id]);
        }
        show.cues.extend([own.clone(), a.clone(), b]);
        let looping = "Starts itself without a pre-wait – is aborted as a loop on GO";
        assert_eq!(check(&show, &own), [looping]);
        assert_eq!(check(&show, &a), [looping]);
        show.cues.last_mut().unwrap().pre_wait = 1.0;
        assert!(check(&show, &a).is_empty(), "b waits before it starts a again");
        let stopwatch = |watch: Option<&str>| {
            let mut c = control(ControlKind::Stopwatch, Targets::Cues(vec![]));
            if let CueKind::Stopwatch(w) = &mut c.kind {
                w.watch = watch.map(Into::into);
            }
            c
        };
        assert_eq!(check(&show, &stopwatch(None)), ["No stopwatch chosen"]);
        assert_eq!(check(&show, &stopwatch(Some("weg"))), ["The stopwatch was deleted"]);
    }

    #[test]
    fn position_jumps_that_cannot_be_made_are_problems() {
        let mut sound = Cue::new(CueKind::Sound(SoundCue {
            marks: vec![4.0, 8.0],
            start: 1.0,
            ..SoundCue::new("a.wav")
        }));
        sound.number = "1".into();
        let mut bare = Cue::new(CueKind::Sound(SoundCue::new("b.wav")));
        bare.number = "2".into();
        let mut list = Cue::new(CueKind::Playlist(PlaylistCue::new(vec!["a.wav".into()])));
        list.number = "3".into();
        let ids = [sound.id, bare.id, list.id];
        let group =
            Cue::new(CueKind::Group(GroupCue { cues: vec![sound, bare, list], ..GroupCue::default() }));
        let group_id = group.id;
        let show = Show { cues: vec![group], ..Show::default() };
        let length = |_: &String| Some(30.0);
        let check = |to: PositionTo, targets: Vec<CueId>| {
            let mut c = Cue::new(CueKind::control(ControlKind::Position, Targets::Cues(targets)));
            if let CueKind::Position(p) = &mut c.kind {
                p.to = to;
            }
            position_problems(&show, &c, length)
        };
        assert_eq!(
            check(PositionTo::Absolute { secs: 99.0 }, vec![ids[0]]),
            ["Position 99.0 s is behind the end of cue 1 (30.0 s) – the cue ends at once"]
        );
        assert_eq!(
            check(PositionTo::Absolute { secs: 0.5 }, vec![ids[0]]),
            ["Position 0.5 s is before the start point of cue 1 (1.0 s) – jumps to the start point"]
        );
        assert!(check(PositionTo::Absolute { secs: 10.0 }, vec![ids[0]]).is_empty());
        assert_eq!(
            check(PositionTo::Mark { index: 3 }, vec![ids[0]]),
            ["Mark 3 does not exist in cue 1 (2 marks) – no jump"]
        );
        assert!(check(PositionTo::Mark { index: 2 }, vec![ids[0]]).is_empty());
        assert_eq!(check(PositionTo::NextMark, vec![ids[1]]), ["Cue 2 has no marks – no jump"]);
        assert!(check(PositionTo::Relative { secs: 5.0 }, vec![ids[2]]).is_empty());
        let playlist = check(PositionTo::Absolute { secs: 5.0 }, vec![ids[2]]);
        assert!(playlist.len() == 1 && playlist[0].starts_with("Cue 3 is a playlist"), "{playlist:?}");
        // A group as target: what is inside.
        assert_eq!(check(PositionTo::PreviousMark, vec![group_id]).len(), 2, "no marks on 2, playlist 3");
    }

    #[test]
    fn control_ops_come_from_json() {
        let json = r#"{"op": "add_control", "kind": "exit_loop", "index": null, "targets": null}"#;
        assert!(matches!(
            serde_json::from_str(json),
            Ok(EditOp::AddControl { kind: ControlKind::ExitLoop, targets: None, group: None, .. })
        ));
        let json = r#"{"op": "set_cue_type", "ids": [], "kind": "start_stop"}"#;
        assert!(matches!(
            serde_json::from_str(json),
            Ok(EditOp::SetCueType { kind: ControlKind::StartStop, .. })
        ));
    }

    #[test]
    fn repeat_is_blocked_for_endless_cues_that_ignore_a_go() {
        let mut cue =
            Cue::new(CueKind::Sound(SoundCue { loop_mode: LoopMode::Forever, ..SoundCue::new("a.wav") }));
        cue.passes = Repeat::Times(2);
        assert!(repeat_blocked(&cue));
        cue.second_trigger = SecondTrigger::Restart;
        assert!(!repeat_blocked(&cue));
    }

    #[test]
    fn import_choices_come_per_path() {
        let json = r#"{"op": "add_sounds", "paths": ["/x/a.wav", "/x/b.wav"], "index": null,
            "copy_outside": true, "on_clash": {"/x/b.wav": "use_existing"}}"#;
        let Ok(EditOp::AddSounds { import, .. }) = serde_json::from_str(json) else { panic!() };
        assert!(import.copy_outside);
        assert_eq!(import.on_clash.get(Path::new("/x/b.wav")), Some(&OnClash::UseExisting));
        assert_eq!(import.on_clash.get(Path::new("/x/a.wav")), None);

        // Without choices (older UI, relink) every clash is renamed.
        let json = r#"{"op": "add_to_playlist", "id": "67e55044-10b1-426f-9247-bb680e5fe0c8", "paths": [], "copy_outside": false}"#;
        assert!(matches!(serde_json::from_str(json), Ok(EditOp::AddToPlaylist { .. })));

        // A sound cue's file without `item`, a playlist item's with it.
        let json = r#"{"op": "set_file", "id": "67e55044-10b1-426f-9247-bb680e5fe0c8", "path": "/x/a.wav", "copy_outside": false}"#;
        assert!(matches!(serde_json::from_str(json), Ok(EditOp::SetFile { item: None, retitle: false, .. })));
        let json = r#"{"op": "set_file", "id": "67e55044-10b1-426f-9247-bb680e5fe0c8", "path": "/x/a.wav", "item": 2, "copy_outside": false}"#;
        assert!(matches!(serde_json::from_str(json), Ok(EditOp::SetFile { item: Some(2), .. })));
        let json =
            r#"{"op": "relink_file", "old_path": "missing.wav", "path": "/x/a.wav", "copy_outside": false}"#;
        assert!(matches!(
            serde_json::from_str(json),
            Ok(EditOp::RelinkFile { old_path, .. }) if old_path == "missing.wav"
        ));
    }

    #[test]
    fn copies_are_unlinked_unless_asked() {
        let id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
        let json = format!(r#"{{"op": "duplicate", "id": "{id}"}}"#);
        assert!(matches!(serde_json::from_str(&json), Ok(EditOp::Duplicate { linked: false, .. })));
        let json = r#"{"op": "paste", "after": null, "linked": true}"#;
        assert!(matches!(serde_json::from_str(json), Ok(EditOp::Paste { linked: true, .. })));
        let json = format!(r#"{{"op": "paste_replace", "ids": ["{id}"]}}"#);
        assert!(
            matches!(serde_json::from_str(&json), Ok(EditOp::PasteReplace { ids, swap: false }) if ids.len() == 1)
        );
    }

    #[test]
    fn outputs_in_use_and_main_stay() {
        let output = |id: &str, name: &str| Output { id: id.into(), name: name.into() };
        let mut show = Show::default();
        show.outputs.push(output("mon", "Monitor"));
        let mut cue = Cue::new(CueKind::Sound(SoundCue { output: "mon".into(), ..SoundCue::new("a.wav") }));
        cue.number = "3".into();
        show.cues.push(cue);

        let renamed = [output("main", "Saal"), output("mon", "Bühne"), output("foyer", "Foyer")];
        assert!(check_outputs(&show, &renamed).is_ok());
        assert_eq!(
            check_outputs(&show, &[output("main", "Main")]),
            Err("Output “Monitor” is still used by cue 3".into())
        );
        assert!(check_outputs(&show, &[output("mon", "Monitor")]).is_err(), "main stays");
        assert!(check_outputs(&show, &[output("main", "A"), output("main", "B")]).is_err(), "unique");
        assert!(check_outputs(&show, &[output("main", " ")]).is_err(), "named");
    }

    #[test]
    fn pairs_must_fit_the_device() {
        let ag08 = device(Some(8));
        assert!(
            check_pairs(&ag08, None, [("Main".to_string(), 0), ("Monitor".to_string(), 6)].into_iter())
                .is_ok()
        );
        assert_eq!(
            check_pairs(&ag08, None, [("Main".to_string(), 0), ("Monitor".to_string(), 8)].into_iter()),
            Err("Monitor: channels 9/10 do not exist, “AG08” has 8 channels".into())
        );
        assert!(
            check_pairs(&device(None), None, [("Main".to_string(), 30)].into_iter()).is_ok(),
            "unknown passes"
        );
    }

    #[test]
    fn pairs_must_fit_a_set_channel_count() {
        let ag08 = device(Some(8));
        assert_eq!(
            check_pairs(&ag08, Some(4), [("Main".to_string(), 4)].into_iter()),
            Err("Main: channels 5/6 do not exist, “AG08” is set to 4 channels".into())
        );
        assert_eq!(
            check_pairs(&ag08, Some(10), [("Main".to_string(), 0)].into_iter()),
            Err("“AG08” only has 8 channels, 10 are set".into())
        );
    }

    #[test]
    fn media_in_use_covers_the_undo_and_redo_history() {
        let sound = |file: &str| Cue::new(CueKind::Sound(SoundCue::new(file)));
        let (kept, deleted) = (sound("bleibt.wav"), sound("geloescht.wav"));
        let id = deleted.id;
        let mut editor = Editor::new(Show { cues: vec![kept, deleted], ..Show::default() });
        editor.mark_saved();
        editor.delete(&[id]);
        editor.mark_saved();
        // Deleted and saved: only undo can bring it back, so its file stays.
        assert_eq!(media_in(editor.history(), &[]), ["bleibt.wav", "geloescht.wav"]);
        editor.undo();
        editor.change(|s| s.cues.clear());
        editor.undo();
        assert_eq!(
            media_in(editor.history(), &[sound("ablage.wav")]),
            ["ablage.wav", "bleibt.wav", "geloescht.wav"]
        );
    }

    #[test]
    fn a_failed_device_does_not_block_settings_while_cues_run() {
        // Changed audio settings: reopened, refused while cues run.
        assert_eq!(audio_reopen(true, false, true), Ok(true));
        assert!(audio_reopen(true, true, false).is_err());
        // Only failed or lost: tried again while idle, else left alone.
        assert_eq!(audio_reopen(false, true, true), Ok(true));
        assert_eq!(audio_reopen(false, true, false), Ok(false), "e.g. entering show mode mid-show");
        assert_eq!(audio_reopen(false, false, false), Ok(false));
    }

    #[test]
    fn a_reopened_runner_keeps_the_stopwatches() {
        let watch = Stopwatch { id: "w".into(), name: "Pause".into(), countdown: Some(600.0) };
        let show = Show { stopwatches: vec![watch], ..Show::default() };
        let runner = |output: &mut Vec<AudioOutput>| {
            let (out, controller) = AudioOutput::null(48_000, 2, 4);
            output.push(out);
            Runner::new(
                show.clone(),
                PathBuf::new(),
                LocalSettings::default(),
                controller,
                Arc::new(FileLoader),
            )
        };
        let mut outputs = Vec::new();
        let mut old = runner(&mut outputs);
        let start = Instant::now();
        old.stopwatch("w", jq_show::StopwatchAction::Start, start);
        let mut new = runner(&mut outputs);
        old.carry_over(&mut new);
        let later = start + Duration::from_secs(90);
        let w = &new.state(later).stopwatches[0];
        assert!(w.running && (w.elapsed - 90.0).abs() < 1e-6, "{:?}", (w.running, w.elapsed));
        assert_eq!(w.remaining.map(f64::round), Some(510.0));
    }

    #[test]
    fn automatic_offsets_join_the_history_and_keep_redo() {
        let mut manual = SoundCue::new("b.wav");
        manual.normalize_db = Some(2.0);
        let (auto, manual) =
            (Cue::new(CueKind::Sound(SoundCue::new("a.wav"))), Cue::new(CueKind::Sound(manual)));
        let (auto_id, manual_id) = (auto.id, manual.id);
        let mut editor = Editor::new(Show { cues: vec![auto, manual], ..Show::default() });
        editor.change(|s| s.name = "eins".into());
        editor.change(|s| s.name = "zwei".into());
        editor.undo();
        let changes = vec![(auto_id, "a.wav".to_string(), -3.0), (manual_id, "b.wav".to_string(), -5.0)];
        editor.amend(|show| set_offsets(show, &changes, true));
        let offset = |show: &Show, id| match &show.cue(id).unwrap().kind {
            CueKind::Sound(s) => s.normalize_db,
            _ => None,
        };
        assert!(editor.can_redo(), "redo is kept");
        assert!(editor.is_dirty());
        assert_eq!(offset(editor.show(), auto_id), Some(-3.0));
        assert_eq!(offset(editor.show(), manual_id), Some(2.0), "a set offset stays");
        editor.undo();
        assert_eq!(offset(editor.show(), auto_id), Some(-3.0), "undo does not take it back");
        editor.redo();
        editor.redo();
        assert_eq!((editor.show().name.as_str(), offset(editor.show(), auto_id)), ("zwei", Some(-3.0)));
    }

    #[test]
    fn previous_targets_are_found_as_playback_finds_them() {
        let sound = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        let previous = || Targets::Previous(PreviousKeyword::Previous);
        let fade = Cue::new(CueKind::control(ControlKind::Volume, previous()));
        let mut position = Cue::new(CueKind::control(ControlKind::Position, previous()));
        if let CueKind::Position(p) = &mut position.kind {
            p.to = PositionTo::NextMark;
        }
        let show = Show { cues: vec![sound.clone(), fade.clone(), position.clone()], ..Show::default() };
        assert_eq!(chosen_targets(&show, &position), [sound.id], "the fade cue in between is passed over");
        assert!(target_problems(&show, &position).is_empty());
        assert_eq!(position_problems(&show, &position, |_| Some(10.0)).len(), 1, "the sound has no marks");

        let wait = Cue::new(CueKind::Wait(WaitCue { duration: 1.0 }));
        let show = Show { cues: vec![wait, fade.clone()], ..Show::default() };
        assert_eq!(
            target_problems(&show, &fade),
            ["No sound, playlist or group before it – the cue acts on nothing"]
        );
    }

    #[test]
    fn verbatim_windows_paths_become_plain_ones() {
        assert_eq!(without_verbatim(r"\\?\C:\Shows\Media").as_deref(), Some(r"C:\Shows\Media"));
        assert_eq!(without_verbatim(r"\\?\d:\").as_deref(), Some(r"d:\"));
        assert_eq!(without_verbatim(r"\\?\UNC\server\share\a").as_deref(), Some(r"\\server\share\a"));
        assert_eq!(without_verbatim(r"C:\Shows"), None, "plain already");
        assert_eq!(without_verbatim(r"\\?\Volume{1234}\x"), None);
        assert_eq!(without_verbatim(r"\\?\C:\Shows\con.wav"), None, "a device name");
        assert_eq!(without_verbatim(r"\\?\C:\Shows\COM1"), None);
        assert_eq!(without_verbatim(r"\\?\C:\Shows\Ende. "), None, "trailing space");
        assert_eq!(without_verbatim(r"\\?\C:\Shows\Console.wav").as_deref(), Some(r"C:\Shows\Console.wav"));
    }
}
