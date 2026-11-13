//! Show thread: owns the session, takes requests from the UI and pushes
//! state to it. The UI never holds show state of its own; if the WebView
//! hangs, playback and fades go on.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use jq_core::{Action, LocalSettings, Notice, PeakCache, SelectionMode, Startup, State};
use jq_show::{Cue, CueId, LevelLimits, ShowFile, StopwatchAction};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::language;
use crate::session::{EditOp, ImportState, LearnTarget, Session};

/// UI events; see `app/src/lib/api.ts` for the TypeScript side.
pub const EVENT_STATE: &str = "state";
pub const EVENT_SHOW: &str = "show";
pub const EVENT_NOTICE: &str = "notice";

/// 20 times a second: smooth enough for times (to a tenth) and bars, and
/// every changed state costs the webview a whole rendered frame.
const STATE_INTERVAL: Duration = Duration::from_millis(50);
/// Longest wait for a request between ticks while cues run or wait: engine
/// events and timers (follow-ons, pre-waits) are handled this precisely.
const POLL: Duration = Duration::from_millis(2);
/// The same while nothing runs but the state still changes by itself (a
/// stopwatch runs, files are being armed).
const LIVELY_POLL: Duration = STATE_INTERVAL;
/// The same while nothing runs at all. A request (GO) still wakes the
/// thread at once; only background results (lengths read, copies, changed
/// files) wait up to this long.
const IDLE_POLL: Duration = Duration::from_millis(250);
/// How often the background work on media files is collected
/// ([`Session::media_tick`]) and the audio devices are looked at.
const MEDIA_INTERVAL: Duration = IDLE_POLL;
/// How often the autosave looks for edits; it waits for a quiet second
/// after the last one anyway.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(1);
/// Requests handled before the next tick at most.
const MAX_BATCH: usize = 256;
/// Shortest time between two "internal error" notices: a bug hit on every
/// tick would otherwise send hundreds a second.
const INTERNAL_ERROR_INTERVAL: Duration = Duration::from_secs(5);
/// Machine settings, in the app's config folder.
const SETTINGS_FILE: &str = "settings.json";

pub type Reply<T = serde_json::Value> = Sender<Result<T, String>>;

pub enum Request {
    Open(PathBuf, Reply),
    New {
        dir: PathBuf,
        name: String,
        create_folder: bool,
        reply: Reply,
    },
    Save(Reply),
    View(Reply),
    Action(Action),
    /// Starts, stops or resets a stopwatch of the show (no reply).
    Stopwatch(String, StopwatchAction),
    Edit(Box<EditOp>, Reply),
    /// A cue still being edited; running cues follow it (no reply).
    LiveCue(Box<Cue>),
    /// Preview of a cue's file from a position to an end (seconds, default: the file's end; no reply).
    PreviewAt(CueId, f64, Option<f64>),
    SetKeepAwake(bool, Reply),
    SetShowMode(bool, Reply),
    Rename(String, Reply),
    /// The media folder and the stored paths to copy for "save as"
    /// (see [`Session::save_as_plan`]).
    SaveAsPlan(Sender<Result<(PathBuf, Vec<String>), String>>),
    /// Goes on in a new show folder (see [`Session::save_as`]); `warnings`
    /// from preparing it are shown once it worked.
    SaveAs {
        path: PathBuf,
        media_root: String,
        /// The files copied into the new media folder (`None`: linked).
        copied: Option<Vec<String>>,
        warnings: Vec<String>,
        reply: Reply,
    },
    /// Restore (`true`) or discard the autosave found on opening.
    Recover(bool, Reply),
    /// Leaving without saving: delete the autosave.
    DiscardAutosave(Reply),
    /// A message from the MIDI input.
    Midi(Vec<u8>),
    /// A packet from the OSC listener.
    Osc(Vec<u8>),
    /// The show opened at startup has run for a while, so it did not crash
    /// the app.
    StartupSurvived,
    MidiLearn(Option<LearnTarget>, Reply),
    SetSettings(Box<LocalSettings>, Reply),
    SetAudioDir(Option<PathBuf>, Reply),
    SetMediaDir(PathBuf, Reply),
    SetLayout(Option<serde_json::Value>, Reply),
    SetSelectionMode(SelectionMode, Reply),
    RescanMedia(Reply),
    MediaDir(Reply<PathBuf>),
    /// Which cues new level limits would change; changes nothing.
    CheckLevelLimits(LevelLimits, Reply),
    /// What closing the window would interrupt; `None` without a show.
    CloseCheck(Sender<Option<CloseCheck>>),
    /// The media folder and the stored paths the show may still need
    /// (see [`Session::used_media`]), for tidying up the media folder.
    UsedMedia(Sender<Result<(PathBuf, Vec<String>), String>>),
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CloseCheck {
    pub show_mode: bool,
    pub dirty: bool,
    /// Running cues plus cues waiting for their pre-wait.
    pub running: usize,
    /// Files still being copied into the media folder; closing stops them.
    pub imports: usize,
}

#[derive(Clone, Serialize)]
struct StateEvent<'a> {
    #[serde(flatten)]
    state: &'a State,
    /// GO on the playhead cue starts without disk access.
    playhead_armed: bool,
    /// Files being copied into the media folder.
    imports: Vec<ImportState<'a>>,
    /// A search for missing files (relink) is under way.
    searching: bool,
}

