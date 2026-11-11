//! Asks before the window closes, so a stray Alt+F4 or click on × cannot end
//! a running performance. The UI shows the question in the app's own style
//! (`close_requested` event); if it does not confirm receipt in time because
//! the WebView hangs, a native dialog asks instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use tauri::{Emitter, Manager, Window, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};
use tauri_plugin_window_state::AppHandleExt;

use jq_i18n::t;

use crate::Backend;
use crate::backend::{CloseCheck, Request};

/// A hanging show thread counts as "something may be running".
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
const SAVE_TIMEOUT: Duration = Duration::from_secs(10);
/// Time the UI has to confirm it shows the question.
const ACK_TIMEOUT: Duration = Duration::from_secs(1);

pub const EVENT_CLOSE_REQUESTED: &str = "close_requested";

/// Only one question at a time, however often × is clicked.
static ASKING: AtomicBool = AtomicBool::new(false);
/// Waits for `close_ack` from the UI.
static PENDING_ACK: Mutex<Option<mpsc::Sender<()>>> = Mutex::new(None);

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
    let backend = window.state::<Backend>();
    let check = match ask_backend(&backend, Request::CloseCheck, CHECK_TIMEOUT) {
        Some(None) => return destroy(window),
        Some(Some(check)) => check,
        None => CloseCheck { show_mode: true, dirty: false, running: 1, imports: 0 },
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
) -> Result<(), String> {
    if save {
        backend.ask(Request::Save).await?;
    } else {
        backend.ask(Request::DiscardAutosave).await?;
    }
    destroy(&window);
    Ok(())
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
                Some(Err(e)) => e,
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

fn destroy(window: &Window) {
    // Saved here as well: on exit the destroyed window can no longer be asked.
    let _ = window.app_handle().save_window_state(crate::WINDOW_STATE);
    // Else the app would go on without its main window.
    crate::panel_windows::close_all(window.app_handle());
    let _ = window.destroy();
}
