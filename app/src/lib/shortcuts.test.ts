import { render } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import KeyboardScope from '../components/KeyboardScope.svelte';
import { backend } from '../test/backend';
import { chord, group, scene, sound, view, wait } from '../test/fixtures';
import { loadShow, settle } from '../test/store';
import { claimUndo } from './commands';
import { keyCapture } from './keys.svelte';
import { FORMAT_KEYS, formatShortcut, isTextField, keepsSpace } from './shortcuts';
import { select, selectAll, selection, ui } from './store.svelte';

const KEYS: Record<string, string> = { Space: ' ', Escape: 'Escape', Delete: 'Delete', Tab: 'Tab', Enter: 'Enter' };

/** Presses a key on `target` (default: the page) as the webview would; returns the event to look at. */
function press(code: string, init: KeyboardEventInit = {}, target: EventTarget = document.body): KeyboardEvent {
  const key = init.key ?? KEYS[code] ?? code.replace(/^(Key|Digit)/, '').toLowerCase();
  const e = new KeyboardEvent('keydown', { code, key, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  return e;
}

function release(code: string, target: EventTarget = document.body) {
  const e = new KeyboardEvent('keyup', { code, key: KEYS[code] ?? code, bubbles: true, cancelable: true });
  target.dispatchEvent(e);
  return e;
}

function element<T extends HTMLElement>(html: string): T {
  document.body.insertAdjacentHTML('beforeend', html);
  return document.body.lastElementChild as T;
}

const cues = () => [
  sound('a', { number: '1', hotkey: chord('KeyH'), release_trigger: true }),
  wait('b', { number: '2', hotkey: chord('KeyH') }),
  scene('s'),
  sound('c', { number: '3' }),
];

async function edit(extra: Parameters<typeof view>[1] = {}) {
  await loadShow(view(cues(), extra));
}

async function show(extra: Parameters<typeof view>[1] = {}) {
  await loadShow(view(cues(), { ...extra, settings: { show_mode: true, ...extra.settings } }));
}

beforeEach(() => {
  render(KeyboardScope);
  // The focus counts as placed by the keyboard until a test clicks.
  press('Tab');
  keyCapture.active = false;
});

describe('without a show', () => {
  it('leaves every key alone', () => {
    expect(press('Space').defaultPrevented).toBe(false);
    expect(backend.actions).toEqual([]);
  });
});

describe('global actions', () => {
  it('sends GO once per press, not for a held key', async () => {
    await show();
    expect(press('Space').defaultPrevented).toBe(true);
    press('Space', { repeat: true });
    expect(backend.actions).toEqual([{ action: 'go' }]);
  });

  it('panics with Esc, also with Shift, once per press', async () => {
    await edit();
    press('Escape');
    press('Escape', { shiftKey: true });
    press('Escape', { repeat: true });
    expect(backend.actions).toEqual([{ action: 'panic' }, { action: 'panic' }]);
  });

  it('leaves Esc to an open dialog with "close_first"', async () => {
    await edit({ settings: { escape: 'close_first' } });
    element('<div data-slot="dialog-content"></div>');
    expect(press('Escape').defaultPrevented).toBe(false);
    expect(backend.actions).toEqual([]);
  });

  it('panics with Esc over an open dialog with "stop_all"', async () => {
    await edit();
    element('<div data-slot="dialog-content"></div>');
    press('Escape');
    expect(backend.actions).toEqual([{ action: 'panic' }]);
  });

  it('acts on the selected cue, repeating only the step actions', async () => {
    await edit();
    press('KeyP');
    expect(backend.actions).toEqual([]);
    select('a');
    press('KeyP');
    press('KeyP', { repeat: true });
    press('Equal', { key: '=' });
    press('Equal', { key: '=', repeat: true });
    expect(backend.actions).toEqual([
      { action: 'preview', cue: 'a' },
      { action: 'volume_up', cue: 'a' },
      { action: 'volume_up', cue: 'a' },
    ]);
  });

  it('focuses the "Go to" field without typing the key into it', async () => {
    await edit();
    const field = element<HTMLInputElement>('<input data-goto-field />');
    const e = press('KeyG', { ctrlKey: true });
    expect(e.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(field);
    expect(backend.actions).toEqual([]);
  });

  it('leaves the "Go to" key alone without the field or behind a dialog', async () => {
    await edit();
    expect(press('KeyG', { ctrlKey: true }).defaultPrevented).toBe(false);
    element('<input data-goto-field />');
    element('<div data-slot="dialog-content"></div>');
    expect(press('KeyG', { ctrlKey: true }).defaultPrevented).toBe(false);
  });

  it('ignores keys that are bound to nothing', async () => {
    await edit();
    expect(press('KeyQ').defaultPrevented).toBe(false);
    expect(press('ShiftLeft', { key: 'Shift' }).defaultPrevented).toBe(false);
  });

  it('lets a key field record the press instead', async () => {
    await show();
    keyCapture.active = true;
    press('Space');
    expect(backend.actions).toEqual([]);
  });
});

describe('cue hotkeys', () => {
  it('start every cue with the key and release those with a release trigger', async () => {
    await show();
    press('KeyH');
    press('KeyH', { repeat: true });
    expect(backend.actions).toEqual([
      { action: 'start_cue', cue: 'a' },
      { action: 'start_cue', cue: 'b' },
    ]);
    release('KeyH');
    release('KeyH');
    expect(backend.actions.slice(2)).toEqual([{ action: 'release_cue', cue: 'a' }]);
  });

  it('release held keys when the window loses the keys', async () => {
    await show();
    press('KeyH');
    window.dispatchEvent(new Event('blur'));
    expect(backend.actions.at(-1)).toEqual({ action: 'release_cue', cue: 'a' });
  });
});

describe('fields', () => {
  it('keep their keys in edit mode', async () => {
    await edit();
    const input = element<HTMLInputElement>('<input />');
    press('Space', {}, input);
    press('Delete', {}, input);
    expect(backend.actions).toEqual([]);
    expect(backend.edits).toEqual([]);
  });

  it('give up GO and panic in show mode, but not other keys', async () => {
    await show();
    const input = element<HTMLInputElement>('<input />');
    select('a');
    press('Space', {}, input);
    press('KeyP', {}, input);
    press('Escape', {}, input);
    expect(backend.actions).toEqual([{ action: 'go' }, { action: 'panic' }]);
  });

  it('do not take Esc, also in edit mode', async () => {
    await edit();
    press('Escape', {}, element('<textarea></textarea>'));
    expect(backend.actions).toEqual([{ action: 'panic' }]);
  });

  it('includes open lists and menus, and triggers reached with Tab', async () => {
    await edit();
    const menu = element('<div data-slot="dropdown-menu-content"><button>x</button></div>');
    press('Space', {}, menu.querySelector('button')!);
    const trigger = element('<button data-slot="select-trigger"></button>');
    press('Space', {}, trigger);
    expect(backend.actions).toEqual([]);
    // A clicked trigger leaves the keys to the shortcuts.
    trigger.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    press('Space', {}, trigger);
    expect(backend.actions).toEqual([{ action: 'go' }]);
  });

  it('includes tabs and dialog buttons reached with Tab, which still panic', async () => {
    await edit();
    const tab = element('<button role="tab"></button>');
    press('Space', {}, tab);
    const dialog = element('<div data-slot="dialog-content" tabindex="-1"><button>OK</button></div>');
    press('Space', {}, dialog.querySelector('button')!);
    expect(backend.actions).toEqual([]);
    // The dialog box itself is no control: GO goes through.
    press('Space', {}, dialog);
    press('Escape', {}, tab);
    expect(backend.actions).toEqual([{ action: 'go' }, { action: 'panic' }]);
  });

  it('tells text fields and what keeps Space', () => {
    const editable = element('<div contenteditable="true"></div>');
    Object.defineProperty(editable, 'isContentEditable', { value: true });
    expect(isTextField(editable)).toBe(true);
    expect(isTextField(element('<select></select>'))).toBe(true);
    expect(isTextField(element('<button></button>'))).toBe(false);
    expect(isTextField(null)).toBe(false);
    expect(keepsSpace(element('<input />'))).toBe(true);
    expect(keepsSpace(document.body)).toBe(false);
    expect(keepsSpace(null)).toBe(false);
  });
});

describe('Space on keyup', () => {
  it('never clicks a focused button (a second GO)', async () => {
    await show();
    const button = element<HTMLButtonElement>('<button></button>');
    expect(release('Space', button).defaultPrevented).toBe(true);
  });

  it('is left to fields in edit mode', async () => {
    await edit();
    expect(release('Space', element('<input />')).defaultPrevented).toBe(false);
  });
});

describe('list keys', () => {
  it('move the selection and extend it with Shift in edit mode', async () => {
    await edit();
    press('ArrowDown');
    expect(ui.selected).toBe('a');
    press('ArrowDown', { shiftKey: true });
    expect(selection()).toEqual(['a', 'b']);
    press('End');
    expect(ui.selected).toBe('c');
    press('Home');
    expect(ui.selected).toBe('a');
    press('ArrowDown', { ctrlKey: true });
    expect(ui.selected).toBe('c');
    press('ArrowUp', { ctrlKey: true, shiftKey: true });
    expect(selection()).toEqual(['a', 'b', 's', 'c']);
    press('ArrowUp');
    expect(ui.selected).toBe('s');
  });

  it('do not extend in show mode', async () => {
    await show();
    select('a');
    press('ArrowDown', { shiftKey: true });
    expect(selection()).toEqual(['b']);
  });

  it('keep still behind a dialog', async () => {
    await edit();
    select('a');
    element('<div data-slot="alert-dialog-content"></div>');
    press('ArrowDown');
    press('ArrowDown', { ctrlKey: true });
    press('Delete');
    expect(ui.selected).toBe('a');
    expect(backend.edits).toEqual([]);
  });

  it('delete the selection in edit mode, once per press', async () => {
    await edit();
    expect(press('Delete').defaultPrevented).toBe(false);
    selectAll(['a', 'b']);
    press('Delete');
    press('Delete', { repeat: true });
    await settle();
    expect(backend.edits).toEqual([{ op: 'delete', ids: ['a', 'b'] }]);
  });

  it('stop the selected cue with Delete in show mode', async () => {
    await show();
    press('Delete');
    select('c');
    press('Delete');
    expect(backend.actions).toEqual([{ action: 'stop_cue', cue: 'c' }]);
  });

  it('leave Space to the waveform player', async () => {
    await edit();
    ui.waveformOpen = 'a';
    press('Space');
    expect(backend.actions).toEqual([]);
  });
});

describe('Ctrl shortcuts in edit mode', () => {
  it('copy, cut, paste and select all', async () => {
    await edit();
    press('KeyC', { ctrlKey: true });
    press('KeyA', { ctrlKey: true });
    expect(selection()).toHaveLength(4);
    press('KeyC', { ctrlKey: true });
    press('KeyV', { ctrlKey: true, shiftKey: true });
    press('KeyV', { ctrlKey: true, repeat: true });
    await settle();
    selectAll(['c']);
    press('KeyX', { ctrlKey: true });
    await settle();
    expect(backend.edits).toEqual([
      { op: 'copy', ids: ['a', 'b', 's', 'c'] },
      { op: 'paste', after: 'c', linked: true },
      { op: 'cut', ids: ['c'] },
    ]);
  });

  it('copy selected text as text, not the cues', async () => {
    await edit();
    select('a');
    const spy = vi.spyOn(window, 'getSelection').mockReturnValue({ toString: () => 'notice' } as Selection);
    try {
      press('KeyC', { ctrlKey: true });
    } finally {
      spy.mockRestore();
    }
    expect(backend.edits).toEqual([]);
  });

  it('undo and redo with Z, Shift+Z and Y', async () => {
    await edit();
    press('KeyZ', { ctrlKey: true });
    press('KeyZ', { ctrlKey: true, shiftKey: true });
    press('KeyY', { metaKey: true });
    await settle();
    expect(backend.edits).toEqual([{ op: 'undo' }, { op: 'redo' }, { op: 'redo' }]);
  });

  it('leave the list keys to a dialog, and undo to its claim', async () => {
    await edit();
    select('a');
    const dialog = element('<div data-slot="dialog-content"></div>');
    const claim = { owner: () => dialog, undo: vi.fn(), redo: vi.fn() };
    const release = claimUndo(claim);
    try {
      press('KeyA', { ctrlKey: true });
      press('KeyC', { ctrlKey: true });
      press('KeyZ', { ctrlKey: true });
    } finally {
      release();
    }
    expect(selection()).toEqual(['a']);
    expect(claim.undo).toHaveBeenCalledOnce();
    expect(backend.edits).toEqual([]);
  });

  it('are not edits in show mode', async () => {
    await show();
    selectAll(['a']);
    press('KeyC', { ctrlKey: true });
    press('KeyZ', { ctrlKey: true });
    await settle();
    expect(backend.edits).toEqual([]);
  });
});

describe('show commands', () => {
  it('save with Ctrl+S, once per press, also in show mode', async () => {
    await show();
    press('KeyS', { ctrlKey: true });
    press('KeyS', { ctrlKey: true, repeat: true });
    await settle();
    expect(backend.invoked('save_show')).toHaveLength(1);
  });

  it('commit the field being typed in before saving', async () => {
    await edit();
    const input = element<HTMLInputElement>('<input />');
    input.focus();
    const committed = vi.fn();
    input.addEventListener('blur', committed);
    press('KeyS', { ctrlKey: true }, input);
    expect(committed).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(input);
  });

  it('save as with Ctrl+Shift+S, not in show mode', async () => {
    await show();
    press('KeyS', { ctrlKey: true, shiftKey: true });
    await settle();
    expect(backend.invoked('plugin:dialog|save')).toEqual([]);
    await edit();
    press('KeyS', { ctrlKey: true, shiftKey: true });
    await settle();
    expect(backend.invoked('plugin:dialog|save')).toHaveLength(1);
  });

  it('create and open shows with Ctrl+N and Ctrl+O, not under a dialog', async () => {
    await edit();
    element('<div data-slot="dialog-content"></div>');
    press('KeyO', { ctrlKey: true });
    document.body.lastElementChild!.remove();
    press('KeyO', { ctrlKey: true, repeat: true });
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toEqual([]);
    press('KeyO', { ctrlKey: true });
    await settle();
    expect(backend.invoked('plugin:dialog|open')).toHaveLength(1);
    press('KeyN', { ctrlKey: true });
    await settle();
    expect(backend.invoked('plugin:dialog|save')).toHaveLength(1);
  });
});

describe('the pointer focus', () => {
  it('is released after a shortcut, so a clicked button does not keep the keys', async () => {
    await show();
    const button = element<HTMLButtonElement>('<button></button>');
    button.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    button.focus();
    press('Space', {}, button);
    expect(document.activeElement).not.toBe(button);
  });

  it('is released by a panic too', async () => {
    await edit();
    const button = element<HTMLButtonElement>('<button></button>');
    button.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    button.focus();
    press('Escape', {}, button);
    expect(document.activeElement).not.toBe(button);
  });
});

describe('detail text formatting', () => {
  it('maps Ctrl+B, I, U and Ctrl+Shift+X', () => {
    const fmt = (code: string, init: KeyboardEventInit = {}) =>
      formatShortcut(new KeyboardEvent('keydown', { code, key: code.slice(3).toLowerCase(), ...init }));
    expect(fmt('KeyB', { ctrlKey: true })).toBe('bold');
    expect(fmt('KeyI', { metaKey: true })).toBe('italic');
    expect(fmt('KeyU', { ctrlKey: true })).toBe('underline');
    expect(fmt('KeyX', { ctrlKey: true, shiftKey: true })).toBe('strike');
    expect(fmt('KeyB', { ctrlKey: true, shiftKey: true })).toBeNull();
    expect(fmt('KeyB', { ctrlKey: true, altKey: true })).toBeNull();
    expect(fmt('KeyB')).toBeNull();
    expect(fmt('KeyQ', { ctrlKey: true })).toBeNull();
    expect(FORMAT_KEYS.strike).toBe('Ctrl + Shift + X');
    expect([FORMAT_KEYS.bold, FORMAT_KEYS.italic, FORMAT_KEYS.underline]).toEqual(['Ctrl + B', 'Ctrl + I', 'Ctrl + U']);
  });
});

describe('a group as the only cue', () => {
  it('still selects with the arrows', async () => {
    await loadShow(view([group('g', [wait('w')])]));
    press('ArrowUp');
    expect(ui.selected).toBe('g');
  });
});
