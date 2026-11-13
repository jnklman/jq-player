//! Loading and saving show files. A show file is next to its media folder,
//! so copying their folder moves the whole show. The file name and folder
//! name are independent of the show's display name.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use jq_i18n::t;

use crate::model::{FORMAT, Show, VERSION};

pub const SHOW_FILE_EXTENSION: &str = "jqshow";

#[derive(Debug)]
pub enum ShowFileError {
    Io { path: PathBuf, source: std::io::Error },
    Parse { path: PathBuf, source: serde_json::Error },
    WrongFormat { path: PathBuf, found: String },
    TooNew { path: PathBuf, found: u32 },
    InvalidName(String),
    Exists(PathBuf),
}

impl std::fmt::Display for ShowFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Io { path, source } => format!("{}: {source}", path.display()),
            Self::Parse { path, source } => t!("show_file.invalid", path = path.display(), error = source),
            Self::WrongFormat { path, found } => {
                t!("show_file.wrong_format", path = path.display(), found = found)
            }
            Self::TooNew { path, found } => {
                t!("show_file.too_new", path = path.display(), found = found, current = VERSION)
            }
            Self::InvalidName(name) => t!("show_file.invalid_name", name = name),
            Self::Exists(path) => t!("show_file.exists", path = path.display()),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for ShowFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// A show together with where it lives on disk.
#[derive(Clone, Debug)]
pub struct ShowFile {
    pub path: PathBuf,
    pub show: Show,
}

impl ShowFile {
    pub fn load(path: &Path) -> Result<Self, ShowFileError> {
        Ok(Self { path: path.to_owned(), show: read_show(path)? })
    }

    /// Unsaved changes, written next to the show file while editing.
    pub fn autosave_path(&self) -> PathBuf {
        autosave_path(&self.path)
    }

    /// Writes `show` to the autosave file (atomically, like [`ShowFile::save`]).
    pub fn write_autosave(&self, show: &Show) -> Result<(), ShowFileError> {
        let path = self.autosave_path();
        write_atomic(&path, &crate::model::show_file_json(show))
            .map_err(|source| ShowFileError::Io { path, source })
    }

    /// The autosave and when it was written, if it is newer than the show
    /// file and differs from it, i.e. holds changes lost in a crash.
    pub fn load_autosave(&self) -> Result<Option<(Show, SystemTime)>, ShowFileError> {
        let path = self.autosave_path();
        let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified());
        let Ok(saved_at) = modified(&path) else { return Ok(None) };
        if modified(&self.path).is_ok_and(|show_time| show_time >= saved_at) {
            return Ok(None);
        }
        let show = read_show(&path)?;
        Ok((show != self.show).then_some((show, saved_at)))
    }

    /// Deletes the autosave; a missing one is fine.
    pub fn remove_autosave(&self) -> Result<(), ShowFileError> {
        let path = self.autosave_path();
        match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(ShowFileError::Io { path, source: e }),
            _ => Ok(()),
        }
    }

    /// Saves atomically: write a temporary file next to the target, then
    /// rename it over the target, so a crash never leaves a half-written show.
    pub fn save(&self) -> Result<(), ShowFileError> {
        write_atomic(&self.path, &crate::model::show_file_json(&self.show))
            .map_err(|source| ShowFileError::Io { path: self.path.clone(), source })
    }

    /// Folder holding the show file.
    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    pub fn media_dir(&self) -> PathBuf {
        crate::paths::resolve(self.dir(), &self.show.media_root)
    }

    /// Creates a show in `<dir>/<name>/` or directly in `dir`, with an empty
    /// media folder. The initial file name uses `name`; later renames only
    /// change the display name stored in the show.
    pub fn create(dir: &Path, name: &str, create_folder: bool) -> Result<Self, ShowFileError> {
        let name = name.trim();
        if !valid_folder_name(name) {
            return Err(ShowFileError::InvalidName(name.into()));
        }
        let folder = if create_folder { dir.join(name) } else { dir.to_owned() };
        let path = if create_folder {
            named_path(&folder)
        } else {
            dir.join(format!("{name}.{SHOW_FILE_EXTENSION}"))
        };
        let media_dir = folder.join("media");
        if create_folder {
            match fs::create_dir(&folder) {
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(ShowFileError::Exists(folder));
                }
                Err(source) => return Err(ShowFileError::Io { path: folder, source }),
                Ok(()) => {}
            }
        }
        if path.exists() {
            if create_folder {
                let _ = fs::remove_dir(&folder);
            }
            return Err(ShowFileError::Exists(path));
        }
        if let Err(source) = fs::create_dir(&media_dir) {
            if create_folder {
                let _ = fs::remove_dir(&folder);
            }
            return Err(if source.kind() == std::io::ErrorKind::AlreadyExists {
                ShowFileError::Exists(media_dir)
            } else {
                ShowFileError::Io { path: media_dir, source }
            });
        }
        let show = Show { name: name.into(), ..Show::default() };
        let file = Self { path, show };
        if let Err(error) = file.save() {
            if create_folder {
                let _ = fs::remove_dir_all(&folder);
            } else {
                let _ = fs::remove_dir(&media_dir);
            }
            return Err(error);
        }
        Ok(file)
    }

    /// Creates the empty show folder `<dir>/<name>/` for saving a show
    /// under a new name and returns the show file's path in it. Refuses a
    /// folder that exists already.
    pub fn create_folder(dir: &Path, name: &str) -> Result<PathBuf, ShowFileError> {
        let name = name.trim();
        if !valid_folder_name(name) {
            return Err(ShowFileError::InvalidName(name.into()));
        }
        let folder = dir.join(name);
        match fs::create_dir(&folder) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(ShowFileError::Exists(folder)),
            Err(source) => Err(ShowFileError::Io { path: folder, source }),
            Ok(()) => Ok(named_path(&folder)),
        }
    }
}

