// UI state. Show data comes only from the backend (`show`/`state` events);
// what lives here is purely view state: selection, notices.

import { tick } from 'svelte';

import {
  type ActiveState,
  api,
  type CloseCheck,
  type Cue,
  type CueId,
  events,
  type FadingState,
  type GapState,
  type ImportClash,
  type Notice,
  type OnClash,
  type RuntimeState,
  type SelectionMode,
  type ShowView,
} from './api';
import { revealRow } from './cueScroll';
import { setLanguage, t } from './i18n/index.svelte';

export type Mode = 'edit' | 'show';

/** A pending question before the show is closed or replaced. */
export interface Leaving {
  /** What is left: the program (quit) or the show (switch to another). */
  kind: 'quit' | 'switch';
  check: CloseCheck;
  /** Leaves, saving first if asked; a failure keeps the show open. */
  proceed: (save: boolean) => Promise<unknown>;
}

/** A button of an in-app question. */
export interface Choice<T extends string> {
  label: string;
  value: T;
  /** Gets the focus (Enter); without one, the first choice does. */
  focus?: boolean;
  danger?: boolean;
}

/** An open in-app question; see `ask`. */
export interface Question {
  title: string;
  text: string;
  choices: Choice<string>[];
  answer: (value: string) => void;
}

/** Name clashes of a pick waiting for a choice per file; see `askClashes`. */
export interface ClashQuestion {
  clashes: ImportClash[];
  /** `null`: the whole pick is cancelled. */
  answer: (choices: Record<string, OnClash> | null) => void;
}

export interface ShownNotice extends Notice {
  id: number;
}

// The backend replaces these whole (a large show's view is several hundred
// kB, the runtime state comes ~30×/s), so they are not made deeply reactive:
// wrapping every cue in a proxy made each update, and rendering the cue list,
// several times slower. Never change them in place, only assign new ones.
let view = $state.raw<ShowView | null>(null);
let runtime = $state.raw<RuntimeState | null>(null);

export const ui = $state({
  get view() {
    return view;
  },
  set view(next: ShowView | null) {
    view = next;
  },
  get runtime() {
    return runtime;
  },
  set runtime(next: RuntimeState | null) {
    runtime = next;
  },
  /** The cue the inspector shows and new cues go after; set it with `select`. */
  selected: null as CueId | null,
  /** Further selected cues of the main list (Ctrl/Shift-click), besides `selected`. */
  marked: [] as CueId[],
  /** Moving end of a Shift range, which runs from `selected` to here; set with `extendSelection`. */
  rangeEnd: null as CueId | null,
  settingsOpen: false,
  aboutOpen: false,
  devicesOpen: false,
  outputsOpen: false,
  stopwatchesOpen: false,
  relinkOpen: false,
  /** Group being edited in the group dialog. */
  groupOpen: null as CueId | null,
  /** Sound cue whose positions are being edited in the waveform dialog. */
  waveformOpen: null as CueId | null,
  /** A show is being picked or loaded; opening another one waits. */
  openingShow: false,
  /** The first `getShow` has answered. Before, `view` is null also when a show
   * is still being opened at start (file argument, last show), which must not
   * flash the start screen. */
  started: false,
  /** "Save as" is under way (copying media may take a while). */
  savingAs: false,
  /** Set while the close (or switch) question is shown. */
  leaving: null as Leaving | null,
  /** In-app questions instead of system dialogs; at most one of each at a time. */
  question: null as Question | null,
  clashQuestion: null as ClashQuestion | null,
  /** Briefly true after a save, for the confirmation in the header. */
  justSaved: false,
  notices: [] as ShownNotice[],
});

let noticeId = 0;

export function notify(notice: Notice) {
  ui.notices = [...ui.notices.slice(-4), { ...notice, id: ++noticeId }];
  // Warnings fade out on their own, errors stay until dismissed.
  if (notice.level === 'warning') {
    const id = noticeId;
    setTimeout(() => dismiss(id), 8000);
  }
}

export function dismiss(id: number) {
  ui.notices = ui.notices.filter((notice) => notice.id !== id);
}

/** Runs a backend call and shows a failure as an error notice. */
export async function attempt<T>(promise: Promise<T>): Promise<T | undefined> {
  try {
    return await promise;
  } catch (error) {
    notify({ level: 'error', message: String(error) });
    return undefined;
  }
}

/**
 * How dialogs take Esc: with `settings.escape = "close_first"` they close, else they ignore it and Esc stays a
 * panic. Questions (alert dialogs) always ignore it: they need an answer.
 */
