//! Asks before the window closes, so a stray Alt+F4 or click on × cannot end
//! a running performance. The UI shows the question in the app's own style
//! (`close_requested` event); if it does not confirm receipt in time because
//! the WebView hangs, a native dialog asks instead.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};
use tauri_plugin_window_state::AppHandleExt;

use jq_i18n::t;

use crate::Backend;
use crate::backend::{CloseCheck, Request};
use crate::error::Error;
use crate::writer::Writer;

/// A hanging show thread counts as "something may be running".
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
const SAVE_TIMEOUT: Duration = Duration::from_secs(10);
/// Time the UI has to confirm it shows the question.
const ACK_TIMEOUT: Duration = Duration::from_secs(1);
/// How long queued settings and autosave writes may take on quitting before
/// the operator is asked whether to wait longer.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

pub const EVENT_CLOSE_REQUESTED: &str = "close_requested";
/// To windows about to close: commit the field being typed in; `fields_committed` answers.
const EVENT_COMMIT_FIELDS: &str = "commit_fields";
/// How long the windows have to commit; a hanging WebView does not hold up closing.
const COMMIT_TIMEOUT: Duration = Duration::from_millis(500);

/// Only one question at a time, however often × is clicked.
static ASKING: AtomicBool = AtomicBool::new(false);
/// Waits for `close_ack` from the UI.
static PENDING_ACK: Mutex<Option<mpsc::Sender<()>>> = Mutex::new(None);
/// Wait for `fields_committed` (the window's label), one per `commit_fields` under way.
static PENDING_COMMITS: Mutex<Vec<mpsc::Sender<String>>> = Mutex::new(Vec::new());

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else { return };
    api.prevent_close();
    if ASKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let window = window.clone();
    // Waiting for the show thread and the dialog must not block the event loop.
    std::thread::spawn(move || {
        check_and_ask(&window);
        ASKING.store(false, Ordering::SeqCst);
    });
}

fn check_and_ask(window: &Window) {
    // Text typed in a field is not in the show yet; without it closing would not ask about it.
    let labels: Vec<String> = window.app_handle().webview_windows().into_keys().collect();
    commit_fields(window.app_handle(), &labels);
    let backend = window.state::<Backend>();
    let check = match ask_backend(&backend, Request::CloseCheck, CHECK_TIMEOUT) {
        Some(None) => return destroy(window),
        Some(Some(check)) => check,
        None => CloseCheck { show_mode: true, show_mode_locked: true, dirty: false, running: 1, imports: 0 },
    };
    if !check.show_mode && !check.dirty && check.running == 0 && check.imports == 0 {
        return destroy(window);
    }
    if !ask_in_ui(window, check) {
        ask_natively(window, check);
    }
}

/// True if the UI took over the question; its answer comes as `confirm_close`.
fn ask_in_ui(window: &Window, check: CloseCheck) -> bool {
    let (tx, rx) = mpsc::channel();
    let Ok(mut pending) = PENDING_ACK.lock() else { return false };
    *pending = Some(tx);
    drop(pending);
    if window.emit_to(crate::panel_windows::MAIN, EVENT_CLOSE_REQUESTED, check).is_err() {
        return false;
    }
    let acked = rx.recv_timeout(ACK_TIMEOUT).is_ok();
    if let Ok(mut pending) = PENDING_ACK.lock() {
        *pending = None;
    }
    acked
}

/// Has the windows `labels` commit what is typed in their focused field (the show gets it only when the field
/// loses the focus) and waits until they did and their edits are applied, at most `COMMIT_TIMEOUT`. Blocks, so
/// never on the event loop.
pub fn commit_fields(app: &AppHandle, labels: &[String]) {
    let (tx, rx) = mpsc::channel();
    let Ok(mut pending) = PENDING_COMMITS.lock() else { return };
    pending.push(tx);
    drop(pending);
    let mut waiting: HashSet<&str> = HashSet::new();
    for label in labels {
        if app.emit_to(label.as_str(), EVENT_COMMIT_FIELDS, ()).is_ok() {
            waiting.insert(label);
        }
    }
    let deadline = Instant::now() + COMMIT_TIMEOUT;
    while !waiting.is_empty() {
        let Ok(label) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) else { break };
        waiting.remove(label.as_str());
    }
    // With `rx` gone, the next `fields_committed` drops its sender.
}

/// A window has committed its field and the edit is applied (`commit_fields`).
#[tauri::command]
pub fn fields_committed(window: Window) {
    if let Ok(mut pending) = PENDING_COMMITS.lock() {
        pending.retain(|tx| tx.send(window.label().to_string()).is_ok());
    }
}

