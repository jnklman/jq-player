//! Panel windows: panels of the main view taken out into windows of their
//! own, as a normal window (e.g. on a second screen) or as an overlay that
//! stays above other programs. The UI's layout (`LocalSettings.layout`)
//! says which ones exist and where they were; this module only makes the
//! open windows match it (`set_panel_windows`) and reports back where the
//! operator moved them. Each panel window is a webview of its own that
//! follows the `show`/`state` events like the main window.
//!
//! Also keeps the focus of all the app's windows together: keys (GO) work
//! in each of them, so the app only "lost the focus" when none has it.
//!
//! Wayland lets no program put its own window above the others; the
//! compositor's window menu (GNOME, KDE) can. There panel windows therefore
//! get the native title bar (`WindowSupport::native_frame`), and overlays
//! are not offered: an overlay in the layout opens as a normal window.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalRect, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent,
};

/// Label of the main window (`tauri.conf.json`).
pub const MAIN: &str = "main";
/// Labels of panel windows: this and the panel's id.
const PREFIX: &str = "panel-";

/// A panel window was moved or resized (to the main window).
const EVENT_MOVED: &str = "panel_window_moved";
/// The operator closed a panel window (to the main window).
const EVENT_CLOSED: &str = "panel_window_closed";
/// Whether any window of the app has the keyboard focus (to all windows).
const EVENT_APP_FOCUS: &str = "app_focus";

/// Size of a panel window without a stored one, logical pixels.
const DEFAULT_SIZE: (f64, f64) = (480.0, 360.0);
const MIN_SIZE: (f64, f64) = (200.0, 120.0);
/// The part of a window at its top left (its title row) that must lie on a
/// screen for a stored position to be used.
const GRIP: (f64, f64) = (120.0, 32.0);
/// Moving the focus from one of the app's windows to another loses it for a
/// moment; the app's focus is only reported once it has been settled this long.
const FOCUS_SETTLE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    /// A normal window, in the taskbar.
    Window,
    /// Always above other programs, not in the taskbar.
    Overlay,
}

/// A panel window as the layout keeps it (more fields there, e.g. the
/// opacity, are the UI's own). Positions and sizes are logical pixels.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PanelWindow {
    pub panel: String,
    pub kind: WindowKind,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct Moved {
    panel: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Serialize)]
struct Closed {
    panel: String,
}

/// A rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    fn from_physical(rect: &PhysicalRect<i32, u32>, scale: f64) -> Self {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        Rect {
            x: f64::from(rect.position.x) / scale,
            y: f64::from(rect.position.y) / scale,
            width: f64::from(rect.size.width) / scale,
            height: f64::from(rect.size.height) / scale,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Step {
    Create(PanelWindow),
    SetKind(String, WindowKind),
    Close(String),
}

/// The open panel windows by panel id, as created here. Locked while they
/// are being changed, so two calls never create the same window; window
/// events never lock it (creating a window waits for the main thread, which
/// runs them).
#[derive(Default)]
pub struct PanelWindows {
    open: Mutex<HashMap<String, WindowKind>>,
    focus: Mutex<Focus>,
    /// Set once at start, see [`detect_platform`].
    native_frame: AtomicBool,
}

impl PanelWindows {
    fn native_frame(&self) -> bool {
        self.native_frame.load(Ordering::Relaxed)
    }
}

/// What panel windows can be on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WindowSupport {
    /// Overlays (always above other programs) can be offered.
    overlay: bool,
    /// Panel windows have the system's title bar instead of the app's own
    /// (Wayland: its window menu has "Always on top").
    native_frame: bool,
}

impl WindowSupport {
    fn new(native_frame: bool) -> Self {
        WindowSupport { overlay: !native_frame, native_frame }
    }
}

/// Finds out whether the app runs on Wayland. Must run on the main thread
/// (GTK), i.e. in `setup`, before any panel window opens.
pub fn detect_platform(app: &AppHandle) {
    app.state::<PanelWindows>().native_frame.store(on_wayland(), Ordering::Relaxed);
}

