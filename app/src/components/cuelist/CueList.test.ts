import { fireEvent, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import { selection, ui } from '../../lib/store.svelte';
import { backend } from '../../test/backend';
import { active, chord, control, goto, group, playlist, runtime, scene, sound, view, wait } from '../../test/fixtures';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import CueList from './CueList.svelte';

const cues = () => [
  scene('s1', { title: 'Act 1' }),
  sound('a', {
    number: '1',
    title: 'Rain',
    notes: 'Soft **start**',
    hotkey: chord('KeyR'),
    midi_trigger: { kind: 'note', channel: 0, note: 60 },
    advance: { start_next: true, at: { at: 'end' } },
    passes: 3,
    link: 'L',
    color: '#f00',
  }),
  wait('b', { number: '2', link: 'L' }),
  scene('s2', { title: 'Act 2' }),
  group('g', [sound('g1', { number: '3' })], { number: '4' }),
  playlist('p', ['x.wav', 'y.wav'], { number: '5' }),
  control('f', 'volume', { number: '6', targets: ['a'] }),
  goto('j', 'a', { number: '7' }),
  sound('ph', { number: '8', file: '' }),
];

const row = (id: string) => document.querySelector<HTMLElement>(`[data-cue="${id}"]`)!;

async function list(extra: Parameters<typeof view>[1] = {}, state = runtime({ playhead: 'b' })) {
  const v = view(cues(), extra);
  v.cue_info.a.timing = [{ kind: 'end_past_file', message: 'End past file' }];
  await loadShow(v, state);
  return renderUi(CueList);
}

describe('CueList', () => {
  it('shows every cue with its number, title and status', async () => {
    await list();
    for (const title of ['Act 1', 'Rain', 'Act 2', 'p']) expect(screen.getAllByText(title).length).toBeGreaterThan(0);
    expect(row('a')).toHaveTextContent('1');
    expect(row('a')).toHaveTextContent('1:00');
    expect(row('a')).toHaveTextContent('×3');
    expect(row('ph')).toHaveTextContent('no file');
    expect(row('f')).toHaveTextContent('→ 1 Rain');
    expect(row('j')).toHaveTextContent('→ 1 Rain');
    // The chain arrow of 1 and the row after the last cue.
    expect(screen.getAllByText('End')).toHaveLength(2);
  });

  it('says when the list is empty', async () => {
    await loadShow(view([]));
    await renderUi(CueList);
    expect(screen.getByText('No cues yet – “+ Sound” adds files.')).toBeInTheDocument();
    await loadShow(view([], { settings: { show_mode: true } }));
    await settle();
    expect(screen.getByText('No cues.')).toBeInTheDocument();
  });

  it('selects with a click, adds with Ctrl and extends with Shift while editing', async () => {
    const { user } = await list();
    await user.click(row('a'));
    expect(ui.selected).toBe('a');
    await user.keyboard('{Control>}');
    await user.click(row('g'));
    await user.keyboard('{/Control}');
    expect(selection()).toEqual(['a', 'g']);
    await user.keyboard('{Shift>}');
    await user.click(row('p'));
    await user.keyboard('{/Shift}');
    expect(selection()).toEqual(['g', 'p']);
    expect(row('p')).toHaveAttribute('aria-selected', 'true');
  });

  it('only selects in show mode, and moves the playhead on a double click', async () => {
    const { user } = await list({ settings: { show_mode: true } });
    await user.click(row('a'));
    await user.keyboard('{Control>}');
    await user.click(row('g'));
    await user.keyboard('{/Control}');
    expect(selection()).toEqual(['g']);
    await user.dblClick(row('p'));
    expect(backend.actions.at(-1)).toEqual({ action: 'set_playhead', cue: 'p' });
  });

  it('selects a scene with its cues', async () => {
    const { user } = await list();
    await user.click(within(row('s2')).getByText('Act 2'));
    expect(ui.selected).toBe('s2');
  });

  it('clears the selection on a click into empty space, unless linked', async () => {
    const { user } = await list();
    await user.click(row('a'));
    const viewport = document.querySelector('[data-slot="scroll-area-viewport"]') as HTMLElement;
    await user.click(viewport);
    expect(ui.selected).toBeNull();
  });

  it('moves the selection or the playhead with the wheel as set', async () => {
    await list({ settings: { wheel: 'selection' } });
    const viewport = document.querySelector('[data-slot="scroll-area-viewport"]') as HTMLElement;
    await fireEvent.wheel(viewport, { deltaY: 100 });
    expect(ui.selected).toBe('s1');
    await fireEvent.wheel(viewport, { deltaY: 10 });
    await fireEvent.wheel(viewport, { deltaY: 10, ctrlKey: true });
    expect(ui.selected).toBe('s1');

    await loadShow(view(cues(), { settings: { wheel: 'playhead' } }));
    await fireEvent.wheel(viewport, { deltaY: -100 });
    expect(backend.actions.at(-1)).toEqual({ action: 'playhead_prev' });
  });

  it('marks running cues and the playhead', async () => {
    await list({}, runtime({ playhead: 'p', active: [active('a', { looping: true })] }));
    expect(row('a')).toHaveTextContent('Loop');
    await backend.state(runtime({ playhead: null }));
    expect(row('a')).not.toHaveTextContent('Loop');
  });
});

describe('the toolbar', () => {
  it('adds cues after the selection', async () => {
    const { user } = await list();
    await user.click(row('a'));
    await user.click(screen.getByRole('button', { name: /^Volume/ }));
    await settle();
    expect(backend.edits.at(-1)).toMatchObject({ op: 'add_control', kind: 'volume', index: 2 });
    await user.click(screen.getByRole('button', { name: /^Sound/ }));
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
  });

  it('copies, cuts, duplicates, groups, renumbers and deletes the selection', async () => {
    const { user } = await list({ can_paste: true });
    await user.click(row('b'));
    // The fake backend answers with the list unchanged, so the order matters: "Duplicate" then selects the row after
    // the original (a scene marker, which cannot be grouped), and "Paste" selects what was pasted (nothing).
    for (const name of ['Copy', 'Group', 'Duplicate', 'Renumber', 'Cut', 'Delete', 'Paste']) {
      const [button] = screen.getAllByRole('button', { name });
      await user.click(button);
      await settle();
    }
    expect(backend.edits.map((op) => op.op)).toEqual([
      'copy',
      'group',
      'duplicate',
      'renumber',
      'cut',
      'delete',
      'paste',
    ]);
  });

  it('offers more ways to add and paste in its menus', async () => {
    const { user } = await list({ can_paste: true });
    await user.click(row('a'));
    await user.click(screen.getByRole('button', { name: 'More' }));
    await user.click(await screen.findByRole('menuitem', { name: /Wait/ }));
    await settle();
    expect(backend.edits.at(-1)).toMatchObject({ op: 'add_wait' });

    await user.click(screen.getByRole('button', { name: 'More paste options' }));
    await user.click(await screen.findByRole('menuitem', { name: /Swap/ }));
    await settle();
    expect(backend.edits.at(-1)).toMatchObject({ op: 'paste_replace', swap: true });
  });
});

describe('the context menu', () => {
  async function menuOn(id: string, extra: Parameters<typeof view>[1] = {}) {
    const result = await list(extra);
    await fireEvent.contextMenu(row(id));
    await settle();
    return result;
  }

  it('selects the cue under the pointer and acts on it', async () => {
    const { user } = await menuOn('b');
    expect(ui.selected).toBe('b');
    await user.click(screen.getByRole('menuitem', { name: /Copy/ }));
    await settle();
    expect(backend.edits).toEqual([{ op: 'copy', ids: ['b'] }]);
  });

  it('offers the group dialog for a group', async () => {
    const { user } = await menuOn('g');
    await user.click(screen.getByRole('menuitem', { name: 'Edit group …' }));
    expect(ui.groupOpen).toBe('g');
  });

  it('offers the waveform and another file for a sound cue', async () => {
    const { user } = await menuOn('a');
    await user.click(screen.getByRole('menuitem', { name: 'Edit waveform …' }));
    expect(ui.waveformOpen).toBe('a');
    await fireEvent.contextMenu(row('ph'));
    await settle();
    await user.click(screen.getByRole('menuitem', { name: 'Choose file …' }));
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
  });

  it('adds items to a playlist', async () => {
    const { user } = await menuOn('p');
    await user.click(screen.getByRole('menuitem', { name: 'Add item …' }));
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
  });

  it('selects all, renumbers and deletes', async () => {
    const { user } = await menuOn('a');
    await user.click(screen.getByRole('menuitem', { name: /Select all/ }));
    expect(selection()).toHaveLength(cues().length);
    await fireEvent.contextMenu(row('a'));
    await settle();
    await user.click(screen.getByRole('menuitem', { name: /Delete/ }));
    await settle();
    expect(backend.edits.at(-1)).toMatchObject({ op: 'delete' });
  });

  it('is not there in show mode', async () => {
    await menuOn('a', { settings: { show_mode: true } });
    expect(screen.queryByRole('menuitem')).toBeNull();
  });
});
