// The smaller pieces of the main window: fields, menus and dialogs not reached through the app tests.

import { fireEvent, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import type { EditOp } from '../lib/api';
import { keyCapture } from '../lib/keys.svelte';
import { ask, ui } from '../lib/store.svelte';
import { backend } from '../test/backend';
import { chord, control, group, runtime, sound, view, wait } from '../test/fixtures';
import { renderUi } from '../test/render';
import { loadShow, settle } from '../test/store';
import AboutDialog from './AboutDialog.svelte';
import GotoField from './cuelist/GotoField.svelte';
import ColorField from './fields/ColorField.svelte';
import SliderField from './fields/SliderField.svelte';
import Footer from './Footer.svelte';
import GroupDialog from './GroupDialog.svelte';
import KeyCapture from './KeyCapture.svelte';
import PanelWindow from './layout/PanelWindow.svelte';
import MarkList from './MarkList.svelte';
import ShowMenu from './ShowMenu.svelte';
import Stopwatches from './Stopwatches.svelte';
import StopwatchesDialog from './StopwatchesDialog.svelte';
import Welcome from './Welcome.svelte';

let errors: MockInstance;
beforeEach(() => {
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  expect(errors).not.toHaveBeenCalled();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

const lastEdit = <K extends EditOp['op']>(op: K) =>
  backend.edits.filter((edit): edit is Extract<EditOp, { op: K }> => edit.op === op).at(-1);

describe('ColorField', () => {
  it('reports a picked colour once the picker settles, and clears it', async () => {
    vi.useFakeTimers();
    const onchange = vi.fn();
    const { container, user } = await renderUi(ColorField, { value: '#ff0000', onchange });
    const input = container.querySelector<HTMLInputElement>('input[type="color"]')!;
    await fireEvent.input(input, { target: { value: '#00ff00' } });
    await fireEvent.input(input, { target: { value: '#0000ff' } });
    expect(onchange).not.toHaveBeenCalled();
    vi.advanceTimersByTime(300);
    expect(onchange).toHaveBeenCalledExactlyOnceWith('#0000ff');
    // Back to the value it has: nothing to report.
    await fireEvent.change(input, { target: { value: '#ff0000' } });
    vi.advanceTimersByTime(300);
    expect(onchange).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
    await user.click(screen.getByRole('button', { name: 'none' }));
    expect(onchange).toHaveBeenLastCalledWith(null);
  });

  it('sends a pending colour when it goes away, and shows mixed values', async () => {
    const onchange = vi.fn();
    const { container, unmount } = await renderUi(ColorField, { value: null, mixed: true, onchange });
    expect(screen.getByText('mixed')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'none' })).toBeEnabled();
    await fireEvent.input(container.querySelector('input')!, { target: { value: '#123456' } });
    unmount();
    expect(onchange).toHaveBeenCalledWith('#123456');
  });
});

describe('KeyCapture', () => {
  async function mount(value = chord('KeyA')) {
    const onchange = vi.fn();
    const result = await renderUi(KeyCapture, { value, onchange, label: 'GO' });
    return { ...result, onchange, field: screen.getByRole('button', { name: 'GO' }) };
  }

  it('records the next key, which runs no shortcut', async () => {
    const { user, field, onchange } = await mount();
    await user.click(field);
    expect(field).toHaveTextContent('Press a key');
    expect(keyCapture.active).toBe(true);
    const shortcut = vi.fn();
    window.addEventListener('keydown', shortcut);
    // A lone modifier is no key yet; a held key repeats.
    await fireEvent.keyDown(field, { code: 'ShiftLeft', key: 'Shift', shiftKey: true });
    await fireEvent.keyDown(field, { code: 'KeyB', key: 'b', repeat: true });
    expect(onchange).not.toHaveBeenCalled();
    await fireEvent.keyDown(field, { code: 'KeyB', key: 'b', ctrlKey: true });
    expect(onchange).toHaveBeenCalledWith(expect.objectContaining({ code: 'KeyB', ctrl: true }));
    // Its keyup does not click the field again.
    await fireEvent.keyUp(field, { code: 'KeyB', key: 'b' });
    await fireEvent.keyUp(field, { code: 'KeyC', key: 'c' });
    window.removeEventListener('keydown', shortcut);
    expect(shortcut).not.toHaveBeenCalled();
    expect(keyCapture.active).toBe(false);
  });

  it('removes the key with Delete or the ×, and cancels when left', async () => {
    const { user, field, onchange } = await mount();
    await user.click(field);
    await fireEvent.keyDown(field, { code: 'Delete', key: 'Delete' });
    expect(onchange).toHaveBeenLastCalledWith(null);
    await user.click(screen.getByRole('button', { name: 'GO: remove' }));
    expect(onchange).toHaveBeenCalledTimes(2);
    await user.click(field);
    await fireEvent.blur(field);
    expect(field).not.toHaveTextContent('Press a key');
    await user.click(field);
    await user.click(field);
    expect(keyCapture.active).toBe(false);
  });

  it('stops recording when it goes away', async () => {
    const { user, field, unmount } = await mount(null as never);
    expect(field).toHaveTextContent('none');
    await user.click(field);
    unmount();
    expect(keyCapture.active).toBe(false);
  });
});

describe('SliderField', () => {
  it('swaps the slider for a number field and back', async () => {
    const onchange = vi.fn();
    const { user } = await renderUi(SliderField, {
      label: 'Volume',
      value: 0,
      min: -60,
      max: 10,
      number: { value: -3, unit: 'dB', step: 0.5, onchange },
    });
    expect(screen.getByRole('slider')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Enter value' }));
    await settle();
    const number = screen.getByRole('spinbutton', { name: 'Volume' });
    expect(number).toHaveValue(-3);
    await fireEvent.change(number, { target: { value: '-6' } });
    await fireEvent.blur(number);
    expect(onchange).toHaveBeenCalledWith(-6);
    await user.click(screen.getByRole('button', { name: 'Use slider' }));
    expect(screen.getByRole('slider')).toBeInTheDocument();
  });
});

describe('MarkList', () => {
  it('edits, removes and adds marks, kept sorted and unique', async () => {
    const onchange = vi.fn();
    const { user } = await renderUi(MarkList, { marks: [1, 3], next: 2.0004, nextTip: 'Add', onchange });
    const second = screen.getByRole('spinbutton', { name: 'Mark 2 (s)' });
    await fireEvent.change(second, { target: { value: '1' } });
    expect(onchange).toHaveBeenLastCalledWith([1]);
    await fireEvent.change(second, { target: { value: '' } });
    expect(onchange).toHaveBeenCalledTimes(1);
    expect(second).toHaveValue(3);
    await user.click(screen.getByRole('button', { name: 'Remove mark 1' }));
    expect(onchange).toHaveBeenLastCalledWith([3]);
    await user.click(screen.getByRole('button', { name: '+ Mark' }));
    expect(onchange).toHaveBeenLastCalledWith([1, 2, 3]);
  });
});

describe('GotoField', () => {
  it('moves the playhead to a cue by its number, or says there is none', async () => {
    await loadShow(view([sound('a', { number: '12' })]));
    const { user } = await renderUi(GotoField);
    const field = screen.getByRole('textbox');
    await user.type(field, '99{Enter}');
    expect(screen.getByText(/No cue with the number 99/)).toBeInTheDocument();
    await user.type(field, '7');
    await user.clear(field);
    await user.type(field, '12{Enter}');
    await settle();
    expect(ui.selected).toBe('a');
    await user.type(field, '3{Escape}');
    expect(field).toHaveValue('');
  });
});

describe('ShowMenu', () => {
  const showView = (extra: Parameters<typeof view>[1] = {}) =>
    view([sound('a')], {
      ...extra,
      show: { others: { effect: 'duck', duck_db: -10, fade: 1, scope: 'all' }, ...extra.show },
    });

  it('renames the show with Enter or the button', async () => {
    await loadShow(showView());
    const onclose = vi.fn();
    const { user } = await renderUi(ShowMenu, { onclose });
    const name = screen.getByRole('textbox', { name: 'Name of the show' });
    await user.clear(name);
    await user.type(name, 'Premiere{Enter}');
    await settle();
    expect(backend.invoked('rename_show')).toEqual([{ name: 'Premiere' }]);
    expect(onclose).toHaveBeenCalledTimes(1);
    await user.type(name, '2');
    await user.click(screen.getByRole('button', { name: 'Rename' }));
    await settle();
    expect(backend.invoked('rename_show')).toHaveLength(2);
  });

  it('sets what happens to other cues, the level limits and normalization', async () => {
    await loadShow(showView());
    const { user } = await renderUi(ShowMenu, { onclose: () => {} });
    for (const [name, value] of [
      ['Fade (s)', '2'],
      ['Reduction (dB)', '-20'],
    ]) {
      const input = screen.getByRole('spinbutton', { name });
      await fireEvent.change(input, { target: { value } });
      await fireEvent.blur(input);
    }
    await settle();
    expect(lastEdit('set_others')).toEqual({
      op: 'set_others',
      others: { effect: 'duck', duck_db: -20, fade: 1, scope: 'all' },
    });
    await user.click(screen.getByRole('combobox', { name: /Scope/ }));
    await user.click(screen.getAllByRole('option').at(-1)!);
    await settle();

    const min = screen.getByRole('spinbutton', { name: 'Minimum (dB)' });
    await fireEvent.change(min, { target: { value: '-50' } });
    await settle();
    expect(backend.edits).toContainEqual({ op: 'set_level_limits', limits: { max_db: 6, min_db: -50 } });
    const max = screen.getByRole('spinbutton', { name: 'Maximum (dB)' });
    await fireEvent.change(max, { target: { value: '' } });
    await settle();
    expect(max).toHaveValue(6);

    const target = screen.getByRole('spinbutton', { name: 'Target loudness (LUFS)' });
    await fireEvent.change(target, { target: { value: '-16' } });
    await fireEvent.blur(target);
    await user.click(screen.getByRole('checkbox', { name: /Normalize new cues/ }));
    await user.click(screen.getByRole('checkbox', { name: /Lock show mode/ }));
    await settle();
    expect(lastEdit('set_normalization')).toEqual({ op: 'set_normalization', target: -23, on_import: true });
    expect(backend.edits).toContainEqual({ op: 'set_normalization', target: -16, on_import: false });
    expect(lastEdit('lock_show_mode')).toEqual({ op: 'lock_show_mode', locked: true });
  });

  it('turns the effect on other cues off', async () => {
    await loadShow(showView());
    const { user } = await renderUi(ShowMenu, { onclose: () => {} });
    const effect = screen.getAllByRole('combobox')[0];
    await user.click(effect);
    await user.click(screen.getAllByRole('option')[0]);
    await settle();
    expect(lastEdit('set_others')?.others.effect).not.toBe('duck');
  });

  it('cleans the media folder', async () => {
    await loadShow(showView());
    const onclose = vi.fn();
    const { user } = await renderUi(ShowMenu, { onclose });
    await user.click(screen.getByRole('button', { name: /Clean up/ }));
    await settle();
    expect(onclose).toHaveBeenCalled();
    expect(backend.invoked('unused_media')).toHaveLength(1);
  });

  it('is read-only in show mode', async () => {
    await loadShow(showView({ settings: { show_mode: true } }));
    await renderUi(ShowMenu, { onclose: () => {} });
    expect(screen.getByRole('textbox', { name: /Name of the show/ })).toBeDisabled();
    expect(screen.getAllByLabelText('Locked in show mode').length).toBeGreaterThan(0);
  });
});

describe('Footer', () => {
  async function openMenu(v = view([sound('a')]), state = runtime()) {
    await loadShow(v, state);
    const result = await renderUi(Footer);
    await result.user.click(screen.getByRole('button', { name: /Media folder/ }));
    await settle();
    return result;
  }

  it('chooses another media folder, opens it and rescans it', async () => {
    backend.on('plugin:dialog|open', () => '/elsewhere');
    const { user } = await openMenu();
    await user.click(screen.getByRole('button', { name: /Choose another folder/ }));
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: /Media folder/ }));
    await user.click(screen.getByRole('button', { name: 'Open in file manager' }));
    await settle();
    expect(backend.invoked('open_media_dir')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: /Media folder/ }));
    await user.click(screen.getByRole('button', { name: 'Rescan' }));
    await settle();
    expect(backend.invoked('rescan_media')).toHaveLength(1);
  });

  it('keeps the folder while cues run, and tells a missing folder and a search', async () => {
    const v = view([sound('a')], { media_dir_exists: false });
    await openMenu(v, runtime({ searching: true, active: [] }));
    expect(screen.getByText(/Media folder not found/)).toBeInTheDocument();
    expect(screen.getByText('Searching for missing files …')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Choose another folder/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Open in file manager' })).toBeDisabled();
  });
});

