import { fireEvent, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import type { LocalSettings } from '../../lib/api';
import { ui } from '../../lib/store.svelte';
import { backend } from '../../test/backend';
import { chord, runtime, sound, view } from '../../test/fixtures';
import { controls, operate } from '../../test/operate';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import Settings from './Settings.svelte';

let errors: MockInstance;
beforeEach(() => {
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
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
      {
        id: 'card',
        name: 'Card',
        description: 'Card',
        default_config: null,
        max_channels: 8,
        buffer_range: [64, 2048],
      },
    ],
  }));
  backend.on('midi_ports', () => ({ inputs: ['Keys'], outputs: ['Synth'] }));
  backend.on('set_settings', ({ settings }) => backend.show({ ...backend.view!, settings: settings as LocalSettings }));
});
afterEach(() => {
  expect(errors).not.toHaveBeenCalled();
  vi.restoreAllMocks();
});

function showView(extra: Parameters<typeof view>[1] = {}) {
  return view([sound('a')], {
    audio_error: 'Device busy',
    audio_frames: 256,
    ...extra,
    settings: {
      buffer_frames: 512,
      outputs: { main: 1, monitor: 1 },
      midi: [{ trigger: { kind: 'note', channel: 0, note: 60 } as never, action: 'go' }],
      ...extra.settings,
    },
    show: {
      devices: [{ id: 'd', name: 'Desk', host: '10.0.0.2', port: 53000 }],
      ...extra.show,
    },
  });
}

async function open(v = showView()) {
  await loadShow(v, runtime());
  const onclose = vi.fn();
  const result = await renderUi(Settings, { onclose });
  await settle();
  const dialog = screen.getByRole('dialog');
  return { ...result, dialog, onclose };
}

const tabs = (dialog: HTMLElement) => within(dialog).getAllByRole('tab');
async function openTab(user: { click: (element: Element) => Promise<void> }, dialog: HTMLElement, id: string) {
  await user.click(dialog.querySelector(`#settings-tab-${id}`)!);
  await settle();
}
const applied = () => (backend.invoked('set_settings').at(-1) as { settings: LocalSettings } | undefined)?.settings;

