/**
 * Turns off the webview's own browser behaviour that could disrupt a show:
 * reloading (which would drop the UI mid-show), printing, finding, history
 * navigation, and the native context menu.
 *
 * Only the default action is prevented, so the app's own key handlers still
 * see these keys.
 */

// Keys with Ctrl (or Cmd) that open browser features.
const CTRL_KEYS = new Set(['r', 'p', 'f', 'g', 'u', 'j', 'h', 'o', 'n', 't', 'w', '+', '-', '=', '0']);

// Keys that act alone, regardless of modifiers.
const PLAIN_KEYS = new Set(['F3', 'F5', 'F7', 'BrowserBack', 'BrowserForward', 'BrowserRefresh', 'BrowserSearch']);

function isBrowserShortcut(e: KeyboardEvent): boolean {
  if (PLAIN_KEYS.has(e.key)) return true;
  if ((e.ctrlKey || e.metaKey) && CTRL_KEYS.has(e.key.toLowerCase())) return true;
  // History navigation.
  return e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight');
}

// Text fields keep their menu for cut, copy and paste.
function isTextField(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target instanceof HTMLTextAreaElement || target instanceof HTMLInputElement)
  );
}

export function lockWebview() {
  // Capture phase, so nothing can stop the event before it gets here.
  window.addEventListener(
    'keydown',
    (e) => {
      if (isBrowserShortcut(e)) e.preventDefault();
    },
    { capture: true },
  );
  // Bubble phase, not capture: the app's own context menus (bits-ui) ignore
  // events whose default is already prevented, so they must see it first.
  // They prevent the default themselves when they open.
  window.addEventListener('contextmenu', (e) => {
    // Development builds keep the menu for "Inspect Element".
    if (!import.meta.env.DEV && !isTextField(e.target)) e.preventDefault();
  });
}