describe('AboutDialog', () => {
  it('shows the version, the licence and the libraries with their licence texts', async () => {
    backend.on('plugin:app|version', () => '1.2.3');
    const onclose = vi.fn();
    const { user } = await renderUi(AboutDialog, { onclose });
    await settle(5);
    expect(screen.getByText('Version 1.2.3')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Libraries used/ }));
    await settle();
    const first = (await screen.findAllByRole('button', { expanded: false }, { timeout: 10_000 }))[0];
    await user.click(first);
    expect(first).toHaveAttribute('aria-expanded', 'true');
    await user.click(first);
    expect(first).toHaveAttribute('aria-expanded', 'false');
    await user.click(screen.getByRole('button', { name: 'GNU GPL v3' }));
    await user.click(screen.getAllByRole('button', { name: 'Close' })[0]);
    expect(onclose).toHaveBeenCalled();
  });

  it('reports the progress of an update and a failed install', async () => {
    ui.update = { version: '9.9.9', notes: null } as never;
    let fail: (reason: string) => void = () => {};
    backend.on('install_update', () => new Promise((_, reject) => (fail = reject)));
    const { user } = await renderUi(AboutDialog, { onclose: () => {} });
    await user.click(screen.getByRole('button', { name: 'Install and restart' }));
    await settle();
    await backend.emit('update_progress', { downloaded: 50, total: 100 });
    expect(screen.getByText(/50/)).toBeInTheDocument();
    await backend.emit('update_progress', { downloaded: 50, total: null });
    fail('disk full');
    await settle();
    expect(screen.getByText(/disk full/)).toBeInTheDocument();
  });
});

