//! Why a command failed, as the UI gets it: `{ kind, …data }`, worded by the
//! UI (`app/src/lib/errors.ts`). Keep `CommandError` in `app/src/lib/api.ts`
//! in step. What comes from the layers below is a [`Detail`]: a message of
//! theirs, or a system's text as it is.

use jq_core::CueName;
use jq_core::keys::{ControlAction, KeyError};
use jq_i18n::Detail;
use jq_show::{Chord, ShowError, ShowFileError};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Error {
    App(AppError),
    Show(ShowError),
    Keys(KeyError),
    Detail(Detail),
}

/// A setting a number is checked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    FadeAll,
    EndWarning,
    PanicFade,
    GoLockout,
    VolumeStep,
    PanStep,
    RateStep,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppError {
    // The app and its show thread
    NoShow,
    NoReply,
    BackendLocked,
    ThreadEnded,
    WriterPanicked,
    PeakCacheNotCleared {
        error: String,
    },
    MediaDirNotFound {
        path: String,
    },
    FileManagerFailed {
        error: String,
    },

    // Editing and the show's folders
    ShowModeLocked,
    MediaDirBusyCue,
    MediaDirBusyFiles,
    DecideRecovery,
    MediaDirUnreachable {
        error: String,
    },
    NotAFolder {
        path: String,
    },
    ShowDirUnreachable {
        error: String,
    },
    MediaDirNotStorable {
        path: String,
    },
    MediaDirHasShow,
    NameEmpty,
    SaveAsBusyCue,
    SaveAsBusyFiles,
    SaveAsChanged,
    MediaNotCopied {
        error: Detail,
    },
    ConvertControlOnly,
    CountdownRange {
        name: String,
    },
    StopwatchId {
        id: String,
    },
    StopwatchName,
    DeviceId {
        id: String,
    },
    DeviceAddress {
        name: String,
    },
    PlaylistNotFound,
    ItemNotFound,
    SoundNotFound,
    FileUnused,
    LoudnessRange {
        min: f64,
        max: f64,
    },
    SearchRunning,
    InvalidFileName,
    NoGroupInGroup,
    GroupNotFound,
    OutputId {
        id: String,
    },
    OutputName,
    MainOutput,
    OutputInUse {
        output: String,
        cue: CueName,
    },
    LevelLimits {
        min: f64,
        max: f64,
    },
    LevelGap,
    NextItemPlaylistOnly,
    SceneMainOnly,
    GotoTopLevel,
    OthersFade {
        max: f64,
    },
    OthersDuck,
    HotkeyMainOnly,
    HotkeyFixed {
        key: Chord,
    },
    HotkeyGlobal {
        key: Chord,
        action: ControlAction,
    },
    MidiMainOnly,
    MidiGlobal {
        action: ControlAction,
    },
    NoMidiInput,
    KeyboardOnly {
        action: ControlAction,
    },

    // Settings
    SettingNegative {
        setting: Setting,
    },
    SettingPositive {
        setting: Setting,
    },
    JumpTimes,
    KeepAliveLevel {
        min: f64,
        max: f64,
    },
    MscDevice,
    BufferSize {
        min: u32,
        max: u32,
    },
    OscPort,
    ChannelsFromOne,
    MinChannels,
    SettingsShowModeLocked,
    AudioDeviceBusy,
    DeviceTooFewChannels {
        name: String,
        max: u16,
        channels: u16,
    },
    /// An output (`None`: the preview) on channels the device (`name`)
    /// lacks: it is set to (`set`) or has `limit` channels.
    PairMissing {
        output: Option<String>,
        first: usize,
        second: usize,
        name: String,
        limit: u16,
        set: bool,
    },

    // Updates
    UpdateCheckFailed {
        error: String,
    },
    UpdateInstallFailed {
        error: String,
    },
    UpdateBusy,
    UpdateLocked,
    UpdateInstalling,
    UpdateUnsaved,
    UpdateNonePending,
    UpdateWritesPending,
    UpdatePortable,
    UpdateRpm,
    UpdateNotBundled,
}

impl From<AppError> for Error {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

impl From<ShowError> for Error {
    fn from(e: ShowError) -> Self {
        Self::Show(e)
    }
}

impl From<ShowFileError> for Error {
    fn from(e: ShowFileError) -> Self {
        Self::Show(e.into())
    }
}

impl From<KeyError> for Error {
    fn from(e: KeyError) -> Self {
        Self::Keys(e)
    }
}

/// Text from below (a system, device or file error).
impl From<Detail> for Error {
    fn from(d: Detail) -> Self {
        Self::Detail(d)
    }
}

/// A system's text (a device, file or network error).
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Detail(Detail::text(message))
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl Error {
    /// As the UI gets it, for a notice that carries it.
    pub fn detail(&self) -> Detail {
        match self {
            Self::Detail(d) => d.clone(),
            other => Detail::of(other),
        }
    }
}

/// For logs and the native close dialog (which shows why saving failed):
/// what saving can fail with reads as text, the rest as its data.
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Detail(d) => write!(f, "{d}"),
            Self::App(AppError::WriterPanicked) => f.write_str(&jq_i18n::t!("writer.panicked")),
            Self::App(AppError::NoReply) => f.write_str(&jq_i18n::t!("app.no_reply")),
            Self::Show(ShowError::FileIo { path, error } | ShowError::FileInvalid { path, error }) => {
                write!(f, "{path}: {error}")
            }
            other => write!(f, "{}", other.detail()),
        }
    }
}