/// GDK picked its Wayland backend (it falls back to X11 through XWayland
/// with `GDK_BACKEND=x11`, where the app's own overlays work).
#[cfg(target_os = "linux")]
fn on_wayland() -> bool {
    use gtk::glib::prelude::ObjectExt;
    gtk::gdk::Display::default().is_some_and(|display| display.type_().name() == "GdkWaylandDisplay")
}

#[cfg(not(target_os = "linux"))]
fn on_wayland() -> bool {
    false
}

#[tauri::command]
pub fn window_support(state: tauri::State<'_, PanelWindows>) -> WindowSupport {
    WindowSupport::new(state.native_frame())
}

/// Without overlays, an overlay in the layout opens as a normal window.
fn supported(mut windows: Vec<PanelWindow>, support: WindowSupport) -> Vec<PanelWindow> {
    if !support.overlay {
        for window in &mut windows {
            window.kind = WindowKind::Window;
        }
    }
    windows
}

/// Panel ids end up in window labels and in a script; the UI's are short words.
fn valid_panel(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn label(panel: &str) -> String {
    format!("{PREFIX}{panel}")
}

/// The panel of a panel window's label.
fn panel_of(label: &str) -> Option<&str> {
    label.strip_prefix(PREFIX)
}

/// What turns the `open` windows into the `wanted` ones. A changed kind
/// changes the window in place; position and size of an open window are the
/// operator's, so a list that only differs in those changes nothing.
/// Invalid and repeated panels are left out.
fn plan(open: &HashMap<String, WindowKind>, wanted: &[PanelWindow]) -> Vec<Step> {
    let mut seen = HashSet::new();
    let mut steps = Vec::new();
    for window in wanted {
        if !valid_panel(&window.panel) || !seen.insert(window.panel.as_str()) {
            continue;
        }
        match open.get(&window.panel) {
            None => steps.push(Step::Create(window.clone())),
            Some(kind) if *kind != window.kind => {
                steps.push(Step::SetKind(window.panel.clone(), window.kind))
            }
            Some(_) => {}
        }
    }
    let mut closing: Vec<_> = open.keys().filter(|panel| !seen.contains(panel.as_str())).cloned().collect();
    closing.sort();
    steps.extend(closing.into_iter().map(Step::Close));
    steps
}

/// A stored window rectangle if its title row still lies on one of the
/// screens (a second screen may be gone); `None` otherwise. Without any
/// screen known, the rectangle is taken as it is.
fn placed(rect: Rect, screens: &[Rect]) -> Option<Rect> {
    if screens.is_empty() {
        return Some(rect);
    }
    let x = rect.x + rect.width.min(GRIP.0) / 2.0;
    let y = rect.y + rect.height.min(GRIP.1) / 2.0;
    screens.iter().any(|screen| screen.contains(x, y)).then_some(rect)
}

/// A window of `width` × `height` in the middle of `screen`.
fn centered(screen: Rect, width: f64, height: f64) -> Rect {
    Rect {
        x: screen.x + ((screen.width - width) / 2.0).max(0.0),
        y: screen.y + ((screen.height - height) / 2.0).max(0.0),
        width,
        height,
    }
}

/// Makes the panel windows match `windows` (the current mode's list in the
/// layout): opens missing ones where they were, changes the kind of open
/// ones and closes the others. Async: creating a window from a synchronous
/// command deadlocks on Windows.
#[tauri::command]
pub async fn set_panel_windows(app: AppHandle, windows: Vec<PanelWindow>) -> Result<(), String> {
    let state = app.state::<PanelWindows>();
    let support = WindowSupport::new(state.native_frame());
    let windows = supported(windows, support);
    let mut open = state.open.lock().map_err(|_| jq_i18n::t!("panels.locked"))?;
    // Closed by the operator meanwhile.
    open.retain(|panel, _| app.get_webview_window(&label(panel)).is_some());
    let mut failed = Vec::new();
    for step in plan(&open, &windows) {
        match step {
            Step::Create(window) => match create(&app, &window, support) {
                Ok(()) => {
                    open.insert(window.panel, window.kind);
                }
                Err(e) => failed.push(format!("{}: {e}", window.panel)),
            },
            Step::SetKind(panel, kind) => {
                if let Some(window) = app.get_webview_window(&label(&panel)) {
                    let overlay = kind == WindowKind::Overlay;
                    let _ = window.set_always_on_top(overlay);
                    let _ = window.set_skip_taskbar(overlay);
                }
                open.insert(panel, kind);
            }
            Step::Close(panel) => {
                if let Some(window) = app.get_webview_window(&label(&panel)) {
                    let _ = window.destroy();
                }
                open.remove(&panel);
            }
        }
    }
    if failed.is_empty() { Ok(()) } else { Err(jq_i18n::t!("panels.not_opened", panels = failed.join(", "))) }
}

fn create(app: &AppHandle, spec: &PanelWindow, support: WindowSupport) -> tauri::Result<()> {
    let width = spec.width.unwrap_or(DEFAULT_SIZE.0).max(MIN_SIZE.0);
    let height = spec.height.unwrap_or(DEFAULT_SIZE.1).max(MIN_SIZE.1);
    let screens: Vec<Rect> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| Rect::from_physical(monitor.work_area(), monitor.scale_factor()))
        .collect();
    let stored = match (spec.x, spec.y) {
        (Some(x), Some(y)) => placed(Rect { x, y, width, height }, &screens),
        _ => None,
    };
    // Else in the middle of the main window's screen. Wayland places windows itself either way.
    let position = stored.or_else(|| {
        let monitor = app.get_webview_window(MAIN)?.current_monitor().ok()??;
        Some(centered(Rect::from_physical(monitor.work_area(), monitor.scale_factor()), width, height))
    });

    let overlay = spec.kind == WindowKind::Overlay;
    let native = support.native_frame;
    // The panel id is checked (`valid_panel`), so it is safe inside the string.
    let script = format!("window.__JQ_PANEL__ = '{}'; window.__JQ_NATIVE_FRAME__ = {native};", spec.panel);
    let mut builder = WebviewWindowBuilder::new(app, label(&spec.panel), WebviewUrl::default())
        .title("jQ Player")
        .inner_size(width, height)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        // Else drawn by the UI like the main window (rounded, own buttons).
        .decorations(native)
        .transparent(!native)
        .disable_drag_drop_handler()
        .always_on_top(overlay)
        .skip_taskbar(overlay)
        // Shown once placed, like the main window.
        .visible(false)
        .initialization_script(script);
    if let Some(at) = position {
        builder = builder.position(at.x, at.y);
    }
    let window = builder.build()?;
    #[cfg(windows)]
    crate::window_border(&window, crate::BORDER_LINE);
    // Not counted as open on failure: a hidden window left behind would
    // block its label, so the panel could never be opened again.
    if let Err(e) = window.show() {
        let _ = window.destroy();
        return Err(e);
    }
    Ok(())
}

