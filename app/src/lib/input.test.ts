import { describe, expect, it } from 'vitest';

import { clamp, liveNumber, numberOf, parseClock, parseNumber, plainNumber, roundMs, showValue } from './input';

describe('parseNumber', () => {
  it('reads decimals with a point or comma and a typographic minus', () => {
    expect(parseNumber(' 1,5 ')).toBe(1.5);
    expect(parseNumber('−2')).toBe(-2);
    expect(parseNumber('.5')).toBe(0.5);
    expect(parseNumber('1e3')).toBe(1000);
  });

  it('rejects what is no plain number', () => {
    for (const bad of ['', ' ', '0x10', '1_0', 'Infinity', '1-2', '1,2,3', '1e', '--1']) {
      expect(parseNumber(bad), bad).toBeNull();
    }
  });
});

describe('plainNumber', () => {
  it('drops floating-point noise', () => {
    expect(plainNumber(0.1 + 0.2)).toBe('0.3');
    expect(plainNumber(12.5)).toBe('12.5');
    expect(plainNumber(-3)).toBe('-3');
  });
});

describe('fields', () => {
  it('reads the number of an event and clamps it', () => {
    const input = document.createElement('input');
    input.value = '2,5';
    const e = new Event('change');
    Object.defineProperty(e, 'currentTarget', { value: input });
    expect(numberOf(e)).toBe(2.5);
    expect(clamp(5, 0, 3)).toBe(3);
    expect(clamp(-5, 0)).toBe(0);
    expect(clamp(5)).toBe(5);
    expect(roundMs(1.23456)).toBe(1.235);
  });

  it('puts text back, also over an unreadable entry', () => {
    const input = document.createElement('input');
    input.value = '7';
    showValue(input, '');
    expect(input.value).toBe('');
    const bad = document.createElement('input');
    Object.defineProperty(bad, 'validity', { value: { badInput: true } });
    showValue(bad, '');
    expect(bad.value).toBe('');
  });

  it('writes on each edit, skipping unreadable ones, and shows the value when left', () => {
    let value: number | null = 3;
    const written: string[] = [];
    const handlers = liveNumber(
      () => value,
      (text) => written.push(text),
    );
    const input = document.createElement('input');
    const event = <T extends Event>(e: T) => Object.defineProperty(e, 'currentTarget', { value: input }) as never;
    input.value = '4';
    handlers.oninput(event(new Event('input')));
    expect(written).toEqual(['4']);
    Object.defineProperty(input, 'validity', { value: { badInput: true }, configurable: true });
    handlers.oninput(event(new Event('input')));
    expect(written).toEqual(['4']);
    Object.defineProperty(input, 'validity', { value: { badInput: false }, configurable: true });
    input.value = 'x';
    handlers.onkeydown(event(new KeyboardEvent('keydown', { key: 'Tab' })));
    expect(input.value).toBe('x');
    handlers.onkeydown(event(new KeyboardEvent('keydown', { key: 'Enter' })));
    expect(input.value).toBe('3');
    value = null;
    handlers.onblur(event(new Event('blur')));
    expect(input.value).toBe('');
  });
});

describe('parseClock', () => {
  it('reads h:mm:ss, m:ss and whole minutes', () => {
    expect(parseClock('15')).toBe(900);
    expect(parseClock('1:30')).toBe(90);
    expect(parseClock(' 1:02:03 ')).toBe(3723);
    expect(parseClock('0,5')).toBe(30);
  });

  it('rejects what it cannot read', () => {
    for (const bad of ['', '1:', ':5', '1:2:3:4', '-1', 'x', '1:-2']) expect(parseClock(bad), bad).toBeNull();
  });
});
