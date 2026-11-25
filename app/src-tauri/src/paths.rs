//! Where the app keeps its own files. Installed, that is the user's folders
//! (Linux: `~/.config/jq-player`, `~/.cache/jq-player`; Windows:
//! `%APPDATA%\jq-player`, `%LOCALAPPDATA%\jq-player`). The portable build
//! (feature `portable`) keeps everything in a folder next to the executable
//! instead, so the program and its settings can travel together on a stick.

use std::path::PathBuf;
#[cfg(feature = "portable")]
use std::sync::LazyLock;

use tauri::{AppHandle, Manager};

/// Name of the portable data folder next to the executable.
#[cfg(feature = "portable")]
const PORTABLE_FOLDER: &str = "jQ Player Data";

/// The portable data folder (`None` in the installed build, or if the
/// executable's location is unknown, which falls back to the user's folders).
#[cfg(feature = "portable")]
fn portable_dir() -> Option<&'static PathBuf> {
    static DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
        let exe = std::env::current_exe().ok()?;
        Some(exe.parent()?.join(PORTABLE_FOLDER))
    });
    DIR.as_ref()
}

#[cfg(not(feature = "portable"))]
fn portable_dir() -> Option<&'static PathBuf> {
    None
}

/// Whether this is the portable build.
pub const fn portable() -> bool {
    cfg!(feature = "portable")
}

/// Settings and window state.
pub fn config_dir(app: &AppHandle) -> Option<PathBuf> {
    match portable_dir() {
        Some(dir) => Some(dir.join("config")),
        None => app.path().app_config_dir().ok(),
    }
}

/// Installed cue libraries.
pub fn library_dir(app: &AppHandle) -> Option<PathBuf> {
    config_dir(app).map(|dir| dir.join("libraries"))
}

/// Waveform peaks.
pub fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    match portable_dir() {
        Some(dir) => Some(dir.join("cache")),
        None => app.path().app_cache_dir().ok(),
    }
}

/// The window-state plugin's file. It joins the name to the user's config
/// folder, which an absolute path replaces (`None`: the plugin's default).
pub fn window_state_file() -> Option<String> {
    let file = portable_dir()?.join("config").join(".window-state.json");
    Some(file.to_string_lossy().into_owned())
}

/// Prepares the portable folder: creates its config folder (the window-state
/// plugin only creates the user's) and points the webview's own data (local
/// storage, its caches) into it. Tauri sets each webview's data folder to the
/// user's local data folder; the environment variables override that. Must
/// run before Tauri starts, while the program has only one thread.
pub fn prepare() {
    let Some(dir) = portable_dir() else { return };
    // Failing (read-only medium) shows up later as settings that cannot be saved.
    let _ = std::fs::create_dir_all(dir.join("config"));
    // SAFETY: called first thing in `main`, before any other thread exists.
    unsafe {
        // WebView2 prefers this variable over the folder Tauri passes.
        #[cfg(windows)]
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", dir.join("webview"));
        // Tauri asks the XDG folders for the local data folder it gives WebKitGTK.
        #[cfg(target_os = "linux")]
        std::env::set_var("XDG_DATA_HOME", dir.join("webview"));
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = dir;
}