export function dialogEscape(): 'close' | 'ignore' {
  return ui.view?.settings.escape === 'close_first' ? 'close' : 'ignore';
}

/**
 * Asks in an in-app dialog; resolves with the value of the chosen button.
 * The first choice should be the one that changes nothing: a newer question
 * answers an open one with it.
 */
export function ask<T extends string>(title: string, text: string, choices: Choice<T>[]): Promise<T> {
  ui.question?.answer(ui.question.choices[0].value);
  return new Promise((resolve) => {
    ui.question = {
      title,
      text,
      choices,
      answer: (value) => {
        ui.question = null;
        resolve(value as T);
      },
    };
  });
}

/**
 * Asked when a dialog with unsaved changes closes other than by "Cancel" (×, Esc, a click outside).
 * Without `canApply` (e.g. invalid input) only staying or discarding is offered.
 */
export function askUnsaved(canApply = true): Promise<'stay' | 'discard' | 'apply'> {
  return ask<'stay' | 'discard' | 'apply'>(t('unsaved.title'), t('unsaved.text'), [
    { label: t('unsaved.stay'), value: 'stay', focus: true },
    { label: t('unsaved.discard'), value: 'discard' },
    ...(canApply ? [{ label: t('unsaved.apply'), value: 'apply' as const }] : []),
  ]);
}

/** Asks what to do with each file whose name is taken; `null` if cancelled. */
export function askClashes(clashes: ImportClash[]): Promise<Record<string, OnClash> | null> {
  ui.clashQuestion?.answer(null);
  return new Promise((resolve) => {
    ui.clashQuestion = {
      clashes,
      answer: (choices) => {
        ui.clashQuestion = null;
        resolve(choices);
      },
    };
  });
}

let savedTimer: ReturnType<typeof setTimeout> | undefined;

/** Saves the show and briefly confirms it in the header. */
export async function saveShow() {
  if ((await attempt(api.saveShow())) === undefined) return;
  ui.justSaved = true;
  clearTimeout(savedTimer);
  savedTimer = setTimeout(() => (ui.justSaved = false), 2000);
}

/** Takes the language from the backend's settings (the config file) and follows its changes; awaited before the UI is mounted. */
export async function initLanguage() {
  await events.onLanguageChanged((info) => {
    // Shown notices are in the old language and the backend repeats what still applies in the new one; the
    // event comes before those, so only the old ones go.
    ui.notices = [];
    setLanguage(info);
  });
  const language = await attempt(api.getLanguage());
  if (language) setLanguage(language);
}

/** Follows the backend's events; notices and the close question only in the main window (`main`). */
export async function init(main: boolean) {
  await events.onShow((view) => {
    ui.view = view;
    pruneSelection();
  });
  // Replaced whole: `runtime` is raw state, a change inside it would go unnoticed.
  await events.onState((state) => {
    ui.runtime = state;
  });
  if (main) await listenAsMain();
  // Answers after a show the backend opens at start (same queue).
  ui.view = (await attempt(api.getShow())) ?? null;
  ui.started = true;
}

async function listenAsMain() {
  await events.onNotice(notify);
  // Not during a show: the operator finds the update in the About dialog later.
  await events.onUpdateAvailable((info) => {
    if (mode() !== 'show')
      notify({ level: 'warning', message: t('update.available_notice', { version: info.version }) });
  });
  await events.onCloseRequested((check) => {
    ui.leaving = {
      kind: 'quit',
      check,
      proceed: (save) => api.confirmClose(save),
    };
    api.closeAck();
  });
}

/** Show mode is a machine setting held by the backend, which also locks edits. */
export function mode(): Mode {
  return ui.view?.settings.show_mode ? 'show' : 'edit';
}

/** How the cue list's selection and the playhead go together (a machine setting). */
export function selectionMode(): SelectionMode {
  return ui.view?.settings.selection_mode ?? 'separate';
}

export function cues(): Cue[] {
  return ui.view?.show.cues ?? [];
}

/**
 * Selects one cue (or none), ending a multi-selection. If the playhead
 * follows the selection, it moves there too.
 */
export function select(id: CueId | null) {
  const follows = ['playhead_follows', 'linked'].includes(selectionMode());
  if (id && follows && id !== ui.runtime?.playhead) {
    api.action({ action: 'set_playhead', cue: id });
  }
  ui.selected = id;
  ui.marked = [];
  ui.rangeEnd = null;
}

