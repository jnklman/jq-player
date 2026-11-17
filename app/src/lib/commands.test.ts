import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { group, playlist, scene, sound, view, wait } from '../test/fixtures';
import { loadShow, settle } from '../test/store';
import type { Cue, EditOp } from './api';
import {
  addCue,
  addCueAndSelect,
  addToPlaylist,
  canOpenWaveform,
  claimUndo,
  copyCues,
  cutCues,
  deleteCues,
  duplicateCue,
  editGroup,
  findGroup,
  focusGoto,
  groupAction,
  linkCues,
  linkSource,
  openWaveform,
  pasteCues,
  pasteOverCues,
  placeAfterSelection,
  placeBeforeSelection,
  redo,
  renumber,
  replacePlaylistFile,
  replaceSoundFile,
  selectEveryCue,
  selectScene,
  setLevelLimits,
  setMode,
  toggleGroup,
  undo,
  ungroupCues,
  unlinkCues,
} from './commands';
import { select, selectAll, selection, ui } from './store.svelte';

const abc = () => [sound('a', { number: '1' }), wait('b', { number: '2' }), sound('c', { number: '3' })];

/** Answers each edit with the cues `cues` returns for it. */
function editsTo(cues: (op: EditOp) => Cue[] | undefined) {
  backend.onEdit((op, current) => {
    const next = cues(op);
    return next && current ? { ...current, show: { ...current.show, cues: next } } : undefined;
  });
}

/** The file dialog picks `paths` (inside the media folder, so nothing is copied). */
function pick(...paths: string[]) {
  backend.on('plugin:dialog|open', ({ options }) =>
    (options as { multiple?: boolean }).multiple ? paths : (paths[0] ?? null),
  );
}

describe('clipboard', () => {
  it('copies the selection, and nothing without one', async () => {
    await loadShow(view(abc()));
    copyCues();
    selectAll(['a', 'c']);
    copyCues();
    await settle();
    expect(backend.edits).toEqual([{ op: 'copy', ids: ['a', 'c'] }]);
  });

  it('selects the cue taking the place of the first deleted one', async () => {
    await loadShow(view(abc()));
    editsTo(() => [sound('a'), sound('c')]);
    selectAll(['b']);
    await deleteCues();
    expect(backend.edits).toEqual([{ op: 'delete', ids: ['b'] }]);
    expect(ui.selected).toBe('c');

    editsTo(() => [sound('a')]);
    await cutCues(['c']);
    expect(backend.edits.at(-1)).toEqual({ op: 'cut', ids: ['c'] });
    expect(ui.selected).toBe('a');

    await deleteCues([]);
    expect(backend.edits).toHaveLength(2);
  });

  it('keeps the selection when deleting inside a group', async () => {
    await loadShow(view([sound('a'), group('g', [wait('g1'), wait('g2')])]));
    select('a');
    editsTo(() => [sound('a'), group('g', [wait('g2')])]);
    await deleteCues(['g1']);
    expect(ui.selected).toBe('a');
  });

  it('pastes after the selection and selects what was pasted', async () => {
    await loadShow(view(abc()));
    select('a');
    editsTo(() => [sound('a'), sound('a2'), sound('a3'), ...abc().slice(1)]);
    await pasteCues(true);
    expect(backend.edits).toEqual([{ op: 'paste', after: 'a', linked: true }]);
    expect(selection()).toEqual(['a2', 'a3']);
  });

  it('pastes over the selection', async () => {
    await loadShow(view(abc()));
    selectAll(['b']);
    editsTo(() => [sound('a'), sound('x'), sound('c')]);
    await pasteOverCues(undefined, true);
    expect(backend.edits).toEqual([{ op: 'paste_replace', ids: ['b'], swap: true }]);
    expect(selection()).toEqual(['x']);
    expect(pasteOverCues([])).toBeUndefined();
  });

  it('does not change the selection when a paste fails', async () => {
    await loadShow(view(abc()));
    select('a');
    backend.fail('edit');
    await pasteCues();
    expect(ui.selected).toBe('a');
  });
});

