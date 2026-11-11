//! The program's language: the setting ("auto", "en", "de") and what it resolves to. The backend's own messages
//! follow it (see `jq-i18n`); the UI asks for it at the start and is told about changes with an event.

use std::sync::Mutex;

use serde::Serialize;

/// Sent to all windows when the language changes; the payload is a [`LanguageInfo`].
pub const EVENT_LANGUAGE_CHANGED: &str = "language_changed";

/// The setting as stored; "auto" until the settings are read.
static SETTING: Mutex<String> = Mutex::new(String::new());

#[derive(Clone, Debug, Serialize)]
pub struct LanguageInfo {
    /// What the settings say: "auto" or a language code.
    pub setting: String,
    /// The language in use: a code of the offered languages.
    pub resolved: &'static str,
}

/// Makes `setting` the language of the backend's messages.
pub fn apply(setting: &str) -> LanguageInfo {
    let lang = jq_i18n::resolve(setting);
    jq_i18n::set_language(lang);
    let setting = if setting.is_empty() { jq_i18n::AUTO } else { setting };
    if let Ok(mut stored) = SETTING.lock() {
        *stored = setting.to_owned();
    }
    LanguageInfo { setting: setting.to_owned(), resolved: lang.code() }
}

pub fn current() -> LanguageInfo {
    let setting = SETTING.lock().map(|s| s.clone()).unwrap_or_default();
    let setting = if setting.is_empty() { jq_i18n::AUTO.to_owned() } else { setting };
    LanguageInfo { setting, resolved: jq_i18n::language().code() }
}

/// The language setting and what it resolves to; asked for before any show is open.
#[tauri::command]
pub fn get_language() -> LanguageInfo {
    current()
}