describe('StopwatchesDialog', () => {
  it('adds, renames, turns into countdowns and removes stopwatches, saved together', async () => {
    await loadShow(view([], { show: { stopwatches: [{ id: 'sw', name: 'Show' }] } }));
    const onclose = vi.fn();
    const { user } = await renderUi(StopwatchesDialog, { onclose });
    const dialog = screen.getByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: '+ Stopwatch' }));
    const names = within(dialog).getAllByRole('textbox', { name: 'Name' });
    await user.clear(names[0]);
    await user.type(names[0], 'Act 1');
    await user.click(within(dialog).getByRole('combobox', { name: 'Type of Act 1' }));
    await user.click(screen.getByRole('option', { name: 'Countdown' }));
    await settle();
    const from = within(dialog).getByRole('textbox', { name: /Countdown start of Act 1/ });
    await fireEvent.change(from, { target: { value: '2:30' } });
    await fireEvent.change(from, { target: { value: 'never' } });
    expect(from).toHaveValue('2:30');
    await user.click(within(dialog).getByRole('combobox', { name: 'Type of Act 1' }));
    await user.click(screen.getByRole('option', { name: 'counts up' }));
    await user.click(
      within(dialog)
        .getAllByRole('button', { name: /Remove/ })
        .at(-1)!,
    );
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await settle();
    expect(lastEdit('set_stopwatches')).toEqual({ op: 'set_stopwatches', stopwatches: [{ id: 'sw', name: 'Act 1' }] });
  });

  it('says when there are none', async () => {
    await loadShow(view([], { show: { stopwatches: [] } }));
    await renderUi(StopwatchesDialog, { onclose: () => {} });
    expect(screen.getByText('No stopwatches yet.')).toBeInTheDocument();
  });
});

