import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type Layout, LayoutState, normalized, panel, panelsIn, split, storedWindow, without } from './layout.svelte';

const known = ['transport', 'cues', 'active', 'inspector'];

const base = (): Layout => ({
  root: split('vertical', [
    [panel('transport'), 20],
    [
      split('horizontal', [
        [panel('cues'), 70],
        [panel('active'), 30],
      ]),
      80,
    ],
  ]),
  hidden: ['inspector'],
  windows: [],
});

describe('normalized', () => {
  it('reads a layout stored before there were windows', () => {
    const { root, hidden } = base();
    const layout = normalized(JSON.parse(JSON.stringify({ root, hidden })), known, base());
    expect(layout.windows).toEqual([]);
    expect(panelsIn(layout.root)).toEqual(['transport', 'cues', 'active']);
  });

  it('keeps a panel in one place only', () => {
    const stored = {
      ...base(),
      windows: [
        { panel: 'cues', kind: 'window' },
        { panel: 'inspector', kind: 'overlay' },
        { panel: 'unknown', kind: 'window' },
        { panel: 'transport', kind: 'overlay', x: 10, y: 20, width: 300, height: 'big', opacity: 0.1 },
      ],
    };
    stored.root = split('horizontal', [
      [panel('cues'), 50],
      [panel('active'), 50],
    ]);
    const layout = normalized(stored, known, base());
    // In the tree and in a window: the tree wins; hidden and in a window: hidden wins.
    expect(layout.windows.map((w) => w.panel)).toEqual(['transport']);
    expect(layout.windows[0]).toEqual({
      panel: 'transport',
      kind: 'overlay',
      x: 10,
      y: 20,
      width: 300,
      height: null,
      opacity: 0.3,
    });
    expect(panelsIn(layout.root)).toEqual(['cues', 'active']);
    expect(layout.hidden).toEqual(['inspector']);
  });

  it('adds a new panel only if it is nowhere yet', () => {
    const stored = { ...base(), hidden: [], windows: [{ panel: 'inspector', kind: 'window' }] };
    expect(panelsIn(normalized(stored, known, base()).root)).toEqual(['transport', 'cues', 'active']);
    const fresh = { ...base(), hidden: [] };
    expect(panelsIn(normalized(fresh, known, base()).root)).toContain('inspector');
  });
});

describe('storedWindow', () => {
  it('finds a panel window of the mode', () => {
    const layouts = { edit: { windows: [{ panel: 'active', kind: 'overlay', opacity: 0.5 }] }, show: {} };
    expect(storedWindow(layouts, 'edit', 'active')?.opacity).toBe(0.5);
    expect(storedWindow(layouts, 'show', 'active')).toBeUndefined();
    expect(storedWindow(null, 'edit', 'active')).toBeUndefined();
  });
});

describe('LayoutState', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const state = () => {
    const layout = new LayoutState(known, base);
    layout.restore(null);
    return layout;
  };

  it('takes a panel out into a window and back', () => {
    const layout = state();
    layout.place('active', 'overlay');
    expect(layout.placement('active')).toBe('overlay');
    expect(panelsIn(layout.root)).toEqual(['transport', 'cues']);
    expect(layout.windows).toEqual([
      { panel: 'active', kind: 'overlay', x: null, y: null, width: null, height: null, opacity: 1 },
    ]);
    layout.place('active', 'main');
    expect(layout.placement('active')).toBe('main');
    expect(layout.windows).toEqual([]);
    expect(panelsIn(layout.root)).toContain('active');
  });

  it('keeps place and opacity when the kind changes', () => {
    const layout = state();
    layout.place('cues', 'window');
    layout.windowMoved({ panel: 'cues', x: 1920, y: 40, width: 800, height: 600 });
    layout.setOpacity('cues', 0.6);
    layout.place('cues', 'overlay');
    expect(layout.window('cues')).toEqual({
      panel: 'cues',
      kind: 'overlay',
      x: 1920,
      y: 40,
      width: 800,
      height: 600,
      opacity: 0.6,
    });
  });

  it('does not rebuild the panes when a window moves', () => {
    const layout = state();
    layout.place('cues', 'window');
    const root = layout.root;
    layout.windowMoved({ panel: 'cues', x: 0, y: 0, width: 400, height: 300 });
    expect(layout.root).toBe(root);
  });

  it('keeps one panel in the main window', () => {
    const layout = state();
    layout.place('transport', 'window');
    layout.place('cues', 'hidden');
    expect(layout.canLeave('active')).toBe(false);
    layout.place('active', 'overlay');
    expect(layout.placement('active')).toBe('main');
    expect(without(layout.root, 'active')).toBeNull();
    // A window can still be switched off.
    layout.place('transport', 'hidden');
    expect(layout.placement('transport')).toBe('hidden');
  });

  it('moves a hidden panel into a window', () => {
    const layout = state();
    layout.place('inspector', 'window');
    expect(layout.hidden).toEqual([]);
    expect(layout.placement('inspector')).toBe('window');
  });

  it('closes the windows when reset', () => {
    const layout = state();
    layout.place('active', 'window');
    layout.reset();
    expect(layout.windows).toEqual([]);
  });
});