/// The show file's path in `folder` when named after the folder.
fn named_path(folder: &Path) -> PathBuf {
    let name = folder.file_name().unwrap_or_default().to_string_lossy();
    folder.join(format!("{name}.{SHOW_FILE_EXTENSION}"))
}

fn autosave_path(show_file: &Path) -> PathBuf {
    let mut path = show_file.as_os_str().to_owned();
    path.push(".autosave");
    path.into()
}

/// Rejects names that are not a single folder on Windows or Linux.
fn valid_folder_name(name: &str) -> bool {
    const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with('.')
        && !name.chars().any(|c| FORBIDDEN.contains(&c) || c.is_control())
}

fn read_show(path: &Path) -> Result<Show, ShowFileError> {
    let io = |source| ShowFileError::Io { path: path.to_owned(), source };
    let text = fs::read_to_string(path).map_err(io)?;
    let parse = |source| ShowFileError::Parse { path: path.to_owned(), source };
    // Checked before the rest, which a foreign or newer file may not match.
    let header: Header = serde_json::from_str(&text).map_err(parse)?;
    if header.format != FORMAT {
        return Err(ShowFileError::WrongFormat { path: path.to_owned(), found: header.format });
    }
    if header.version > VERSION {
        return Err(ShowFileError::TooNew { path: path.to_owned(), found: header.version });
    }
    serde_json::from_str(&text).map_err(parse)
}

#[derive(serde::Deserialize)]
struct Header {
    format: String,
    version: u32,
}

