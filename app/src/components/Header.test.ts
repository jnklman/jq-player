import { fireEvent, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import type { LocalSettings } from '../lib/api';
import { ui } from '../lib/store.svelte';
import { backend } from '../test/backend';
import { runtime, sound, view } from '../test/fixtures';
import { renderUi } from '../test/render';
import { loadShow, settle } from '../test/store';
import Header from './Header.svelte';
import PanelWindow from './layout/PanelWindow.svelte';
import PlayheadCue from './PlayheadCue.svelte';
import StopwatchesPanel from './StopwatchesPanel.svelte';

const showView = (extra: Parameters<typeof view>[1] = {}) =>
  view([sound('a', { number: '1', title: 'Rain', notes: 'Soft **rain**', script_ref: 'p. 4' })], {
    dirty: true,
    can_undo: true,
    can_redo: true,
    ...extra,
    show: {
      stopwatches: [
        { id: 'sw', name: 'Show' },
        { id: 'cd', name: 'Break', countdown: 600 },
      ],
      ...extra.show,
    },
  });

/** The open popover (it has no dialog role). */
async function popover(): Promise<HTMLElement> {
  await settle();
  const content = document.querySelector<HTMLElement>('[data-slot="popover-content"]');
  if (!content) throw new Error('no popover open');
  return content;
}

/** What this window sent to the main window. */
const sentToMain = () => backend.invoked('plugin:event|emit_to').map(({ event, payload }) => ({ event, payload }));

describe('Header', () => {
  it('undoes, redoes, saves and opens the settings', async () => {
    await loadShow(showView());
    const { user } = await renderUi(Header);
    await user.click(screen.getByRole('button', { name: 'Undo' }));
    await user.click(screen.getByRole('button', { name: 'Redo' }));
    await settle();
    expect(backend.edits).toEqual([{ op: 'undo' }, { op: 'redo' }]);
    await user.click(screen.getByRole('button', { name: 'Save, unsaved changes' }));
    await settle();
    expect(backend.invoked('save_show')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: 'Settings' }));
    expect(ui.settingsOpen).toBe(true);
  });

  it('has nothing to undo or redo in a fresh show', async () => {
    await loadShow(showView({ dirty: false, can_undo: false, can_redo: false }));
    await renderUi(Header);
    expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Redo' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
  });

  it('opens the about dialog and a show from the app menu', async () => {
    await loadShow(showView());
    const { user } = await renderUi(Header);
    await user.click(screen.getByRole('button', { name: 'App menu' }));
    await user.click(await screen.findByRole('menuitem', { name: /About/ }));
    expect(ui.aboutOpen).toBe(true);
    await user.click(screen.getByRole('button', { name: 'App menu' }));
    await user.click(await screen.findByRole('menuitem', { name: /^Open/ }));
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
  });

  for (const [row, flag] of [
    ['Outputs', 'outputsOpen'],
    ['Network devices', 'devicesOpen'],
    ['Stopwatches', 'stopwatchesOpen'],
  ] as const) {
    it(`opens the ${row} dialog from the show menu`, async () => {
      await loadShow(showView());
      const { user } = await renderUi(Header);
      await user.click(screen.getByRole('button', { name: 'Test Show' }));
      const menu = await popover();
      expect(menu).toHaveAttribute('aria-label', 'Show');
      await user.click(within(menu).getByRole('button', { name: new RegExp(`^${row}`) }));
      await settle();
      expect(ui[flag]).toBe(true);
    });
  }

  it('normalizes every cue from the show menu', async () => {
    await loadShow(showView());
    const { user } = await renderUi(Header);
    await user.click(screen.getByRole('button', { name: 'Test Show' }));
    await user.click(await screen.findByRole('button', { name: 'Normalize all cues' }));
    await settle();
    expect(backend.edits).toEqual([{ op: 'normalize', ids: ['a'] }]);
  });

  it('switches to show mode and back', async () => {
    await loadShow(showView());
    const { user } = await renderUi(Header);
    const modes = screen.getByRole('group', { name: 'Mode' });
    expect(within(modes).getByRole('radio', { name: 'Edit' })).toBeChecked();
    await user.click(within(modes).getByRole('radio', { name: 'Show' }));
    await settle();
    expect(backend.invoked('set_show_mode')).toEqual([{ enabled: true }]);
    await loadShow(showView({ settings: { show_mode: true } }));
    await user.click(within(screen.getByRole('group', { name: 'Mode' })).getByRole('radio', { name: 'Edit' }));
    await settle();
    expect(backend.invoked('set_show_mode')).toEqual([{ enabled: true }, { enabled: false }]);
  });

  it('changes the audio device from the status in the header', async () => {
    backend.on('audio_devices', () => ({
      hosts: ['WASAPI', 'ASIO'],
      host: 'WASAPI',
      devices: [
        {
          id: 'spk',
          name: 'Speakers',
          description: 'Speakers',
          default_config: null,
          max_channels: 2,
          buffer_range: null,
        },
        { id: 'card', name: 'Card', description: 'Card', default_config: null, max_channels: 8, buffer_range: null },
      ],
    }));
    backend.on('set_settings', ({ settings }) =>
      backend.show({ ...backend.view!, settings: settings as LocalSettings }),
    );
    await loadShow(showView());
    const { user } = await renderUi(Header);
    await user.click(screen.getByRole('button', { name: 'Audio device: Speakers' }));
    const menu = await popover();
    expect(menu).toHaveAttribute('aria-label', 'Audio device');
    const applied = () => backend.invoked('set_settings').map(({ settings }) => settings as LocalSettings);
    await user.click(within(menu).getByRole('combobox', { name: 'Device' }));
    await user.click(await screen.findByRole('option', { name: 'Card' }));
    await settle();
    expect(applied()).toEqual([expect.objectContaining({ host: null, device: 'card', channels: null })]);
    // Once a nested select closed, jsdom sees the popover as hidden (no layout), so its controls are found by label.
    await user.click(within(menu).getByLabelText('Buffer size'));
    await user.click(await screen.findByRole('option', { name: /^256/ }));
    await settle();
    expect(applied().at(-1)).toMatchObject({ device: 'card', buffer_frames: 256 });
    // Channels apply when the field is left, not on every digit.
    const channels = within(menu).getByLabelText('Channels');
    await fireEvent.input(channels, { target: { value: '4' } });
    expect(applied()).toHaveLength(2);
    await fireEvent.change(channels);
    await settle();
    expect(applied()).toHaveLength(3);
    expect(applied().at(-1)).toMatchObject({ device: 'card', channels: 4, buffer_frames: 256 });
    expect(within(menu).queryByRole('button', { name: 'Apply' })).toBeNull();
  });

  it('keeps the audio menu locked while a cue runs, and frees it once all have stopped', async () => {
    await loadShow(showView(), runtime({ waiting: [{ cue: 'a', remaining: 2, paused: false, root: 'a' }] }));
    const { user } = await renderUi(Header);
    await user.click(screen.getByRole('button', { name: 'Audio device: Speakers' }));
    const menu = await popover();
    expect(within(menu).getByText(/Cues are running/)).toBeInTheDocument();
    for (const name of ['Host', 'Device', 'Buffer size', 'Channels'])
      expect(within(menu).getByLabelText(name)).toBeDisabled();
    await backend.state(runtime());
    expect(within(menu).queryByText(/Cues are running/)).toBeNull();
    for (const name of ['Host', 'Device', 'Channels']) expect(within(menu).getByLabelText(name)).toBeEnabled();
  });

  it('opens a failed device again from the audio menu', async () => {
    await loadShow(showView({ audio_error: 'Device busy' }));
    const { user } = await renderUi(Header);
    await user.click(screen.getByRole('button', { name: 'Audio device: Speakers' }));
    const menu = await popover();
    expect(within(menu).getByText('Device not available: Device busy')).toBeInTheDocument();
    await user.click(within(menu).getByRole('button', { name: 'Try again' }));
    await settle();
    expect(backend.invoked('set_settings')).toHaveLength(1);
  });

  it('opens the device settings from the audio menu', async () => {
    await loadShow(showView());
    const { user } = await renderUi(Header);
    await user.click(screen.getByRole('button', { name: 'Audio device: Speakers' }));
    await user.click(within(await popover()).getByRole('button', { name: 'All audio settings …' }));
    expect(ui.settingsOpen).toBe(true);
  });

  it('has no mode switch while show mode is locked', async () => {
    await loadShow(showView({ settings: { show_mode: true }, show: { show_mode_locked: true } }));
    await renderUi(Header);
    expect(screen.queryByRole('group', { name: 'Mode' })).toBeNull();
  });
});

