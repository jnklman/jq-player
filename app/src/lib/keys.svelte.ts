// Keys as the backend stores them (`Chord`): the physical key
// (`KeyboardEvent.code`) plus modifiers. Matching ignores the label; the
// label shown comes from the current keyboard layout where it can be asked.

import { api, type Chord, type ControlAction, type KeyBinding } from './api';
import { labels, t } from './i18n/index.svelte';

/** Keys that only modify another one; pressed alone they make no chord. */
const MODIFIERS = new Set([
  'ControlLeft',
  'ControlRight',
  'ShiftLeft',
  'ShiftRight',
  'AltLeft',
  'AltRight',
  'MetaLeft',
  'MetaRight',
  'OSLeft',
  'OSRight',
  'CapsLock',
  'NumLock',
  'Fn',
]);

/** The chord of a key press; null for a modifier alone or an unknown key. */
export function chordOf(e: KeyboardEvent): Chord | null {
  if (!e.code || e.code === 'Unidentified' || MODIFIERS.has(e.code)) return null;
  return { code: e.code, ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey, alt: e.altKey, label: e.key };
}

/** Same key with the same modifiers (the label does not count). */
export function sameChord(a: Chord, b: Chord): boolean {
  return a.code === b.code && a.ctrl === b.ctrl && a.shift === b.shift && a.alt === b.alt;
}

/** Names of the keys that have one (by `KeyboardEvent.code`), as printed on a keyboard of the language. */
const NAMES: Record<string, string> = labels({
  Space: 'key.space',
  Escape: 'key.escape',
  Enter: 'key.enter',
  Tab: 'key.tab',
  Backspace: 'key.backspace',
  Delete: 'key.delete',
  Insert: 'key.insert',
  Home: 'key.home',
  End: 'key.end',
  PageUp: 'key.page_up',
  PageDown: 'key.page_down',
  ArrowUp: 'key.arrow_up',
  ArrowDown: 'key.arrow_down',
  ArrowLeft: 'key.arrow_left',
  ArrowRight: 'key.arrow_right',
  Pause: 'key.pause',
  PrintScreen: 'key.print_screen',
  ScrollLock: 'key.scroll_lock',
  ContextMenu: 'key.context_menu',
  NumpadAdd: 'key.numpad_add',
  NumpadSubtract: 'key.numpad_subtract',
  NumpadMultiply: 'key.numpad_multiply',
  NumpadDivide: 'key.numpad_divide',
  NumpadDecimal: 'key.numpad_decimal',
  NumpadEnter: 'key.numpad_enter',
  NumpadEqual: 'key.numpad_equal',
});

/** The key that always panics (fixed, mirrors the backend's `PANIC_KEY`); more keys can be bound to panic. */
export const PANIC_KEY: Chord = { code: 'Escape', ctrl: false, shift: false, alt: false, label: 'Escape' };

/** Esc, with or without Shift: always panic. */
export const isPanicKey = (chord: Chord) => chord.code === PANIC_KEY.code && !chord.ctrl && !chord.alt;

/** A key field is recording: the next press is its key, not a shortcut. */
export const keyCapture = $state({ active: false });

/** True if the app keeps the chord for itself (mirrors the backend: Ctrl+letter by the letter on the key). */
export function isFixed(chord: Chord, fixed: Chord[]): boolean {
  return fixed.some((key) =>
    key.ctrl
      ? chord.ctrl && !chord.alt && keyName(chord).toLowerCase() === key.label
      : !chord.ctrl && !chord.alt && chord.code === key.code,
  );
}

/** Unshifted text of the character keys on the current layout, by code. */
const layout = $state<{ labels: Record<string, string> }>({ labels: {} });

interface KeyboardApi {
  getLayoutMap(): Promise<Map<string, string>>;
}

/** Asks the current layout: the webview (WebView2) or else the backend (GTK, Windows); keeps what it knew. */
export async function loadKeyLabels() {
  const keyboard = (navigator as Navigator & { keyboard?: KeyboardApi }).keyboard;
  try {
    if (keyboard?.getLayoutMap) {
      layout.labels = Object.fromEntries(await keyboard.getLayoutMap());
      return;
    }
  } catch {
    // Falls through to the backend.
  }
  try {
    layout.labels = await api.keyLabels();
  } catch {
    // The stored labels are shown.
  }
}

