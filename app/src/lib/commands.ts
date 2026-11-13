// What the operator does to the show, shared by toolbars, menus and keys: one
// function per command, so a button and its shortcut always do the same
// (selection afterwards included). Every call reports a failure as a notice.

import { cueAfter, firstAdded } from './added';
import { api, type Cue, type CueId, type EditOp, type GroupCue, type LevelLimits, type ShowView } from './api';
import type { AddItem } from './control';
import { cueLabel, isPlaceholder } from './cue';
import { fmtDb } from './format';
import { t, translations } from './i18n/index.svelte';
import { pickAudio } from './media';
import {
  allCues,
  ask,
  attempt,
  cueById,
  cues,
  type Mode,
  revealCue,
  select,
  selectAll,
  selection,
  ui,
} from './store.svelte';

/** Sends one edit; the new view, or `undefined` if it failed (shown as a notice). */
export function edit(op: EditOp): Promise<ShowView | undefined> {
  return attempt(api.edit(op));
}

// --- Clipboard and selection

/** Copies the cues (default: the selection) onto the backend's clipboard. */
export function copyCues(ids: CueId[] = selection()) {
  if (ids.length) void edit({ op: 'copy', ids });
}

/** Cuts the cues (default: the selection); the cue taking the place of the first gets selected. */
export function cutCues(ids: CueId[] = selection()) {
  return removeCues(ids, 'cut');
}

/** Deletes the cues (default: the selection); the cue taking the place of the first gets selected. */
export function deleteCues(ids: CueId[] = selection()) {
  return removeCues(ids, 'delete');
}

async function removeCues(ids: CueId[], op: 'cut' | 'delete') {
  if (ids.length === 0) return;
  const at = cues().findIndex((cue) => cue.id === ids[0]);
  const view = await edit({ op, ids });
  // Inside a group (`at` < 0) the selection of the main list stays.
  if (view && at >= 0) select(view.show.cues[Math.min(at, view.show.cues.length - 1)]?.id ?? null);
}

/** Pastes after the last selected cue (or at the end) and selects what was pasted;
 * `linked` makes the copies instances of their originals. */
export function pasteCues(linked = false) {
  return pasteAndSelect({ op: 'paste', after: selection().at(-1) ?? null, linked });
}

/** Replaces the selected cues with the clipboard (at the place of the first) and selects what was pasted;
 * `swap` puts the replaced cues on the clipboard. */
export function pasteOverCues(ids: CueId[] = selection(), swap = false) {
  if (ids.length) return pasteAndSelect({ op: 'paste_replace', ids, swap });
}

async function pasteAndSelect(op: EditOp) {
  const before = new Set(cues().map((cue) => cue.id));
  const view = await edit(op);
  if (!view) return;
  const pasted = view.show.cues.filter((cue) => !before.has(cue.id)).map((cue) => cue.id);
  selectAll(pasted);
  if (pasted.length) void revealCue(pasted[0], 'smooth');
}

/** Duplicates the cue (default: the one in the inspector); `linked`: the copy is an instance of it. A copy in the
 * main list gets selected; returns its id (`undefined` if nothing was duplicated). */
export async function duplicateCue(id: CueId | null = ui.selected, linked = false): Promise<CueId | undefined> {
  if (!id) return undefined;
  const inMain = cues().some((cue) => cue.id === id);
  const view = await edit({ op: 'duplicate', id, linked });
  const copy = view && cueAfter(view.show.cues, id)?.id;
  if (!copy) return undefined;
  if (inMain) {
    select(copy);
    void revealCue(copy, 'smooth');
  }
  return copy;
}

/** The cue whose settings linking the selection would take (the one in the
 * inspector), if the selection can be linked: several cues of its type, no groups or scenes. */
export function linkSource(): Cue | undefined {
  const selected = selection();
  const selectedCues = allCues().filter((cue) => selected.includes(cue.id));
  const source = selectedCues.find((cue) => cue.id === ui.selected);
  if (!source || source.type === 'group' || source.type === 'scene' || selectedCues.length < 2) return undefined;
  return selectedCues.every((cue) => cue.type === source.type) ? source : undefined;
}

/** The selected cues take the settings of `source` and stay in sync with it. */
export function linkCues(source: CueId, ids: CueId[] = selection()) {
  void edit({ op: 'link_cues', source, ids });
}

export function unlinkCues(ids: CueId[] = selection()) {
  if (ids.length) void edit({ op: 'unlink_cues', ids });
}

/** Selects every cue of the main list. */
export function selectEveryCue() {
  selectAll(cues().map((cue) => cue.id));
}