describe('GroupDialog', () => {
  const groupView = (cues = [sound('g1', { title: 'One' }), wait('g2', { title: 'Two' })], mode = 'sequence') =>
    view([group('g', cues, { number: '4', mode: mode as never }), control('x', 'volume')]);

  it('deletes the selected cue with the Delete key, then the next one is selected', async () => {
    await loadShow(groupView());
    const { user } = await renderUi(GroupDialog, { groupId: 'g', onclose: () => {} });
    const dialog = screen.getByRole('dialog');
    await user.click(within(dialog).getByText('One'));
    await fireEvent.keyDown(document.body, { code: 'Delete', key: 'Delete' });
    await settle();
    expect(lastEdit('delete')).toEqual({ op: 'delete', ids: ['g1'] });
    // Not with a modifier, nor in a text field.
    await fireEvent.keyDown(document.body, { code: 'Delete', key: 'Delete', shiftKey: true });
    expect(backend.edits.filter((op) => op.op === 'delete')).toHaveLength(1);
  });

  it('adds cues of any kind, duplicates and moves them', async () => {
    await loadShow(groupView());
    const { user } = await renderUi(GroupDialog, { groupId: 'g', onclose: () => {} });
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText(/The first cue starts/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: '+ Sound' }));
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);

    await user.click(within(dialog).getByText('One'));
    await user.click(within(dialog).getByRole('button', { name: 'Move down' }));
    await user.click(within(dialog).getByRole('button', { name: 'Duplicate' }));
    await settle();
    expect(lastEdit('move')).toEqual({ op: 'move', ids: ['g1'], to: 1 });
    expect(lastEdit('duplicate')).toMatchObject({ id: 'g1' });

    await user.click(within(dialog).getByRole('button', { name: '+ More' }));
    await settle();
    const items = screen.getAllByRole('menuitem');
    expect(items.length).toBeGreaterThan(3);
    await user.click(items.find((item) => /Wait/.test(item.textContent!))!);
    await settle();
    expect(backend.edits.at(-1)).toMatchObject({ op: 'add_wait', group: 'g' });
  });

  it('says what to do in an empty group, and closes when the group goes away', async () => {
    await loadShow(groupView([], 'all'));
    const onclose = vi.fn();
    await renderUi(GroupDialog, { groupId: 'g', onclose });
    expect(screen.getByText('No cues yet – add some below.')).toBeInTheDocument();
    expect(screen.getByText(/All cues start with the group's GO/)).toBeInTheDocument();
    await loadShow(view([]));
    await settle();
    expect(onclose).toHaveBeenCalled();
  });
});