/** A plain press tells the key's text on the layout active right now (e.g. a second layout group). */
export function learnKeyLabel(e: KeyboardEvent) {
  if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey || [...e.key].length !== 1 || !e.code) return;
  if (layout.labels[e.code] !== e.key) layout.labels[e.code] = e.key;
}

/** The key alone, as printed on it: "F5", "Num 1", "Space", "Ü". */
export function keyName(chord: Chord): string {
  const { code } = chord;
  if (NAMES[code]) return NAMES[code];
  if (/^F\d+$/.test(code)) return code;
  const numpad = code.match(/^Numpad(\d)$/);
  if (numpad) return `Num ${numpad[1]}`;
  const text = layout.labels[code] ?? (chord.label.trim() && [...chord.label].length === 1 ? chord.label : '');
  if (text) return text.toUpperCase();
  return code.replace(/^(Key|Digit)/, '');
}

/** The keys of the chord, e.g. ["Ctrl", "Shift", "F5"]. */
export function keyParts(chord: Chord): string[] {
  const parts = [chord.ctrl && t('key.ctrl'), chord.alt && t('key.alt'), chord.shift && t('key.shift'), keyName(chord)];
  return parts.filter((part): part is string => !!part);
}

/** The whole chord, e.g. "Ctrl + Shift + F5". */
export function keyText(chord: Chord): string {
  return keyParts(chord).join(' + ');
}

/** The keys bound to the first of `actions` that has any, e.g. "Leertaste / F1"; "" if none has. Panic always has Esc. */
export function boundKeys(keys: KeyBinding[], ...actions: ControlAction[]): string {
  for (const action of actions) {
    const own = keys.filter((binding) => binding.action === action).map((binding) => keyText(binding.chord));
    if (action === 'panic') own.unshift(keyText(PANIC_KEY));
    if (own.length) return own.join(' / ');
  }
  return '';
}

/** Global actions that work on the selected cue (keys only). */
export const ON_SELECTED: ControlAction[] = [
  'preview',
  'toggle_pause_cue',
  'stop_cue',
  'fade_cue',
  'restart_cue',
  'exit_loop_cue',
  'next_item',
  'jump_back',
  'jump_forward',
  'jump_end',
  'volume_up',
  'volume_down',
  'pan_left',
  'pan_right',
  'rate_up',
  'rate_down',
];

/** Step actions repeat while their key is held; everything else fires once per press. */
export const REPEATING: ControlAction[] = ['volume_up', 'volume_down', 'pan_left', 'pan_right', 'rate_up', 'rate_down'];

/** Names of the global actions (as the backend's `action.*` texts). */
export const ACTION_NAMES = labels<ControlAction>({
  go: 'action.go',
  stop_all: 'action.stop_all',
  panic: 'action.panic',
  fade_all: 'action.fade_all',
  pause_all: 'action.pause_all',
  resume_all: 'action.resume_all',
  toggle_pause_all: 'action.toggle_pause_all',
  playhead_next: 'action.playhead_next',
  playhead_prev: 'action.playhead_prev',
  stop_preview: 'action.stop_preview',
  preview: 'action.preview',
  toggle_pause_cue: 'action.toggle_pause_cue',
  stop_cue: 'action.stop_cue',
  fade_cue: 'action.fade_cue',
  restart_cue: 'action.restart_cue',
  exit_loop_cue: 'action.exit_loop_cue',
  next_item: 'action.next_item',
  jump_back: 'action.jump_back',
  jump_forward: 'action.jump_forward',
  jump_end: 'action.jump_end',
  volume_up: 'action.volume_up',
  volume_down: 'action.volume_down',
  pan_left: 'action.pan_left',
  pan_right: 'action.pan_right',
  rate_up: 'action.rate_up',
  rate_down: 'action.rate_down',
});
