// The keyboard of every window of the app: fixed app keys (save, clipboard,
// list navigation), the configurable global actions and the cue hotkeys.
// KeyboardScope.svelte only wires the window events to the handlers here.

import { type Action, api, type CueId } from './api';
import { copyCues, cutCues, deleteCues, pasteCues, redo, selectEveryCue, undo } from './commands';
import { t } from './i18n/index.svelte';
import { chordOf, isPanicKey, keyCapture, learnKeyLabel, ON_SELECTED, REPEATING, sameChord } from './keys.svelte';
import type { Format } from './richText';
import { createShow, openShow, saveShowAs } from './shows';
import { cues, mode, moveSelection, saveShow, selection, ui } from './store.svelte';
import { isMainWindow, runInMain } from './windows.svelte';

// Whether the focus was last placed with the mouse (a click) rather than
// the keyboard (Tab). A webview shows the focus ring once a key is pressed
// on a clicked button, so GO after clicking e.g. "+ Lautstärke" would light up
// that button; and a clicked menu trigger keeps the focus after its menu
// closed, where Space would open it again instead of GO.
let pointerFocus = false;

/** A pointer press anywhere (capture phase): the focus it places is a clicked one. */
export function notePointer() {
  pointerFocus = true;
}

/** A field that takes typed text (also a focused `contenteditable`). */
export function isTextField(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
  );
}

// Fields and open lists or menus keep their keys. Select and menu triggers
// and panel dividers keep them only when reached with Tab, so they can be
// used from the keyboard; clicked ones leave the keys to the shortcuts.
function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (isTextField(target)) return true;
  if (target.closest('[data-slot="select-content"], [data-slot="dropdown-menu-content"]')) return true;
  const control = '[data-slot="select-trigger"], [data-slot="dropdown-menu-trigger"], [data-slot="resizable-handle"]';
  return !pointerFocus && target.closest(control) !== null;
}

/** A dialog, menu or list is open that Esc closes (questions never close with Esc). */
function overlayOpen(): boolean {
  const slots = ['dialog', 'dropdown-menu', 'popover', 'select'];
  return document.querySelector(slots.map((slot) => `[data-slot="${slot}-content"]`).join(', ')) !== null;
}

/** Takes the focus from a clicked control (see `pointerFocus`); focus moved with Tab stays. */
function releasePointerFocus() {
  const focused = document.activeElement;
  if (pointerFocus && focused instanceof HTMLElement && !isTyping(focused)) focused.blur();
}

// Cues started by a held hotkey with a release trigger, by key code: the
// key's release (whatever happened to the modifiers meanwhile) releases them.
const held = new Map<string, CueId[]>();

/** A key went up: the cues its press started with a release trigger are released. */
export function releaseKey(code: string) {
  held.get(code)?.forEach((cue) => api.action({ action: 'release_cue', cue }));
  held.delete(code);
}

/** The window lost the keys: nothing will tell when a held key is let go, so all of them count as released. */
export function releaseAllKeys() {
  [...held.keys()].forEach(releaseKey);
}

/** Space must never also "click" a focused button on keyup: that would be a second GO. */
export function onKeyup(e: KeyboardEvent) {
  if (e.key === ' ' && ui.view && !(mode() === 'edit' && isTyping(e.target))) e.preventDefault();
}

/** Ctrl (or Cmd) with a letter, by the letter typed (so it follows the layout). */
function ctrlLetter(e: KeyboardEvent): string | null {
  return e.ctrlKey || e.metaKey ? e.key.toLowerCase() : null;
}

/** The formatting keys of the detail text: Ctrl + B, I, U and Ctrl + Shift + X (names as the language has them). */
export const FORMAT_KEYS: Record<Format, string> = {
  get bold() {
    return `${t('key.ctrl')} + B`;
  },
  get italic() {
    return `${t('key.ctrl')} + I`;
  },
  get underline() {
    return `${t('key.ctrl')} + U`;
  },
  get strike() {
    return `${t('key.ctrl')} + ${t('key.shift')} + X`;
  },
};

/** The format a key in the detail text switches, if any (the field handles it itself). */
export function formatShortcut(e: KeyboardEvent): Format | null {
  const letter = ctrlLetter(e);
  if (!letter || e.altKey) return null;
  if (e.shiftKey) return letter === 'x' ? 'strike' : null;
  const plain: Record<string, Format> = { b: 'bold', i: 'italic', u: 'underline' };
  return plain[letter] ?? null;
}