/// Closes all panel windows without reporting them closed, so the layout
/// keeps them for the next start (the main window closes).
pub fn close_all(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if panel_of(&label).is_some() {
            let _ = window.destroy();
        }
    }
}

/// Whether any window of the app has the keyboard focus; for a window that just opened.
#[tauri::command]
pub fn app_focused(state: tauri::State<'_, PanelWindows>) -> bool {
    state.focus.lock().is_ok_and(|focus| focus.any())
}

/// Brings the main window to the front, e.g. for a dialog asked for in a panel window.
#[tauri::command]
pub fn focus_main(app: AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Window events of all windows: the focus of the app, and moves and closes of panel windows.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::Focused(focused) => focus_changed(window, Some(*focused)),
        WindowEvent::Destroyed => focus_changed(window, None),
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => report_moved(window),
        WindowEvent::CloseRequested { .. } => {
            if let Some(panel) = panel_of(window.label()) {
                let _ = window.emit_to(MAIN, EVENT_CLOSED, Closed { panel: panel.into() });
            }
        }
        _ => {}
    }
}

fn report_moved(window: &Window) {
    let Some(panel) = panel_of(window.label()) else { return };
    // Minimized windows report a position off-screen (Windows).
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let (Ok(scale), Ok(position), Ok(size)) =
        (window.scale_factor(), window.outer_position(), window.inner_size())
    else {
        return;
    };
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    let moved =
        Moved { panel: panel.into(), x: position.x, y: position.y, width: size.width, height: size.height };
    let _ = window.emit_to(MAIN, EVENT_MOVED, moved);
}

