import { describe, expect, it } from 'vitest';

import {
  clock,
  fmtClock,
  fmtCountdown,
  fmtDb,
  fmtLufs,
  fmtMegabytes,
  fmtMidi,
  fmtMidiShort,
  fmtPercent,
  fmtPosition,
  fmtRunning,
  fmtSecs,
  fmtTime,
  meterPercent,
  nearEnd,
  shortDevice,
} from './format';
import { i18n } from './i18n/index.svelte';

describe('fmtTime', () => {
  it('turns over to the next minute instead of printing 60', () => {
    expect(fmtTime(119.7)).toBe('2:00');
    expect(fmtTime(59.96)).toBe('1:00');
    expect(fmtTime(59.94)).toBe('59.9 s');
    expect(fmtTime(59.5)).toBe('59.5 s');
    expect(fmtTime(60)).toBe('1:00');
    expect(fmtTime(60.4)).toBe('1:00');
    expect(fmtTime(3599.6)).toBe('60:00');
  });

  it('shows plain values and the unset ones', () => {
    expect(fmtTime(0)).toBe('0.0 s');
    expect(fmtTime(-3)).toBe('0.0 s');
    expect(fmtTime(65)).toBe('1:05');
    expect(fmtTime(null)).toBe('∞');
  });
});

describe('rounded units', () => {
  it('carries a rounded size into the whole numbers', () => {
    expect(fmtMegabytes(9.96 * (1 << 20))).toBe('10 MB');
    expect(fmtMegabytes(5 * (1 << 20))).toBe('5.0 MB');
  });

  it('shows no sign for a level that rounds to zero', () => {
    expect(fmtDb(-0.04)).toBe('0.0 dB');
    expect(fmtDb(-3)).toBe('−3.0 dB');
    expect(fmtDb(0.5)).toBe('+0.5 dB');
    expect(fmtLufs(-0.01)).toBe('0.0 LUFS');
  });
});

describe('fmtMidi', () => {
  it('shows program numbers raw (0 … 127) and channels from 1', () => {
    expect(fmtMidi({ kind: 'program', channel: 0, program: 0 })).toBe('Program 0 · Channel 1');
    expect(fmtMidi({ kind: 'program', channel: 9, program: 127 })).toBe('Program 127 · Channel 10');
  });

  it('has a short form', () => {
    expect(fmtMidiShort({ kind: 'program', channel: 1, program: 5 })).toBe('PC 5 · Ch 2');
    expect(fmtMidiShort({ kind: 'cc', channel: 9, controller: 64 })).toBe('CC 64 · Ch 10');
    expect(fmtMidiShort({ kind: 'note', channel: 0, note: 60 })).toBe('C4 · Ch 1');
  });
});

describe('fmtRunning', () => {
  it('ticks the elapsed time with the remaining time', () => {
    expect(fmtRunning(1.04, 8.96)).toEqual(['1.0 s', '9.0 s']);
    expect(fmtRunning(1.06, 8.94)).toEqual(['1.0 s', '9.0 s']);
    expect(fmtRunning(1.0, 9.0)).toEqual(['1.0 s', '9.0 s']);
    // At most one step ahead, so both turn over together.
    expect(fmtRunning(61.5, 30)).toEqual(['1:02', '30.0 s']);
  });

  it('counts up without a known end', () => {
    expect(fmtRunning(3.27, null)).toEqual(['3.2 s', '∞']);
    expect(fmtRunning(3.27, Infinity)).toEqual(['3.2 s', '∞']);
  });
});

describe('other formats', () => {
  it('warns near the end only with a warning time', () => {
    expect(nearEnd(5, 10)).toBe(true);
    expect(nearEnd(11, 10)).toBe(false);
    expect(nearEnd(5, 0)).toBe(false);
    expect(nearEnd(null, 10)).toBe(false);
  });

  it('formats positions, seconds, percent and levels', () => {
    expect(fmtPosition(65.25)).toBe('1:05.250');
    expect(fmtPosition(12.5)).toBe('12.500 s');
    expect(fmtPosition(-1)).toBe('0.000 s');
    expect(fmtSecs(2)).toBe('2.0 s');
    expect(fmtPercent(0.256)).toBe('26 %');
    expect(fmtPercent(null)).toBe('∞ %');
    expect(fmtDb(null)).toBe('−∞ dB');
    expect(fmtDb(-120)).toBe('−∞ dB');
    expect(fmtLufs(null)).toBe('−∞ LUFS');
  });

  it('maps a level onto a meter between the limits', () => {
    const limits = { max_db: 0, min_db: -60 };
    expect(meterPercent(-30, limits)).toBe(50);
    expect(meterPercent(6, limits)).toBe(100);
    expect(meterPercent(-80, limits)).toBe(0);
    expect(meterPercent(null, limits)).toBe(0);
  });

  it('shortens device names and shows clocks', () => {
    expect(shortDevice('Speakers (Realtek Audio)')).toBe('Speakers');
    expect(shortDevice('(only)')).toBe('(only)');
    expect(fmtClock(59)).toBe('0:59');
    expect(fmtClock(3600)).toBe('1:00:00');
    expect(fmtClock(-5)).toBe('0:00');
    expect(fmtCountdown(0.2)).toBe('0:01');
    expect(fmtCountdown(-61)).toBe('−1:01');
    expect(clock(new Date(2026, 0, 1, 13, 5, 9))).toBe('13:05:09');
  });

  it('names MIDI notes and controllers', () => {
    expect(fmtMidi({ kind: 'cc', channel: 0, controller: 7 })).toBe('CC 7 · Channel 1');
    expect(fmtMidi({ kind: 'note', channel: 0, note: 71 })).toBe('Note B4 · Channel 1');
    i18n.locale = 'de';
    expect(fmtMidi({ kind: 'note', channel: 0, note: 71 })).toContain('H4');
  });
});