describe('Stopwatches in the header', () => {
  it('starts, stops, restarts and resets each one from its popover', async () => {
    await loadShow(
      view([], {
        show: {
          stopwatches: [
            { id: 'sw', name: 'Show' },
            { id: 'cd', name: 'Break', countdown: 60 },
          ],
        },
      }),
      runtime({
        stopwatches: [
          { id: 'sw', elapsed: 5, remaining: null, running: true },
          { id: 'cd', elapsed: 70, remaining: -10, running: false },
        ],
      }),
    );
    const { user } = await renderUi(Stopwatches);
    for (const [name, first] of [
      ['Stopwatch Show', 'Stop'],
      ['Countdown Break', 'Start'],
    ]) {
      for (const action of [first, 'Restart', 'Reset']) {
        await user.click(screen.getByRole('button', { name }));
        await settle();
        const popover = [...document.querySelectorAll<HTMLElement>('[data-slot="popover-content"]')].at(-1)!;
        await user.click(within(popover).getByRole('button', { name: new RegExp(action) }));
        await user.keyboard('{Escape}');
      }
    }
    await settle();
    expect(backend.invoked('stopwatch')).toEqual([
      { id: 'sw', action: 'stop' },
      { id: 'sw', action: 'restart' },
      { id: 'sw', action: 'reset' },
      { id: 'cd', action: 'start' },
      { id: 'cd', action: 'restart' },
      { id: 'cd', action: 'reset' },
    ]);
  });
});