/// `None`: the window is gone.
fn focus_changed(window: &Window, focused: Option<bool>) {
    let state = window.state::<PanelWindows>();
    let Ok(mut focus) = state.focus.lock() else { return };
    let generation = focus.set(window.label(), focused.unwrap_or(false));
    drop(focus);
    let app = window.app_handle().clone();
    let _ = std::thread::Builder::new().name("jq-focus".into()).spawn(move || {
        std::thread::sleep(FOCUS_SETTLE);
        let state = app.state::<PanelWindows>();
        let settled = state.focus.lock().ok().and_then(|mut focus| focus.settle(generation));
        if let Some(focused) = settled {
            let _ = app.emit(EVENT_APP_FOCUS, focused);
        }
    });
}

/// The windows that have the focus, and what was last reported.
#[derive(Debug, Default)]
struct Focus {
    focused: HashSet<String>,
    sent: Option<bool>,
    /// Counts changes; only the last one's settling reports.
    generation: u64,
}

impl Focus {
    fn set(&mut self, label: &str, focused: bool) -> u64 {
        if focused {
            self.focused.insert(label.to_string());
        } else {
            self.focused.remove(label);
        }
        self.generation += 1;
        self.generation
    }

    fn any(&self) -> bool {
        !self.focused.is_empty()
    }