/** Forgets selected cues that are gone (deleted, cut, undone), so no shortcut acts on them. */
function pruneSelection() {
  const ids = new Set(cues().map((cue) => cue.id));
  const kept = [ui.selected, ...ui.marked].filter((id) => id !== null && ids.has(id)) as CueId[];
  if (kept.length === ui.marked.length + (ui.selected ? 1 : 0)) return;
  if (ui.rangeEnd && !ids.has(ui.rangeEnd)) ui.rangeEnd = null;
  ui.selected = kept[0] ?? null;
  ui.marked = kept.slice(1);
}

/** Selects these cues; the first one is shown in the inspector. */
export function selectAll(ids: CueId[]) {
  ui.selected = ids[0] ?? null;
  ui.marked = ids.slice(1);
  ui.rangeEnd = null;
}

/** Adds the cue to the selection or takes it out (Ctrl-click); an added one is shown in the inspector. */
export function toggleSelected(id: CueId) {
  ui.rangeEnd = null;
  if (id === ui.selected) {
    ui.selected = ui.marked[0] ?? null;
    ui.marked = ui.marked.slice(1);
  } else if (ui.marked.includes(id)) {
    ui.marked = ui.marked.filter((other) => other !== id);
  } else {
    ui.marked = ui.selected ? [...ui.marked, ui.selected] : ui.marked;
    ui.selected = id;
  }
}

/** Selects the range from the selected cue to `to` (Shift-click, Shift+arrow); without a selection, just `to`. */
export function extendSelection(to: CueId) {
  const list = cues();
  const from = list.findIndex((cue) => cue.id === ui.selected);
  const at = list.findIndex((cue) => cue.id === to);
  if (from < 0 || at < 0) return select(to);
  ui.rangeEnd = to;
  ui.marked = list
    .slice(Math.min(from, at), Math.max(from, at) + 1)
    .map((cue) => cue.id)
    .filter((id) => id !== ui.selected);
}

/**
 * Moves the selection by `move`; without a selection, jumps to the first
 * (down) or last (up) cue. Passing `"start"` or `"end"` jumps to the start or
 * end of the list respectively, `"prev_scene"` and `"next_scene"` to the first
 * cue of the scene before or after (bounded by scene markers; after the last
 * scene's start, to the last cue). With `extend`, the end of a range
 * moves instead (Shift), and the selected cue stays.
 */
export function moveSelection(move: number | 'start' | 'end' | 'prev_scene' | 'next_scene', extend = false) {
  const list = cues();
  if (list.length === 0) return;

  const from = extend ? (ui.rangeEnd ?? ui.selected) : ui.selected;
  let at: number;
  let next: number;
  switch (move) {
    case 'start':
      next = 0;
      break;
    case 'end':
      next = list.length - 1;
      break;
    case 'prev_scene':
    case 'next_scene': {
      // The first cue of each scene; cues before the first marker count as one.
      const playable = list.flatMap((cue, i) => (cue.type === 'scene' ? [] : [i]));
      const starts = playable.filter((i) => i === 0 || list[i - 1].type === 'scene');
      if (starts.length === 0) return;
      // Without a selection, from the end (up) or the start (down). Up stops at the first scene's start, down
      // goes to the last cue after the last scene's start.
      at = list.findIndex((cue) => cue.id === from);
      if (move === 'prev_scene') {
        const before = starts.filter((i) => i < (at < 0 ? list.length : at));
        next = before.length ? before[before.length - 1] : starts[0];
      } else {
        next = starts.find((i) => i > at) ?? playable[playable.length - 1];
      }
      break;
    }
    default:
      at = list.findIndex((cue) => cue.id === from);
      if (at < 0) {
        next = move > 0 ? 0 : list.length - 1;
        break;
      }
      next = Math.min(Math.max(0, at + Math.trunc(move)), list.length - 1);
  }
  // The selection follows the playhead, so the keys move that; it never stands on a scene marker, so they step over
  // them (towards the move; at the list's end back the other way).
  if (!extend && ['selection_follows', 'linked'].includes(selectionMode())) {
    const step = move === 'end' || (typeof move === 'number' && move < 0) ? -1 : 1;
    const playable = (i: number) => i >= 0 && i < list.length && list[i].type !== 'scene';
    let to = next;
    while (to >= 0 && to < list.length && !playable(to)) to += step;
    if (!playable(to)) {
      to = next;
      while (to >= 0 && to < list.length && !playable(to)) to -= step;
    }
    if (playable(to)) api.action({ action: 'set_playhead', cue: list[to].id });
    return;
  }
  const id = list[next].id;
  if (extend) extendSelection(id);
  else select(id);
  void revealCue(id);
}

