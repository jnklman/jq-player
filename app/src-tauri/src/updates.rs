//! Self-update from the latest GitHub release (`latest.json`, see the Build
//! workflow). Only ever on the operator's request: the check is a button in
//! the About dialog, and installing is refused while a show could be
//! disturbed, because on Windows the installer ends the program.

use std::sync::{Mutex, mpsc};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use jq_i18n::t;

use crate::Backend;
use crate::backend::{CloseCheck, Request};

const EVENT_PROGRESS: &str = "update_progress";
/// A hanging show thread counts as "something may be running".
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);

/// The update found by the last `check_update`, installed by `install_update`.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress {
    downloaded: u64,
    /// `None` if the server does not tell the size.
    total: Option<u64>,
}

const EVENT_AVAILABLE: &str = "update_available";
/// Lets the program settle (audio, show) before it touches the network.
const STARTUP_DELAY: Duration = Duration::from_secs(10);

/// Looks for an update a while after the start and tells the UI (`update_available`) if there is one.
/// Failures (offline, no release yet) are ignored: nobody asked at that moment.
pub fn check_at_startup(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("jq-update-check".into()).spawn(move || {
        std::thread::sleep(STARTUP_DELAY);
        let pending = app.state::<Pending>();
        if let Ok(Some(info)) = tauri::async_runtime::block_on(check_update(app.clone(), pending)) {
            let _ = app.emit_to(crate::panel_windows::MAIN, EVENT_AVAILABLE, info);
        }
    });
    drop(spawned);
}

/// The newer release, if there is one.
#[tauri::command]
pub async fn check_update(app: AppHandle, pending: State<'_, Pending>) -> Result<Option<UpdateInfo>, String> {
    let updater = app
        .updater_builder()
        .timeout(NETWORK_TIMEOUT)
        .build()
        .map_err(|e| t!("update.check_failed", error = e))?;
    let update = updater.check().await.map_err(|e| t!("update.check_failed", error = e))?;
    let info = update.as_ref().map(|u| UpdateInfo { version: u.version.clone(), notes: u.body.clone() });
    *pending.0.lock().map_err(|_| t!("app.backend_locked"))? = update;
    Ok(info)
}

/// Downloads and installs the update found by `check_update`, then restarts.
/// Refused while a show is in show mode, a cue is running or the show has
/// unsaved changes.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    backend: State<'_, Backend>,
    pending: State<'_, Pending>,
) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    // No answer in time counts as "something may be running".
    let check = match backend.send(Request::CloseCheck(tx)) {
        Ok(()) => tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(CHECK_TIMEOUT))
            .await
            .ok()
            .and_then(Result::ok),
        Err(_) => None,
    };
    let check = check.unwrap_or(Some(CloseCheck { show_mode: true, dirty: false, running: 1, imports: 0 }));
    if let Some(check) = check
        && (check.show_mode || check.running > 0 || check.imports > 0 || check.dirty)
    {
        return Err(if check.dirty { t!("update.unsaved") } else { t!("update.busy") });
    }
    let update = pending
        .0
        .lock()
        .map_err(|_| t!("app.backend_locked"))?
        .take()
        .ok_or_else(|| t!("update.none_pending"))?;
    let mut downloaded = 0;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(EVENT_PROGRESS, Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| t!("update.install_failed", error = e))?;
    // The installer ends the program on Windows; elsewhere this starts the new version.
    app.restart();
}