describe('Settings', () => {
  it('operates every setting of every tab and applies them', async () => {
    const { dialog, user } = await open();
    for (const tab of tabs(dialog)) {
      await user.click(tab);
      await settle();
      const panel = within(dialog).getByRole('tabpanel');
      const found = controls(panel);
      for (let i = 0; i < found.length; i++) {
        const element = controls(panel)[i] ?? found[i];
        await operate(user, element);
      }
    }
    const apply = within(dialog).getByRole('button', { name: 'Apply' });
    await user.click(apply);
    await settle();
    expect(backend.invoked('set_settings').length).toBeGreaterThan(0);
  });

  it('moves between tabs with the arrow keys', async () => {
    const { dialog } = await open();
    const list = within(dialog).getByRole('tablist');
    const first = tabs(dialog)[0];
    expect(first).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(list, { key: 'ArrowDown' });
    expect(tabs(dialog)[1]).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(list, { key: 'ArrowUp' });
    await fireEvent.keyDown(list, { key: 'ArrowLeft' });
    expect(tabs(dialog).at(-1)).toHaveAttribute('aria-selected', 'true');
    await fireEvent.keyDown(list, { key: 'Tab' });
  });

  it('searches the settings, opens the first hit and clears the search with Escape', async () => {
    const { dialog, user } = await open();
    await fireEvent.keyDown(within(dialog).getByRole('tablist'), { key: 'f', ctrlKey: true });
    const search = within(dialog).getByRole('searchbox', { name: 'Search settings' });
    expect(search).toHaveFocus();
    await user.type(search, 'zzzzzz');
    expect(within(dialog).getByText('Nothing found for “zzzzzz”.')).toBeInTheDocument();
    await user.clear(search);
    await user.type(search, 'MIDI');
    expect(tabs(dialog).some((tab) => /\d/.test(tab.textContent!))).toBe(true);
    await user.keyboard('{Enter}');
    await settle();
    await user.type(search, 'x');
    await user.keyboard('{Escape}');
    expect(search).toHaveValue('');
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('cannot apply an output routed past the channels of the device', async () => {
    const { dialog, user } = await open(showView({ settings: { outputs: { main: 1, monitor: 3 } } }));
    await openTab(user, dialog, 'routing');
    const fade = within(dialog).queryAllByRole('spinbutton')[0];
    if (fade) await operate(user, fade);
    expect(within(dialog).getByRole('button', { name: 'Apply' })).toBeDisabled();
  });
});

describe('Routing tab', () => {
  it('routes the outputs and the preview to a channel pair, and warns when they overlap', async () => {
    const v = showView();
    v.audio = { ...v.audio, channels: 8, max_channels: 8 };
    const { dialog, user } = await open(v);
    await openTab(user, dialog, 'routing');
    const monitor = within(dialog).getByRole('spinbutton', { name: 'Monitor, first channel' });
    // Emptied, the field waits for the next digit; left, it shows the pair again.
    await fireEvent.input(monitor, { target: { value: '' } });
    await fireEvent.blur(monitor);
    expect(monitor).toHaveValue(1);
    await user.click(within(dialog).getByRole('combobox', { name: 'Preview, output' }));
    await user.click(screen.getByRole('option', { name: /Channel pair on the audio device/ }));
    await settle();
    const preview = within(dialog).getByRole('spinbutton', { name: 'Preview, first channel' });
    await fireEvent.input(preview, { target: { value: '2' } });
    expect(within(dialog).getByText(/the preview would be heard in the hall/)).toBeInTheDocument();
    await fireEvent.input(preview, { target: { value: '5' } });
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    expect(applied()).toMatchObject({ preview_channel: 5, preview_device: null });
  });

  it('plays the preview on a device of its own, also one not connected now', async () => {
    const v = showView({ settings: { preview_device: 'gone', preview_channel: 1 } });
    v.preview.error = 'Preview device missing';
    const { dialog, user } = await open(v);
    await openTab(user, dialog, 'routing');
    expect(within(dialog).getByText('Preview device missing')).toBeInTheDocument();
    const device = within(dialog).getByRole('combobox', { name: 'Preview, device' });
    expect(device).toHaveTextContent(/gone/);
    await user.click(device);
    await user.click(screen.getByRole('option', { name: 'Card' }));
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    expect(applied()).toMatchObject({ preview_device: 'card' });
  });

  it('says when the show has no outputs', async () => {
    const { dialog, user } = await open(showView({ show: { outputs: [] } }));
    await openTab(user, dialog, 'routing');
    expect(within(dialog).getByText(/no outputs/i)).toBeInTheDocument();
  });
});

describe('Media and MIDI tabs', () => {
  it('clears the peak cache', async () => {
    backend.on('peak_cache_size', () => 200 * 1024 * 1024);
    backend.on('clear_peak_cache', () => 1024);
    const { dialog, user } = await open();
    await openTab(user, dialog, 'media');
    await settle();
    expect(within(dialog).getByText(/of at most 500 MB/)).toHaveTextContent(/200/);
    await user.click(within(dialog).getByRole('button', { name: 'Clear peak cache' }));
    await settle();
    expect(backend.invoked('clear_peak_cache')).toHaveLength(1);
    expect(within(dialog).getByText(/of at most 500 MB/)).not.toHaveTextContent(/200/);
  });

  it('shows why MIDI ports failed, and leads to the shortcuts', async () => {
    const v = showView();
    v.midi = { ...v.midi, error: 'Input busy', out_error: 'Output busy' };
    const { dialog, user } = await open(v);
    await openTab(user, dialog, 'midi');
    expect(within(dialog).getByText(/Input busy/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Output busy/)).toBeInTheDocument();
    const msc = within(dialog).getByRole('spinbutton', { name: /MSC device ID/ });
    await fireEvent.input(msc, { target: { value: '5' } });
    await user.click(within(dialog).getByRole('combobox', { name: 'MIDI output' }));
    await user.click(screen.getByRole('option', { name: 'Synth' }));
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    expect(applied()).toMatchObject({ msc_device: 5, midi_out: 'Synth' });
    const keysTab = dialog.querySelector<HTMLElement>('#settings-tab-keys')!;
    await user.click(
      within(within(dialog).getByRole('tabpanel')).getByRole('button', { name: keysTab.textContent!.trim() }),
    );
    expect(keysTab).toHaveAttribute('aria-selected', 'true');
  });
});

describe('Shortcuts tab', () => {
  async function openKeys(v = showView()) {
    const result = await open(v);
    await openTab(result.user, result.dialog, 'keys');
    return result;
  }

  it('records a key, refuses a fixed one, and resets to the defaults', async () => {
    const { dialog, user } = await openKeys();
    const goSecond = within(dialog).getByRole('button', { name: 'GO, second key' });
    await user.click(goSecond);
    await fireEvent.keyDown(goSecond, { code: 'KeyS', key: 's', ctrlKey: true });
    await settle();
    expect(within(dialog).getByText(/reserved/)).toBeInTheDocument();
    await user.click(goSecond);
    await fireEvent.keyDown(goSecond, { code: 'KeyP', key: 'p' });
    await settle();
    // P was the preview's: it moves over, which the row says.
    expect(within(dialog).getAllByText(/Preview/).length).toBeGreaterThan(0);
    await user.click(within(dialog).getByRole('button', { name: 'GO: default keys' }));
    await user.click(within(dialog).getByRole('button', { name: 'Keys to default' }));
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    expect(applied()?.keys).toEqual([{ chord: chord('Space', { label: ' ' }), action: 'go' }]);
  });

  it('filters the actions by what is assigned or changed', async () => {
    const { dialog, user } = await openKeys();
    const all = within(dialog).getAllByRole('button', { name: /, first key$/ }).length;
    await user.click(within(dialog).getByRole('radio', { name: 'Assigned' }));
    expect(within(dialog).getAllByRole('button', { name: /, first key$/ }).length).toBeLessThan(all);
    await user.click(within(dialog).getByRole('radio', { name: 'Changed' }));
    await settle();
    expect(within(dialog).queryAllByRole('button', { name: /, first key$/ }).length).toBeLessThan(all);
  });

  it('learns and removes MIDI assignments of actions', async () => {
    const v = showView({ settings: { midi_port: 'Keys' } });
    v.midi.port = 'Keys';
    const { user, dialog } = await openKeys(v);
    await user.click(within(dialog).getByRole('button', { name: 'GO: remove MIDI assignment' }));
    await settle();
    expect(applied()?.midi).toEqual([]);
    await user.click(within(dialog).getAllByRole('button', { name: 'Learn' })[0]);
    await settle();
    expect(backend.invoked('midi_learn').at(-1)).toEqual({ target: { kind: 'action', action: expect.any(String) } });
    const learning = showView({ settings: { midi_port: 'Keys' } });
    learning.midi.port = 'Keys';
    learning.midi.learning = backend.invoked('midi_learn').at(-1)!.target as never;
    await loadShow(learning, runtime());
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await settle();
    expect(backend.invoked('midi_learn').at(-1)).toEqual({ target: null });
  });

  it('opens a MIDI input chosen in the draft before learning, and says when it fails', async () => {
    const { dialog, user } = await openKeys();
    await openTab(user, dialog, 'midi');
    await user.click(within(dialog).getByRole('combobox', { name: /MIDI input/i }));
    await user.click(screen.getByRole('option', { name: 'Keys' }));
    await openTab(user, dialog, 'keys');
    backend.on('set_settings', ({ settings }) => {
      const next = { ...backend.view!, settings: settings as LocalSettings };
      next.midi = { ...next.midi, port: null, error: 'Port busy' };
      return backend.show(next);
    });
    await user.click(within(dialog).getAllByRole('button', { name: 'Learn' })[0]);
    await settle();
    expect(applied()?.midi_port).toBe('Keys');
    expect(backend.invoked('midi_learn')).toEqual([]);
    expect(ui.notices.at(-1)?.message).toBe('Port busy');
  });
});