describe('PlayheadCue', () => {
  it('shows the playhead cue with its details, and the end of the list', async () => {
    await loadShow(showView(), runtime({ playhead: 'a' }));
    await renderUi(PlayheadCue);
    expect(screen.getByText('Rain')).toBeInTheDocument();
    expect(screen.getByText('p. 4')).toBeInTheDocument();
    await backend.state(runtime());
    expect(screen.getByText('End of the cue list.')).toBeInTheDocument();
  });
});

describe('StopwatchesPanel', () => {
  it('stops a running stopwatch, starts a standing one, restarts and resets', async () => {
    await loadShow(
      showView(),
      runtime({
        stopwatches: [
          { id: 'sw', elapsed: 65, remaining: null, running: true },
          { id: 'cd', elapsed: 0, remaining: 600, running: false },
        ],
      }),
    );
    const { user } = await renderUi(StopwatchesPanel);
    expect(screen.getByText('1:05')).toBeInTheDocument();
    expect(screen.getByText('10:00')).toBeInTheDocument();
    // One running (Stop), one standing (Start); Restart and Reset on each, in order.
    expect(screen.getAllByRole('button', { name: /^Stop$/ })).toHaveLength(1);
    expect(screen.getAllByRole('button', { name: /^Start$/ })).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: /^Stop$/ }));
    await user.click(screen.getByRole('button', { name: /^Start$/ }));
    await user.click(screen.getAllByRole('button', { name: 'Restart' })[0]);
    await user.click(screen.getAllByRole('button', { name: /Reset/ })[1]);
    await settle();
    expect(backend.invoked('stopwatch')).toEqual([
      { id: 'sw', action: 'stop' },
      { id: 'cd', action: 'start' },
      { id: 'sw', action: 'restart' },
      { id: 'cd', action: 'reset' },
    ]);
  });

  it('offers to create stopwatches when the show has none', async () => {
    await loadShow(showView({ show: { stopwatches: [] } }));
    const { user } = await renderUi(StopwatchesPanel);
    expect(screen.getByText(/The show has no stopwatches/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Create stopwatches' }));
    expect(ui.stopwatchesOpen).toBe(true);
  });
});