/** The fixed Ctrl shortcuts of edit mode; true if `e` was one (and nothing else may act on it). */
function editShortcut(e: KeyboardEvent, letter: string): boolean {
  switch (letter) {
    case 'z':
      if (e.shiftKey) redo();
      else undo();
      break;
    case 'y':
      redo();
      break;
    case 'c':
    case 'x':
      // Selected text (e.g. of a notice) is copied as text, not the cues.
      if (selection().length === 0 || window.getSelection()?.toString()) return true;
      if (letter === 'c') copyCues();
      else void cutCues();
      break;
    case 'a':
      selectEveryCue();
      break;
    case 'v':
      if (!e.repeat) void pasteCues(e.shiftKey);
      break;
    default:
      return false;
  }
  e.preventDefault();
  return true;
}

// Handled before the focused element sees the key (capture phase), so a
// shortcut never also acts on that element (opens a menu, moves a divider).
// Show mode: GO and STOP ALL win even over a focused field, which keeps its
// other keys. Edit mode: typing wins.
export function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Tab') pointerFocus = false;
  learnKeyLabel(e);
  // A key field records this press.
  if (!ui.view || keyCapture.active) return;
  const letter = ctrlLetter(e);
  // Commands with dialogs of their own run in the main window.
  if (letter === 's') {
    e.preventDefault();
    if (!e.shiftKey) saveShow();
    else if (isMainWindow) saveShowAs();
    else runInMain('save_show_as');
    return;
  }
  if (!e.shiftKey && (letter === 'n' || letter === 'o')) {
    e.preventDefault();
    if (!isMainWindow) runInMain(letter === 'n' ? 'new_show' : 'open_show');
    else if (letter === 'n') createShow();
    else openShow();
    return;
  }
  const typing = isTyping(e.target);
  const editing = mode() === 'edit';
  if (editing && typing) return;
  if (editing && letter && editShortcut(e, letter)) return;
  // The waveform dialog takes space for its preview player.
  if (ui.waveformOpen && e.key === ' ') return;
  const chord = chordOf(e);
  if (!chord) return;
  // Esc always panics (also with Shift); other keys as bound in the settings.
  const action = isPanicKey(chord) ? 'panic' : ui.view.settings.keys.find((b) => sameChord(b.chord, chord))?.action;
  // In show mode GO and panic win even over a focused field.
  if (typing && action !== 'go' && action !== 'panic') return;

  // Keys of the list (fixed): arrows, Home and End move the selection,
  // Ctrl+arrows from scene to scene, Shift extends it while editing;
  // Delete deletes (edit) or stops (show).
  const plain = !chord.ctrl && !chord.alt;
  if (plain && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.code)) {
    e.preventDefault();
    const to = { ArrowDown: 1, ArrowUp: -1, Home: 'start', End: 'end' } as const;
    moveSelection(to[e.code as keyof typeof to], e.shiftKey && editing);
  } else if (chord.ctrl && !chord.alt && ['ArrowDown', 'ArrowUp'].includes(e.code)) {
    e.preventDefault();
    moveSelection(e.code === 'ArrowUp' ? 'prev_scene' : 'next_scene', e.shiftKey && editing);
  } else if (plain && !chord.shift && e.code === 'Delete') {
    if (editing) {
      if (selection().length === 0) return;
      e.preventDefault();
      if (!e.repeat) void deleteCues();
    } else if (ui.selected) {
      e.preventDefault();
      api.action({ action: 'stop_cue', cue: ui.selected });
    }
  } else if (action) {
    if (action === 'panic') {
      // Left to the dialog, menu or list to close it.
      if (e.code === 'Escape' && ui.view.settings.escape === 'close_first' && overlayOpen()) return;
      e.preventDefault();
      // A held key must not count as the second press, which stops at once.
      if (!e.repeat) api.action({ action: 'panic' });
      // An open menu or list still closes (Esc).
      releasePointerFocus();
      return;
    }
    if (ON_SELECTED.includes(action)) {
      if (!ui.selected) return;
      e.preventDefault();
      if (!e.repeat || REPEATING.includes(action)) {
        api.action({ action, cue: ui.selected } as Action);
      }
    } else {
      e.preventDefault();
      // A held key must not fire GO repeatedly.
      if (!e.repeat) api.action({ action } as Action);
    }
  } else {
    // Cue hotkeys come last; a key several cues have starts them all.
    const hotkeyCues = cues().filter((cue) => cue.hotkey && sameChord(cue.hotkey, chord));
    if (!hotkeyCues.length) return;
    e.preventDefault();
    if (!e.repeat) {
      hotkeyCues.forEach((cue) => api.action({ action: 'start_cue', cue: cue.id }));
      const releasing = hotkeyCues.filter((cue) => cue.release_trigger).map((cue) => cue.id);
      if (releasing.length) held.set(e.code, releasing);
    }
  }
  e.stopPropagation();
  releasePointerFocus();
}
