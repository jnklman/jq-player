import { SvelteMap } from 'svelte/reactivity';

import type { Direction, LayoutNode, PanelId } from '../../lib/layout.svelte';

// Panels whose content has a natural height (the transport) must not get
// less: split sizes are percentages, which on a small screen or in a low
// window leave too little. Such a panel reports what it needs
// (`PanelFrame` with `fit`), and `LayoutTree` turns that into the least
// share of each vertical split for the current height. A pane given 0 %
// (the default layout) is thus exactly as high as its panels need.
// Widths work alike, from the fixed least width a panel declares
// (`PanelDef.minWidth`), for the horizontal splits.

/** Height (px) each fitting panel needs for its content, title row included. */
export const fitHeights = new SvelteMap<PanelId, number>();

/** Least width (px) of the panels that declare one (filled from `panels.ts`). */
export const minWidths = new SvelteMap<PanelId, number>();

/** Thickness of a divider between panes (`Resizable.Handle`, `h-4`/`w-4`). */
export const HANDLE_PX = 16;

/**
 * Least size (px) of a part of the layout along `direction` (vertical: height, horizontal: width): parts stacked in
 * that direction add up, side by side the largest counts.
 */
export function minSize(node: LayoutNode, direction: Direction): number {
  if (node.type === 'panel') return (direction === 'vertical' ? fitHeights : minWidths).get(node.panel) ?? 0;
  const parts = node.children.map((child) => minSize(child, direction));
  if (node.direction !== direction) return Math.max(0, ...parts);
  return parts.reduce((sum, part) => sum + part, 0) + HANDLE_PX * (parts.length - 1);
}

/** Least height (px) of a part of the layout. */
export const minHeight = (node: LayoutNode) => minSize(node, 'vertical');

/**
 * Least share (percent) of each pane of a split `size` px long in its direction: enough for what its panels need
 * where the split has room for it (`floor` for panes that need nothing), and never more than all of it together.
 */
export function minShares(children: LayoutNode[], direction: Direction, size: number, floor: number): number[] {
  const room = size - HANDLE_PX * (children.length - 1);
  if (!(room > 0)) return children.map(() => floor);
  const shares = children.map((child) => {
    const need = minSize(child, direction);
    return need > 0 ? (need * 100) / room : floor;
  });
  const total = shares.reduce((sum, share) => sum + share, 0);
  // Too little room for all: shared out in proportion, the rest scrolls.
  return total > 100 ? shares.map((share) => (share * 100) / total) : shares;
}
