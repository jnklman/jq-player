import { getCurrentWindow } from '@tauri-apps/api/window';

import { api, events, type WindowSupport } from './api';

declare global {
  interface Window {
    /** Set by the backend in a panel window with the system's title bar (`panel_windows.rs`). */
    __JQ_NATIVE_FRAME__?: boolean;
  }
}

/** What panel windows can be here; until the backend answered, what the backend told this window, if anything. */
export const windowSupport = $state<WindowSupport>({
  overlay: !window.__JQ_NATIVE_FRAME__,
  native_frame: !!window.__JQ_NATIVE_FRAME__,
});

export function loadWindowSupport() {
  void api
    .windowSupport()
    .then((support) => Object.assign(windowSupport, support))
    .catch(() => {});
}

/** Whether the window is maximized (`trackMaximized`), whether it and whether any window of the app has the keyboard
 * focus (`trackFocus`). */
export const windowState = $state({ maximized: false, focused: document.hasFocus(), appFocused: document.hasFocus() });

/** Keys (GO) work in every window of the app, so only a focus outside all of them counts as lost. */
export function hasKeys(): boolean {
  return windowState.focused || windowState.appFocused;
}

/**
 * Follows this window's focus and the app's (all its windows, from the backend). This window's own counts at once:
 * the app's comes a moment late, and the backend may not see every change.
 */
export function trackFocus() {
  window.addEventListener('focus', () => (windowState.focused = true));
  window.addEventListener('blur', () => (windowState.focused = false));
  void events.onAppFocus((focused) => (windowState.appFocused = focused));
  void api
    .appFocused()
    .then((focused) => (windowState.appFocused = focused || document.hasFocus()))
    .catch(() => {});
}

/**
 * Follows the window's maximized state for the window controls, and marks
 * the root element with `data-maximized` meanwhile, so the rounded window
 * corners (transparent window, see `styles.css`) are squared off when the
 * window fills the screen.
 */
export function trackMaximized() {
  const appWindow = getCurrentWindow();
  const check = async () => {
    windowState.maximized = await appWindow.isMaximized();
    document.documentElement.toggleAttribute('data-maximized', windowState.maximized);
  };
  check();
  // Maximizing (button, double-click, window manager) always resizes.
  appWindow.onResized(check);
}

/**
 * Marks the root element with `data-windows` on Windows. Windows 11 rounds
 * the borderless window and draws its edge itself (in the app's line colour,
 * see `window_border` in the backend), so `styles.css` leaves out its own.
 */
export function markPlatform() {
  document.documentElement.toggleAttribute('data-windows', navigator.userAgent.includes('Windows'));
}

// Like Tauri's own check for `data-tauri-drag-region` (its drag script):
// "deep" drags from anywhere inside, a bare attribute only from the element
// itself, "false" never; controls without the attribute never do.
const CLICKABLE = 'a, button, input, select, textarea, label, summary, [contenteditable]:not([contenteditable=false])';
const ROLES = new Set(['button', 'link', 'menuitem', 'tab', 'checkbox', 'radio', 'switch', 'option']);

function inDragRegion(path: EventTarget[]): boolean {
  for (const element of path) {
    if (!(element instanceof HTMLElement)) continue;
    const region = element.getAttribute('data-tauri-drag-region');
    const clickable =
      element.matches(CLICKABLE) ||
      (element.hasAttribute('tabindex') && element.getAttribute('tabindex') !== '-1') ||
      ROLES.has(element.getAttribute('role') ?? '');
    if (region === null) {
      if (clickable) return false;
      continue;
    }
    if (region === 'false') return false;
    if (region === 'deep') return true;
    return element === path[0];
  }
  return false;
}

/**
 * Lets the header move the window by touch and pen as well. Tauri's drag
 * regions only react to `mousedown`, which a touch fires only after lifting
 * the finger, never while dragging. Hardly on Windows: there the regions
 * without controls are the native title bar (`app-region` in `styles.css`),
 * which never passes the pointer to the page.
 */
export function dragByTouch() {
  const appWindow = getCurrentWindow();
  document.addEventListener('pointerdown', (e) => {
    if (e.pointerType === 'mouse' || !e.isPrimary || !inDragRegion(e.composedPath())) return;
    e.preventDefault();
    void appWindow.startDragging();
  });
}
