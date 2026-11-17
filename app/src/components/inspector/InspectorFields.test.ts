// Every field of every inspector tab, operated once for cues in many states: each one must reach the backend (an
// edit, a live update or a command) without an error.

import { fireEvent, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import type { Cue, CueId } from '../../lib/api';
import { inspectorView } from '../../lib/inspector.svelte';
import { select, selectAll, ui } from '../../lib/store.svelte';
import { backend } from '../../test/backend';
import { chord, control, goto, group, info, playlist, runtime, sound, view, wait } from '../../test/fixtures';
import { controls, operate } from '../../test/operate';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import Inspector from './Inspector.svelte';

const midi = (id: CueId, messages: object[]) => ({ ...wait(id), type: 'midi', messages }) as unknown as Cue;

const cues = (): Cue[] => [
  sound('s', {
    number: '1',
    title: 'Rain',
    vamps: [
      { start: 1, end: 2 },
      { start: 4, end: 5 },
    ],
    marks: [3],
    end: 50,
    normalize_db: -2,
    release_trigger: true,
    second_trigger: { action: 'stop' },
    hotkey: chord('KeyR'),
    midi_trigger: { kind: 'note', channel: 0, note: 60 } as never,
    others: { effect: 'duck', duck_db: -10, fade: 1, scope: 'all' },
    link: 'L1',
    advance: { start_next: true, at: { at: 'after', secs: 2 } },
    passes: 3,
  }),
  sound('s2', {
    number: '2',
    title: 'Rain copy',
    link: 'L1',
    second_trigger: { action: 'ignore' },
    release_trigger: true,
  }),
  sound('ph', { file: '', release_trigger: true, second_trigger: { action: 'restart' } as never }),
  playlist('p', ['a.wav', 'dir/b.wav', 'c.wav'], {
    items: [
      { file: 'a.wav', start: 1, gain_db: -3, preserve_pitch: true },
      { file: 'dir/b.wav', preserve_pitch: false, rate: 90 },
      { file: 'c.wav' },
    ],
  } as never),
  group('g', [sound('g1', { number: '9', title: 'Inner' })]),
  group('ge', []),
  goto('j', 'g1'),
  goto('jx', 'gone'),
  { ...wait('osc'), type: 'osc', device: null, messages: [{ address: '/go', args: [] }] } as unknown as Cue,
  midi('mcc', [
    { kind: 'control_change', channel: 0, controller: 7, value: 100 },
    { kind: 'note_on', channel: 1, note: 60, velocity: 90 },
    { kind: 'note_off', channel: 1, note: 60 },
    { kind: 'msc', device: 127, command: 'go', cue: '1', list: '' },
  ]),
  control('cs', 'start_stop', { targets: ['s'] }),
  control('cv', 'volume', { targets: { output: 'main' } }),
  control('cn', 'pan', { pan: -0.5, duration: 1, targets: 'all' }),
  control('cr', 'rate', { rate: 80, transpose: 1, duration: 2 }),
  control('cpa', 'position', { to: { to: 'absolute', secs: 3 } }),
  control('cpr', 'position', { to: { to: 'relative', secs: -2 } }),
  control('cpm', 'position', { to: { to: 'mark', index: 1 } }),
  control('ce', 'exit_loop', { advance: { start_next: true, at: { at: 'loop_exit', of: 's' } } }),
  control('cw', 'stopwatch', { watch: 'gone', action: 'restart' }),
  wait('w', { advance: { start_next: true, at: { at: 'end' } }, passes: 'forever' }),
];

function showView() {
  const v = view(cues(), { show: { stopwatches: [{ id: 'sw', name: 'Show' }], devices: [] } });
  v.cue_info.s = info({
    timing: [{ field: 'end', message: 'Ends after the file' } as never],
    loudness: [{ lufs: -20, peak_db: -3, suggested_db: -3 }],
    normalize_stale: true,
    level_silenced: true,
    hotkey_shared: ['s2', 'gone'],
    case_mismatch: 'Rain.WAV',
    importing: true,
    missing: true,
  });
  v.cue_info.p = info({
    missing_items: [false, true, false],
    item_timing: [[{ field: 'start', message: 'Starts after the end' } as never], [], []],
    loudness: [{ lufs: -18, peak_db: -1, suggested_db: -5 }, null, null],
  });
  v.cue_info.ph = info({ missing: false });
  v.cue_info.jx = info({ target_missing: true });
  v.cue_info.ce = info({ target_missing: true, repeat_blocked: true });
  v.cue_info.cw = info({ target_missing: true });
  return v;
}

let errors: MockInstance;
beforeEach(() => {
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
  inspectorView.tab = 'main';
});
afterEach(() => {
  expect(errors).not.toHaveBeenCalled();
  errors.mockRestore();
});

/** Whatever the inspector sent: edits, live updates and other commands. */
const sent = () => backend.calls.filter((call) => !call.cmd.startsWith('plugin:') && call.cmd !== 'get_show').length;

describe('Inspector fields', () => {
  for (const cue of cues()) {
    it(`operates every field of a ${cue.type} cue (${cue.id})`, async () => {
      await loadShow(showView(), runtime());
      select(cue.id);
      const { user, container } = await renderUi(Inspector);
      /** Operates the controls `of` returns (found anew after each change, which may re-render them). */
      const operateAll = async (of: () => HTMLElement[]) => {
        const found = of();
        for (let i = 0; i < found.length; i++) {
          await operate(user, of()[i] ?? found[i]);
          if (document.querySelector('[role="dialog"], [role="alertdialog"]')) await user.keyboard('{Escape}');
        }
        return found.length;
      };
      // The header above the tabs: number, title, script, colour, type.
      const header = () => controls(container).filter((element) => !element.closest('[role="tabpanel"]'));
      const before = sent();
      if ((await operateAll(header)) > 0) expect(sent(), 'header').toBeGreaterThan(before);
      const color = screen.queryByRole('button', { name: 'Color' });
      if (color && !color.hasAttribute('disabled')) {
        await user.click(color);
        await user.click(screen.getByRole('button', { name: 'none' }));
        await user.keyboard('{Escape}');
      }

      const tabs = screen.queryAllByRole('tab').map((tab) => tab.getAttribute('data-tab'));
      for (const tab of tabs) {
        await user.click(container.querySelector<HTMLElement>(`[data-tab="${tab}"]`)!);
        await settle();
        const panel = screen.getByRole('tabpanel');
        const before = sent();
        let count = await operateAll(() => controls(panel));
        // Each slider once more as its number field.
        for (const toggle of within(panel).queryAllByRole('button', { name: 'Enter value' })) await user.click(toggle);
        count += await operateAll(() => within(panel).queryAllByRole('spinbutton'));
        if (count > 0) expect(sent(), `tab ${tab}`).toBeGreaterThan(before);
      }
    });
  }

  it('operates the settings of each playlist item', async () => {
    await loadShow(showView(), runtime());
    select('p');
    const { user } = await renderUi(Inspector);
    for (const index of [1, 2]) {
      await user.click(screen.getByRole('button', { name: `Settings of item ${index}` }));
      await settle();
      const popover = () => [...document.querySelectorAll<HTMLElement>('[data-slot="popover-content"]')].at(-1);
      expect(popover()).toHaveTextContent(index === 1 ? 'a.wav' : 'b.wav');
      for (const element of controls(popover()!)) {
        if (!popover()) await user.click(screen.getByRole('button', { name: `Settings of item ${index}` }));
        await operate(user, element.isConnected ? element : controls(popover()!)[0]);
      }
      if (!popover()) await user.click(screen.getByRole('button', { name: `Settings of item ${index}` }));
      await settle();
      await user.click(within(popover()!).getAllByText('like playlist', { selector: 'button' }).at(-1)!);
      await user.keyboard('{Escape}');
      await settle();
    }
    const items = backend.edits.filter((op) => op.op === 'update_cue').map((op) => (op as { cue: Cue }).cue);
    expect(items.length).toBeGreaterThan(8);
  });

  it('moves, replaces, removes and adds playlist items', async () => {
    await loadShow(showView(), runtime());
    select('p');
    const { user } = await renderUi(Inspector);
    for (const name of [/Move item \d up/, /Move item \d down/, /Replace item/, /Remove item/]) {
      for (const button of screen.queryAllByRole('button', { name })) await user.click(button);
    }
    await user.click(screen.getByRole('button', { name: '+ Item' }));
    await settle();
    expect(backend.edits.length).toBeGreaterThan(3);
  });

  it('removes and adds vamps, replaces the file and opens the waveform', async () => {
    await loadShow(showView(), runtime());
    select('s');
    const { user } = await renderUi(Inspector);
    await user.click(screen.getByRole('button', { name: 'Remove vamp 2' }));
    await user.click(screen.getByRole('button', { name: '+ Vamp' }));
    await user.click(screen.getByRole('button', { name: 'Replace …' }));
    await settle();
    expect(backend.edits.filter((op) => op.op === 'update_cue')).toHaveLength(2);
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
  });

  it('unlinks linked cues', async () => {
    await loadShow(showView(), runtime());
    select('s');
    const { user } = await renderUi(Inspector);
    expect(screen.getByText(/Linked with/, { exact: false })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Unlink' }));
    await settle();
    expect(backend.edits).toContainEqual({ op: 'unlink_cues', ids: ['s'] });
  });

  it('removes a MIDI trigger', async () => {
    await loadShow(showView(), runtime());
    select('s');
    inspectorView.tab = 'trigger';
    const { user } = await renderUi(Inspector);
    await user.click(screen.getByRole('button', { name: 'Remove MIDI trigger' }));
    await settle();
    expect(backend.edits.at(-1)).toMatchObject({ op: 'update_cue', cue: { id: 's', midi_trigger: null } });
  });

  it('learns a MIDI trigger, and cancels learning when it goes away', async () => {
    const withPort = () => {
      const v = showView();
      v.midi.port = 'Keys';
      return v;
    };
    await loadShow(withPort(), runtime());
    select('s2');
    inspectorView.tab = 'trigger';
    const { user, unmount } = await renderUi(Inspector);
    await user.click(screen.getByRole('button', { name: 'Learn' }));
    await settle();
    expect(backend.invoked('midi_learn').at(-1)).toEqual({ target: { kind: 'cue', cue: 's2' } });
    const learning = withPort();
    learning.midi.learning = { kind: 'cue', cue: 's2' };
    await loadShow(learning, runtime());
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await settle();
    expect(backend.invoked('midi_learn').at(-1)).toEqual({ target: null });
    // Still learning when the inspector closes: it stops.
    unmount();
    await settle();
    expect(backend.invoked('midi_learn')).toHaveLength(3);
  });

  it('switches tabs with the arrow keys', async () => {
    await loadShow(showView(), runtime());
    select('s');
    await renderUi(Inspector);
    const first = screen.getAllByRole('tab')[0];
    first.focus();
    await fireEvent.keyDown(first, { key: 'ArrowRight' });
    expect(screen.getAllByRole('tab')[1]).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(screen.getAllByRole('tab')[1], { key: 'ArrowLeft' });
    await fireEvent.keyDown(screen.getAllByRole('tab')[0], { key: 'ArrowLeft' });
    await fireEvent.keyDown(screen.getAllByRole('tab')[0], { key: 'Enter' });
    expect(screen.getAllByRole('tab').at(-1)).toHaveAttribute('aria-selected', 'true');
  });

  it('jumps to the field of a notice', async () => {
    await loadShow(showView(), runtime());
    select('s');
    const { user } = await renderUi(Inspector);
    const notices = screen.queryAllByRole('button', { name: /→/ });
    for (const notice of notices) await user.click(notice);
    expect(notices.length).toBeGreaterThan(0);
  });

  it('edits several cues at once with one update', async () => {
    await loadShow(showView(), runtime());
    for (const ids of [
      ['s', 's2', 'ph'],
      ['cv', 'cn', 'cr'],
      ['p', 's'],
      ['j', 'w'],
    ]) {
      selectAll(ids);
      const { user, unmount } = await renderUi(Inspector);
      for (const tab of screen.queryAllByRole('tab')) {
        await user.click(tab);
        await settle();
        for (const element of controls(screen.getByRole('tabpanel')).slice(0, 6)) await operate(user, element);
      }
      unmount();
    }
    expect(backend.edits.filter((op) => op.op === 'update_cues').length).toBeGreaterThan(4);
  });

  it('links to the dialogs for stopwatches and devices', async () => {
    await loadShow(showView(), runtime());
    select('cw');
    const { user, unmount } = await renderUi(Inspector);
    await user.click(screen.getByRole('button', { name: 'Stopwatches …' }));
    expect(ui.stopwatchesOpen).toBe(true);
    unmount();
    select('osc');
    await renderUi(Inspector);
    await user.click(screen.getByRole('button', { name: 'Create network devices …' }));
    expect(ui.devicesOpen).toBe(true);
  });
});
