//! Tauri shell of jQ Player. The commands only forward to the show thread
//! (`backend`); see `app/src/lib/api.ts` for their TypeScript signatures.

mod backend;
mod close;
mod jobs;
mod key_labels;
mod language;
mod media;
mod panel_windows;
mod save_as;
mod session;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use backend::Request;
use jq_audio::DeviceInfo;
use jq_core::{Action, LocalSettings, PeakCache, SelectionMode, Startup};
use jq_show::{Cue, CueId, StopwatchAction};
use serde::Serialize;
use session::EditOp;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_window_state::StateFlags;

/// Window state kept between runs. Decorations and visibility come from
/// the config.
const WINDOW_STATE: StateFlags =
    StateFlags::SIZE.union(StateFlags::POSITION).union(StateFlags::MAXIMIZED).union(StateFlags::FULLSCREEN);

/// A show file opened from outside while the program runs (file manager,
/// second start); the UI asks first, like for `open_show`.
const EVENT_OPEN_REQUESTED: &str = "open_requested";

/// A show opened at startup that runs this long did not crash the app.
const STARTUP_GRACE: Duration = Duration::from_secs(5);

/// Requests that take longer than this are reported as failed.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

struct Backend(Mutex<Sender<Request>>);

/// The last show, if the start screen is to offer opening it.
struct StartupOffer(Mutex<Option<PathBuf>>);

impl Backend {
    fn send(&self, request: Request) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| jq_i18n::t!("app.backend_locked"))?
            .send(request)
            .map_err(|_| jq_i18n::t!("app.thread_ended"))
    }

    /// Sends a request and waits for its reply off the async runtime.
    async fn ask<T: Send + 'static>(
        &self,
        make: impl FnOnce(backend::Reply<T>) -> Request,
    ) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.send(make(tx))?;
        tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(REPLY_TIMEOUT))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|_| jq_i18n::t!("app.no_reply"))?
    }
}

#[tauri::command]
async fn open_show(backend: State<'_, Backend>, path: PathBuf) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::Open(path, r)).await
}

/// The last show the start screen offers to open; only the first call gets
/// it.
#[tauri::command]
fn startup_show(offer: State<'_, StartupOffer>) -> Option<PathBuf> {
    offer.0.lock().ok()?.take()
}

#[tauri::command]
async fn new_show(
    backend: State<'_, Backend>,
    dir: PathBuf,
    name: String,
    create_folder: bool,
) -> Result<serde_json::Value, String> {
    backend.ask(|reply| Request::New { dir, name, create_folder, reply }).await
}

#[tauri::command]
async fn save_show(backend: State<'_, Backend>) -> Result<serde_json::Value, String> {
    backend.ask(Request::Save).await
}

#[tauri::command]
async fn get_show(backend: State<'_, Backend>) -> Result<serde_json::Value, String> {
    backend.ask(Request::View).await
}

/// Fire-and-forget, so GO never waits for a reply.
#[tauri::command]
fn action(backend: State<'_, Backend>, action: Action) -> Result<(), String> {
    backend.send(Request::Action(action))
}

/// Fire-and-forget like `action`: the stopwatch buttons in the header.
#[tauri::command]
fn stopwatch(backend: State<'_, Backend>, id: String, action: StopwatchAction) -> Result<(), String> {
    backend.send(Request::Stopwatch(id, action))
}

#[tauri::command]
async fn edit(backend: State<'_, Backend>, op: EditOp) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::Edit(Box::new(op), r)).await
}

/// Fire-and-forget like `action`: sent for every step of a slider.
#[tauri::command]
fn live_cue(backend: State<'_, Backend>, cue: Cue) -> Result<(), String> {
    backend.send(Request::LiveCue(Box::new(cue)))
}

/// Fire-and-forget like `action`: plays a sound cue's file on the preview
/// output from `at` seconds to `until` or its end (waveform editor).
#[tauri::command]
fn preview_at(backend: State<'_, Backend>, cue: CueId, at: f64, until: Option<f64>) -> Result<(), String> {
    backend.send(Request::PreviewAt(cue, at, until))
}

#[tauri::command]
async fn set_keep_awake(backend: State<'_, Backend>, enabled: bool) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::SetKeepAwake(enabled, r)).await
}