describe('duplicate and link', () => {
  it('selects a copy in the main list and returns it', async () => {
    await loadShow(view(abc()));
    select('a');
    editsTo(() => [sound('a'), sound('a-copy'), ...abc().slice(1)]);
    expect(await duplicateCue()).toBe('a-copy');
    expect(backend.edits).toEqual([{ op: 'duplicate', id: 'a', linked: false }]);
    expect(ui.selected).toBe('a-copy');
  });

  it('leaves the selection for a copy inside a group', async () => {
    await loadShow(view([sound('a'), group('g', [wait('g1')])]));
    select('a');
    editsTo(() => [sound('a'), group('g', [wait('g1'), wait('g1-copy')])]);
    expect(await duplicateCue('g1', true)).toBe('g1-copy');
    expect(ui.selected).toBe('a');
    expect(await duplicateCue(null)).toBeUndefined();
  });

  it('offers linking only for several cues of one type', async () => {
    await loadShow(view([...abc(), sound('d'), group('g', []), scene('s')]));
    select('a');
    expect(linkSource()).toBeUndefined();
    selectAll(['a', 'c', 'd']);
    expect(linkSource()?.id).toBe('a');
    selectAll(['a', 'b']);
    expect(linkSource()).toBeUndefined();
    selectAll(['g', 'a']);
    expect(linkSource()).toBeUndefined();
  });

  it('links and unlinks the selection', async () => {
    await loadShow(view(abc()));
    selectAll(['a', 'c']);
    linkCues('a');
    unlinkCues();
    unlinkCues([]);
    await settle();
    expect(backend.edits).toEqual([
      { op: 'link_cues', source: 'a', ids: ['a', 'c'] },
      { op: 'unlink_cues', ids: ['a', 'c'] },
    ]);
  });
});

describe('selection', () => {
  it('selects every cue and a scene with its cues', async () => {
    await loadShow(view([scene('s1'), sound('a'), scene('s2'), sound('b'), wait('c')]));
    selectEveryCue();
    expect(selection()).toHaveLength(5);
    selectScene('s1');
    expect(selection()).toEqual(['s1', 'a']);
    selectScene('s2');
    expect(selection()).toEqual(['s2', 'b', 'c']);
    selectScene('nope');
    expect(selection()).toEqual(['s2', 'b', 'c']);
  });

  it('focuses the "Go to" field with its text selected', () => {
    expect(focusGoto()).toBe(false);
    document.body.innerHTML = '<input data-goto-field value="12" />';
    const field = document.querySelector('input')!;
    expect(focusGoto()).toBe(true);
    expect(document.activeElement).toBe(field);
    expect(field.selectionStart).toBe(0);
    expect(field.selectionEnd).toBe(2);
  });
});

describe('undo and redo', () => {
  it('act on the show without a dialog', async () => {
    await loadShow(view(abc()));
    undo();
    redo();
    renumber();
    await settle();
    expect(backend.edits).toEqual([{ op: 'undo' }, { op: 'redo' }, { op: 'renumber' }]);
  });

  it('go to the claim of the topmost dialog, and nowhere in another dialog', async () => {
    await loadShow(view(abc()));
    document.body.innerHTML = '<div data-slot="dialog-content" id="d1"></div>';
    const claim = { owner: () => document.getElementById('d1'), undo: vi.fn(), redo: vi.fn() };
    const release = claimUndo(claim);
    undo();
    redo();
    expect(claim.undo).toHaveBeenCalledOnce();
    expect(claim.redo).toHaveBeenCalledOnce();

    document.body.insertAdjacentHTML('beforeend', '<div data-slot="alert-dialog-content"></div>');
    undo();
    expect(claim.undo).toHaveBeenCalledOnce();

    release();
    release();
    document.body.innerHTML = '<div data-slot="dialog-content" id="d1"></div>';
    undo();
    expect(claim.undo).toHaveBeenCalledOnce();
    await settle();
    expect(backend.edits).toEqual([]);
  });
});

describe('groups', () => {
  it('offers grouping for plain cues and ungrouping for one group', async () => {
    await loadShow(view([...abc(), group('g', [wait('g1')]), scene('s')]));
    expect(groupAction([])).toEqual({ ungroup: false, enabled: false });
    expect(groupAction(['a', 'b'])).toEqual({ ungroup: false, enabled: true });
    expect(groupAction(['g'])).toEqual({ ungroup: true, enabled: true });
    expect(groupAction(['g', 'a'])).toEqual({ ungroup: false, enabled: false });
    expect(groupAction(['s', 'a'])).toEqual({ ungroup: false, enabled: false });
  });

  it('groups the selection and selects the new group', async () => {
    await loadShow(view(abc()));
    selectAll(['a', 'b']);
    editsTo(() => [group('new', [sound('a'), wait('b')]), sound('c')]);
    await toggleGroup();
    expect(backend.edits).toEqual([{ op: 'group', ids: ['a', 'b'] }]);
    expect(ui.selected).toBe('new');
  });

  it('dissolves a group and selects its cues', async () => {
    await loadShow(view([group('g', [wait('g1'), wait('g2')]), sound('c')]));
    select('g');
    editsTo(() => [wait('g1'), wait('g2'), sound('c')]);
    await toggleGroup();
    expect(backend.edits).toEqual([{ op: 'ungroup', ids: ['g'] }]);
    expect(selection()).toEqual(['g1', 'g2']);
  });

  it('does nothing when nothing can be grouped or ungrouped', async () => {
    await loadShow(view([scene('s'), sound('a')]));
    await toggleGroup(['s', 'a']);
    await ungroupCues(['a']);
    expect(backend.edits).toEqual([]);
  });

  it('finds nested groups and opens only groups in the dialog', async () => {
    const inner = group('inner', []);
    const cues = [sound('a'), group('outer', [inner])];
    expect(findGroup(cues, 'inner')).toBe(inner);
    expect(findGroup(cues, 'a')).toBeUndefined();
    await loadShow(view(cues));
    editGroup('a');
    expect(ui.groupOpen).toBeNull();
    editGroup('outer');
    expect(ui.groupOpen).toBe('outer');
  });
});

