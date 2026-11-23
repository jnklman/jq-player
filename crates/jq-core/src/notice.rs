//! What the operator should see: a notice of a kind with its data, the UI
//! words it (and decides which notices replace each other). Details that
//! come as text from below (device, file and network errors) stay text.

use jq_i18n::Detail;
use jq_show::{Cue, TimingIssue};
use serde::Serialize;

/// How a notice names a cue; the UI builds the label from it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CueName {
    pub number: String,
    pub title: String,
}

impl CueName {
    pub fn of(cue: &Cue) -> Self {
        Self { number: cue.number.clone(), title: cue.title.clone() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Fades out by itself.
    Warning,
    /// Stays until dismissed.
    Error,
}

/// The audio device a device notice is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    Main,
    Preview,
}

/// What a fading cue ignores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FadingIgnores {
    FadeToLevel,
    Reset,
}

/// Something the operator should see. Keep `app/src/lib/api.ts` (`Notice`)
/// and the texts in `app/src/lib/notices.ts` in step.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Notice {
    // Runner
    AlreadyFading {
        cue: CueName,
    },
    /// `ms` of silence in `file` (a file name): the disk was too slow.
    Underrun {
        file: String,
        ms: u64,
    },
    TooManyStretchers {
        cue: CueName,
    },
    TooManyVoices,
    AlreadyRunning {
        cue: CueName,
    },
    Placeholder {
        cue: CueName,
    },
    NoStopwatchChosen {
        cue: CueName,
    },
    StopwatchMissing {
        cue: CueName,
    },
    /// A cue (or a playlist `item`, counted from 1) could not do what it should.
    CueFailed {
        cue: CueName,
        item: Option<usize>,
        error: Detail,
    },
    NoTarget {
        cue: CueName,
    },
    TargetInGroup {
        cue: CueName,
        target: CueName,
    },
    TargetMissing {
        cue: CueName,
    },
    GotoLoop {
        cue: CueName,
    },
    StartLoop {
        cue: CueName,
    },
    NotPreloaded {
        cue: CueName,
    },
    FadingNoEffect {
        cue: CueName,
        ignores: FadingIgnores,
    },
    MarkMissing {
        cue: CueName,
    },
    PreviewPlaceholder,
    NoPreviewOutput,
    PreviewNotLoadable {
        error: Detail,
    },
    PreviewFailed {
        error: Detail,
    },
    EngineFailed {
        error: Detail,
    },
    WatcherUnavailable {
        error: Detail,
    },

    // Devices and connections
    AudioUnavailable {
        error: Detail,
    },
    PreviewUnavailable {
        error: Detail,
    },
    /// An error of the running stream.
    StreamError {
        device: DeviceRole,
        error: Detail,
    },
    DeviceLost {
        device: DeviceRole,
        name: String,
        reason: Detail,
    },
    DeviceBack {
        device: DeviceRole,
        name: String,
    },
    /// Now on another device (the system default changed).
    DeviceSwitched {
        device: DeviceRole,
        name: String,
    },
    /// An output (`None`: the preview) is routed to channels the device lacks.
    OutputChannels {
        output: Option<String>,
        first: usize,
        second: usize,
        channels: usize,
    },
    KeepAwakeFailed {
        error: Detail,
    },
    OscListenFailed {
        port: u16,
        error: Detail,
    },
    OscSendUnavailable {
        error: Detail,
    },
    MidiInFailed {
        error: Detail,
    },
    MidiOutFailed {
        error: Detail,
    },
    MscFailed {
        error: Detail,
    },
    /// Learning a MIDI trigger failed; `error` is a command error as the UI
    /// gets it from a command.
    LearnFailed {
        error: Detail,
    },

    // Show and media files
    WritesPending,
    SettingsIgnored {
        error: Detail,
    },
    LinkSynced {
        cue: CueName,
    },
    AutosaveUnreadable {
        error: Detail,
    },
    AutosaveNotDeleted {
        error: Detail,
    },
    FileMissing {
        file: String,
    },
    CaseMismatch {
        file: String,
        actual: String,
    },
    ChangedWhilePlaying {
        cue: CueName,
    },
    FileNowMissing {
        file: String,
    },
    FileBack {
        file: String,
    },
    FileChanged {
        file: String,
    },
    /// A position that does not fit the file (of a playlist `item`, from 1).
    Timing {
        cue: CueName,
        item: Option<usize>,
        issue: TimingIssue,
    },
    LockFailed {
        count: usize,
        file: String,
        error: Detail,
    },
    NotCopied {
        file: String,
        error: Detail,
    },
    SearchAborted,
    /// A file was not taken in; `error` as with `LearnFailed`.
    ImportFailed {
        error: Detail,
    },
    RelinkedSome {
        relinked: usize,
        missing: usize,
    },
    NormalizeWaiting {
        count: usize,
    },
    OutsideShowFolder {
        path: String,
    },
    NameTaken {
        name: String,
        stored: String,
    },
    SaveAsMissing {
        count: usize,
        file: String,
    },
    SaveAsOutside {
        count: usize,
        file: String,
    },
    /// `error` as with `LearnFailed`.
    StartupOpenFailed {
        path: String,
        error: Detail,
    },
    SettingsNotSaved {
        error: Detail,
    },
    AutosaveFailed {
        error: Detail,
    },
    WriterPanicked,
    InternalError,
}

impl Notice {
    pub fn level(&self) -> Level {
        use Notice::*;
        match self {
            TooManyVoices
            | StopwatchMissing { .. }
            | CueFailed { .. }
            | TargetInGroup { .. }
            | TargetMissing { .. }
            | GotoLoop { .. }
            | StartLoop { .. }
            | PreviewNotLoadable { .. }
            | PreviewFailed { .. }
            | EngineFailed { .. }
            | AudioUnavailable { .. }
            | DeviceLost { .. }
            | NotCopied { .. }
            | SearchAborted
            | StartupOpenFailed { .. }
            | WriterPanicked
            | InternalError => Level::Error,
            _ => Level::Warning,
        }
    }

    /// The notice as the `notice` event carries it: its level next to its
    /// kind and data.
    pub fn sent(&self) -> SentNotice<'_> {
        SentNotice { level: self.level(), notice: self }
    }
}

#[derive(Clone, Serialize)]
pub struct SentNotice<'a> {
    pub level: Level,
    #[serde(flatten)]
    pub notice: &'a Notice,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_sent_with_its_level_kind_and_data() {
        let cue = CueName { number: "1".into(), title: "Rain".into() };
        let json = serde_json::to_value(Notice::AlreadyRunning { cue }.sent()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "level": "warning", "kind": "already_running", "cue": { "number": "1", "title": "Rain" } })
        );
        let json = serde_json::to_value(Notice::TooManyVoices.sent()).unwrap();
        assert_eq!(json, serde_json::json!({ "level": "error", "kind": "too_many_voices" }));
    }
}