#[tauri::command]
async fn midi_learn(
    backend: State<'_, Backend>,
    target: Option<session::LearnTarget>,
) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::MidiLearn(target, r)).await
}

/// Labels of the character keys on the current keyboard layout, by
/// `KeyboardEvent.code`.
#[tauri::command]
async fn key_labels(app: AppHandle) -> Result<std::collections::HashMap<String, String>, String> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(key_labels::labels());
    })
    .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct MidiPorts {
    inputs: Vec<String>,
    outputs: Vec<String>,
}

/// Names of the MIDI input and output ports.
#[tauri::command]
async fn midi_ports() -> Result<MidiPorts, String> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(MidiPorts { inputs: jq_midi::input_ports()?, outputs: jq_midi::output_ports()? })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Peaks of a media file for drawing its waveform, from the peak cache or
/// decoded in the background, never on the show thread.
#[tauri::command]
async fn waveform(
    peaks: State<'_, Arc<PeakCache>>,
    media_dir: PathBuf,
    file: String,
    buckets: usize,
) -> Result<jq_media::Waveform, String> {
    let peaks = peaks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = jq_show::paths::resolve(&media_dir, &file);
        // Enough for the waveform editor zoomed in on a long file.
        peaks.waveform(&path, buckets.clamp(1, 200_000)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Detail of `from`–`to` seconds of a media file for a zoomed-in waveform.
#[tauri::command]
async fn waveform_range(
    peaks: State<'_, Arc<PeakCache>>,
    media_dir: PathBuf,
    file: String,
    from: f64,
    to: f64,
    buckets: usize,
) -> Result<jq_media::Waveform, String> {
    let peaks = peaks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = jq_show::paths::resolve(&media_dir, &file);
        peaks.waveform_range(&path, from, to, buckets.clamp(1, 20_000)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Bytes the peak cache takes on disk.
#[tauri::command]
async fn peak_cache_size(peaks: State<'_, Arc<PeakCache>>) -> Result<u64, String> {
    let peaks = peaks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || peaks.size()).await.map_err(|e| e.to_string())
}

/// Deletes the stored peaks; waveforms are decoded again when next shown.
/// Returns what is left (peaks being built meanwhile are stored).
#[tauri::command]
async fn clear_peak_cache(peaks: State<'_, Arc<PeakCache>>) -> Result<u64, String> {
    let peaks = peaks.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        peaks.clear().map_err(|e| jq_i18n::t!("app.peak_cache_not_cleared", error = e))?;
        Ok(peaks.size())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Which picked files lie outside the media folder and whose names are
/// taken there, so the UI can ask before adding them. Compares file
/// contents, so it runs in the background, never on the show thread.
#[tauri::command]
async fn check_import(media_dir: PathBuf, paths: Vec<PathBuf>) -> Result<session::ImportCheck, String> {
    tauri::async_runtime::spawn_blocking(move || session::check_import(&media_dir, &paths))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn recover_autosave(backend: State<'_, Backend>, apply: bool) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::Recover(apply, r)).await
}

#[tauri::command]
async fn rename_show(backend: State<'_, Backend>, name: String) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::Rename(name, r)).await
}

#[tauri::command]
async fn set_show_mode(backend: State<'_, Backend>, enabled: bool) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::SetShowMode(enabled, r)).await
}

#[tauri::command]
async fn set_settings(
    backend: State<'_, Backend>,
    settings: LocalSettings,
) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::SetSettings(Box::new(settings), r)).await
}

/// Folder the audio file picker starts in; `None` = the show's media folder.
#[tauri::command]
async fn set_audio_dir(
    backend: State<'_, Backend>,
    dir: Option<PathBuf>,
) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::SetAudioDir(dir, r)).await
}

/// Changes the show's media folder.
#[tauri::command]
async fn set_media_dir(backend: State<'_, Backend>, dir: PathBuf) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::SetMediaDir(dir, r)).await
}

/// Which cues new level limits would change (`{ capped, silenced }`), so
/// the UI can warn before setting them.
#[tauri::command]
async fn check_level_limits(
    backend: State<'_, Backend>,
    limits: jq_show::LevelLimits,
) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::CheckLevelLimits(limits, r)).await
}