/// Writes a temporary file next to `path`, then renames it over `path`.
/// The temporary name keeps the whole file name and adds the process id and a
/// counter (`Show.jqshow.1234.0.tmp`), so neither files differing only in
/// their extension nor concurrent writers (two app instances, two threads)
/// share it. The temporary file is removed if writing fails.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut tmp = path.as_os_str().to_owned();
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    tmp.push(format!(".{}.{n}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::show_file_json;
    use crate::model::*;
    use crate::paths::tests::tempdir;

    #[test]
    fn write_atomic_leaves_no_temp_files_and_replaces() {
        let dir = tempdir("atomic");
        let path = dir.join("a.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        // Failure (target is a directory) removes the temp file.
        let target = dir.join("d");
        fs::create_dir(&target).unwrap();
        assert!(write_atomic(&target, b"x").is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn changing_display_name_keeps_file_and_folder_paths() {
        let dir = tempdir("rename");
        let mut file = ShowFile::create(&dir, "Alt", true).unwrap();
        fs::write(file.media_dir().join("a.wav"), b"x").unwrap();
        file.write_autosave(&file.show).unwrap();
        file.show.name = "Neu".into();
        file.save().unwrap();
        assert_eq!(file.path, dir.join("Alt").join("Alt.jqshow"));
        assert!(file.autosave_path().is_file());
        assert!(dir.join("Alt").exists());
        assert!(file.media_dir().join("a.wav").is_file());
        assert_eq!(ShowFile::load(&file.path).unwrap().show.name, "Neu");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creates_show_in_folder_or_selected_directory() {
        let dir = tempdir("create-show");
        let folder_show = ShowFile::create(&dir, "Show", true).unwrap();
        assert_eq!(folder_show.path, dir.join("Show/Show.jqshow"));
        assert_eq!(folder_show.show.name, "Show");
        assert!(folder_show.media_dir().is_dir());
        assert!(matches!(ShowFile::create(&dir, "Show", true), Err(ShowFileError::Exists(_))));

        let direct = dir.join("Direkt");
        fs::create_dir(&direct).unwrap();
        let direct_show = ShowFile::create(&direct, "Name", false).unwrap();
        assert_eq!(direct_show.path, direct.join("Name.jqshow"));
        assert_eq!(direct_show.media_dir(), direct.join("media"));
        assert_eq!(ShowFile::load(&direct_show.path).unwrap().show.name, "Name");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn show_name_is_required() {
        let dir = tempdir("missing-name");
        let path = dir.join("Show.jqshow");
        fs::write(&path, r#"{"format":"jqshow","version":1,"id":"9b2f4c1e-7a3d-4e8b-a5c6-1d2e3f4a5b6c"}"#)
            .unwrap();
        assert!(matches!(ShowFile::load(&path), Err(ShowFileError::Parse { .. })));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creates_a_folder_for_saving_as() {
        let dir = tempdir("save-as");
        let path = ShowFile::create_folder(&dir, " Kopie ").unwrap();
        assert_eq!(path, dir.join("Kopie").join("Kopie.jqshow"));
        assert!(dir.join("Kopie").is_dir() && !path.exists());
        assert!(matches!(ShowFile::create_folder(&dir, "Kopie"), Err(ShowFileError::Exists(_))));
        assert!(matches!(ShowFile::create_folder(&dir, "a/b"), Err(ShowFileError::InvalidName(_))));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn autosave_is_offered_only_when_newer_and_different() {
        let dir = tempdir("autosave");
        let mut file = ShowFile::create(&dir, "Show", true).unwrap();
        assert!(file.load_autosave().unwrap().is_none());

        let mut changed = file.show.clone();
        changed.cues.push(Cue::new(CueKind::Sound(SoundCue::new("a.wav"))));
        // File times can be coarse; make the autosave clearly newer.
        std::thread::sleep(std::time::Duration::from_millis(20));
        file.write_autosave(&changed).unwrap();
        assert!(file.autosave_path().ends_with("Show.jqshow.autosave"));
        let (found, _) = file.load_autosave().unwrap().expect("newer autosave");
        assert_eq!(found, changed);

        file.write_autosave(&file.show).unwrap();
        assert!(file.load_autosave().unwrap().is_none(), "same as the show");

        file.write_autosave(&changed).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        file.show = changed;
        file.save().unwrap();
        assert!(file.load_autosave().unwrap().is_none(), "older than the show file");

        file.remove_autosave().unwrap();
        file.remove_autosave().unwrap();
        assert!(!file.autosave_path().exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parses_design_doc_example() {
        let json = r#"{
          "format": "jqshow", "version": 1, "id": "9b2f4c1e-7a3d-4e8b-a5c6-1d2e3f4a5b6c",
          "name": "Designbeispiel",
          "media_root": "media",
          "outputs": [{ "id": "main", "name": "Main" }],
          "cues": [
            { "id": "4f1c0000-0000-4000-8000-000000000000", "number": "1", "title": "Einlassmusik",
              "script_ref": "vor Beginn", "continue": "none",
              "type": "sound", "file": "musik/einlass.mp3", "output": "main",
              "volume_db": -6, "fade_in": 0, "fade_out": 5, "loop": 0 },
            { "id": "9a2e0000-0000-4000-8000-000000000000", "number": "2", "title": "Einlass aus",
              "type": "volume", "targets": ["4f1c0000-0000-4000-8000-000000000000"],
              "action": "fade_stop", "duration": 5 }
          ]
        }"#;
        let show: Show = serde_json::from_str(json).unwrap();
        assert_eq!(show.cues[0].advance, Advance::default(), "the old continue field is dropped");
        let CueKind::Sound(s) = &show.cues[0].kind else { panic!() };
        assert_eq!((s.volume_db, s.fade_out, s.loop_mode), (-6.0, 5.0, LoopMode::Off));
        assert_eq!(s.normalize_db, None);
        assert_eq!((show.level_limits, show.loudness_target), (LevelLimits::default(), -20.0));
        let CueKind::Volume(f) = &show.cues[1].kind else { panic!() };
        assert_eq!(f.targets, Targets::Cues(vec![show.cues[0].id]));
        assert_eq!((f.action, f.duration), (VolumeAction::FadeStop, Some(5.0)));
    }

    #[test]
    fn round_trips_all_variants() {
        let mut show = Show::default();
        let mut sound = SoundCue::new("atmo/wind.wav");
        sound.loop_mode = LoopMode::Forever;
        sound.end = Some(108.5);
        sound.curve = FadeCurve::DecibelS;
        let mut cue = Cue::new(CueKind::Sound(sound));
        cue.advance = Advance::new(AdvanceAt::BeforeEnd { secs: 2.5 });
        cue.passes = Repeat::Times(3);
        cue.pre_wait = 1.5;
        cue.color = Some("#5b4a8a".into());
        cue.hotkey = Some(Chord::new("Numpad1", "1").ctrl());
        cue.midi_trigger = Some(MidiTrigger::Program { channel: 3, program: 12 });
        cue.release_trigger = true;
        show.cues.push(cue);
        for targets in [
            Targets::Previous(PreviousKeyword::Previous),
            Targets::All(AllKeyword::All),
            Targets::Output { output: "main".into() },
            Targets::Cues(vec![show.cues[0].id]),
        ] {
            show.cues.push(Cue::new(CueKind::Volume(VolumeCue {
                targets,
                action: VolumeAction::FadeTo,
                volume_db: Some(-12.0),
                duration: None,
                curve: FadeCurve::Linear,
            })));
        }
        show.cues.push(Cue::new(CueKind::Sound(SoundCue {
            loop_mode: LoopMode::Times(3),
            vamps: vec![Vamp { start: 12.0, end: 24.5 }],
            marks: vec![12.0, 18.25],
            normalize_db: Some(-4.5),
            ..SoundCue::new("x.wav")
        })));
        show.others = OthersEffect { effect: OthersAction::FadeStop, fade: 2.5, ..Default::default() };
        show.show_mode_locked = true;
        show.level_limits = LevelLimits { max_db: 0.0, min_db: -50.0 };
        show.loudness_target = -16.0;
        show.normalize_on_import = true;
        show.devices.push(NetDevice {
            id: "lx".into(),
            name: "Lichtpult".into(),
            host: "192.168.1.20".into(),
            port: 8000,
        });
        show.cues.push(Cue::new(CueKind::Group(GroupCue {
            mode: GroupMode::Sequence,
            cues: vec![
                Cue::new(CueKind::Wait(WaitCue { duration: 1.0 })),
                Cue::new(CueKind::Sound(SoundCue::new("a.wav"))),
            ],
        })));
        show.cues.push(Cue::new(CueKind::Playlist(PlaylistCue {
            gap: 2.0,
            repeat: true,
            items: vec![
                PlaylistItem::new("einlass/a.mp3"),
                PlaylistItem {
                    normalize_db: Some(3.0),
                    overrides: ItemOverrides {
                        gain_db: -4.0,
                        fade_in: Some(1.5),
                        ..ItemOverrides::default()
                    },
                    ..PlaylistItem::new("einlass/b.mp3")
                },
            ],
            speed: Speed { rate: 90.0, preserve_pitch: false, transpose: 2.5 },
            ..PlaylistCue::new(vec![])
        })));
        show.cues.push(Cue::new(CueKind::Midi(MidiCue {
            messages: vec![
                MidiMessage::ProgramChange { channel: 0, program: 5 },
                MidiMessage::Msc {
                    device: 127,
                    command: MscCommand::Go,
                    cue: "12.5".into(),
                    list: String::new(),
                },
            ],
        })));
        show.cues.push(Cue::new(CueKind::Osc(OscCue {
            device: Some("lx".into()),
            messages: vec![OscMessage {
                address: "/pb/1/go".into(),
                args: vec![
                    OscArg::Int(1),
                    OscArg::Float(0.5),
                    OscArg::Float(1.0),
                    OscArg::String("x".into()),
                ],
            }],
        })));
        show.cues.push(Cue::new(CueKind::Wait(WaitCue { duration: 4.0 })));
        show.cues.push(Cue::new(CueKind::ExitLoop(ExitLoopCue {
            targets: Targets::Cues(vec![show.cues[0].id]),
            exit: ExitLoop { mode: DevampMode::NextMark, level: LoopLevel::All, stop: true },
        })));
        let devamp = show.cues.len() - 1;
        show.cues[devamp].advance = Advance::new(AdvanceAt::LoopExit { of: Some(show.cues[0].id) });
        show.cues.push(Cue::new(CueKind::GoTo(GoToCue { target: Some(show.cues[1].id), start: true })));
        let first = Targets::Cues(vec![show.cues[0].id]);
        show.cues.push(Cue::new(CueKind::StartStop(StartStopCue {
            targets: first.clone(),
            action: StartStopAction::Start,
            from: Some(12.5),
        })));
        show.cues.push(Cue::new(CueKind::Pause(PauseCue {
            targets: Targets::All(AllKeyword::All),
            action: PauseAction::Toggle,
        })));
        show.cues.push(Cue::new(CueKind::Pan(PanCue {
            targets: first.clone(),
            pan: -0.5,
            duration: 3.0,
            curve: PanCurve::Linear,
        })));
        show.cues.push(Cue::new(CueKind::Rate(RateCue {
            targets: first.clone(),
            rate: Some(80.0),
            transpose: None,
            duration: 2.0,
            curve: PanCurve::SCurve,
        })));
        for to in [
            PositionTo::Absolute { secs: 30.0 },
            PositionTo::Relative { secs: -5.0 },
            PositionTo::Mark { index: 2 },
            PositionTo::NextMark,
            PositionTo::PreviousMark,
        ] {
            show.cues.push(Cue::new(CueKind::Position(PositionCue {
                targets: first.clone(),
                to,
                crossfade_ms: 0.0,
            })));
        }
        show.cues.push(Cue::new(CueKind::Reset(ResetCue { targets: first, duration: 2.0 })));
        show.stopwatches.push(Stopwatch { id: "show".into(), name: "Vorstellung".into(), countdown: None });
        show.stopwatches.push(Stopwatch { id: "pause".into(), name: "Pause".into(), countdown: Some(900.0) });
        show.cues.push(Cue::new(CueKind::Scene(SceneCue {})));
        show.cues.push(Cue::new(CueKind::Sound(SoundCue::placeholder())));
        show.cues.push(Cue::new(CueKind::Stopwatch(StopwatchCue {
            watch: Some("show".into()),
            action: StopwatchAction::Restart,
        })));
        show.cues[1].passes = Repeat::Forever;
        show.cues[0].second_trigger = SecondTrigger::ExitLoop { exit: ExitLoop::default() };
        show.cues[1].others = Some(OthersEffect { effect: OthersAction::Duck, ..Default::default() });

        let json = serde_json::to_string_pretty(&show).unwrap();
        assert!(json.contains(r#""loop": "forever""#), "{json}");
        assert!(json.contains(r#""curve": "decibel_s""#), "{json}");
        assert!(json.contains(r#""targets": "all""#), "{json}");
        assert!(json.contains(r#""mode": "next_mark""#), "{json}");
        assert!(json.contains(r#""type": "goto""#), "{json}");
        assert!(json.contains(r#""type": "scene""#), "{json}");
        assert!(json.contains(r#""countdown": 900.0"#), "{json}");
        assert!(json.contains(r#""passes": "forever""#), "{json}");
        let compact = json.replace([' ', '\n'], "");
        assert!(
            compact.contains(r#""advance":{"start_next":true,"at":{"at":"before_end","secs":2.5}}"#),
            "{json}"
        );
        assert!(json.contains(r#""kind": "program_change""#), "{json}");
        assert!(
            compact.contains(r#""args":[{"int":1},{"float":0.5},{"float":1.0},{"string":"x"}]"#),
            "ints, floats and strings stay apart: {json}"
        );
        // From JavaScript a float 1.0 arrives as 1.
        let arg: OscArg = serde_json::from_str(r#"{"float": 1}"#).unwrap();
        assert_eq!(arg, OscArg::Float(1.0));
        assert_eq!(serde_json::from_str::<Show>(&json).unwrap(), show);
        let file = String::from_utf8(show_file_json(&show)).unwrap();
        assert_eq!(serde_json::from_str::<Show>(&file).unwrap(), show, "{file}");
    }

    #[test]
    fn the_file_leaves_out_defaults() {
        let mut show = Show { name: "Show".into(), ..Default::default() };
        let sound = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        show.cues.push(sound.clone());
        let file: serde_json::Value = serde_json::from_slice(&show_file_json(&show)).unwrap();
        let keys = |value: &serde_json::Value| value.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&file), ["cues", "format", "id", "name", "version"]);
        assert_eq!(keys(&file["cues"][0]), ["file", "id", "type"]);
        // The UI still gets every field.
        let full = serde_json::to_value(&sound).unwrap();
        assert_eq!(full["notes"], "");
        assert_eq!(full["output"], "main");

        // Values that differ from a non-empty default are kept.
        show.media_root = String::new();
        show.outputs.clear();
        show.loudness_target = -23.0;
        show.others.duck_db = 0.0;
        show.cues[0].advance.at = AdvanceAt::Start;
        let CueKind::Sound(cue) = &mut show.cues[0].kind else { unreachable!() };
        cue.output = String::new();
        cue.speed.rate = 100.0;
        let mut group = GroupCue::default();
        group.cues.push(Cue::new(CueKind::Playlist(PlaylistCue::new(vec!["b.wav".into()]))));
        group.cues.push(Cue::new(CueKind::GoTo(GoToCue::default())));
        group.cues.push(Cue::new(CueKind::Osc(OscCue::default())));
        group.cues.push(Cue::new(CueKind::Midi(MidiCue::default())));
        group.cues.push(Cue::new(CueKind::Scene(SceneCue {})));
        show.cues.push(Cue::new(CueKind::Group(group)));
        show.cues.push(Cue::new(CueKind::Position(PositionCue {
            targets: Targets::Previous(PreviousKeyword::Previous),
            to: PositionTo::NextMark,
            crossfade_ms: 20.0,
        })));
        let file = String::from_utf8(show_file_json(&show)).unwrap();
        assert_eq!(serde_json::from_str::<Show>(&file).unwrap(), show, "{file}");
        assert!(!file.contains("crossfade_ms"), "{file}");
    }

    #[test]
    fn level_limits_cap_and_silence() {
        let limits = LevelLimits::default();
        assert_eq!(limits.level(-6.0, 0.0), Some(-6.0));
        assert_eq!(limits.level(-6.0, -4.0), Some(-10.0));
        assert_eq!(limits.level(10.0, 0.0), Some(6.0));
        assert_eq!(limits.level(0.0, 9.0), Some(6.0), "the offset is capped too");
        assert_eq!(limits.level(-60.0, 20.0), None, "the minimum is silence");
        assert_eq!(limits.level(f64::NEG_INFINITY, 0.0), None);
        assert_eq!(limits.level(f64::NAN, 0.0), None);
        assert_eq!(limits.level(-55.0, -10.0), Some(-65.0), "above the minimum, offsets may go below it");
        assert!(limits.caps(3.0, 4.0) && !limits.caps(3.0, 3.0));

        assert!(limits.silences(-70.0) && limits.silences(-60.0));
        assert!(!limits.silences(SILENCE_DB) && !limits.silences(-59.0), "set to silence, or audible");
    }

    fn sound_at(db: f64) -> Cue {
        Cue::new(CueKind::Sound(SoundCue { volume_db: db, ..SoundCue::new("a.wav") }))
    }

    fn volume(show: &Show, i: usize) -> f64 {
        show.cues[i].levels()[0].1
    }

    #[test]
    fn only_changed_levels_are_capped() {
        let read = sound_at(12.0);
        // Another field changed: the volume from the file stays.
        let mut edited = read.clone();
        edited.title = "neu".into();
        edited.cap_changed_levels(Some(&read), 6.0);
        assert_eq!(edited.levels()[0].1, 12.0);
        // The volume changed (still too high): capped, also inside groups.
        let group = Cue::new(CueKind::Group(GroupCue { mode: GroupMode::All, cues: vec![read] }));
        let mut louder = group.clone();
        let CueKind::Group(g) = &mut louder.kind else { panic!() };
        let CueKind::Sound(s) = &mut g.cues[0].kind else { panic!() };
        s.volume_db = 20.0;
        louder.cap_changed_levels(Some(&group), 6.0);
        assert_eq!(louder.levels()[0].1, 6.0);
        let mut new = sound_at(9.0);
        new.cap_changed_levels(None, 6.0);
        assert_eq!(new.levels()[0].1, 6.0);
    }

    #[test]
    fn new_limits_clamp_levels_and_keep_silence_silent() {
        let fade = Cue::new(CueKind::Volume(VolumeCue {
            targets: Targets::All(AllKeyword::All),
            action: VolumeAction::FadeTo,
            volume_db: Some(-55.0),
            duration: None,
            curve: FadeCurve::Linear,
        }));
        let cues = vec![
            sound_at(4.0),
            sound_at(-55.0),
            sound_at(-58.0),
            sound_at(SILENCE_DB),
            sound_at(-65.0),
            fade,
        ];
        let ids: Vec<CueId> = cues.iter().map(|c| c.id).collect();
        let mut show = Show { cues, ..Show::default() };
        let old = show.level_limits;
        let new = LevelLimits { max_db: 0.0, min_db: -56.0 };
        let changes = show.clamp_levels(old, new);
        assert_eq!(changes.capped, [ids[0]]);
        assert_eq!(changes.silenced, [ids[2]], "-55 stays audible, -58 goes silent; -65 was silent already");
        let levels: Vec<f64> = (0..5).map(|i| volume(&show, i)).collect();
        assert_eq!(levels, [0.0, -55.0, SILENCE_DB, SILENCE_DB, SILENCE_DB]);
        assert_eq!(show.cues[5].levels()[0].1, -55.0);

        // Lowering the minimum again brings nothing back.
        show.level_limits = new;
        let changes = show.clamp_levels(new, LevelLimits { max_db: 0.0, min_db: -80.0 });
        assert_eq!(changes, LevelChanges::default());
        assert_eq!(volume(&show, 2), SILENCE_DB);
    }

    #[test]
    fn create_save_load() {
        let dir = tempdir("file");
        let mut file = ShowFile::create(&dir, "Meine-Show", true).unwrap();
        assert!(dir.join("Meine-Show/media").is_dir());
        file.show.cues.push(Cue::new(CueKind::Sound(SoundCue::new("a.wav"))));
        file.save().unwrap();
        assert!(!dir.join("Meine-Show/Meine-Show.jqshow.tmp").exists());

        let loaded = ShowFile::load(&dir.join("Meine-Show/Meine-Show.jqshow")).unwrap();
        assert_eq!(loaded.show, file.show);
        assert_eq!(loaded.media_dir(), dir.join("Meine-Show").join("media"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_foreign_and_newer_files() {
        let dir = tempdir("reject");
        let p = dir.join("x.jq");
        fs::write(&p, r#"{"format": "other", "version": 1}"#).unwrap();
        assert!(matches!(ShowFile::load(&p), Err(ShowFileError::WrongFormat { .. })));
        fs::write(&p, r#"{"format": "jqshow", "version": 99}"#).unwrap();
        assert!(matches!(ShowFile::load(&p), Err(ShowFileError::TooNew { .. })));
        fs::remove_dir_all(dir).unwrap();
    }
}
