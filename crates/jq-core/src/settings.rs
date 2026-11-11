//! Machine-specific settings, never part of the show, so a show runs
//! unchanged on a machine with a different sound card. They live in one file
//! per machine (the app's config folder), not in the show folder, so copying
//! a show to another machine takes none of them along.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::keys::{KeyBinding, default_keys};
use crate::midi::MidiBinding;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalSettings {
    /// Prevent sleep and screen blanking while a show is open.
    pub keep_awake: bool,
    /// Show mode: editing is locked so a stray click changes nothing. Not
    /// saved: each show opens in show mode.
    pub show_mode: bool,
    /// Audio host, e.g. "ALSA", "JACK", "WASAPI"; `None` = system default.
    pub host: Option<String>,
    /// Output device (part of its name or its id); `None` = system default.
    pub device: Option<String>,
    /// Stream channel count; `None` = device default.
    pub channels: Option<u16>,
    /// Logical output id → first channel of its stereo pair (1-based).
    pub outputs: BTreeMap<String, usize>,
    /// FADE ALL time in seconds.
    pub fade_all: f64,
    /// Rehearsal jump steps and the "jump to end" lead time, seconds.
    pub jump_back: f64,
    pub jump_forward: f64,
    pub jump_end: f64,
    /// Seconds before its end from which a running cue shows the warning
    /// colour; 0 = never.
    pub end_warning: f64,
    /// What the mouse wheel does over the cue list. Owned by the UI.
    pub wheel: WheelMode,
    /// Fixed callback buffer of the audio device in frames; `None` = the
    /// host's default.
    pub buffer_frames: Option<u32>,
    /// What Esc does while a dialog or menu is open.
    pub escape: EscapeMode,
    /// Fade time of a panic (Esc); 0 = stop at once. A second panic while
    /// the fade runs stops at once.
    pub panic_fade: f64,
    /// Shortest time between two GOs, seconds; a GO sooner after the last
    /// one is dropped. 0 = off.
    pub go_lockout: f64,
    /// Whether the last show is opened when the app starts.
    pub open_last: OpenLast,
    /// The show opened last; set by the backend, not by `set_settings`.
    pub last_show: Option<PathBuf>,
    /// Separate preview device (e.g. headphones); `None` with a
    /// `preview_channel` = a channel pair of the main device.
    pub preview_device: Option<String>,
    /// First channel (1-based) of the preview pair; both `None` = no preview.
    pub preview_channel: Option<usize>,
    /// MIDI input port (part of its name); `None` = no MIDI.
    pub midi_port: Option<String>,
    pub midi: Vec<MidiBinding>,
    /// MIDI Show Control device id (0–126) this machine answers to (and the
    /// all call); `None` = MSC is ignored.
    pub msc_device: Option<u8>,
    /// Global shortcuts (see [`crate::keys`]).
    pub keys: Vec<KeyBinding>,
    /// Steps of the "+/−" actions on the selected cue: dB, pan (−1…1) and
    /// rate percent points.
    pub volume_step: f64,
    pub pan_step: f64,
    pub rate_step: f64,
    /// A clear warning while the window does not have the keyboard focus
    /// (keys do nothing then).
    pub focus_warning: bool,
    /// MIDI output port for MIDI cues (part of its name); `None` = none.
    pub midi_out: Option<String>,
    /// UDP port for incoming OSC; `None` = not listening.
    pub osc_listen: Option<u16>,
    /// Show device id → `host:port` on this machine, overriding the show.
    pub osc_hosts: BTreeMap<String, String>,
    /// Folder the audio file picker starts in; `None` = the show's media
    /// folder.
    pub audio_dir: Option<PathBuf>,
    /// Arrangement of the main window's panels. Owned by the UI and stored
    /// as it comes; `None` = the UI's default.
    pub layout: Option<serde_json::Value>,
    /// How the cue list's selection and the playhead go together. Owned by
    /// the UI, set with its own command.
    pub selection_mode: SelectionMode,
    /// How changes of media files on disk are noticed.
    pub media_watch: WatchMode,
    /// Language of the program: "auto" (the system language, English if that
    /// is not offered) or the code of an offered language ("en", "de").
    /// Texts of the backend and of the UI follow it.
    pub language: String,
    /// Look for a newer release a few seconds after the program starts (a
    /// request to GitHub). Only tells the operator; installing is always
    /// their decision.
    pub check_updates: bool,
}

