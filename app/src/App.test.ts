import { fireEvent, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import App from './App.svelte';
import type { EditOp, LocalSettings } from './lib/api';
import { ask, askClashes, ui } from './lib/store.svelte';
import { windowState } from './lib/window.svelte';
import { backend } from './test/backend';
import { active, chord, control, group, playlist, runtime, scene, sound, view, wait } from './test/fixtures';
import { renderUi } from './test/render';
import { loadShow, settle } from './test/store';

const show = () => [
  scene('s1', { title: 'Act 1' }),
  sound('a', { number: '1', title: 'Rain', hotkey: chord('KeyR') }),
  wait('b', { number: '2', advance: { start_next: true, at: { at: 'end' } } }),
  group('g', [sound('g1', { number: '3', title: 'Thunder' }), wait('g2', { title: 'Pause' })], { number: '4' }),
  playlist('p', ['x.wav', 'y.wav'], { number: '5' }),
  control('f', 'volume', { number: '6' }),
];

const busy = () =>
  runtime({
    playhead: 'b',
    active: [
      active('a', { pass: { pass: 1, count: 2, vamp: null }, level_db: -6, pan: -0.3 }),
      active('p', { item: { index: 0, file: 0, count: 2 }, has_next: true, paused: true }),
    ],
    gaps: [{ cue: 'g1', remaining: 2, next: { index: 1, file: 1, count: 2 }, paused: false }],
    waiting: [
      { cue: 'b', remaining: 1, paused: false, root: 'a' },
      { cue: 'g2', remaining: 3, paused: true, root: 'g2' },
    ],
    fading: [{ cue: 'f', elapsed: 1, remaining: 2, progress: 0.3, paused: false }],
    following: [{ cue: 'b', after: 'a', root: 'a', paused: false, remaining: 4 }],
    stopwatches: [{ id: 'sw', elapsed: 65, remaining: null, running: true }],
    imports: [{ file: 'x.wav', progress: 0.5 }],
  });

/** The show; cue 1's file ("a.wav") is missing. */
function showView(extra: Parameters<typeof view>[1] = {}) {
  const v = view(show(), {
    ...extra,
    show: { stopwatches: [{ id: 'sw', name: 'Show' }], ...extra.show },
  });
  v.cue_info.a.missing = true;
  return v;
}

/** The last edit op of kind `op`. */
function lastEdit<K extends EditOp['op']>(op: K): Extract<EditOp, { op: K }> | undefined {
  return backend.edits.filter((edit): edit is Extract<EditOp, { op: K }> => edit.op === op).at(-1);
}

let errors: MockInstance;
beforeEach(() => {
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  expect(errors).not.toHaveBeenCalled();
  vi.restoreAllMocks();
});

describe('App', () => {
  it('shows nothing before the first show answer, then the start screen', async () => {
    const { container } = await renderUi(App);
    expect(container.querySelector('[data-slot="panel-frame"]')).toBeNull();
    await loadShow(null);
    await settle();
    expect(container.querySelector('[data-slot="panel-frame"]')).toBeNull();
    expect(screen.getAllByRole('button').length).toBeGreaterThan(0);
  });

  it('renders the main view of a show, also while everything runs', async () => {
    await loadShow(showView(), busy());
    await renderUi(App);
    expect(screen.getAllByText('Rain').length).toBeGreaterThan(0);
    expect(screen.getByRole('region', { name: 'Transport' })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Running cues' })).toHaveTextContent('Rain');
    await backend.state(runtime({ playhead: 'a' }));
    expect(screen.getByRole('region', { name: 'Running cues' })).toHaveTextContent('Nothing active.');
  });

  it('shows the show-mode layout: the playhead cue instead of the inspector', async () => {
    await loadShow(showView({ settings: { show_mode: true } }), busy());
    await renderUi(App);
    expect(screen.getByRole('region', { name: 'Playhead cue' })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Inspector' })).toBeNull();
  });

  it('selects the playhead cue when the selection follows it', async () => {
    await loadShow(showView({ settings: { selection_mode: 'selection_follows' } }), runtime({ playhead: 'b' }));
    await renderUi(App);
    expect(ui.selected).toBe('b');
    await backend.state(runtime({ playhead: 'p' }));
    expect(ui.selected).toBe('p');
  });

  it('warns while no window of the app has the keys', async () => {
    windowState.focused = false;
    windowState.appFocused = false;
    try {
      await loadShow(showView({ settings: { focus_warning: true } }));
      await renderUi(App);
      expect(screen.getByRole('alert')).toBeInTheDocument();
      expect(document.documentElement.hasAttribute('data-focus-lost')).toBe(true);
      windowState.focused = true;
      await settle();
      expect(screen.queryByRole('alert')).toBeNull();
      expect(document.documentElement.hasAttribute('data-focus-lost')).toBe(false);
    } finally {
      windowState.focused = true;
      windowState.appFocused = true;
    }
  });
});

describe('leaving', () => {
  const check = { show_mode: true, show_mode_locked: false, dirty: true, running: 2, imports: 1 };

  async function asked(extra: Partial<typeof check> = {}) {
    await loadShow(showView());
    const rendered = await renderUi(App);
    await backend.emit('close_requested', { ...check, ...extra });
    await settle();
    return { ...rendered, dialog: screen.getByRole('alertdialog') };
  }

  it('names every reason and saves before quitting', async () => {
    const { user, dialog } = await asked();
    expect(within(dialog).getByText('Quit jQ Player?')).toBeInTheDocument();
    for (const reason of ['Show mode is active.', '2 cues are still running.', 'The show has unsaved changes.']) {
      expect(within(dialog).getByText(reason)).toBeInTheDocument();
    }
    await user.click(within(dialog).getByRole('button', { name: 'Save and quit' }));
    await settle();
    expect(backend.invoked('confirm_close')).toEqual([{ save: true }]);
    expect(ui.leaving).toBeNull();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('quits without saving when asked to', async () => {
    const { user, dialog } = await asked();
    await user.click(within(dialog).getByRole('button', { name: 'Quit without saving' }));
    await settle();
    expect(backend.invoked('confirm_close')).toEqual([{ save: false }]);
  });

  it('offers a plain quit without unsaved changes', async () => {
    const { user, dialog } = await asked({ dirty: false, show_mode: false, imports: 0, running: 1 });
    expect(within(dialog).getByText('One cue is still running.')).toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: 'Save and quit' })).toBeNull();
    await user.click(within(dialog).getByRole('button', { name: 'Quit' }));
    await settle();
    expect(backend.invoked('confirm_close')).toEqual([{ save: false }]);
  });

  it('stays on cancel', async () => {
    const { user, dialog } = await asked();
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await settle();
    expect(ui.leaving).toBeNull();
    expect(backend.invoked('confirm_close')).toEqual([]);
  });

  it('stays open and reports it when saving fails', async () => {
    backend.fail('confirm_close', 'disk full');
    const { user, dialog } = await asked();
    await user.click(within(dialog).getByRole('button', { name: 'Save and quit' }));
    await settle();
    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
    expect(ui.leaving).not.toBeNull();
    expect(screen.getByText('disk full')).toBeInTheDocument();
  });
});

describe('questions', () => {
  it('answers with the clicked choice', async () => {
    await loadShow(showView());
    const { user } = await renderUi(App);
    const answer = ask('Really?', 'Text', [
      { label: 'No', value: 'no' },
      { label: 'Yes', value: 'yes', danger: true },
    ]);
    await settle();
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Really?');
    await user.click(screen.getByRole('button', { name: 'Yes' }));
    expect(await answer).toBe('yes');
    await settle();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  describe('name clashes', () => {
    const clashes = [
      { path: '/x/a.wav', existing: 'a.wav', picked: false },
      { path: '/x/b.wav', existing: 'b.wav', picked: true },
    ];

    async function clashDialog() {
      await loadShow(showView());
      const rendered = await renderUi(App);
      const answer = askClashes(clashes);
      await settle();
      return { ...rendered, answer, dialog: screen.getByRole('alertdialog') };
    }

    it('renames every file unless chosen otherwise', async () => {
      const { user, answer, dialog } = await clashDialog();
      expect(within(dialog).getByText('the media folder already has “a.wav”')).toBeInTheDocument();
      expect(within(dialog).getByText('same name as another chosen file')).toBeInTheDocument();
      await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
      expect(await answer).toEqual({ '/x/a.wav': 'rename', '/x/b.wav': 'rename' });
    });

    it('sets a choice for all, using an existing file only where there is one', async () => {
      const { user, answer, dialog } = await clashDialog();
      await user.click(within(dialog).getByRole('button', { name: 'Use the existing one' }));
      await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
      expect(await answer).toEqual({ '/x/a.wav': 'use_existing', '/x/b.wav': 'rename' });
    });

    it('skips all, or cancels the whole pick', async () => {
      const rendered = await clashDialog();
      const { user } = rendered;
      let { answer, dialog } = rendered;
      await user.click(within(dialog).getByRole('button', { name: 'Skip' }));
      await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
      expect(await answer).toEqual({ '/x/a.wav': 'skip', '/x/b.wav': 'skip' });

      answer = askClashes(clashes);
      await settle();
      dialog = screen.getByRole('alertdialog');
      await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
      expect(await answer).toBeNull();
    });
  });

  it('restores the autosave', async () => {
    await loadShow(showView({ recovery: Date.UTC(2026, 0, 1) }));
    const { user } = await renderUi(App);
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Restore' }));
    await settle();
    expect(backend.invoked('recover_autosave')).toEqual([{ apply: true }]);
  });

  it('discards the autosave', async () => {
    await loadShow(showView({ recovery: Date.UTC(2026, 0, 1) }));
    const { user } = await renderUi(App);
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Discard' }));
    await settle();
    expect(backend.invoked('recover_autosave')).toEqual([{ apply: false }]);
  });

  it('shows notices until they are dismissed', async () => {
    await loadShow(showView());
    const { user } = await renderUi(App);
    await backend.emit('notice', { level: 'error', message: 'Device lost' });
    await backend.emit('notice', { level: 'warning', message: 'Slow disk' });
    const [first] = screen.getAllByRole('button', { name: 'Dismiss message' });
    await user.click(first);
    expect(screen.queryByText('Device lost')).toBeNull();
    expect(screen.getByText('Slow disk')).toBeInTheDocument();
  });
});

describe('dialogs', () => {
  async function open(flag: () => void, extra: Parameters<typeof view>[1] = {}) {
    await loadShow(showView(extra), runtime());
    const rendered = await renderUi(App);
    flag();
    await settle(5);
    return { ...rendered, dialog: screen.getByRole('dialog') };
  }

  const closed = async () => {
    await settle(5);
    expect(screen.queryByRole('dialog')).toBeNull();
  };

  it('adds an output and applies the list as one edit', async () => {
    const { user, dialog } = await open(() => (ui.outputsOpen = true));
    expect(within(dialog).getByRole('button', { name: 'Remove Main' })).toBeDisabled();
    await user.click(within(dialog).getByRole('button', { name: '+ Output' }));
    await user.click(within(dialog).getByRole('button', { name: 'Remove Monitor' }));
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    const outputs = lastEdit('set_outputs')!.outputs;
    expect(outputs.map((output) => output.name)).toEqual(['Main', 'Output 3']);
    expect(ui.outputsOpen).toBe(false);
    await closed();
  });

  it('adds a network device, refusing an invalid port', async () => {
    const { user, dialog } = await open(() => (ui.devicesOpen = true));
    expect(within(dialog).getByText('No devices yet.')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: '+ Device' }));
    const port = within(dialog).getByRole('spinbutton', { name: 'Port' });
    await user.clear(port);
    expect(within(dialog).getByRole('button', { name: 'Apply' })).toBeDisabled();
    await user.type(port, '53000');
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    expect(lastEdit('set_devices')!.devices).toEqual([
      { id: expect.any(String), name: 'Lighting desk', host: '192.168.1.20', port: 53000 },
    ]);
  });

  it('adds a stopwatch', async () => {
    const { user, dialog } = await open(() => (ui.stopwatchesOpen = true));
    await user.click(within(dialog).getByRole('button', { name: '+ Stopwatch' }));
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    expect(lastEdit('set_stopwatches')!.stopwatches.map((watch) => watch.name)).toEqual(['Show', 'Break']);
  });

  it('asks before throwing away a changed draft, and keeps it on "keep editing"', async () => {
    const { user, dialog } = await open(() => (ui.outputsOpen = true));
    await user.click(within(dialog).getByRole('button', { name: '+ Output' }));
    await user.click(within(dialog).getByRole('button', { name: 'Close' }));
    await settle();
    expect(ui.question?.title).toBe('Apply unsaved changes?');
    await user.click(screen.getByRole('button', { name: 'Keep editing' }));
    await settle();
    expect(ui.outputsOpen).toBe(true);

    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
    await settle();
    await user.click(screen.getByRole('button', { name: 'Discard' }));
    await settle();
    expect(ui.outputsOpen).toBe(false);
    expect(lastEdit('set_outputs')).toBeUndefined();
  });

  it('closes an unchanged draft at once, and Cancel never asks', async () => {
    const { user, dialog: first } = await open(() => (ui.devicesOpen = true));
    let dialog = first;
    await user.click(within(dialog).getByRole('button', { name: 'Close' }));
    await closed();
    expect(ui.question).toBeNull();

    ui.devicesOpen = true;
    await settle(5);
    dialog = screen.getByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: '+ Device' }));
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await closed();
    expect(ui.question).toBeNull();
    expect(backend.edits).toEqual([]);
  });

  it('applies a changed setting with the other settings as they are, and stays open', async () => {
    backend.on('set_settings', ({ settings }) =>
      backend.show({ ...backend.view!, settings: settings as LocalSettings }),
    );
    const { user, dialog } = await open(() => (ui.settingsOpen = true));
    const apply = within(dialog).getByRole('button', { name: 'Apply' });
    await user.click(within(dialog).getByRole('tab', { name: /^Flow/ }));
    await settle();
    const fadeAll = within(dialog).getByRole('spinbutton', { name: 'FADE ALL (s)' });
    await user.clear(fadeAll);
    await user.type(fadeAll, '7');
    await user.tab();
    await user.click(apply);
    await settle();
    const [{ settings }] = backend.invoked('set_settings') as { settings: LocalSettings }[];
    expect(settings.fade_all).toBe(7);
    expect(settings.panic_fade).toBe(1);
    // The applied settings are the new baseline: nothing left to apply.
    expect(ui.settingsOpen).toBe(true);
    expect(ui.view?.settings.fade_all).toBe(7);
    expect(fadeAll).toHaveValue(7);
    expect(apply).toBeDisabled();
  });

  it('keeps a setting in range: an emptied field gets its value back, a negative one is clamped', async () => {
    const { user, dialog } = await open(() => (ui.settingsOpen = true));
    await user.click(within(dialog).getByRole('tab', { name: /^Flow/ }));
    await settle();
    const fadeAll = within(dialog).getByRole('spinbutton', { name: 'FADE ALL (s)' });
    await user.clear(fadeAll);
    await user.tab();
    expect(fadeAll).toHaveValue(2);
    expect(within(dialog).getByRole('button', { name: 'Apply' })).toBeDisabled();
    // Set at once: jsdom drops a lone "-" typed into a number field.
    await fireEvent.change(fadeAll, { target: { value: '-3' } });
    await settle();
    expect(fadeAll).toHaveValue(0);
  });

  it('relinks a missing file', async () => {
    const { user, dialog } = await open(() => (ui.relinkOpen = true));
    expect(within(dialog).getByText('Used in: 1')).toBeInTheDocument();
    backend.on('plugin:dialog|open', () => '/shows/Test Show/media/rain-new.wav');
    await user.click(within(dialog).getByRole('button', { name: 'Choose file …' }));
    await settle();
    expect(lastEdit('relink_file')).toEqual({
      op: 'relink_file',
      old_path: 'a.wav',
      path: '/shows/Test Show/media/rain-new.wav',
      copy_outside: false,
    });
  });

  it('edits the cues of a group', async () => {
    const { user, dialog } = await open(() => (ui.groupOpen = 'g'));
    const cues = within(dialog).getByRole('region', { name: 'Cues of the group' });
    expect(cues).toHaveTextContent('Thunder');
    await user.click(within(dialog).getByRole('button', { name: '+ Volume' }));
    await settle();
    expect(lastEdit('add_control')).toMatchObject({ kind: 'volume', group: 'g' });

    await user.click(within(cues).getByText('Pause'));
    await user.click(within(dialog).getByRole('button', { name: 'Move up' }));
    await settle();
    expect(lastEdit('move')).toEqual({ op: 'move', ids: ['g2'], to: 0 });
    await user.click(within(dialog).getByRole('button', { name: 'Delete' }));
    await settle();
    expect(lastEdit('delete')).toEqual({ op: 'delete', ids: ['g2'] });

    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(ui.groupOpen).toBeNull();
  });

  it('trims a sound cue in the waveform and writes it on "Done"', async () => {
    backend.on('waveform', () => ({ duration: 60, peaks: Array.from({ length: 100 }, (_, i) => (i % 10) / 10) }));
    backend.on('audible_range', () => ({ start: 1.5, end: null }));
    const { user, dialog } = await open(() => (ui.waveformOpen = 'g1'));
    await user.click(within(dialog).getByRole('button', { name: /Trim start/ }));
    await settle();
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await settle();
    expect(lastEdit('update_cue')!.cue).toMatchObject({ id: 'g1', start: 1.5 });
    expect(ui.waveformOpen).toBeNull();
  });

  it('has no preview in the waveform without a preview output', async () => {
    backend.on('waveform', () => ({ duration: 60, peaks: [0.5] }));
    const { dialog } = await open(() => (ui.waveformOpen = 'g1'));
    expect(within(dialog).getByRole('button', { name: /From start/ })).toBeDisabled();
  });

  it('previews from the start in the waveform', async () => {
    backend.on('waveform', () => ({ duration: 60, peaks: [0.5] }));
    const { user, dialog } = await open(() => (ui.waveformOpen = 'g1'), { settings: { preview_channel: 3 } });
    await user.click(within(dialog).getByRole('button', { name: /From start/ }));
    expect(backend.invoked('preview_at')).toEqual([{ cue: 'g1', at: 0, until: null }]);
  });

  it('checks for an update and installs it', async () => {
    const { user, dialog } = await open(() => (ui.aboutOpen = true));
    await user.click(within(dialog).getByRole('button', { name: 'Check for updates' }));
    await settle();
    expect(within(dialog).getByText('jQ Player is up to date.')).toBeInTheDocument();

    backend.on('check_update', () => ({ version: '9.9.9', notes: null }));
    await user.click(within(dialog).getByRole('button', { name: 'Check for updates' }));
    await settle();
    expect(within(dialog).getByText('Version 9.9.9 is available.')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Install and restart' }));
    await settle();
    expect(backend.invoked('install_update')).toHaveLength(1);
  });

  it('reports a failed update check', async () => {
    backend.fail('check_update', 'offline');
    const { user, dialog } = await open(() => (ui.aboutOpen = true));
    await user.click(within(dialog).getByRole('button', { name: 'Check for updates' }));
    await settle();
    expect(within(dialog).getByText(/offline/)).toBeInTheDocument();
  });

  it('cannot apply in show mode', async () => {
    const { user, dialog } = await open(() => (ui.outputsOpen = true), { settings: { show_mode: true } });
    expect(within(dialog).getByRole('button', { name: 'Apply' })).toBeDisabled();
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await closed();
  });
});
