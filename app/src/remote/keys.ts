// Keys on the remote page (a keyboard at a tablet): the show-wide actions with the host's key bindings
// (`settings.keys`) or the device's own (`ui.remote.keys`, set in `KeysDialog`) and Esc for panic, nothing that edits
// or works on a selection. Only while the device may operate the show (`canControl`), and not while a key field
// records.

import { type Action, api, type Chord, type ControlAction, type KeyBinding } from '../lib/api';
import { chordOf, isPanicKey, keyCapture, sameChord } from '../lib/keys.svelte';
import { canControl, escapeCloses, globalKeys, ui } from '../lib/store.svelte';

/** The bound actions the remote page carries out; each is sent as it is (mirrors `REMOTE_ACTIONS` in
 * `app/src-tauri/src/remote/protocol.rs`, which refuses own keys for others). */
export const REMOTE_ACTIONS = [
  'go',
  'stop_all',
  'panic',
  'fade_all',
  'pause_all',
  'resume_all',
  'toggle_pause_all',
  'playhead_next',
  'playhead_prev',
] as const satisfies readonly ControlAction[];

export type RemoteAction = (typeof REMOTE_ACTIONS)[number];

/** Keys per action at most (`KEYS_PER_ACTION` in `crates/jq-core/src/keys.rs`; Esc for panic does not count). */
export const KEYS_PER_ACTION = 2;

const isRemoteAction = (action: ControlAction): action is RemoteAction =>
  (REMOTE_ACTIONS as readonly ControlAction[]).includes(action);

/** The host's keys of the actions the remote page carries out: where the device's own keys start from. */
export function hostKeys(): KeyBinding[] {
  return (ui.view?.settings.keys ?? []).filter((binding) => isRemoteAction(binding.action));
}

/** The keys of `action` in `keys`, in their order. */
export function keysOf(keys: KeyBinding[], action: RemoteAction): Chord[] {
  return keys.filter((binding) => binding.action === action).map((binding) => binding.chord);
}

/**
 * `keys` with key `index` of `action` set to `chord` (past the last: one more), or removed with null. A key another
 * action had moves to this one; the result is in the order of `REMOTE_ACTIONS`.
 */
export function withKey(keys: KeyBinding[], action: RemoteAction, index: number, chord: Chord | null): KeyBinding[] {
  const own = keysOf(keys, action);
  if (chord === null) own.splice(index, 1);
  else if (index < own.length) own[index] = chord;
  else own.push(chord);
  // The same key twice for one action counts once.
  const unique = own.filter((key, at) => own.findIndex((other) => sameChord(other, key)) === at);
  const others = keys.filter((binding) => binding.action !== action && !(chord && sameChord(binding.chord, chord)));
  const all = [...others, ...unique.slice(0, KEYS_PER_ACTION).map((key) => ({ chord: key, action }))];
  const order = (binding: KeyBinding) => (REMOTE_ACTIONS as readonly ControlAction[]).indexOf(binding.action);
  return all.sort((a, b) => order(a) - order(b));
}

/** The action a key press stands for on the remote page, if any. */
export function remoteAction(e: KeyboardEvent): RemoteAction | null {
  const chord = chordOf(e);
  if (!chord) return null;
  const bound = globalKeys().find((binding) => sameChord(binding.chord, chord))?.action;
  if (bound && isRemoteAction(bound)) return bound;
  return isPanicKey(chord) ? 'panic' : null;
}

/** Handles a key press on the page; returns whether it did something. */
export function onRemoteKey(e: KeyboardEvent): boolean {
  if (e.repeat || e.defaultPrevented || keyCapture.active) return false;
  // Typing a name (join form) is not operating.
  const target = e.target as HTMLElement | null;
  if (target?.closest('input, textarea, [contenteditable="true"]')) return false;
  const action = remoteAction(e);
  if (!action || !canControl()) return false;
  // In a dialog (the device's keys) only panic: Space there clicks the focused button. With "close_first" Esc
  // closes the dialog instead (as in the app).
  const inDialog = document.querySelector('[data-slot="dialog-content"]') !== null;
  if (inDialog && (action !== 'panic' || (e.code === 'Escape' && escapeCloses()))) return false;
  e.preventDefault();
  void api.action({ action } as Action).catch(() => {});
  return true;
}