/** Selects the scene marker and its cues, up to the next marker or the end of the list. */
export function selectScene(id: CueId) {
  const list = cues();
  const at = list.findIndex((cue) => cue.id === id);
  if (at < 0) return;
  const next = list.findIndex((cue, i) => i > at && cue.type === 'scene');
  selectAll(list.slice(at, next < 0 ? undefined : next).map((cue) => cue.id));
}

export function undo() {
  void edit({ op: 'undo' });
}

export function redo() {
  void edit({ op: 'redo' });
}

export function renumber() {
  void edit({ op: 'renumber' });
}

/** What grouping offers for the cues (default: the selection): a single group is dissolved, anything else
 * grouped. `enabled` is false with nothing selected or a scene marker or group among several (groups don't nest). */
export function groupAction(ids: CueId[] = selection()): { ungroup: boolean; enabled: boolean } {
  const picked = allCues().filter((cue) => ids.includes(cue.id));
  if (picked.length === 1 && picked[0].type === 'group') return { ungroup: true, enabled: true };
  const enabled = picked.length > 0 && picked.every((cue) => cue.type !== 'group' && cue.type !== 'scene');
  return { ungroup: false, enabled };
}

/** Groups the cues (default: the selection) or dissolves the single group, see `groupAction`; what comes out
 * gets selected. */
export async function toggleGroup(ids: CueId[] = selection()) {
  const { ungroup, enabled } = groupAction(ids);
  if (!enabled) return;
  if (ungroup) return ungroupCues(ids);
  const before = new Set(cues().map((cue) => cue.id));
  const view = await edit({ op: 'group', ids });
  const group = view?.show.cues.find((cue) => !before.has(cue.id));
  if (!group) return;
  select(group.id);
  void revealCue(group.id, 'smooth');
}

/** Dissolves the groups (default: the selected ones) into their cues, which then get selected. */
export async function ungroupCues(ids: CueId[] = selection()) {
  const groups = cues().filter((cue): cue is GroupCue => cue.type === 'group' && ids.includes(cue.id));
  if (!groups.length) return;
  const freed = groups.flatMap((group) => group.cues.map((cue) => cue.id));
  if (!(await edit({ op: 'ungroup', ids: groups.map((group) => group.id) }))) return;
  selectAll(freed);
  if (freed.length) void revealCue(freed[0], 'smooth');
}

// --- Adding cues

/** Where a new cue goes: the main list or a group, at `index` within it. */
export interface Place {
  index: number;
  group?: CueId;
}

/** Where new cues go in the main list: after the cue in the inspector, else at the end. */
export function placeAfterSelection(): Place {
  const at = cues().findIndex((cue) => cue.id === ui.selected);
  return { index: at >= 0 ? at + 1 : cues().length };
}

/** Where new cues go in the main list: before the cue in the inspector, else at the end. */
export function placeBeforeSelection(): Place {
  const at = cues().findIndex((cue) => cue.id === ui.selected);
  return { index: at >= 0 ? at : cues().length };
}

/**
 * Adds a cue of `item`'s kind at `place` and returns the id of the (first) new
 * cue; `undefined` if nothing was added (picker cancelled, failure). Sound and
 * playlist cues ask for files first. A control cue in the main list acts on
 * the selected cues if several are, else on the entry before it. No group goes
 * into a group.
 */
export async function addCue(item: AddItem, place: Place = placeAfterSelection()): Promise<CueId | undefined> {
  const op = await addOp(item, place);
  if (!op) return undefined;
  // Taken after the file picker, which can take long: the show may have changed meanwhile.
  const listOf = (list: Cue[]) => (place.group ? findGroup(list, place.group)?.cues : list) ?? [];
  const before = listOf(cues());
  const view = await edit(op);
  if (!view) return undefined;
  return firstAdded(before, listOf(view.show.cues));
}

/** Adds a cue to the main list (default: after the selection), selects it and scrolls it into view. */
export async function addCueAndSelect(item: AddItem, place: Place = placeAfterSelection()) {
  const id = await addCue(item, place);
  if (!id) return;
  select(id);
  void revealCue(id, 'smooth');
}

async function addOp(item: AddItem, { index, group }: Place): Promise<EditOp | null> {
  switch (item.kind) {
    case 'sound':
    case 'playlist': {
      const picked = ui.view && (await pickAudio(ui.view));
      if (!picked) return null;
      return { op: item.kind === 'sound' ? 'add_sounds' : 'add_playlist', index, group, ...picked };
    }
    case 'group':
    case 'scene':
      return group ? null : { op: `add_${item.kind}`, index };
    case 'placeholder':
    case 'wait':
    case 'goto':
    case 'osc':
    case 'midi':
      return { op: `add_${item.kind}`, index, group };
    default: {
      const marked = selection();
      const targets = !group && marked.length > 1 ? marked : null;
      return { op: 'add_control', kind: item.kind, index, targets, group };
    }
  }
}

