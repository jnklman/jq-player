// Putting the store into a known state: a show loaded through the fake backend, as the app does at start.

import { tick } from 'svelte';

import type { RuntimeState, ShowView } from '$lib/api';
import { init, ui } from '$lib/store.svelte';

import { backend } from './backend';

let listening = false;

/**
 * Lets pending promises (command replies, events) settle and the UI render them. Only microtasks: the fake backend
 * answers with promises, and so it also works while a test fakes the timers.
 */
export async function settle(rounds = 3) {
  for (let i = 0; i < rounds * 10; i++) await Promise.resolve();
  for (let i = 0; i < rounds; i++) await tick();
}

/**
 * Opens `view` in the store as the backend would send it (`show` event), with `runtime` as the state. The store
 * follows the backend's events from the first call on (once per test file, like the app's start).
 */
export async function loadShow(view: ShowView | null, runtime: RuntimeState | null = null) {
  if (!listening) {
    listening = true;
    await init(true);
  }
  backend.show(view);
  await backend.state(runtime);
  await settle(1);
}

/** Back to a fresh window: no show, no selection, no dialogs or questions. */
export function resetUi() {
  ui.view = null;
  ui.runtime = null;
  ui.selected = null;
  ui.marked = [];
  ui.rangeEnd = null;
  ui.settingsOpen = false;
  ui.aboutOpen = false;
  ui.devicesOpen = false;
  ui.outputsOpen = false;
  ui.stopwatchesOpen = false;
  ui.relinkOpen = false;
  ui.groupOpen = null;
  ui.waveformOpen = null;
  ui.openingShow = false;
  ui.savingAs = false;
  ui.leaving = null;
  ui.question = null;
  ui.clashQuestion = null;
  ui.justSaved = false;
  ui.notices = [];
  ui.update = null;
  ui.remote = null;
  ui.remoteStatus = null;
}
