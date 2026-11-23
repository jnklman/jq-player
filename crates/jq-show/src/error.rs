//! Why a show edit or a show file operation failed (the UI words it).

use serde::Serialize;

use crate::file::ShowFileError;
use crate::model::VERSION;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShowError {
    CueNotFound,
    LinkNeedsTwo,
    LinkNotGroups,
    LinkSameType,
    NothingSelected,
    GroupMainOnly,
    GroupNotScenes,
    GroupNotNested,
    /// Reading or writing `path` failed with the system's `error`.
    FileIo {
        path: String,
        error: String,
    },
    FileInvalid {
        path: String,
        error: String,
    },
    /// The file is something else (its `format` field says `found`).
    FileWrongFormat {
        path: String,
        found: String,
    },
    /// Written by a newer program: format version `found`, this one reads up to `current`.
    FileTooNew {
        path: String,
        found: u32,
        current: u32,
    },
    InvalidName {
        name: String,
    },
    Exists {
        path: String,
    },
}

impl From<ShowFileError> for ShowError {
    fn from(e: ShowFileError) -> Self {
        let shown = |path: &std::path::Path| path.display().to_string();
        match e {
            ShowFileError::Io { path, source } => {
                Self::FileIo { path: shown(&path), error: source.to_string() }
            }
            ShowFileError::Parse { path, source } => {
                Self::FileInvalid { path: shown(&path), error: source.to_string() }
            }
            ShowFileError::WrongFormat { path, found } => Self::FileWrongFormat { path: shown(&path), found },
            ShowFileError::TooNew { path, found } => {
                Self::FileTooNew { path: shown(&path), found, current: VERSION }
            }
            ShowFileError::InvalidName(name) => Self::InvalidName { name },
            ShowFileError::Exists(path) => Self::Exists { path: shown(&path) },
        }
    }
}
