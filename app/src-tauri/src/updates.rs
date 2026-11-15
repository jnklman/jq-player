//! Self-update from the latest GitHub release (`latest.json`, see the Build
//! workflow). Only ever on the operator's request: the check is a button in
//! the About dialog, and installing is refused while a show could be
//! disturbed, because on Windows the installer ends the program.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use serde::Serialize;
use tauri::utils::{config::BundleType, platform::bundle_type};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use jq_i18n::t;

use crate::Backend;
use crate::backend::{CloseCheck, Request};
use crate::writer::Writer;

const EVENT_PROGRESS: &str = "update_progress";
/// A hanging show thread counts as "something may be running".
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);
/// How long queued settings and autosave writes may take before installing.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// The update found by the last `check_update`, installed by `install_update`,
/// and whether an install is under way (a second one is refused).
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>, AtomicBool);

/// Clears the "install under way" flag when the install ends either way.
struct Installing<'a>(&'a AtomicBool);

impl Drop for Installing<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

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
/// unsaved changes: checked before the download and again right before
/// installing, since the download takes a while and the installer ends the
/// program on Windows. The update stays pending if it fails, so a retry
/// needs no new check.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    backend: State<'_, Backend>,
    pending: State<'_, Pending>,
) -> Result<(), String> {
    // The updater would run the installer, which installs instead of replacing this file.
    if crate::paths::portable() {
        return Err(t!("update.portable"));
    }
    updatable()?;
    if pending.1.swap(true, Ordering::AcqRel) {
        return Err(t!("update.installing"));
    }
    let _installing = Installing(&pending.1);
    show_allows_update(&backend).await?;
    let update = pending
        .0
        .lock()
        .map_err(|_| t!("app.backend_locked"))?
        .clone()
        .ok_or_else(|| t!("update.none_pending"))?;
    let mut downloaded = 0;
    let progress = app.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(EVENT_PROGRESS, Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| t!("update.install_failed", error = e))?;
    // Meanwhile a cue may have started or show mode begun.
    show_allows_update(&backend).await?;
    // The installer ends the program: what is still queued goes to disk first.
    let writer = app.state::<Writer>().inner().clone();
    let flushed = tauri::async_runtime::spawn_blocking(move || writer.flush(FLUSH_TIMEOUT)).await;
    if !flushed.unwrap_or(false) {
        return Err(t!("update.writes_pending"));
    }
    update.install(bytes).map_err(|e| t!("update.install_failed", error = e))?;
    // The installer ends the program on Windows; elsewhere this starts the new version.
    app.restart();
}

/// Refuses an update this copy cannot install. `latest.json` has the NSIS
/// installer, the MSI, the AppImage and the deb package; the updater picks its
/// installer from the bundle type, so an rpm install would download one of
/// those and fail with "invalid updater binary format". A deb update runs
/// `dpkg -i` through pkexec (or sudo), which asks for the password.
fn updatable() -> Result<(), String> {
    match bundle_type() {
        Some(BundleType::Nsis | BundleType::Msi | BundleType::AppImage | BundleType::Deb) => Ok(()),
        Some(BundleType::Rpm) => Err(t!("update.rpm")),
        _ => Err(t!("update.not_bundled")),
    }
}

/// Refuses an update while the show could be disturbed: show mode, cues
/// running, files being copied in, unsaved changes.
async fn show_allows_update(backend: &Backend) -> Result<(), String> {
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
    match check {
        Some(check) if check.show_mode || check.running > 0 || check.imports > 0 || check.dirty => {
            Err(if check.dirty { t!("update.unsaved") } else { t!("update.busy") })
        }
        _ => Ok(()),
    }
}