describe('PanelWindow', () => {
  it('shows its panel with the window controls', async () => {
    await loadShow(showView(), runtime({ playhead: 'a' }));
    const { user } = await renderUi(PanelWindow, { id: 'transport' });
    expect(screen.getByRole('button', { name: /^GO/ })).toBeInTheDocument();
    const controls = screen.getByRole('group', { name: 'Window' });
    await user.click(within(controls).getByRole('button', { name: 'Minimize' }));
    await user.click(within(controls).getByRole('button', { name: 'Maximize' }));
    await user.click(within(controls).getByRole('button', { name: 'Close' }));
    await settle();
    expect(backend.invoked('plugin:window|minimize')).toHaveLength(1);
    expect(backend.invoked('plugin:window|toggle_maximize')).toHaveLength(1);
    expect(backend.invoked('plugin:window|close')).toHaveLength(1);
  });

  it('turns into an overlay or goes back to the main window', async () => {
    await loadShow(showView(), runtime({ playhead: 'a' }));
    const { user } = await renderUi(PanelWindow, { id: 'transport' });
    const openMenu = async () => {
      const trigger = screen
        .getAllByRole('button', { name: 'Window' })
        .find((button) => button.closest('[role="group"]') === null)!;
      await user.click(trigger);
      return popover();
    };
    await user.click(within(await openMenu()).getByRole('radio', { name: 'Overlay' }));
    await settle();
    expect(sentToMain().at(-1)).toEqual({
      event: 'ui_panel_window',
      payload: { panel: 'transport', placement: 'overlay' },
    });
    const menu = document.querySelector<HTMLElement>('[data-slot="popover-content"]') ?? (await openMenu());
    await user.click(within(menu).getByRole('button', { name: 'Back to the main window' }));
    await settle();
    expect(sentToMain().at(-1)).toEqual({
      event: 'ui_panel_window',
      payload: { panel: 'transport', placement: 'main' },
    });
  });
});