/// How the cue list's selection and the playhead go together.
#[tauri::command]
async fn set_selection_mode(
    backend: State<'_, Backend>,
    mode: SelectionMode,
) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::SetSelectionMode(mode, r)).await
}

/// Looks at the show's media files again, for changes the file watcher
/// missed (e.g. on a network share).
#[tauri::command]
async fn rescan_media(backend: State<'_, Backend>) -> Result<serde_json::Value, String> {
    backend.ask(Request::RescanMedia).await
}

/// Arrangement of the main window's panels, stored as the UI sends it.
#[tauri::command]
async fn set_layout(
    backend: State<'_, Backend>,
    layout: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    backend.ask(|r| Request::SetLayout(layout, r)).await
}

#[derive(Serialize)]
struct AudioDevices {
    hosts: Vec<String>,
    /// The listed host (the system default if none was asked for).
    host: String,
    devices: Vec<DeviceInfo>,
}

/// Probing devices can take a while, so it runs off the show thread.
#[tauri::command]
async fn audio_devices(host: Option<String>) -> Result<AudioDevices, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let devices = jq_audio::list_devices(host.as_deref()).map_err(|e| e.to_string())?;
        let host = host.unwrap_or_else(jq_audio::default_host_name);
        Ok(AudioDevices { hosts: jq_audio::host_names(), host, devices })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The show file among command-line arguments (without the program name),
/// resolved against the folder the program was started in.
fn show_arg(args: impl IntoIterator<Item = String>, cwd: &Path) -> Option<PathBuf> {
    args.into_iter()
        .map(PathBuf::from)
        .find(|path| path.extension().is_some_and(|ext| ext == jq_show::SHOW_FILE_EXTENSION))
        .map(|path| cwd.join(path))
}

/// The program was started again: only one may run (two would fight over
/// the audio and MIDI devices), so this one comes to the front and opens
/// the show file passed along, if any.
fn second_instance(app: &AppHandle, args: Vec<String>, cwd: String) {
    if let Some(window) = app.get_webview_window(panel_windows::MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(path) = show_arg(args.into_iter().skip(1), Path::new(&cwd)) {
        let _ = app.emit_to(panel_windows::MAIN, EVENT_OPEN_REQUESTED, path);
    }
}

/// `--color-line-strong` (#2f343c) and `--color-stop` (#c7484c) as COLORREF, 0x00BBGGRR.
#[cfg(windows)]
pub(crate) const BORDER_LINE: u32 = 0x003c_342f;
#[cfg(windows)]
const BORDER_ALERT: u32 = 0x004c_48c7;

/// Windows 11 rounds the borderless window and draws a thin border around
/// it, along with its shadow; the border gets the colour of the app's lines
/// instead of the system's (or the stop colour, see `window_alert`). The UI
/// leaves out its own rounded corners and edge there (`markPlatform` in
/// `lib/window.svelte.ts`). Windows 10 ignores the colour.
#[cfg(windows)]
pub(crate) fn window_border(window: &tauri::WebviewWindow, color: u32) {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmSetWindowAttribute};
    if let Ok(hwnd) = window.hwnd() {
        // SAFETY: a valid window handle and a pointer to a u32 of the size given.
        unsafe {
            DwmSetWindowAttribute(
                hwnd.0,
                DWMWA_BORDER_COLOR as u32,
                std::ptr::from_ref(&color).cast(),
                size_of::<u32>() as u32,
            );
        }
    }
}