impl LocalSettings {
    /// The language the program speaks with these settings.
    pub fn resolved_language(&self) -> jq_i18n::Lang {
        jq_i18n::resolve(&self.language)
    }
}

/// How changes of media files on disk are noticed (see [`crate::MediaWatcher`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchMode {
    /// The system's file events: no load while nothing changes, but blind
    /// to changes other machines make on a network share.
    #[default]
    Events,
    /// The files are looked at every second.
    Poll,
}

/// How the cue list's selection and the playhead go together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    /// Each moves on its own.
    #[default]
    Separate,
    /// Selecting a cue moves the playhead to it.
    PlayheadFollows,
    /// When the playhead moves, its cue gets selected.
    SelectionFollows,
    /// Both: selection and playhead are always on the same cue.
    Linked,
}

/// What the mouse wheel does over the cue list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelMode {
    /// Scrolls the list.
    #[default]
    Scroll,
    /// Moves the selection.
    Selection,
    /// Moves the playhead.
    Playhead,
}

/// What Esc does while a dialog or menu is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscapeMode {
    /// Panic, and the dialog closes as well.
    #[default]
    StopAll,
    /// Only closes the dialog; Esc panics when nothing is open.
    CloseFirst,
}

/// Whether the last show is opened when the app starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenLast {
    #[default]
    Off,
    /// Opened right away, as if it was given on the command line.
    Last,
    /// The start screen asks.
    Ask,
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            keep_awake: true,
            show_mode: false,
            host: None,
            device: None,
            channels: None,
            outputs: BTreeMap::new(),
            fade_all: 5.0,
            jump_back: 10.0,
            jump_forward: 10.0,
            jump_end: 10.0,
            end_warning: 10.0,
            wheel: WheelMode::Scroll,
            buffer_frames: None,
            escape: EscapeMode::StopAll,
            panic_fade: 1.0,
            go_lockout: 0.3,
            open_last: OpenLast::Off,
            last_show: None,
            preview_device: None,
            preview_channel: None,
            midi_port: None,
            midi: Vec::new(),
            msc_device: None,
            keys: default_keys(),
            volume_step: 1.0,
            pan_step: 0.1,
            rate_step: 1.0,
            focus_warning: true,
            midi_out: None,
            osc_listen: None,
            osc_hosts: BTreeMap::new(),
            audio_dir: None,
            layout: None,
            selection_mode: SelectionMode::Separate,
            media_watch: WatchMode::Events,
            language: jq_i18n::AUTO.into(),
            check_updates: false,
        }
    }
}

/// The parts of [`LocalSettings`] that refer to one show's outputs and
/// devices, kept per show.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct ShowSettings {
    outputs: BTreeMap<String, usize>,
    osc_hosts: BTreeMap<String, String>,
}

/// The machine's settings file: the general settings at the top level, each
/// show's [`ShowSettings`] under `shows`, keyed by show id.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SettingsFile {
    #[serde(flatten)]
    general: LocalSettings,
    shows: BTreeMap<String, ShowSettings>,
    /// Set while a show opened at startup has not yet run for a while; still
    /// set on the next start means it crashed the app.
    startup_pending: bool,
    /// One jump time for all three jumps (before they were separate).
    jump: Option<f64>,
}