/** The group `id`, also nested. */
export function findGroup(list: Cue[], id: CueId): GroupCue | undefined {
  for (const cue of list) {
    if (cue.type !== 'group') continue;
    if (cue.id === id) return cue;
    const inner = findGroup(cue.cues, id);
    if (inner) return inner;
  }
}

/** Opens the dialog for a group's cues. */
export function editGroup(id: CueId) {
  if (cueById(id)?.type === 'group') ui.groupOpen = id;
}

/** Whether the waveform dialog can show this cue: a sound cue whose file is there. */
export function canOpenWaveform(cue: Cue | undefined): boolean {
  if (cue?.type !== 'sound' || isPlaceholder(cue)) return false;
  const info = ui.view?.cue_info[cue.id];
  return !info?.missing && !info?.importing;
}

/** Opens the big waveform of a sound cue. */
export function openWaveform(id: CueId) {
  if (canOpenWaveform(cueById(id))) ui.waveformOpen = id;
}

/** Picks files and appends them to a playlist cue. */
export async function addToPlaylist(id: CueId) {
  const picked = ui.view && (await pickAudio(ui.view));
  if (picked) void edit({ op: 'add_to_playlist', id, ...picked });
}

/** Another file for this sound cue only (other cues using the old file keep it); asks whether the title follows.
 * A placeholder's default title follows without asking. */
export async function replaceSoundFile(cue: Cue) {
  if (cue.type !== 'sound' || !ui.view) return;
  const picked = await pickAudio(ui.view, { multiple: false });
  if (!picked) return;
  const { paths, ...storage } = picked;
  const path = paths[0];
  // Same stem as the backend: file name without extension.
  const name = (path.split(/[\\/]/).at(-1) ?? '').replace(/\.[^.]*$/, '');
  let retitle = isPlaceholder(cue) && (cue.title === '' || translations('cue.placeholder_title').includes(cue.title));
  if (!retitle && name !== cue.title) {
    const answer = await ask(t('commands.retitle_title'), t('commands.retitle_text', { title: cue.title, name }), [
      { label: t('common.cancel'), value: 'cancel' },
      { label: t('commands.keep_title'), value: 'keep', focus: true },
      { label: t('commands.take_name'), value: 'retitle' },
    ]);
    if (answer === 'cancel') return;
    retitle = answer === 'retitle';
  }
  void edit({ op: 'set_file', id: cue.id, path, retitle, ...storage });
}

/** Another file for one item of a playlist (other cues using the old file keep it). */
export async function replacePlaylistFile(cue: Cue, item: number) {
  if (cue.type !== 'playlist' || !ui.view) return;
  const picked = await pickAudio(ui.view, { multiple: false });
  if (!picked) return;
  const { paths, ...storage } = picked;
  void edit({ op: 'set_file', id: cue.id, path: paths[0], item, ...storage });
}

// --- Show-wide

/** Switches between edit and show mode (a machine setting). */
export function setMode(mode: Mode) {
  void attempt(api.setShowMode(mode === 'show'));
  // Leave fields so the GO keys are not swallowed by a focused input.
  (document.activeElement as HTMLElement | null)?.blur();
}

/**
 * Sets the show's level limits. Cues outside them are named first and only
 * brought in once confirmed; false if nothing changed (cancelled, refused).
 */
export async function setLevelLimits(next: LevelLimits): Promise<boolean> {
  const changes = await attempt(api.checkLevelLimits(next));
  if (!changes) return false;
  const names = (ids: string[]) => {
    const labels = ids.map((id) => {
      const cue = cueById(id);
      return cue ? cueLabel(cue) : '?';
    });
    return labels.length > 6 ? `${labels.slice(0, 6).join(', ')} … (${labels.length})` : labels.join(', ');
  };
  const lines = [
    changes.capped.length && t('commands.limits_capped', { level: fmtDb(next.max_db), cues: names(changes.capped) }),
    changes.silenced.length && t('commands.limits_silenced', { cues: names(changes.silenced) }),
  ].filter((line) => typeof line === 'string');
  if (lines.length) {
    const answer = await ask(t('commands.limits_title'), `${lines.join('\n')}\n\n${t('commands.limits_undoable')}`, [
      { label: t('common.cancel'), value: 'cancel', focus: true },
      { label: t('commands.limits_apply'), value: 'apply' },
    ]);
    if (answer !== 'apply') return false;
  }
  return !!(await edit({ op: 'set_level_limits', limits: next }));
}