/// The UI shows the close question.
#[tauri::command]
pub fn close_ack() {
    if let Ok(pending) = PENDING_ACK.lock()
        && let Some(tx) = pending.as_ref()
    {
        let _ = tx.send(());
    }
}

/// The operator confirmed closing in the UI; with `save`, only if saving works.
#[tauri::command]
pub async fn confirm_close(
    window: Window,
    backend: tauri::State<'_, Backend>,
    save: bool,
) -> Result<(), Error> {
    if save {
        backend.ask(Request::Save).await?;
    } else {
        backend.ask(Request::DiscardAutosave).await?;
    }
    // Waiting for the writes (and maybe a dialog) must not block the async runtime.
    tauri::async_runtime::spawn_blocking(move || destroy(&window))
        .await
        .map_err(|e| Error::from(e.to_string()))
}

/// Fallback while the UI hangs.
fn ask_natively(window: &Window, check: CloseCheck) {
    let (quit, save_quit, discard_quit, cancel) =
        (t!("close.quit"), t!("close.save_quit"), t!("close.discard_quit"), t!("close.cancel"));
    let mut lines = Vec::new();
    if check.show_mode {
        lines.push(t!("close.show_mode"));
    }
    if check.running > 0 {
        lines.push(t!("close.running", count = check.running));
    }
    if check.imports > 0 {
        lines.push(t!("close.imports", count = check.imports));
    }
    if check.dirty {
        lines.push(t!("close.dirty"));
    }
    lines.push(format!("\n{}", t!("close.quit_anyway")));

    let buttons = if check.dirty {
        MessageDialogButtons::YesNoCancelCustom(save_quit.clone(), discard_quit.clone(), cancel)
    } else {
        MessageDialogButtons::OkCancelCustom(quit.clone(), cancel)
    };
    let answer = window
        .dialog()
        .message(lines.join("\n"))
        .title("jQ Player")
        .kind(MessageDialogKind::Warning)
        .buttons(buttons)
        .parent(window)
        .blocking_show_with_result();

    // Depending on the platform, custom buttons come back by label or by role.
    match answer {
        MessageDialogResult::Custom(label) if label == quit => destroy(window),
        MessageDialogResult::Ok if !check.dirty => destroy(window),
        MessageDialogResult::Custom(label) if label == discard_quit => discard_and_close(window),
        MessageDialogResult::No if check.dirty => discard_and_close(window),
        MessageDialogResult::Custom(label) if label == save_quit => save_and_close(window),
        MessageDialogResult::Yes if check.dirty => save_and_close(window),
        _ => {}
    }
}

/// Closes even if the show thread does not answer; the autosave then stays.
fn discard_and_close(window: &Window) {
    let backend = window.state::<Backend>();
    let _ = ask_backend(&backend, Request::DiscardAutosave, CHECK_TIMEOUT);
    destroy(window);
}

fn save_and_close(window: &Window) {
    let backend = window.state::<Backend>();
    match ask_backend(&backend, Request::Save, SAVE_TIMEOUT) {
        Some(Ok(_)) => destroy(window),
        failed => {
            let error = match failed {
                Some(Err(e)) => e.to_string(),
                _ => t!("app.no_reply"),
            };
            window
                .dialog()
                .message(t!("close.save_failed_text", error = error))
                .title(t!("close.save_failed"))
                .kind(MessageDialogKind::Error)
                .parent(window)
                .blocking_show();
        }
    }
}

fn ask_backend<T>(
    backend: &Backend,
    make: impl FnOnce(mpsc::Sender<T>) -> Request,
    timeout: Duration,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    backend.send(make(tx)).ok()?;
    rx.recv_timeout(timeout).ok()
}

/// Waits until the settings and autosave writes still queued are on disk.
/// If the disk does not respond, the operator decides: wait longer (also
/// when the dialog is dismissed) or quit and possibly lose them.
fn flush_writes(window: &Window) {
    let Some(writer) = window.try_state::<Writer>() else { return };
    let quit_now = t!("close.quit_now");
    while !writer.flush(FLUSH_TIMEOUT) {
        let answer = window
            .dialog()
            .message(t!("close.writes_pending"))
            .title(t!("close.writes_pending_title"))
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(quit_now.clone(), t!("close.keep_waiting")))
            .parent(window)
            .blocking_show_with_result();
        match answer {
            MessageDialogResult::Custom(label) if label == quit_now => return,
            MessageDialogResult::Ok => return,
            _ => {}
        }
    }
}

fn destroy(window: &Window) {
    // Everything the operator changed is on disk before the program ends.
    flush_writes(window);
    // Saved here as well: on exit the destroyed window can no longer be asked.
    let _ = window.app_handle().save_window_state(crate::WINDOW_STATE);
    // Else the app would go on without its main window.
    crate::panel_windows::close_all(window.app_handle());
    let _ = window.destroy();
}