impl SettingsFile {
    /// `None` if there is no file yet.
    fn read(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut file: Self =
                    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                if let Some(jump) = file.jump.take() {
                    let g = &mut file.general;
                    (g.jump_back, g.jump_forward, g.jump_end) = (jump, jump, jump);
                }
                Ok(Some(file))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        let mut json = serde_json::to_value(&self.general).expect("settings serialize");
        let map = json.as_object_mut().expect("settings are an object");
        // Only per show.
        map.remove("outputs");
        map.remove("osc_hosts");
        // Per session.
        map.remove("show_mode");
        map.insert("shows".into(), serde_json::to_value(&self.shows).expect("settings serialize"));
        if self.startup_pending {
            map.insert("startup_pending".into(), true.into());
        }
        let text = serde_json::to_vec_pretty(&json).expect("settings serialize");
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        jq_show::write_atomic(path, &text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn settings(self, show: &str) -> LocalSettings {
        let ShowSettings { outputs, osc_hosts } = self.shows.get(show).cloned().unwrap_or_default();
        LocalSettings { outputs, osc_hosts, ..self.general }
    }
}

impl LocalSettings {
    /// The settings for show `show` from the machine's settings file at
    /// `path`; a missing file gives the defaults.
    pub fn load(path: &Path, show: &str) -> Result<Self, String> {
        Ok(SettingsFile::read(path)?.unwrap_or_default().settings(show))
    }

    /// Saves to the machine's settings file: the general settings for all
    /// shows, outputs and OSC addresses for `show`. Other shows' entries are
    /// read again first, so another instance's changes are kept.
    pub fn save(&self, path: &Path, show: &str) -> Result<(), String> {
        // An unreadable file was reported (and ignored) on loading.
        let mut file = SettingsFile::read(path).ok().flatten().unwrap_or_default();
        file.shows.insert(
            show.to_owned(),
            ShowSettings { outputs: self.outputs.clone(), osc_hosts: self.osc_hosts.clone() },
        );
        file.general = self.clone();
        file.write(path)
    }

    /// The general settings, without any show's outputs and OSC addresses;
    /// a missing file gives the defaults.
    pub fn load_general(path: &Path) -> Result<Self, String> {
        Ok(SettingsFile::read(path)?.unwrap_or_default().general)
    }

    /// True if `other` needs the audio output to be opened again.
    pub fn audio_differs(&self, other: &Self) -> bool {
        (&self.host, &self.device, self.channels, self.buffer_frames)
            != (&other.host, &other.device, other.channels, other.buffer_frames)
    }

    pub fn preview_enabled(&self) -> bool {
        self.preview_device.is_some() || self.preview_channel.is_some()
    }

    /// Device channels the outputs and a preview pair on the main device
    /// reach up to (at least stereo).
    pub fn needed_channels(&self) -> u16 {
        let preview = if self.preview_device.is_none() { self.preview_channel } else { None };
        let last = self.outputs.values().copied().chain(preview).max().unwrap_or(1);
        u16::try_from(last.saturating_add(1)).unwrap_or(u16::MAX).max(2)
    }

    /// Zero-based first channel of the preview pair.
    pub fn preview_first_channel(&self) -> usize {
        self.preview_channel.map_or(0, |c| c.saturating_sub(1))
    }

    /// Zero-based first device channel of a logical output (default: 1/2).
    pub fn output_channel(&self, output: &str) -> usize {
        self.outputs.get(output).map_or(0, |c| c.saturating_sub(1))
    }
}

/// What happens with the last show when the app starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Startup {
    Nothing,
    /// Opened right away, like a show given on the command line.
    Open(PathBuf),
    /// The start screen asks whether to open it.
    Ask(PathBuf),
}

impl Startup {
    /// Decides from the settings file at `path`; `cli` = a show was given on
    /// the command line, which always wins. An unreadable file decides
    /// nothing.
    pub fn read(path: &Path, cli: bool) -> Self {
        match SettingsFile::read(path) {
            Ok(Some(file)) => Self::decide(&file.general, file.startup_pending, cli, Path::exists),
            _ => Self::Nothing,
        }
    }

    /// `pending` = the last start opened a show and did not get past it;
    /// that show is then only offered, so a show that crashes the app does
    /// not do so on every start.
    fn decide(settings: &LocalSettings, pending: bool, cli: bool, exists: impl Fn(&Path) -> bool) -> Self {
        let Some(last) = settings.last_show.clone().filter(|p| !cli && exists(p)) else {
            return Self::Nothing;
        };
        match settings.open_last {
            OpenLast::Off => Self::Nothing,
            OpenLast::Last if !pending => Self::Open(last),
            OpenLast::Last | OpenLast::Ask => Self::Ask(last),
        }
    }

