import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { view } from '../test/fixtures';
import { loadShow, settle } from '../test/store';
import type { LayoutState } from './layout.svelte';
import { ui } from './store.svelte';
import {
  forwardDialogs,
  isMainWindow,
  placeFromWindow,
  runInMain,
  serveDialogs,
  servePanelWindows,
  setOpacityFromWindow,
  syncSelection,
  windowPanel,
} from './windows.svelte';

/** What this window sent to the main window: event and payload. */
const sentToMain = () =>
  backend.invoked('plugin:event|emit_to').map(({ event, payload }) => ({ event, payload }) as Record<string, unknown>);

describe('this window', () => {
  it('is the main window without a panel', () => {
    expect(windowPanel).toBeUndefined();
    expect(isMainWindow).toBe(true);
  });
});

describe('selection sync', () => {
  it('sends changes once and takes those of other windows without sending them back', async () => {
    syncSelection();
    ui.selected = 'a';
    ui.marked = ['b'];
    await settle();
    const sent = () =>
      backend
        .invoked('plugin:event|emit')
        .filter((args) => args.event === 'ui_selection' && (args.payload as { from: string }).from === 'main');
    expect(sent().map((args) => args.payload)).toEqual([
      { from: 'main', selected: 'a', marked: ['b'], rangeEnd: null },
    ]);

    await backend.emit('ui_selection', { from: 'panel-inspector', selected: 'c', marked: [], rangeEnd: null });
    await settle();
    expect([ui.selected, ui.marked]).toEqual(['c', []]);
    expect(sent()).toHaveLength(1);

    // Its own messages come back on the shared bus and are left alone.
    await backend.emit('ui_selection', { from: 'main', selected: 'zzz', marked: [], rangeEnd: null });
    expect(ui.selected).toBe('c');

    // A new panel window asks the main window for the selection.
    await backend.emit('ui_selection_ask');
    expect(sent().at(-1)?.payload).toMatchObject({ selected: 'c' });
  });
});

describe('dialogs and commands across windows', () => {
  it('runs a command in the main window and brings it to the front', async () => {
    runInMain('open_show');
    await settle();
    expect(sentToMain()).toEqual([{ event: 'ui_open', payload: { command: 'open_show' } }]);
    expect(backend.invoked('focus_main')).toHaveLength(1);
  });

  it('opens what panel windows ask for in the main window', async () => {
    await loadShow(view());
    const unlisten = await serveDialogs();
    await backend.emit('ui_open', { dialog: 'groupOpen', value: 'g' });
    await backend.emit('ui_open', { dialog: 'relinkOpen', value: true });
    await backend.emit('ui_open', { dialog: 'waveformOpen', value: true });
    expect(ui.groupOpen).toBe('g');
    expect(ui.relinkOpen).toBe(true);
    expect(ui.waveformOpen).toBeNull();

    await backend.emit('ui_open', { command: 'open_show' });
    await settle();
    await backend.emit('ui_open', { command: 'new_show' });
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
    expect(backend.invoked('plugin:dialog|save')).toHaveLength(1);
    await backend.emit('ui_open', { command: 'save_show_as' });
    await settle();
    expect(backend.invoked('plugin:dialog|save')).toHaveLength(2);
    unlisten();
  });

  it('sends panel placement and opacity to the main window', async () => {
    placeFromWindow('inspector', 'main');
    setOpacityFromWindow('inspector', 0.5);
    await settle();
    expect(sentToMain().map((args) => args.payload)).toEqual([
      { panel: 'inspector', placement: 'main' },
      { panel: 'inspector', opacity: 0.5 },
    ]);
  });
});

describe('servePanelWindows', () => {
  it('opens the windows of the layout and follows what happens to them', async () => {
    const layout = {
      windows: [{ panel: 'inspector', kind: 'window' }],
      applyWindows: vi.fn(),
      windowMoved: vi.fn(),
      place: vi.fn(),
      setOpacity: vi.fn(),
    };
    const stop = servePanelWindows(layout as unknown as LayoutState);
    await settle();
    expect(layout.applyWindows).toHaveBeenCalledOnce();

    const rect = { panel: 'inspector', x: 1, y: 2, width: 3, height: 4 };
    await backend.emit('panel_window_moved', rect);
    await backend.emit('panel_window_closed', { panel: 'inspector' });
    await backend.emit('ui_panel_window', { panel: 'inspector', placement: 'overlay' });
    await backend.emit('ui_panel_window', { panel: 'inspector', opacity: 0.7 });
    expect(layout.windowMoved).toHaveBeenCalledWith(rect);
    expect(layout.place).toHaveBeenCalledWith('inspector', 'main');
    expect(layout.place).toHaveBeenCalledWith('inspector', 'overlay');
    expect(layout.setOpacity).toHaveBeenCalledWith('inspector', 0.7);

    stop();
    await settle();
    expect(backend.invoked('set_panel_windows')).toEqual([{ windows: [] }]);
  });
});

// Last: the forwarding stays in effect for the rest of the file, as in a panel window.
describe('a panel window', () => {
  it('forwards dialogs opened in a panel window to the main window', async () => {
    forwardDialogs();
    ui.waveformOpen = 'cue';
    ui.settingsOpen = true;
    await settle();
    expect(ui.waveformOpen).toBeNull();
    expect(ui.settingsOpen).toBe(false);
    expect(sentToMain().map((args) => args.payload)).toEqual(
      expect.arrayContaining([
        { dialog: 'waveformOpen', value: 'cue' },
        { dialog: 'settingsOpen', value: true },
      ]),
    );
  });
});
