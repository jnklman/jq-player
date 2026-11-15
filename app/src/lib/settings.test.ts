import { describe, expect, it } from 'vitest';

import type { Chord, ControlAction, KeyBinding } from './api';
import { bindKey, idOrNull, keysOf, positiveOrNull } from './settings';

const chord = (code: string): Chord => ({ code, ctrl: false, shift: false, alt: false, label: code });
const bind = (code: string, action: string): KeyBinding => ({ chord: chord(code), action: action as ControlAction });

describe('positiveOrNull', () => {
  it('takes whole numbers above 0 only', () => {
    expect(positiveOrNull('5')).toBe(5);
    expect(positiveOrNull(' 12 ')).toBe(12);
    expect(positiveOrNull('')).toBeNull();
    expect(positiveOrNull('0')).toBeNull();
    expect(positiveOrNull('12abc')).toBeNull();
    expect(positiveOrNull('1.5')).toBeNull();
  });
});

describe('idOrNull', () => {
  it('takes 0 and whole numbers only', () => {
    expect(idOrNull('0')).toBe(0);
    expect(idOrNull('7x')).toBeNull();
    expect(idOrNull('')).toBeNull();
  });
});

describe('bindKey', () => {
  it('does not duplicate a chord the action has in another slot', () => {
    const keys = [bind('KeyA', 'go'), bind('KeyB', 'go')];
    const { keys: next } = bindKey(keys, 'go' as ControlAction, 1, chord('KeyA'), []);
    expect(keysOf(next, 'go' as ControlAction).map((c) => c.code)).toEqual(['KeyA']);
  });

  it('moves a chord of another action here and reports it', () => {
    const keys = [bind('KeyA', 'go'), bind('KeyB', 'stop')];
    const { keys: next, displaced } = bindKey(keys, 'go' as ControlAction, 1, chord('KeyB'), []);
    expect(keysOf(next, 'go' as ControlAction).map((c) => c.code)).toEqual(['KeyA', 'KeyB']);
    expect(keysOf(next, 'stop' as ControlAction)).toEqual([]);
    expect(displaced).toHaveLength(1);
  });
});