/**
 * Jumps to the cue with this number (of the main list): selects it, or, when the selection follows the playhead,
 * moves the playhead there (which never stands on a scene marker). Returns whether there is such a cue.
 */
export function gotoCue(number: string): boolean {
  const wanted = number.trim();
  if (!wanted) return false;
  const following = ['selection_follows', 'linked'].includes(selectionMode());
  const cue = cues().find((cue) => cue.number === wanted && !(following && cue.type === 'scene'));
  if (!cue) return false;
  if (following) api.action({ action: 'set_playhead', cue: cue.id });
  else select(cue.id);
  void revealCue(cue.id);
  return true;
}

/** Scrolls the cue list just far enough to show the cue's row, once a pending update has rendered it. Smooth only
 * where it cannot lag behind (not for held arrow keys). */
export async function revealCue(id: CueId, behavior: ScrollBehavior = 'auto') {
  await tick();
  const row = document.querySelector<HTMLElement>(`[data-cue="${id}"]`);
  if (row) revealRow(row, behavior);
}

/** The selected cues of the main list, in list order; cues gone meanwhile are left out. */
export function selection(): CueId[] {
  return cues()
    .map((cue) => cue.id)
    .filter((id) => id === ui.selected || ui.marked.includes(id));
}

/** Every cue (groups' cues included) and a lookup by id, rebuilt only when the show changes. */
const index = $derived.by(() => {
  const all: Cue[] = [];
  const walk = (list: Cue[]) =>
    list.forEach((cue) => {
      all.push(cue);
      if (cue.type === 'group') walk(cue.cues);
    });
  walk(cues());
  return { all, byId: new Map(all.map((cue) => [cue.id, cue])) };
});

/** Every cue, groups' cues included. */
export function allCues(): Cue[] {
  return index.all;
}

/** Also finds cues inside groups. */
export function cueById(id: CueId | null | undefined): Cue | undefined {
  return id ? index.byId.get(id) : undefined;
}

/** The list holding cue `id`: the main list or a group's; empty if there is no such cue. */
export function siblingsOf(id: CueId): Cue[] {
  function find(list: Cue[]): Cue[] | undefined {
    if (list.some((cue) => cue.id === id)) return list;
    for (const cue of list) {
      if (cue.type === 'group') {
        const found = find(cue.cues);
        if (found) return found;
      }
    }
    return undefined;
  }

  return find(cues()) ?? [];
}

function acceptsControlChanges(cue: Cue): boolean {
  switch (cue.type) {
    case 'sound':
    case 'playlist':
      return true;
    case 'group':
      return cue.cues.some(acceptsControlChanges);
    default:
      return false;
  }
}

/** The nearest previous sibling that can receive control-cue changes. */
export function previousOf(id: CueId): Cue | undefined {
  const siblings = siblingsOf(id);
  const index = siblings.findIndex((cue) => cue.id === id);
  return index < 1 ? undefined : siblings.slice(0, index).reverse().find(acceptsControlChanges);
}

/** True if the cue is in the main list (not in a group). */
export function isTopLevel(id: CueId): boolean {
  return cues().some((cue) => cue.id === id);
}

/** True if the cue (or, for a group, anything in it) plays, fades or waits. */
export function isRunning(cue: Cue): boolean {
  const busy = (id: CueId) => !!(activeFor(id) || fadingFor(id) || gapFor(id) || waitingFor(id) != null);
  return busy(cue.id) || (cue.type === 'group' && cue.cues.some(isRunning));
}

export function activeFor(id: CueId): ActiveState | undefined {
  return ui.runtime?.active.find((active) => active.cue === id);
}

export function fadingFor(id: CueId): FadingState | undefined {
  return ui.runtime?.fading.find((fading) => fading.cue === id);
}

export function gapFor(id: CueId): GapState | undefined {
  return ui.runtime?.gaps.find((gap) => gap.cue === id);
}

/** Copy progress (0…1) of a file being copied into the media folder, by stored path. */
export function importFor(file: string): number | undefined {
  return ui.runtime?.imports.find((copy) => copy.file === file)?.progress;
}

export function waitingFor(id: CueId): number | undefined {
  return ui.runtime?.waiting.find((waiting) => waiting.cue === id)?.remaining;
}