    /// The app's focus to report once change `generation` has settled:
    /// nothing if a later change came or the value did not change.
    fn settle(&mut self, generation: u64) -> Option<bool> {
        if generation != self.generation {
            return None;
        }
        let now = self.any();
        if self.sent == Some(now) {
            return None;
        }
        self.sent = Some(now);
        Some(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(panel: &str, kind: WindowKind) -> PanelWindow {
        PanelWindow { panel: panel.into(), kind, x: None, y: None, width: None, height: None }
    }

    fn open(list: &[(&str, WindowKind)]) -> HashMap<String, WindowKind> {
        list.iter().map(|(panel, kind)| (panel.to_string(), *kind)).collect()
    }

    use WindowKind::{Overlay, Window as Normal};

    #[test]
    fn plan_creates_changes_and_closes() {
        let wanted = [window("active", Overlay), window("cues", Normal), window("transport", Normal)];
        let steps = plan(&open(&[("cues", Normal), ("transport", Overlay), ("inspector", Normal)]), &wanted);
        assert_eq!(
            steps,
            vec![
                Step::Create(window("active", Overlay)),
                Step::SetKind("transport".into(), Normal),
                Step::Close("inspector".into()),
            ]
        );
    }

    #[test]
    fn plan_ignores_moves_of_open_windows() {
        let mut moved = window("cues", Normal);
        moved.x = Some(100.0);
        moved.width = Some(900.0);
        assert!(plan(&open(&[("cues", Normal)]), &[moved]).is_empty());
    }

    #[test]
    fn plan_twice_is_nothing_the_second_time() {
        let wanted = [window("active", Overlay)];
        let mut current = open(&[]);
        for step in plan(&current, &wanted) {
            if let Step::Create(w) = step {
                current.insert(w.panel, w.kind);
            }
        }
        assert!(plan(&current, &wanted).is_empty());
    }

    #[test]
    fn plan_leaves_out_invalid_and_repeated_panels() {
        let wanted = [
            window("cues", Normal),
            window("cues", Overlay),
            window("x'; alert(1)", Normal),
            window("", Normal),
        ];
        assert_eq!(plan(&open(&[]), &wanted), vec![Step::Create(window("cues", Normal))]);
    }

    #[test]
    fn without_overlays_every_window_is_a_normal_one() {
        let wanted = vec![window("active", Overlay), window("cues", Normal)];
        let native = supported(wanted.clone(), WindowSupport::new(true));
        assert!(native.iter().all(|w| w.kind == Normal));
        assert_eq!(supported(wanted.clone(), WindowSupport::new(false)), wanted);
        // An overlay opened before (X11 session) is switched to a normal window.
        assert_eq!(
            plan(&open(&[("active", Overlay)]), &native[..1]),
            vec![Step::SetKind("active".into(), Normal)]
        );
    }

    #[test]
    fn plan_closes_all_for_an_empty_list() {
        assert_eq!(
            plan(&open(&[("b", Normal), ("a", Overlay)]), &[]),
            vec![Step::Close("a".into()), Step::Close("b".into())]
        );
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn placed_keeps_windows_whose_title_is_on_a_screen() {
        let screens = [rect(0.0, 0.0, 1920.0, 1040.0), rect(1920.0, 0.0, 1280.0, 1024.0)];
        let whole = rect(100.0, 100.0, 400.0, 300.0);
        assert_eq!(placed(whole, &screens), Some(whole));
        // Partly off the right edge of the second screen.
        let half = rect(3000.0, 500.0, 400.0, 300.0);
        assert_eq!(placed(half, &screens), Some(half));
        // On the second screen.
        let second = rect(2000.0, 10.0, 400.0, 300.0);
        assert_eq!(placed(second, &screens), Some(second));
    }

    #[test]
    fn placed_drops_windows_off_every_screen() {
        let screens = [rect(0.0, 0.0, 1920.0, 1040.0)];
        // The second screen was unplugged.
        assert_eq!(placed(rect(2000.0, 100.0, 400.0, 300.0), &screens), None);
        // Title above the screen, only the lower part would show.
        assert_eq!(placed(rect(100.0, -200.0, 400.0, 300.0), &screens), None);
        assert_eq!(placed(rect(-500.0, 100.0, 400.0, 300.0), &screens), None);
    }

    #[test]
    fn placed_takes_any_rect_without_screens() {
        let r = rect(-5000.0, 0.0, 10.0, 10.0);
        assert_eq!(placed(r, &[]), Some(r));
    }

    #[test]
    fn screens_are_logical_pixels() {
        let area = PhysicalRect {
            position: tauri::PhysicalPosition::new(2880, 0),
            size: tauri::PhysicalSize::new(3840, 2160),
        };
        assert_eq!(Rect::from_physical(&area, 1.5), rect(1920.0, 0.0, 2560.0, 1440.0));
        // At 1.5 the stored logical position lies on that screen.
        assert!(placed(rect(2000.0, 100.0, 400.0, 300.0), &[Rect::from_physical(&area, 1.5)]).is_some());
    }

    #[test]
    fn centered_fits_the_screen() {
        assert_eq!(
            centered(rect(1920.0, 0.0, 1280.0, 1000.0), 480.0, 360.0),
            rect(2320.0, 320.0, 480.0, 360.0)
        );
        assert_eq!(centered(rect(0.0, 0.0, 300.0, 200.0), 480.0, 360.0), rect(0.0, 0.0, 480.0, 360.0));
    }

    #[test]
    fn focus_moving_between_app_windows_reports_nothing() {
        let mut focus = Focus::default();
        let first = focus.set(MAIN, true);
        assert_eq!(focus.settle(first), Some(true));
        // Click into a panel window: the main window loses the focus, then the panel gets it.
        let blur = focus.set(MAIN, false);
        let gain = focus.set("panel-active", true);
        assert_eq!(focus.settle(blur), None, "superseded");
        assert_eq!(focus.settle(gain), None, "still focused");
    }

    #[test]
    fn focus_leaving_the_app_is_reported_once() {
        let mut focus = Focus::default();
        let gain = focus.set("panel-cues", true);
        assert_eq!(focus.settle(gain), Some(true));
        let blur = focus.set("panel-cues", false);
        assert_eq!(focus.settle(blur), Some(false));
        assert_eq!(focus.settle(blur), None);
        // A closed window counts as unfocused.
        let gone = focus.set("panel-x", false);
        assert_eq!(focus.settle(gone), None);
        assert!(!focus.any());
    }
}
