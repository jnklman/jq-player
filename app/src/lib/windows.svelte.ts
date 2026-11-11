// The app's windows: the main window and the panel windows (one panel each,
// see `components/layout/PanelWindow.svelte`). Each is a webview of its own
// with its own store; show and state come to all of them from the backend.
// What is view state but must be the same everywhere goes between them here,
// as UI-only events (no backend): the selection, dialogs (only the main
// window shows them) and changes of the layout (only the main window writes it).

import { emit, emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { untrack } from 'svelte';

import { api, type CueId, events } from './api';
import type { LayoutState, PanelId, Placement } from './layout.svelte';
import { createShow, openShow, saveShowAs } from './shows';
import { revealCue, ui } from './store.svelte';

declare global {
  interface Window {
    /** Set by the backend in a panel window (`panel_windows.rs`): the panel it shows. */
    __JQ_PANEL__?: string;
  }
}

/** The panel this window shows; undefined in the main window. */
export const windowPanel: PanelId | undefined = window.__JQ_PANEL__;
export const isMainWindow = windowPanel === undefined;

const MAIN = 'main';
const label = getCurrentWindow().label;

const SELECTION = 'ui_selection';
const SELECTION_ASK = 'ui_selection_ask';
const OPEN = 'ui_open';
const PANEL_COMMAND = 'ui_panel_window';

interface Selection {
  from: string;
  selected: CueId | null;
  marked: CueId[];
  rangeEnd: CueId | null;
}

/**
 * Keeps the selection the same in all windows, so a cue picked in the cue list shows in an inspector in another
 * window. Each change is sent once; what comes in is taken without sending it back.
 */
export function syncSelection() {
  const current = () => JSON.stringify([ui.selected, ui.marked, ui.rangeEnd]);
  let last = current();
  const send = () => {
    last = current();
    const selection: Selection = { from: label, selected: ui.selected, marked: ui.marked, rangeEnd: ui.rangeEnd };
    void emit(SELECTION, selection);
  };
  void listen<Selection>(SELECTION, ({ payload }) => {
    if (payload.from === label) return;
    last = JSON.stringify([payload.selected, payload.marked, payload.rangeEnd]);
    const moved = payload.selected !== ui.selected;
    ui.selected = payload.selected;
    ui.marked = payload.marked;
    ui.rangeEnd = payload.rangeEnd;
    // A cue list here follows a selection moved elsewhere (arrow keys there).
    if (moved && payload.selected) void revealCue(payload.selected);
  });
  // A new panel window starts with the main window's selection.
  if (isMainWindow) void listen(SELECTION_ASK, send);
  else void emitTo(MAIN, SELECTION_ASK);
  $effect.root(() => {
    $effect(() => {
      if (current() !== last) untrack(send);
    });
  });
}

/** Dialogs only the main window shows, by their field in `ui`. */
const DIALOGS = [
  'settingsOpen',
  'devicesOpen',
  'outputsOpen',
  'stopwatchesOpen',
  'relinkOpen',
  'groupOpen',
  'waveformOpen',
] as const;
type Dialog = (typeof DIALOGS)[number];

/** Commands with a dialog of their own (file picker, questions) run in the main window. */
export type MainCommand = 'new_show' | 'open_show' | 'save_show_as';

type OpenRequest = { dialog: Dialog; value: string | true } | { command: MainCommand };

/** Runs a command in the main window and brings it to the front (from a panel window). */
export function runInMain(command: MainCommand) {
  void emitTo(MAIN, OPEN, { command } satisfies OpenRequest);
  void api.focusMain();
}

/** In a panel window: a dialog opened here (e.g. the waveform from an inspector) opens in the main window. */
export function forwardDialogs() {
  $effect.root(() => {
    $effect(() => {
      for (const dialog of DIALOGS) {
        const value = ui[dialog];
        if (!value) continue;
        untrack(() => {
          if (dialog === 'groupOpen' || dialog === 'waveformOpen') ui[dialog] = null;
          else ui[dialog] = false;
          void emitTo(MAIN, OPEN, { dialog, value } satisfies OpenRequest);
          void api.focusMain();
        });
      }
    });
  });
}

/** In the main window: opens what panel windows ask for. */
export function serveDialogs(): Promise<UnlistenFn> {
  return listen<OpenRequest>(OPEN, ({ payload }) => {
    if ('command' in payload) {
      if (payload.command === 'new_show') void createShow();
      else if (payload.command === 'open_show') void openShow();
      else void saveShowAs();
    } else if (payload.dialog === 'groupOpen' || payload.dialog === 'waveformOpen') {
      if (typeof payload.value === 'string') ui[payload.dialog] = payload.value;
    } else {
      ui[payload.dialog] = true;
    }
  });
}

type PanelCommand = { panel: PanelId; placement: Placement } | { panel: PanelId; opacity: number };

/** From a panel window: puts its panel elsewhere (main window, other kind of window). */
export function placeFromWindow(panel: PanelId, placement: Placement) {
  void emitTo(MAIN, PANEL_COMMAND, { panel, placement } satisfies PanelCommand);
}

/** From a panel window: its overlay's opacity. */
export function setOpacityFromWindow(panel: PanelId, opacity: number) {
  void emitTo(MAIN, PANEL_COMMAND, { panel, opacity } satisfies PanelCommand);
}

/**
 * In the main window, while a show is open: opens the panel windows of the current mode (again whenever which ones
 * or their kind change), takes in where the operator moves them and what they ask for, and brings back the panel
 * of a window the operator closed. Returns the cleanup, which closes all of them.
 */
export function servePanelWindows(layout: LayoutState): () => void {
  // Moving a window changes the layout as well, but needs nothing opened or closed.
  let applied: string | null = null;
  const stop = $effect.root(() => {
    $effect(() => {
      const key = JSON.stringify(layout.windows.map((window) => [window.panel, window.kind]));
      if (key === applied) return;
      applied = key;
      untrack(() => layout.applyWindows());
    });
  });
  const listeners = [
    events.onPanelWindowMoved((rect) => layout.windowMoved(rect)),
    events.onPanelWindowClosed((panel) => layout.place(panel, 'main')),
    listen<PanelCommand>(PANEL_COMMAND, ({ payload }) => {
      if ('placement' in payload) layout.place(payload.panel, payload.placement);
      else layout.setOpacity(payload.panel, payload.opacity);
    }),
  ];
  return () => {
    stop();
    listeners.forEach((listener) => void listener.then((unlisten) => unlisten()));
    void api.setPanelWindows([]).catch(() => {});
  };
}
