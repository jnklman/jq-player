import { describe, expect, it } from 'vitest';

import { backend } from '../test/backend';
import { chord } from '../test/fixtures';
import { i18n } from './i18n/index.svelte';
import {
  ACTION_NAMES,
  boundKeys,
  chordOf,
  isFixed,
  isPanicKey,
  keyName,
  keyParts,
  KEYS_ONLY,
  keyText,
  learnKeyLabel,
  loadKeyLabels,
  sameChord,
} from './keys.svelte';

const press = (init: KeyboardEventInit) => new KeyboardEvent('keydown', init);

describe('chords', () => {
  it('takes the physical key and the modifiers, Cmd as Ctrl', () => {
    expect(chordOf(press({ code: 'KeyA', key: 'a', metaKey: true, shiftKey: true }))).toEqual({
      code: 'KeyA',
      ctrl: true,
      shift: true,
      alt: false,
      label: 'a',
    });
    expect(chordOf(press({ code: 'ShiftLeft', key: 'Shift' }))).toBeNull();
    expect(chordOf(press({ code: 'Unidentified', key: 'x' }))).toBeNull();
    expect(chordOf(press({ code: '', key: 'x' }))).toBeNull();
  });

  it('compares without the label', () => {
    expect(sameChord(chord('KeyA', { label: 'a' }), chord('KeyA', { label: 'q' }))).toBe(true);
    expect(sameChord(chord('KeyA'), chord('KeyA', { alt: true }))).toBe(false);
  });

  it('treats Esc with or without Shift as panic', () => {
    expect(isPanicKey(chord('Escape'))).toBe(true);
    expect(isPanicKey(chord('Escape', { shift: true }))).toBe(true);
    expect(isPanicKey(chord('Escape', { ctrl: true }))).toBe(false);
  });

  it('knows the fixed keys, Ctrl letters by the letter on the key', () => {
    const fixed = [chord('KeyS', { ctrl: true, label: 's' }), chord('Delete')];
    expect(isFixed(chord('KeyS', { ctrl: true, label: 's' }), fixed)).toBe(true);
    expect(isFixed(chord('KeyS', { label: 's' }), fixed)).toBe(false);
    expect(isFixed(chord('Delete'), fixed)).toBe(true);
    expect(isFixed(chord('Delete', { alt: true }), fixed)).toBe(false);
  });
});

describe('key names', () => {
  it('names special keys, function keys and the number pad', () => {
    expect(keyName(chord('Space'))).toBe('Space');
    expect(keyName(chord('F5'))).toBe('F5');
    expect(keyName(chord('Numpad3'))).toBe('Num 3');
    expect(keyName(chord('ArrowUp'))).toBe('↑');
  });

  it('takes the label of the current layout, else the stored one, else the code', () => {
    expect(keyName(chord('KeyQ', { label: 'q' }))).toBe('Q');
    expect(keyName(chord('Digit7', { label: '' }))).toBe('7');
    learnKeyLabel(press({ code: 'KeyZ', key: 'y' }));
    expect(keyName(chord('KeyZ', { label: 'z' }))).toBe('Y');
    // Presses with modifiers or of named keys teach nothing.
    learnKeyLabel(press({ code: 'KeyX', key: 'X', shiftKey: true }));
    learnKeyLabel(press({ code: 'Enter', key: 'Enter' }));
    expect(keyName(chord('KeyX', { label: '' }))).toBe('X');
  });

  it('asks the webview, else the backend for the layout', async () => {
    const nav = navigator as Navigator & { keyboard?: unknown };
    nav.keyboard = { getLayoutMap: async () => new Map([['KeyM', 'ö']]) };
    await loadKeyLabels();
    expect(keyName(chord('KeyM', { label: 'm' }))).toBe('Ö');

    nav.keyboard = { getLayoutMap: () => Promise.reject(new Error('denied')) };
    backend.on('key_labels', () => ({ KeyM: 'µ' }));
    await loadKeyLabels();
    expect(keyName(chord('KeyM', { label: 'm' }))).toBe('Μ');

    delete nav.keyboard;
    backend.fail('key_labels');
    await loadKeyLabels();
    expect(keyName(chord('KeyM', { label: 'm' }))).toBe('Μ');
  });

  it('joins the modifiers in a fixed order, in the language shown', () => {
    const ctrlShift = chord('F5', { ctrl: true, shift: true, alt: true });
    expect(keyParts(ctrlShift)).toEqual(['Ctrl', 'Alt', 'Shift', 'F5']);
    expect(keyText(ctrlShift)).toBe('Ctrl + Alt + Shift + F5');
    i18n.locale = 'de';
    expect(keyText(chord('Space', { ctrl: true }))).toBe('Strg + Leertaste');
  });
});

describe('boundKeys', () => {
  it('lists the keys of the first action that has any; panic always has Esc', () => {
    const keys = [
      { chord: chord('Space'), action: 'go' as const },
      { chord: chord('F1'), action: 'go' as const },
      { chord: chord('F12'), action: 'panic' as const },
    ];
    expect(boundKeys(keys, 'go')).toBe('Space / F1');
    expect(boundKeys(keys, 'stop_all', 'go')).toBe('Space / F1');
    expect(boundKeys(keys, 'panic')).toBe('Esc / F12');
    expect(boundKeys([], 'stop_all')).toBe('');
  });

  it('names every action', () => {
    expect(ACTION_NAMES.go).not.toBe('');
    for (const action of KEYS_ONLY) expect(ACTION_NAMES[action]).toBeTruthy();
  });
});