pub fn spawn(app: AppHandle) -> Sender<Request> {
    let (tx, rx) = mpsc::channel();
    let midi_tx = tx.clone();
    std::thread::Builder::new()
        .name("jq-show".into())
        .spawn(move || run(app, rx, midi_tx))
        .expect("spawn show thread");
    tx
}

fn run(app: AppHandle, rx: Receiver<Request>, midi_tx: Sender<Request>) {
    let mut session: Option<Session> = None;
    let mut last_state = Instant::now();
    // The state last sent; an unchanged one (a quiet show) is not sent
    // again, so the UI does not render 20 times a second for nothing. A
    // reloaded UI gets the state with `Request::View`.
    let mut sent_state = serde_json::Value::Null;
    // A tick looked at the show but its state was not due yet. The next
    // state must not wait for an idle poll: the last change before the show
    // falls quiet (a fade reaching silence, the cue leaving the list) would
    // stand for up to `IDLE_POLL`.
    let mut state_pending = false;
    let (mut media, mut autosave) = (Every::new(MEDIA_INTERVAL), Every::new(AUTOSAVE_INTERVAL));
    let mut internal_errors = Every::new(INTERNAL_ERROR_INTERVAL);
    loop {
        let wait = match session.as_ref() {
            Some(s) if !s.runner.is_quiet() => POLL,
            Some(s) if s.runner.is_lively() => LIVELY_POLL,
            _ => IDLE_POLL,
        };
        let wait = match state_pending {
            true => wait.min(STATE_INTERVAL.saturating_sub(last_state.elapsed())),
            false => wait,
        };
        let first = match rx.recv_timeout(wait) {
            Ok(r) => Some(r),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Everything queued meanwhile, e.g. a burst of MIDI from a fader, so
        // a GO behind it does not wait for a tick per message.
        let requests: Vec<Request> = first.into_iter().chain(rx.try_iter().take(MAX_BATCH)).collect();
        let mut notices = Vec::new();
        // A bug in one request must not take the running show down.
        let mut failed = false;
        for r in requests {
            let handled =
                catch_unwind(AssertUnwindSafe(|| handle(&app, &mut session, r, &midi_tx, &mut notices)));
            failed |= handled.is_err();
        }
        let ticked = catch_unwind(AssertUnwindSafe(|| {
            let mut view_changed = false;
            state_pending = false;
            if let Some(s) = session.as_mut() {
                let now = Instant::now();
                s.runner.tick(now);
                if autosave.due(now) {
                    s.autosave_tick(now, &mut notices);
                }
                s.update_busy();
                if media.due(now) {
                    view_changed = s.media_tick(&mut notices);
                    view_changed |= s.audio_tick(&mut notices);
                }
                notices.extend(s.runner.take_notices());
                state_pending = now.duration_since(last_state) < STATE_INTERVAL;
                if !state_pending {
                    last_state = now;
                    let state = state_event(s, now);
                    if state != sent_state {
                        let _ = app.emit(EVENT_STATE, &state);
                        sent_state = state;
                    }
                }
            }
            if view_changed {
                let _ = publish(&app, &session);
            }
        }));
        failed |= ticked.is_err();
        if failed && internal_errors.due(Instant::now()) {
            notices.push(Notice::Error(jq_i18n::t!("backend.internal_error")));
        }
        for n in notices {
            let _ = app.emit_to(crate::panel_windows::MAIN, EVENT_NOTICE, n);
        }
    }
}

/// Lets a periodic job of the show thread run at most every `interval`.
struct Every {
    interval: Duration,
    last: Option<Instant>,
}

impl Every {
    fn new(interval: Duration) -> Self {
        Self { interval, last: None }
    }

    /// True (and restarts the interval) once it has passed.
    fn due(&mut self, now: Instant) -> bool {
        let due = self.last.is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if due {
            self.last = Some(now);
        }
        due
    }
}

fn handle(
    app: &AppHandle,
    session: &mut Option<Session>,
    request: Request,
    midi_tx: &Sender<Request>,
    notices: &mut Vec<Notice>,
) {
    let no_show = || Err::<serde_json::Value, String>(jq_i18n::t!("app.no_show"));
    match request {
        Request::Open(path, reply) => {
            // A show opens ready to run; a new one ready to fill.
            let result = ShowFile::load(&path).map_err(|e| e.to_string()).map(|file| {
                replace(app, session, file, true, midi_tx, notices);
            });
            let _ = reply.send(result.and_then(|()| publish(app, session)));
        }
        Request::New { dir, name, create_folder, reply } => {
            let result =
                ShowFile::create(&dir, &name, create_folder).map_err(|e| e.to_string()).map(|file| {
                    replace(app, session, file, false, midi_tx, notices);
                });
            let _ = reply.send(result.and_then(|()| publish(app, session)));
        }
        Request::Save(reply) => {
            let result = match session.as_mut() {
                Some(s) => s.save().and_then(|()| publish(app, session)),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::View(reply) => {
            // The UI asks on start (also after a reload), the state comes with it.
            if let Some(s) = session {
                let _ = app.emit(EVENT_STATE, state_event(s, Instant::now()));
            }
            let view = session.as_ref().map(|s| s.view());
            let _ = reply.send(serde_json::to_value(view).map_err(|e| e.to_string()));
        }
        Request::Action(action) => {
            if let Some(s) = session.as_mut() {
                s.runner.handle(action, Instant::now());
            }
        }
        Request::Stopwatch(id, action) => {
            if let Some(s) = session.as_mut() {
                s.runner.stopwatch(&id, action, Instant::now());
            }
        }
        Request::Edit(op, reply) => {
            let result = match session.as_mut() {
                Some(s) => s.edit(*op, notices).and_then(|()| publish(app, session)),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::LiveCue(cue) => {
            if let Some(s) = session.as_mut() {
                s.live_cue(*cue);
            }
        }
        Request::PreviewAt(cue, at, until) => {
            if let Some(s) = session.as_mut() {
                s.runner.preview_at(cue, at, until);
            }
        }
        Request::SetKeepAwake(enabled, reply) => {
            let result = match session.as_mut() {
                Some(s) => s.set_keep_awake(enabled, notices).and_then(|()| publish(app, session)),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::Midi(message) => {
            if let Some(s) = session.as_mut()
                && s.handle_midi(&message, notices)
            {
                let _ = publish(app, session);
            }
        }
        Request::Osc(packet) => {
            if let Some(s) = session.as_mut() {
                s.handle_osc(&packet);
            }
        }
        Request::MidiLearn(action, reply) => {
            let result = match session.as_mut() {
                Some(s) => s.midi_learn(action).and_then(|()| publish(app, session)),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::Recover(apply, reply) => {
            let result = match session.as_mut() {
                Some(s) => s.recover(apply, notices).and_then(|()| publish(app, session)),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::DiscardAutosave(reply) => {
            let result = match session.as_ref() {
                Some(s) => s.discard_autosave().map(|()| serde_json::Value::Null),
                None => Ok(serde_json::Value::Null),
            };
            let _ = reply.send(result);
        }
        Request::StartupSurvived => {
            if let Err(e) = Startup::set_pending(&settings_path(app), false) {
                notices.push(Notice::Warning(jq_i18n::t!("session.settings_not_saved", error = e)));
            }
        }
        Request::Rename(name, reply) => {
            let result = match session.as_mut() {
                Some(s) => {
                    s.rename(&name).map(|()| s.remember_path(notices)).and_then(|()| publish(app, session))
                }
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::SaveAsPlan(reply) => {
            let _ = reply.send(
                session.as_ref().ok_or_else(|| jq_i18n::t!("app.no_show")).and_then(Session::save_as_plan),
            );
        }
        Request::SaveAs { path, media_root, copied, warnings, reply } => {
            let result = match session.as_mut() {
                Some(s) => s
                    .save_as(path, media_root, copied.as_deref())
                    .map(|()| s.remember_path(notices))
                    .and_then(|()| publish(app, session)),
                None => no_show(),
            };
            if result.is_ok() {
                notices.extend(warnings.into_iter().map(Notice::Warning));
            }
            let _ = reply.send(result);
        }
        Request::SetShowMode(enabled, reply) => {
            let result = match session.as_mut() {
                Some(s) => s.set_show_mode(enabled, notices).and_then(|()| publish(app, session)),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::CloseCheck(reply) => {
            let _ = reply.send(session.as_ref().map(|s| s.close_check(Instant::now())));
        }
        Request::UsedMedia(reply) => {
            let _ = reply.send(
                session.as_ref().ok_or_else(|| jq_i18n::t!("app.no_show")).and_then(Session::used_media),
            );
        }
        Request::SetSettings(settings, reply) => {
            // Before the settings are applied: what they cause (errors, notices) is said in the new language.
            let before = language::current();
            let after = language::apply(&settings.language);
            let changed = before.setting != after.setting || before.resolved != after.resolved;
            let result = match session.as_mut() {
                Some(s) => s.set_settings(*settings, notices).map(|()| {
                    if changed {
                        s.language_changed();
                    }
                }),
                None => no_show().map(|_| ()),
            }
            .and_then(|()| publish(app, session));
            if result.is_err() {
                // Not applied: the old language stays.
                language::apply(&before.setting);
            } else if changed {
                let _ = app.emit(language::EVENT_LANGUAGE_CHANGED, after);
            }
            let _ = reply.send(result);
        }
        Request::SetAudioDir(dir, reply) => {
            let result = match session.as_mut() {
                Some(s) => {
                    s.set_audio_dir(dir, notices);
                    publish(app, session)
                }
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::SetMediaDir(dir, reply) => {
            let result = match session.as_mut() {
                Some(s) => s.set_media_dir(dir).and_then(|()| publish(app, session)),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::SetLayout(layout, reply) => {
            let result = match session.as_mut() {
                Some(s) => {
                    s.set_layout(layout, notices);
                    publish(app, session)
                }
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::CheckLevelLimits(limits, reply) => {
            let result = match session.as_ref() {
                Some(s) => s
                    .check_level_limits(limits)
                    .and_then(|changes| serde_json::to_value(changes).map_err(|e| e.to_string())),
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::SetSelectionMode(mode, reply) => {
            let result = match session.as_mut() {
                Some(s) => {
                    s.set_selection_mode(mode, notices);
                    publish(app, session)
                }
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::RescanMedia(reply) => {
            let result = match session.as_mut() {
                Some(s) => {
                    s.rescan_media();
                    publish(app, session)
                }
                None => no_show(),
            };
            let _ = reply.send(result);
        }
        Request::MediaDir(reply) => {
            let result =
                session.as_ref().ok_or_else(|| jq_i18n::t!("app.no_show")).map(|s| s.view().media_dir);
            let _ = reply.send(result);
        }
    }
}

/// Opens `file` in place of the current show. The UI asked before, so the
/// old show's unsaved changes (and its autosave) are given up.
fn replace(
    app: &AppHandle,
    session: &mut Option<Session>,
    file: ShowFile,
    show_mode: bool,
    midi_tx: &Sender<Request>,
    notices: &mut Vec<Notice>,
) {
    if let Some(old) = session.take()
        && let Err(e) = old.discard_autosave()
    {
        notices.push(Notice::Warning(jq_i18n::t!("backend.autosave_not_deleted", error = e)));
    }
    let peaks = app.state::<Arc<PeakCache>>().inner().clone();
    let mut opened = Session::open(file, show_mode, settings_path(app), peaks, midi_tx.clone(), notices);
    opened.remember_path(notices);
    *session = Some(opened);
}

/// The machine's settings file in the app's config folder (Linux:
/// `~/.config/de.jqplayer.app`, Windows: `%APPDATA%\de.jqplayer.app`).
pub fn settings_path(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."));
    dir.join(SETTINGS_FILE)
}

/// Sends the runtime state (playhead, running cues) as event.
fn state_event(s: &Session, now: Instant) -> serde_json::Value {
    let state = s.runner.state(now);
    let playhead_armed = state.playhead.is_none_or(|id| s.runner.is_armed(id));
    let event = StateEvent { state: &state, playhead_armed, imports: s.imports(), searching: s.searching() };
    serde_json::to_value(event).unwrap_or_default()
}

/// Sends the current show view as event and returns it. The state goes
/// first: after deleting the playhead cue the old state would point the
/// new view at a cue it no longer has until the next tick (GO flashes
/// disabled), while the new playhead is almost always in the old view too.
fn publish(app: &AppHandle, session: &Option<Session>) -> Result<serde_json::Value, String> {
    if let Some(s) = session {
        let _ = app.emit(EVENT_STATE, state_event(s, Instant::now()));
    }
    let view = serde_json::to_value(session.as_ref().map(|s| s.view())).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_SHOW, &view);
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_are_reported_at_most_every_interval() {
        let mut every = Every::new(INTERNAL_ERROR_INTERVAL);
        let start = Instant::now();
        assert!(every.due(start), "the first one at once");
        assert!(!every.due(start + POLL), "not again on the next tick");
        assert!(!every.due(start + INTERNAL_ERROR_INTERVAL / 2));
        assert!(every.due(start + INTERNAL_ERROR_INTERVAL));
    }
}