describe('adding cues', () => {
  it('places new cues after or before the selection, else at the end', async () => {
    await loadShow(view(abc()));
    expect(placeAfterSelection()).toEqual({ index: 3 });
    expect(placeBeforeSelection()).toEqual({ index: 3 });
    select('b');
    expect(placeAfterSelection()).toEqual({ index: 2 });
    expect(placeBeforeSelection()).toEqual({ index: 1 });
  });

  it('adds a cue and selects it', async () => {
    await loadShow(view(abc()));
    select('a');
    editsTo(() => [sound('a'), wait('new'), ...abc().slice(1)]);
    await addCueAndSelect({ label: '', kind: 'wait' });
    expect(backend.edits).toEqual([{ op: 'add_wait', index: 1, group: undefined }]);
    expect(ui.selected).toBe('new');
  });

  it('adds into a group and returns the new cue', async () => {
    await loadShow(view([group('g', [wait('g1')])]));
    editsTo(() => [group('g', [wait('g1'), wait('osc')])]);
    expect(await addCue({ label: '', kind: 'osc' }, { index: 1, group: 'g' })).toBe('osc');
    expect(backend.edits).toEqual([{ op: 'add_osc', index: 1, group: 'g' }]);
  });

  it('puts no group or scene marker into a group', async () => {
    await loadShow(view([group('g', [])]));
    expect(await addCue({ label: '', kind: 'group' }, { index: 0, group: 'g' })).toBeUndefined();
    expect(await addCue({ label: '', kind: 'scene' }, { index: 0, group: 'g' })).toBeUndefined();
    expect(backend.edits).toEqual([]);
  });

  it('aims a control cue at the selected cues, or at the entry before it', async () => {
    await loadShow(view(abc()));
    selectAll(['a', 'c']);
    await addCue({ label: '', kind: 'volume' }, { index: 3 });
    select('a');
    await addCue({ label: '', kind: 'pause' }, { index: 1 });
    expect(backend.edits).toEqual([
      { op: 'add_control', kind: 'volume', index: 3, targets: ['a', 'c'], group: undefined },
      { op: 'add_control', kind: 'pause', index: 1, targets: null, group: undefined },
    ]);
  });

  it('asks for files for sound and playlist cues', async () => {
    await loadShow(view([]));
    await addCueAndSelect({ label: '', kind: 'sound' });
    expect(backend.edits).toEqual([]);

    pick('/shows/Test Show/media/rain.wav');
    editsTo(() => [sound('rain')]);
    await addCueAndSelect({ label: '', kind: 'playlist' });
    expect(backend.edits).toEqual([
      {
        op: 'add_playlist',
        index: 0,
        group: undefined,
        paths: ['/shows/Test Show/media/rain.wav'],
        copy_outside: false,
      },
    ]);
    expect(ui.selected).toBe('rain');
    // The folder picked from is the media folder: remembered as "none".
    expect(backend.invoked('set_audio_dir')).toEqual([{ dir: null }]);
  });

  it('adds nothing when the edit fails', async () => {
    await loadShow(view([]));
    backend.fail('edit');
    expect(await addCue({ label: '', kind: 'placeholder' })).toBeUndefined();
  });

  it('appends picked files to a playlist', async () => {
    await loadShow(view([playlist('p', ['a.wav'])]));
    pick('/shows/Test Show/media/b.wav');
    await addToPlaylist('p');
    await settle();
    expect(backend.edits).toEqual([
      { op: 'add_to_playlist', id: 'p', paths: ['/shows/Test Show/media/b.wav'], copy_outside: false },
    ]);
  });
});

