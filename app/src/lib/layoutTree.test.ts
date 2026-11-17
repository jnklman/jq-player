// Moving and docking panels in the layout tree, and how the layout is saved (see also layout.test.ts).

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import {
  docked,
  type Layout,
  LayoutState,
  moved,
  panel,
  panelsIn,
  split,
  type SplitNode,
  without,
} from './layout.svelte';

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

describe('tree operations', () => {
  it('drops a panel and tidies what is left', () => {
    const rest = without(base().root, 'active')!;
    expect(rest).toEqual(
      split('vertical', [
        [panel('transport'), 20],
        [panel('cues'), 80],
      ]),
    );
    expect(without(panel('cues'), 'cues')).toBeNull();
  });

  it('swaps two panels on the centre', () => {
    const root = moved(base().root, 'transport', 'active', 'center');
    expect(panelsIn(root)).toEqual(['active', 'cues', 'transport']);
    expect(moved(base().root, 'cues', 'cues', 'left')).toEqual(base().root);
  });

  it('puts a panel beside another, sharing its space in a split running the same way', () => {
    const root = moved(base().root, 'transport', 'active', 'left') as SplitNode;
    expect(panelsIn(root)).toEqual(['cues', 'transport', 'active']);
    expect(root.direction).toBe('horizontal');
    expect(root.sizes.map(Math.round)).toEqual([70, 15, 15]);
  });

  it('splits a panel across when the split runs the other way', () => {
    const root = moved(base().root, 'transport', 'cues', 'bottom') as SplitNode;
    const row = root.children[0] as SplitNode;
    expect(panelsIn(root)).toEqual(['cues', 'transport', 'active']);
    expect(row.direction).toBe('vertical');
    expect(row.sizes).toEqual([50, 50]);
  });

  it('cannot move the only panel', () => {
    expect(moved(panel('cues'), 'cues', 'x', 'top')).toEqual(panel('cues'));
    expect(docked(panel('cues'), 'cues', 'top')).toEqual(panel('cues'));
  });

  it('docks a panel along a whole edge', () => {
    const root = docked(base().root, 'active', 'left') as SplitNode;
    expect(root.direction).toBe('horizontal');
    expect(panelsIn(root)).toEqual(['active', 'transport', 'cues']);
    expect(root.sizes).toEqual([30, 70]);
    const bottom = docked(base().root, 'transport', 'bottom') as SplitNode;
    expect(panelsIn(bottom)).toEqual(['cues', 'active', 'transport']);
    expect(bottom.direction).toBe('vertical');
    expect(bottom.sizes).toEqual([70, 30]);
  });
});

describe('LayoutState', () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] }));
  afterEach(() => vi.useRealTimers());

  const state = () => {
    const layout = new LayoutState(known, base);
    layout.restore(null);
    return layout;
  };

  it('saves a change once it settles, and at once on flush', () => {
    const layout = state();
    layout.move('transport', 'active', 'center');
    layout.dock('cues', 'top');
    expect(backend.invoked('set_layout')).toEqual([]);
    vi.advanceTimersByTime(400);
    expect(backend.invoked('set_layout')).toHaveLength(1);
    const { layout: stored } = backend.invoked('set_layout')[0] as { layout: Record<string, Layout> };
    expect(panelsIn(stored.edit.root)[0]).toBe('cues');

    layout.flush();
    expect(backend.invoked('set_layout')).toHaveLength(1);
    layout.setVisible('inspector', true);
    layout.flush();
    expect(backend.invoked('set_layout')).toHaveLength(2);
  });

  it('shows and hides panels, but keeps one in the main window', () => {
    const layout = state();
    layout.setVisible('inspector', true);
    expect(layout.visible('inspector')).toBe(true);
    layout.setVisible('inspector', true);
    layout.setVisible('cues', false);
    expect(layout.hidden).toContain('cues');
  });

  it('takes new sizes of a split, not a wrong number of them', () => {
    const layout = state();
    const root = layout.root as SplitNode;
    layout.resize(root, [1, 2, 3]);
    expect(root.sizes).toEqual([20, 80]);
    layout.resize(root, [40, 60]);
    expect(root.sizes).toEqual([40, 60]);
  });

  it('sends its windows to the backend, without the opacity', () => {
    const layout = state();
    layout.place('active', 'window');
    layout.setOpacity('active', 0.5);
    layout.setOpacity('nope', 0.5);
    layout.applyWindows();
    expect(backend.invoked('set_panel_windows')).toEqual([
      { windows: [{ panel: 'active', kind: 'window', x: null, y: null, width: null, height: null }] },
    ]);
  });
});
