import { afterEach, describe, expect, it } from 'vitest';

import { panel, split } from '../../lib/layout.svelte';
import { fitHeights, HANDLE_PX, minHeight, minShares, minSize, minWidths } from './fit.svelte';

describe('fit', () => {
  afterEach(() => {
    fitHeights.clear();
    minWidths.clear();
  });

  const workspace = () =>
    split('vertical', [
      [panel('transport'), 20],
      [
        split('horizontal', [
          [panel('cues'), 70],
          [panel('active'), 30],
        ]),
        80,
      ],
    ]);

  it('gives the transport its height on a low screen', () => {
    fitHeights.set('transport', 170);
    const root = workspace();
    if (root.type !== 'split') throw new Error('split');
    // 560 px workspace: 20 % would be 109 px.
    const [transport, rest] = minShares(root.children, 'vertical', 560, 8);
    expect(((560 - HANDLE_PX) * transport) / 100).toBeCloseTo(170);
    expect(rest).toBe(8);
  });

  it('adds stacked panels and takes the highest side by side', () => {
    fitHeights.set('transport', 100);
    fitHeights.set('active', 50);
    const stacked = split('vertical', [
      [panel('transport'), 50],
      [panel('active'), 50],
    ]);
    expect(minHeight(stacked)).toBe(150 + HANDLE_PX);
    const beside = split('horizontal', [
      [panel('transport'), 50],
      [panel('active'), 50],
    ]);
    expect(minHeight(beside)).toBe(100);
  });

  it('never asks for more than the whole split', () => {
    fitHeights.set('transport', 400);
    const root = workspace();
    if (root.type !== 'split') throw new Error('split');
    const shares = minShares(root.children, 'vertical', 300, 8);
    expect(shares.reduce((sum, share) => sum + share, 0)).toBeCloseTo(100);
    expect(minShares(root.children, 'vertical', 0, 8)).toEqual([8, 8]);
  });

  it('asks no floor of a pane that has a height of its own', () => {
    fitHeights.set('transport', 100);
    const root = workspace();
    if (root.type !== 'split') throw new Error('split');
    // 2000 px workspace: the floor of 8 % would be 159 px.
    const [transport] = minShares(root.children, 'vertical', 2000, 8);
    expect(((2000 - HANDLE_PX) * transport) / 100).toBeCloseTo(100);
  });

  it('keeps a panel with a least width wide enough in a horizontal split', () => {
    minWidths.set('cues', 720);
    const root = workspace();
    if (root.type !== 'split') throw new Error('split');
    // Stacked under the transport, the row's width carries through (cues, divider, running cues).
    expect(minSize(root, 'horizontal')).toBe(720 + HANDLE_PX);
    const row = root.children[1];
    if (row.type !== 'split') throw new Error('split');
    const [cues, active] = minShares(row.children, 'horizontal', 1016, 8);
    expect(((1016 - HANDLE_PX) * cues) / 100).toBeCloseTo(720);
    expect(active).toBe(8);
  });
});