describe('PanelWindow', () => {
  const overlayView = (opacity: number) =>
    view([sound('a')], {
      settings: { layout: { edit: { windows: [{ panel: 'transport', kind: 'overlay', opacity }] } } as never },
    });
  const sentToMain = () => backend.invoked('plugin:event|emit_to').map(({ payload }) => payload);

  it('sets the opacity of an overlay from its menu', async () => {
    await loadShow(overlayView(0.8), runtime());
    const { user } = await renderUi(PanelWindow, { id: 'transport' });
    expect(document.documentElement).toHaveAttribute('data-overlay');
    expect(document.documentElement.style.getPropertyValue('--overlay-opacity')).toBe('0.8');
    const trigger = screen
      .getAllByRole('button', { name: 'Window' })
      .find((button) => button.closest('[role="group"]') === null)!;
    await user.click(trigger);
    await settle();
    const popover = [...document.querySelectorAll<HTMLElement>('[data-slot="popover-content"]')].at(-1)!;
    const slider = within(popover).getByRole('slider');
    slider.focus();
    await user.keyboard('{ArrowLeft}');
    await settle();
    expect(sentToMain().at(-1)).toEqual({ panel: 'transport', opacity: 0.79 });
    // The slider's value holds until the saved layout has it.
    await loadShow(overlayView(0.79), runtime());
    expect(document.documentElement.style.getPropertyValue('--overlay-opacity')).toBe('0.79');
  });

  it('says when its panel does not exist, and closes', async () => {
    await loadShow(view([]));
    const { user } = await renderUi(PanelWindow, { id: 'gone' });
    expect(screen.getByText('This panel does not exist (any more).')).toBeInTheDocument();
    await user.click(screen.getAllByRole('button', { name: 'Close' }).at(-1)!);
    await settle();
    expect(backend.invoked('plugin:window|close')).toHaveLength(1);
  });

  it('asks its own questions', async () => {
    await loadShow(view([sound('a')]), runtime());
    await renderUi(PanelWindow, { id: 'cues' });
    const answer = ask('Delete?', 'Really?', [
      { value: 'yes', label: 'Delete' },
      { value: 'no', label: 'Keep' },
    ]);
    await settle();
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Delete?');
    await fireEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Delete' }));
    expect(await answer).toBe('yes');
  });
});

describe('Welcome', () => {
  it('creates a new show with Enter, in a folder of its own or not', async () => {
    backend.on('plugin:dialog|open', () => '/shows');
    await loadShow(null);
    const { user } = await renderUi(Welcome);
    expect(screen.getByRole('button', { name: 'Create new show …' })).toBeDisabled();
    await user.click(screen.getByRole('checkbox', { name: /folder of its own/ }));
    await user.type(screen.getByPlaceholderText('Show name'), ' Gala {Enter}');
    await settle();
    expect(backend.invoked('new_show')).toEqual([{ dir: '/shows', name: 'Gala', createFolder: false }]);
    await user.click(screen.getByRole('button', { name: /About jQ Player/ }));
    expect(ui.aboutOpen).toBe(true);
  });

  it('offers the last show on startup and opens it', async () => {
    backend.on('startup_show', () => '/shows/Last/Last.jqshow');
    await loadShow(null);
    await renderUi(Welcome);
    await settle();
    expect(ui.question?.title).toBe('Open the last show?');
    ui.question!.answer('open');
    await settle();
    expect(backend.invoked('open_show')).toEqual([{ path: '/shows/Last/Last.jqshow' }]);
  });

  it('opens nothing when the last show is not wanted', async () => {
    backend.on('startup_show', () => '/shows/Last/Last.jqshow');
    await loadShow(null);
    await renderUi(Welcome);
    await settle();
    ui.question!.answer('skip');
    await settle();
    expect(backend.invoked('open_show')).toEqual([]);
  });
});
