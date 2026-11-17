import { describe, expect, it } from 'vitest';

import { sound, view, wait } from '../test/fixtures';
import { i18n } from './i18n/index.svelte';
import {
  countKind,
  countOf,
  loopKind,
  loopOptions,
  optionsOf,
  outputOptions,
  secondTriggerOptions,
  withMissing,
} from './options';

describe('counts', () => {
  it('reads 0 and 1 as off', () => {
    expect(countKind(0)).toBe('off');
    expect(countKind(1)).toBe('off');
    expect(countKind(3)).toBe('times');
    expect(countKind('forever')).toBe('forever');
  });

  it('starts a new kind with a useful count, keeping a number of times', () => {
    expect(countOf('forever')).toBe('forever');
    expect(countOf('off', 5)).toBe(0);
    expect(countOf('times')).toBe(2);
    expect(countOf('times', 'forever')).toBe(2);
    expect(countOf('times', 7)).toBe(7);
  });

  it('only sound cues loop', () => {
    expect(loopKind(sound('s', { loop: 'forever' }))).toBe('forever');
    expect(loopKind(wait('w'))).toBe('off');
  });
});

describe('option lists', () => {
  it('reads labels when shown, so they follow the language', () => {
    const options = optionsOf({ a: 'A', b: 'B' });
    expect(options.map((o) => [o.value, o.label])).toEqual([
      ['a', 'A'],
      ['b', 'B'],
    ]);
    const label = () => loopOptions.find((o) => o.value === 'off')!.label;
    const english = label();
    i18n.locale = 'de';
    expect(label()).not.toBe(english);
    expect(secondTriggerOptions.map((o) => o.value)).toContain('exit_loop');
  });

  it('lists the outputs and keeps a value gone from the show', () => {
    const outputs = outputOptions(view());
    expect(outputs).toEqual([
      { value: 'main', label: 'Main' },
      { value: 'monitor', label: 'Monitor' },
    ]);
    expect(withMissing(outputs, 'main', '?')).toBe(outputs);
    expect(withMissing(outputs, null, '?')).toBe(outputs);
    expect(withMissing(outputs, 'gone', 'gone (missing)').at(-1)).toEqual({ value: 'gone', label: 'gone (missing)' });
  });
});