describe('replacing files', () => {
  it('names a placeholder after its new file without asking', async () => {
    const placeholder = sound('p', { file: '', title: 'Placeholder' });
    await loadShow(view([placeholder]));
    pick('/shows/Test Show/media/Thunder.wav');
    await replaceSoundFile(placeholder);
    await settle();
    expect(ui.question).toBeNull();
    expect(backend.edits).toEqual([
      { op: 'set_file', id: 'p', path: '/shows/Test Show/media/Thunder.wav', retitle: true, copy_outside: false },
    ]);
  });

  it('asks whether the title follows the file', async () => {
    const cue = sound('s', { title: 'Rain' });
    await loadShow(view([cue]));
    pick('/shows/Test Show/media/Storm.flac');
    const done = replaceSoundFile(cue);
    await settle();
    expect(ui.question?.text).toContain('“Rain”');
    expect(ui.question?.text).toContain('“Storm”');
    ui.question!.answer('retitle');
    await done;
    await settle();
    expect(backend.edits).toEqual([expect.objectContaining({ op: 'set_file', retitle: true })]);

    const cancelled = replaceSoundFile(cue);
    await settle();
    ui.question!.answer('cancel');
    await cancelled;
    await settle();
    expect(backend.edits).toHaveLength(1);
  });

  it('does not ask when the title already is the file name', async () => {
    const cue = sound('s', { title: 'Storm' });
    await loadShow(view([cue]));
    pick('C:\\media\\Storm.flac');
    await replaceSoundFile(cue);
    await settle();
    expect(backend.edits).toEqual([expect.objectContaining({ path: 'C:\\media\\Storm.flac', retitle: false })]);
  });

  it('replaces one playlist item; other cue types are left alone', async () => {
    const list = playlist('p', ['a.wav', 'b.wav']);
    await loadShow(view([list, wait('w')]));
    pick('/shows/Test Show/media/c.wav');
    await replacePlaylistFile(list, 1);
    await replacePlaylistFile(wait('w'), 0);
    await replaceSoundFile(wait('w'));
    await settle();
    expect(backend.edits).toEqual([
      { op: 'set_file', id: 'p', path: '/shows/Test Show/media/c.wav', item: 1, copy_outside: false },
    ]);
  });

  it('gives up when the picker is cancelled', async () => {
    const cue = sound('s');
    await loadShow(view([cue, playlist('p', ['a'])]));
    await replaceSoundFile(cue);
    await replacePlaylistFile(playlist('p', ['a']), 0);
    expect(backend.edits).toEqual([]);
  });
});

describe('waveform', () => {
  it('opens only for sound cues with their file there', async () => {
    const cues = [sound('ok'), sound('gone'), sound('copying'), sound('p', { file: '' }), wait('w')];
    const v = view(cues);
    v.cue_info.gone.missing = true;
    v.cue_info.copying.importing = true;
    await loadShow(v);
    expect(canOpenWaveform(cues[0])).toBe(true);
    expect(canOpenWaveform(cues[1])).toBe(false);
    expect(canOpenWaveform(cues[2])).toBe(false);
    expect(canOpenWaveform(cues[3])).toBe(false);
    expect(canOpenWaveform(cues[4])).toBe(false);
    expect(canOpenWaveform(undefined)).toBe(false);
    openWaveform('gone');
    expect(ui.waveformOpen).toBeNull();
    openWaveform('ok');
    expect(ui.waveformOpen).toBe('ok');
  });
});

describe('show-wide', () => {
  it('switches the mode and leaves the focused field', async () => {
    await loadShow(view());
    document.body.innerHTML = '<input />';
    const field = document.querySelector('input')!;
    field.focus();
    setMode('show');
    await settle();
    expect(backend.invoked('set_show_mode')).toEqual([{ enabled: true }]);
    expect(document.activeElement).not.toBe(field);
  });

  it('sets level limits at once when no cue is affected', async () => {
    await loadShow(view(abc()));
    expect(await setLevelLimits({ max_db: 0, min_db: -50 })).toBe(true);
    expect(backend.edits).toEqual([{ op: 'set_level_limits', limits: { max_db: 0, min_db: -50 } }]);
  });

  it('names the affected cues and applies only when confirmed', async () => {
    const many = Array.from({ length: 8 }, (_, i) => sound(`s${i}`, { number: String(i + 1) }));
    await loadShow(view(many));
    backend.on('check_level_limits', () => ({ capped: many.map((cue) => cue.id), silenced: ['s0', 'gone'] }));
    const limits = { max_db: -6, min_db: -40 };

    const cancelled = setLevelLimits(limits);
    await settle();
    expect(ui.question?.text).toContain('Lowered to −6.0 dB: 1, 2, 3, 4, 5, 6 … (8)');
    expect(ui.question?.text).toContain('Become silent: 1, ?');
    ui.question!.answer('cancel');
    expect(await cancelled).toBe(false);

    const applied = setLevelLimits(limits);
    await settle();
    ui.question!.answer('apply');
    expect(await applied).toBe(true);
    expect(backend.edits).toEqual([{ op: 'set_level_limits', limits }]);
  });

  it('reports a failed check', async () => {
    await loadShow(view());
    backend.fail('check_level_limits');
    expect(await setLevelLimits({ max_db: 0, min_db: -50 })).toBe(false);
    expect(ui.notices).toHaveLength(1);
  });
});
