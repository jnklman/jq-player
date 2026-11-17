import { screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import type { Cue, EditOp } from '../../lib/api';
import { inspectorView } from '../../lib/inspector.svelte';
import { select, selectAll, ui } from '../../lib/store.svelte';
import { backend } from '../../test/backend';
import { chord, control, goto, group, playlist, runtime, scene, sound, view, wait } from '../../test/fixtures';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import Inspector from './Inspector.svelte';

const osc = {
  ...wait('osc'),
  type: 'osc',
  device: 'd',
  messages: [{ address: '/go', args: [{ int: 1 }, { string: 'x y' }] }],
} as unknown as Cue;
const midi = {
  ...wait('midi'),
  type: 'midi',
  messages: [
    { kind: 'program_change', channel: 0, program: 1 },
    { kind: 'msc', device: 127, command: 'go', cue: '1', list: '' },
  ],
} as unknown as Cue;

const all = (): Cue[] => [
  sound('s', {
    number: '1',
    title: 'Rain',
    vamps: [{ start: 1, end: 2 }],
    marks: [3],
    fade_in: 1,
    end_fade: true,
    fade_out: 2,
    hotkey: chord('KeyR'),
    rate: 90,
    normalize_db: -2,
    others: { effect: 'duck', duck_db: -10, fade: 1, scope: 'all' },
    second_trigger: { action: 'exit_loop', exit: { mode: 'loop_end', level: 'innermost', stop: false } },
  }),
  sound('ph', { file: '' }),
  playlist('p', ['a.wav', 'b.wav'], { shuffle: 'per_pass', repeat: true, gap: 1 }),
  group('g', [sound('g1'), wait('g2')], { mode: 'sequence' }),
  wait('w', { advance: { start_next: true, at: { at: 'after', secs: 2 } }, passes: 'forever', pre_wait: 1 }),
  goto('j', 's', { start: true }),
  scene('sc', { notes: '**Act** one' }),
  osc,
  midi,
  control('cs', 'start_stop', { targets: ['s'], from: 5 }),
  control('cp', 'pause', { targets: 'all' }),
  control('cv', 'volume', { targets: { output: 'main' } }),
  control('cn', 'pan', { pan: -0.5, duration: 1 }),
  control('cr', 'rate', { rate: 80, transpose: 1, duration: 2 }),
  control('cpos', 'position', { to: { to: 'mark', index: 1 } }),
  control('ce', 'exit_loop', { targets: ['s'], advance: { start_next: true, at: { at: 'loop_exit', of: 's' } } }),
  control('cres', 'reset', { duration: 1 }),
  control('cw', 'stopwatch', { watch: 'sw', action: 'restart' }),
];

function showView(extra: Parameters<typeof view>[1] = {}) {
  const v = view(all(), {
    ...extra,
    show: {
      devices: [{ id: 'd', name: 'Desk', host: '10.0.0.2', port: 53000 }],
      stopwatches: [{ id: 'sw', name: 'Show', countdown: 60 }],
      ...extra.show,
    },
  });
  v.cue_info.s = { ...v.cue_info.s, problems: ['Unknown output'], level_capped: true };
  v.cue_info.s.loudness = [{ lufs: -20, peak_db: -3, suggested_db: -3 }];
  v.cue_info.p.loudness = [null, { lufs: -18, peak_db: -1, suggested_db: -5 }];
  v.cue_info.j = { ...v.cue_info.s, problems: [], target_missing: true, level_capped: false };
  return v;
}

let errors: MockInstance;
beforeEach(() => {
  // The inspector catches a failing field and logs it; any such error fails the test.
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
  inspectorView.tab = 'main';
});
afterEach(() => {
  expect(errors).not.toHaveBeenCalled();
  errors.mockRestore();
});

describe('Inspector', () => {
  it('asks for a cue without a selection', async () => {
    await loadShow(showView());
    await renderUi(Inspector);
    expect(screen.getByText(/Click a cue/i)).toBeInTheDocument();
  });

  for (const cue of all()) {
    it(`shows every tab of a ${cue.type} cue (${cue.id})`, async () => {
      await loadShow(showView(), runtime());
      select(cue.id);
      const { user } = await renderUi(Inspector);
      const tabs = screen.queryAllByRole('tab');
      expect(tabs.length).toBeGreaterThan(0);
      for (const tab of tabs) {
        await user.click(tab);
        await settle();
        expect(tab).toHaveAttribute('aria-selected', 'true');
      }
    });
  }

  for (const cue of all()) {
    it(`shows a ${cue.type} cue (${cue.id}) locked in show mode`, async () => {
      await loadShow(showView({ settings: { show_mode: true } }));
      select(cue.id);
      const { user } = await renderUi(Inspector);
      for (const tab of screen.queryAllByRole('tab')) await user.click(tab);
      expect(screen.getByText('locked')).toBeInTheDocument();
    });
  }

  it('edits several cues of a kind at once', async () => {
    for (const ids of [
      ['s', 'p'],
      ['w', 'g'],
      ['cv', 'cn'],
      ['cv', 'cres'],
      ['s', 'cv'],
    ]) {
      await loadShow(showView());
      selectAll(ids);
      const { user, unmount } = await renderUi(Inspector);
      for (const tab of screen.queryAllByRole('tab')) await user.click(tab);
      unmount();
    }
  });

  it('writes a changed title as an edit of the cue', async () => {
    await loadShow(showView());
    select('s');
    const { user } = await renderUi(Inspector);
    const title = screen.getByRole('textbox', { name: 'Title' });
    await user.clear(title);
    await user.type(title, 'Storm');
    await user.tab();
    await settle();
    const update = backend.edits.find((op) => op.op === 'update_cue') as Extract<EditOp, { op: 'update_cue' }>;
    expect(update.cue).toMatchObject({ id: 's', title: 'Storm' });
  });

  it('previews a sound cue and stops its preview', async () => {
    await loadShow(showView(), runtime());
    select('s');
    const { user } = await renderUi(Inspector);
    await user.click(screen.getByRole('button', { name: /Preview/ }));
    expect(backend.actions).toEqual([{ action: 'preview', cue: 's' }]);
    await backend.state(runtime({ preview: { cue: 's', playing: true, elapsed: 1, progress: 0.1, position: 1 } }));
    expect(screen.getByRole('button', { name: /Stop/ })).toHaveAttribute('aria-pressed', 'true');
  });

  it('shows a fixed cue when given one, whatever is selected', async () => {
    await loadShow(showView());
    select('s');
    await renderUi(Inspector, { cueId: 'w' });
    expect(screen.getByRole('textbox', { name: 'Title' })).toHaveValue('w');
    expect(ui.selected).toBe('s');
  });
});