    /// Marks in the settings file at `path` that a show opened at startup
    /// has not yet run for a while (`true`), or that it has (`false`).
    pub fn set_pending(path: &Path, pending: bool) -> Result<(), String> {
        let mut file = SettingsFile::read(path)?.unwrap_or_default();
        if file.startup_pending == pending {
            return Ok(());
        }
        file.startup_pending = pending;
        file.write(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("jq-core-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn general_settings_are_shared_and_routing_is_per_show() {
        let path = tempdir("settings").join("config").join("settings.json");
        assert_eq!(LocalSettings::load(&path, "a").unwrap(), LocalSettings::default());

        let mut a = LocalSettings { device: Some("Interface".into()), ..LocalSettings::default() };
        a.outputs.insert("main".into(), 3);
        a.osc_hosts.insert("desk".into(), "10.0.0.5:8000".into());
        a.show_mode = true;
        a.layout = Some(serde_json::json!({ "root": "cues" }));
        a.selection_mode = SelectionMode::PlayheadFollows;
        a.save(&path, "a").unwrap();

        let b = LocalSettings::load(&path, "b").unwrap();
        assert_eq!(b.device.as_deref(), Some("Interface"));
        assert!(b.outputs.is_empty() && b.osc_hosts.is_empty());
        assert!(!b.show_mode, "show mode is not saved");
        assert_eq!(b.layout, a.layout, "the layout is the same for every show");
        assert_eq!(b.selection_mode, SelectionMode::PlayheadFollows);

        let b = LocalSettings { fade_all: 2.0, outputs: [("main".into(), 5)].into(), ..b };
        b.save(&path, "b").unwrap();
        let a = LocalSettings::load(&path, "a").unwrap();
        assert_eq!((a.fade_all, a.outputs["main"]), (2.0, 3));
        assert_eq!(a.osc_hosts["desk"], "10.0.0.5:8000");
        assert_eq!(LocalSettings::load(&path, "b").unwrap().outputs["main"], 5);
    }

    #[test]
    fn missing_shortcuts_are_the_defaults() {
        let path = tempdir("keys").join("settings.json");
        std::fs::write(&path, r#"{ "fade_all": 2.0 }"#).unwrap();
        let s = LocalSettings::load(&path, "a").unwrap();
        assert_eq!(s.keys, crate::keys::default_keys());
        assert!(s.focus_warning && s.msc_device.is_none());
        std::fs::write(&path, r#"{ "keys": [] }"#).unwrap();
        assert!(LocalSettings::load(&path, "a").unwrap().keys.is_empty(), "an empty list stays empty");
    }

    #[test]
    fn one_old_jump_time_sets_all_three() {
        let path = tempdir("jump").join("settings.json");
        std::fs::write(&path, r#"{ "jump": 4.0, "fade_all": 2.0 }"#).unwrap();
        let s = LocalSettings::load(&path, "a").unwrap();
        assert_eq!((s.jump_back, s.jump_forward, s.jump_end, s.fade_all), (4.0, 4.0, 4.0, 2.0));

        LocalSettings { jump_end: 1.0, ..s }.save(&path, "a").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("\"jump\""), "only the new fields are written");
        let s = LocalSettings::load(&path, "a").unwrap();
        assert_eq!((s.jump_back, s.jump_end), (4.0, 1.0));
    }

    #[test]
    fn startup_opens_or_offers_the_last_show() {
        let last = LocalSettings {
            open_last: OpenLast::Last,
            last_show: Some("/shows/a.jqshow".into()),
            ..LocalSettings::default()
        };
        let exists = |_: &Path| true;
        let open = Startup::Open("/shows/a.jqshow".into());
        let ask = Startup::Ask("/shows/a.jqshow".into());
        assert_eq!(Startup::decide(&last, false, false, exists), open);
        assert_eq!(Startup::decide(&last, true, false, exists), ask, "crashed last time");
        assert_eq!(Startup::decide(&last, false, true, exists), Startup::Nothing, "command line wins");
        assert_eq!(Startup::decide(&last, false, false, |_| false), Startup::Nothing, "file gone");
        let asking = LocalSettings { open_last: OpenLast::Ask, ..last.clone() };
        assert_eq!(Startup::decide(&asking, false, false, exists), ask);
        let off = LocalSettings { open_last: OpenLast::Off, ..last };
        assert_eq!(Startup::decide(&off, false, false, exists), Startup::Nothing);
    }

    #[test]
    fn startup_pending_survives_saving_settings() {
        let path = tempdir("pending").join("settings.json");
        Startup::set_pending(&path, true).unwrap();
        let s = LocalSettings {
            open_last: OpenLast::Last,
            last_show: Some(path.clone()),
            ..LocalSettings::default()
        };
        s.save(&path, "a").unwrap();
        assert_eq!(Startup::read(&path, false), Startup::Ask(path.clone()));
        Startup::set_pending(&path, false).unwrap();
        assert_eq!(Startup::read(&path, false), Startup::Open(path.clone()));
        assert_eq!(LocalSettings::load_general(&path).unwrap().open_last, OpenLast::Last);
    }
}
