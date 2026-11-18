// Keys on the remote page (a keyboard at a tablet): the show-wide actions with the host's key bindings
// (`settings.keys`) and Esc for panic, nothing that edits or works on a selection. Only while the device may
// operate the show (`canControl`).

import { type Action, api, type ControlAction } from '../lib/api';
import { chordOf, isPanicKey, sameChord } from '../lib/keys.svelte';
import { canControl, ui } from '../lib/store.svelte';

/** The bound actions the remote page carries out; each is sent as it is. */
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

type RemoteAction = (typeof REMOTE_ACTIONS)[number];

const isRemoteAction = (action: ControlAction): action is RemoteAction =>
  (REMOTE_ACTIONS as readonly ControlAction[]).includes(action);

/** The action a key press stands for on the remote page, if any. */
export function remoteAction(e: KeyboardEvent): RemoteAction | null {
  const chord = chordOf(e);
  if (!chord) return null;
  const bound = ui.view?.settings.keys.find((binding) => sameChord(binding.chord, chord))?.action;
  if (bound && isRemoteAction(bound)) return bound;
  return isPanicKey(chord) ? 'panic' : null;
}

/** Handles a key press on the page; returns whether it did something. */
export function onRemoteKey(e: KeyboardEvent): boolean {
  if (e.repeat || e.defaultPrevented) return false;
  // Typing a name (join form) is not operating.
  const target = e.target as HTMLElement | null;
  if (target?.closest('input, textarea, [contenteditable="true"]')) return false;
  const action = remoteAction(e);
  if (!action || !canControl()) return false;
  e.preventDefault();
  void api.action({ action } as Action).catch(() => {});
  return true;
}