/// The window border in the stop colour while the window has no keyboard
/// focus (Windows draws it; elsewhere the UI's own edge turns red).
#[tauri::command]
fn window_alert(window: tauri::WebviewWindow, on: bool) {
    #[cfg(windows)]
    window_border(&window, if on { BORDER_ALERT } else { BORDER_LINE });
    #[cfg(not(windows))]
    let _ = (window, on);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, so a second start ends before setting anything up.
        .plugin(tauri_plugin_single_instance::init(second_instance))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(WINDOW_STATE)
                // Panel windows keep their place in the layout.
                .with_filter(|label| label == panel_windows::MAIN)
                .build(),
        )
        .manage(panel_windows::PanelWindows::default())
        .on_window_event(|window, event| {
            panel_windows::on_window_event(window, event);
            if window.label() == panel_windows::MAIN {
                close::on_window_event(window, event);
            }
        })
        .setup(|app| {
            panel_windows::detect_platform(app.handle());
            // The window starts hidden: on Wayland, tao sets the client-side titlebar after
            // making it visible, which GTK warns about. Hidden, it also appears at the size
            // and position restored by the window-state plugin instead of jumping there.
            if let Some(window) = app.get_webview_window(panel_windows::MAIN) {
                #[cfg(windows)]
                window_border(&window, BORDER_LINE);
                window.show()?;
            }
            // Before the show thread, which hands it to each session.
            let peaks = Arc::new(match app.path().app_cache_dir() {
                Ok(dir) => PeakCache::new(dir.join("peaks")),
                Err(_) => PeakCache::disabled(),
            });
            let cleaning = peaks.clone();
            std::thread::Builder::new().name("jq-peaks-clean".into()).spawn(move || cleaning.clean_up())?;
            app.manage(peaks);
            // Before the show thread, whose messages are in this language.
            let settings_file = backend::settings_path(app.handle());
            language::apply(
                &jq_core::LocalSettings::load_general(&settings_file).unwrap_or_default().language,
            );
            let tx = backend::spawn(app.handle().clone());
            // A show path on the command line (e.g. a show file opened in the file
            // manager) is opened right away: no show is open yet to ask about.
            let cwd = std::env::current_dir().unwrap_or_default();
            let cli = show_arg(std::env::args().skip(1), &cwd);
            let settings = backend::settings_path(app.handle());
            let mut offer = None;
            let open = match Startup::read(&settings, cli.is_some()) {
                Startup::Open(path) => {
                    // Cleared once the show has run for a while; still set on the
                    // next start, it is only offered then.
                    let _ = Startup::set_pending(&settings, true);
                    let survived = tx.clone();
                    std::thread::Builder::new().name("jq-startup".into()).spawn(move || {
                        std::thread::sleep(STARTUP_GRACE);
                        let _ = survived.send(Request::StartupSurvived);
                    })?;
                    Some(path)
                }
                Startup::Ask(path) => {
                    offer = Some(path);
                    let _ = Startup::set_pending(&settings, false);
                    None
                }
                Startup::Nothing => {
                    let _ = Startup::set_pending(&settings, false);
                    cli
                }
            };
            if let Some(path) = open {
                let (reply, _ignored) = mpsc::channel();
                let _ = tx.send(Request::Open(path, reply));
            }
            app.manage(StartupOffer(Mutex::new(offer)));
            app.manage(Backend(Mutex::new(tx)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_show,
            startup_show,
            new_show,
            save_show,
            get_show,
            action,
            stopwatch,
            edit,
            live_cue,
            preview_at,
            set_keep_awake,
            set_show_mode,
            rename_show,
            save_as::save_show_as,
            recover_autosave,
            midi_learn,
            key_labels,
            window_alert,
            midi_ports,
            waveform,
            waveform_range,
            peak_cache_size,
            clear_peak_cache,
            check_import,
            media::unused_media,
            media::clean_media,
            media::open_media_dir,
            set_settings,
            language::get_language,
            set_audio_dir,
            set_media_dir,
            set_layout,
            set_selection_mode,
            rescan_media,
            check_level_limits,
            audio_devices,
            panel_windows::set_panel_windows,
            panel_windows::app_focused,
            panel_windows::focus_main,
            panel_windows::window_support,
            close::close_ack,
            close::confirm_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_arg_finds_the_show_file() {
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let cwd = Path::new("/home/op");
        assert_eq!(show_arg(args(&["--flag", "Show/Show.jqshow"]), cwd), Some(cwd.join("Show/Show.jqshow")));
        assert_eq!(show_arg(args(&["/shows/a.jqshow"]), cwd), Some(PathBuf::from("/shows/a.jqshow")));
        assert_eq!(show_arg(args(&["notes.txt", "show.jq", "fakejqshow"]), cwd), None);
        assert_eq!(show_arg(args(&[]), cwd), None);
    }
}
