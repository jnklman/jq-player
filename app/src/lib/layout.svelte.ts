// Layout of the main view: panels in nested splits that can be resized and
// rearranged, one layout for edit and one for show mode. A panel can also be
// taken out into a window of its own (`windows`, see `setPanelWindows`), so
// each panel is in exactly one place: the tree, `hidden` or `windows`. Kept
// with this machine's settings (`LocalSettings.layout`), which store them as
// they are (`{ edit, show }`); never part of the show. Only the main window
// changes it; panel windows send it their changes (`lib/windows.ts`). Which
// panels exist is decided by the registry in `components/layout/panels.ts`;
// this module only knows their ids.

import { api, type PanelWindow, type PanelWindowRect, type WindowKind } from './api';
import { attempt, type Mode, mode } from './store.svelte';

export type PanelId = string;
export type Direction = 'horizontal' | 'vertical';

export interface PanelNode {
  type: 'panel';
  panel: PanelId;
}

/** Children side by side (`horizontal`) or stacked (`vertical`); `sizes`
 * are percentages of the split, one per child, summing to 100. */
export interface SplitNode {
  type: 'split';
  direction: Direction;
  children: LayoutNode[];
  sizes: number[];
}

export type LayoutNode = PanelNode | SplitNode;

/** A side of a panel or of the whole workspace. */
export type Edge = 'left' | 'right' | 'top' | 'bottom';

/** Where a dragged panel lands on another one: beside it, or in its place (swap). */
export type DropZone = Edge | 'center';

/** A panel in a window of its own; `opacity` (0.3–1) is the overlay's background, the UI's alone. */
export interface DetachedPanel extends PanelWindow {
  opacity: number;
}

export interface Layout {
  root: LayoutNode;
  /** Panels the user switched off. */
  hidden: PanelId[];
  /** Panels in windows of their own while this mode is on. */
  windows: DetachedPanel[];
}

/** Where a panel is: in the main window, in a window of its own (of a kind), or switched off. */
export type Placement = 'main' | WindowKind | 'hidden';

export const MIN_OPACITY = 0.3;

const inWindow = (placement: Placement): placement is WindowKind => placement === 'window' || placement === 'overlay';

const clampOpacity = (value: unknown) => {
  const opacity = Number(value);
  return Number.isFinite(opacity) ? Math.min(1, Math.max(MIN_OPACITY, opacity)) : 1;
};

/** A stored number, or null. */
const maybe = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? value : null);

function parseWindow(raw: unknown): DetachedPanel | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const entry = raw as Record<string, unknown>;
  if (typeof entry.panel !== 'string') return null;
  return {
    panel: entry.panel,
    kind: entry.kind === 'overlay' ? 'overlay' : 'window',
    x: maybe(entry.x),
    y: maybe(entry.y),
    width: maybe(entry.width),
    height: maybe(entry.height),
    opacity: clampOpacity(entry.opacity),
  };
}

/** A panel window's entry in the stored layouts (`LocalSettings.layout`), as a panel window reads it about itself. */
export function storedWindow(layouts: unknown, mode: Mode, id: PanelId): DetachedPanel | undefined {
  const layout = (layouts as Partial<Record<Mode, { windows?: unknown }>> | null)?.[mode];
  const windows = Array.isArray(layout?.windows) ? (layout.windows as unknown[]) : [];
  return windows.map(parseWindow).find((entry) => entry?.panel === id) ?? undefined;
}

export function detachedPanel(id: PanelId, kind: WindowKind): DetachedPanel {
  return { panel: id, kind, x: null, y: null, width: null, height: null, opacity: 1 };
}

export const panel = (id: PanelId): PanelNode => ({ type: 'panel', panel: id });

/** A split from `[node, size]` pairs. */
export function split(direction: Direction, parts: [LayoutNode, number][]): SplitNode {
  return { type: 'split', direction, children: parts.map(([node]) => node), sizes: parts.map(([, size]) => size) };
}

export function panelsIn(node: LayoutNode): PanelId[] {
  return node.type === 'panel' ? [node.panel] : node.children.flatMap(panelsIn);
}

/** Scales sizes to sum to 100; unusable ones become equal shares. */
function scaled(sizes: number[]): number[] {
  const total = sizes.reduce((sum, size) => sum + size, 0);
  if (!(total > 0) || sizes.some((size) => !(size >= 0))) return sizes.map(() => 100 / sizes.length);
  return sizes.map((size) => (size * 100) / total);
}

/** Drops empty splits, replaces one-child splits by their child and merges
 * a split into its parent when both run the same way. */
function tidy(node: LayoutNode): LayoutNode | null {
  if (node.type === 'panel') return node;
  const children: LayoutNode[] = [];
  const sizes: number[] = [];
  node.children.forEach((child, i) => {
    const kept = tidy(child);
    if (!kept) return;
    if (kept.type === 'split' && kept.direction === node.direction) {
      const inner = scaled(kept.sizes);
      kept.children.forEach((grandchild, j) => {
        children.push(grandchild);
        sizes.push((node.sizes[i] * inner[j]) / 100);
      });
    } else {
      children.push(kept);
      sizes.push(node.sizes[i]);
    }
  });
  if (children.length === 0) return null;
  if (children.length === 1) return children[0];
  return { ...node, children, sizes: scaled(sizes) };
}

/** The tree without `id`; `null` if nothing is left. */
export function without(node: LayoutNode, id: PanelId): LayoutNode | null {
  const drop = (node: LayoutNode): LayoutNode | null => {
    if (node.type === 'panel') return node.panel === id ? null : node;
    const kept = node.children.map((child, i) => [drop(child), node.sizes[i]] as const);
    const left = kept.filter(([child]) => child !== null) as [LayoutNode, number][];
    return { ...node, children: left.map(([child]) => child), sizes: left.map(([, size]) => size) };
  };
  const rest = drop(node);
  return rest && tidy(rest);
}

/** Moves panel `id` onto `target`: next to it on the given side, or for
 * `center` into its place, swapping the two. */
export function moved(root: LayoutNode, id: PanelId, target: PanelId, zone: DropZone): LayoutNode {
  if (id === target) return root;
  if (zone === 'center') {
    const swap = (node: LayoutNode): LayoutNode => {
      if (node.type === 'split') return { ...node, children: node.children.map(swap) };
      return node.panel === id ? panel(target) : node.panel === target ? panel(id) : node;
    };
    return swap(root);
  }
  const rest = without(root, id);
  if (!rest) return root;
  const direction: Direction = zone === 'left' || zone === 'right' ? 'horizontal' : 'vertical';
  const before = zone === 'left' || zone === 'top';
  const leaf = panel(id);

  const place = (node: LayoutNode): LayoutNode => {
    if (node.type === 'panel') {
      if (node.panel !== target) return node;
      return split(
        direction,
        before
          ? [
              [leaf, 50],
              [node, 50],
            ]
          : [
              [node, 50],
              [leaf, 50],
            ],
      );
    }
    // Beside a panel in a split running the same way: share its space.
    const at = node.children.findIndex((child) => child.type === 'panel' && child.panel === target);
    if (at >= 0 && node.direction === direction) {
      const children = [...node.children];
      const sizes = [...node.sizes];
      const half = sizes[at] / 2;
      sizes[at] = half;
      children.splice(before ? at : at + 1, 0, leaf);
      sizes.splice(before ? at : at + 1, 0, half);
      return { ...node, children, sizes };
    }
    return { ...node, children: node.children.map(place) };
  };
  return tidy(place(rest)) ?? root;
}

/** Share (percent) a panel docked at an edge of the workspace gets. */
export const DOCK_SIZE = 30;

/** Moves panel `id` to an edge of the workspace, spanning its full width or height. */
export function docked(root: LayoutNode, id: PanelId, edge: Edge): LayoutNode {
  const rest = without(root, id);
  if (!rest) return root;
  const direction: Direction = edge === 'left' || edge === 'right' ? 'horizontal' : 'vertical';
  const leaf: [LayoutNode, number] = [panel(id), DOCK_SIZE];
  const others: [LayoutNode, number] = [rest, 100 - DOCK_SIZE];
  return tidy(split(direction, edge === 'left' || edge === 'top' ? [leaf, others] : [others, leaf])) ?? root;
}

/** Adds a panel at the right edge. */
function added(root: LayoutNode, id: PanelId): LayoutNode {
  if (root.type === 'split' && root.direction === 'horizontal') {
    const share = 100 / (root.children.length + 1);
    return {
      ...root,
      children: [...root.children, panel(id)],
      sizes: [...root.sizes.map((size) => (size * (100 - share)) / 100), share],
    };
  }
  return split('horizontal', [
    [root, 70],
    [panel(id), 30],
  ]);
}

/** Checks stored data: drops unknown or repeated panels, repairs sizes and
 * adds known panels that are neither placed nor hidden (e.g. new ones). */
export function normalized(data: unknown, known: PanelId[], fallback: Layout): Layout {
  // A local scratch set, not state.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const seen = new Set<PanelId>();
  const check = (raw: unknown): LayoutNode | null => {
    if (typeof raw !== 'object' || raw === null) return null;
    const node = raw as { type?: unknown; panel?: unknown; direction?: unknown; children?: unknown; sizes?: unknown };
    if (node.type === 'panel') {
      if (typeof node.panel !== 'string' || !known.includes(node.panel) || seen.has(node.panel)) return null;
      seen.add(node.panel);
      return panel(node.panel);
    }
    if (node.type !== 'split' || !Array.isArray(node.children)) return null;
    const direction: Direction = node.direction === 'vertical' ? 'vertical' : 'horizontal';
    const sizes = Array.isArray(node.sizes) ? node.sizes : [];
    const parts = (node.children as unknown[])
      .map((child, i) => [check(child), Number(sizes[i])] as const)
      .filter(([child]) => child !== null) as [LayoutNode, number][];
    return { type: 'split', direction, children: parts.map(([child]) => child), sizes: parts.map(([, size]) => size) };
  };

  const stored = (typeof data === 'object' && data !== null ? data : {}) as Partial<Record<keyof Layout, unknown>>;
  const checked = check(stored.root);
  let root = checked && tidy(checked);
  if (!root) return fallback;
  const hidden = (Array.isArray(stored.hidden) ? stored.hidden : []).filter(
    (id, i, all): id is PanelId =>
      typeof id === 'string' && known.includes(id) && !seen.has(id) && all.indexOf(id) === i,
  );
  const windows: DetachedPanel[] = [];
  for (const raw of Array.isArray(stored.windows) ? stored.windows : []) {
    const entry = parseWindow(raw);
    if (!entry || !known.includes(entry.panel) || seen.has(entry.panel) || hidden.includes(entry.panel)) continue;
    if (!windows.some((other) => other.panel === entry.panel)) windows.push(entry);
  }
  for (const id of known) {
    if (!seen.has(id) && !hidden.includes(id) && !windows.some((window) => window.panel === id)) {
      root = added(root, id);
    }
  }
  return { root, hidden, windows };
}

/** Wait after the last change before saving, so dragging a divider sends one request. */
const SAVE_DELAY_MS = 400;

const modes: Mode[] = ['edit', 'show'];

/** The layouts as reactive state, saved with the machine settings on every
 * change. `root`, `hidden` and all changes refer to the current mode's layout. */
export class LayoutState {
  /** Replaced (never mutated) when panels move, so `{#key}` on a root
   * rebuilds the panes; only `sizes` are written in place while resizing. */
  #layouts = $state.raw<Record<Mode, Layout>>({
    edit: { root: panel(''), hidden: [], windows: [] },
    show: { root: panel(''), hidden: [], windows: [] },
  });
  /** Panels can be moved (switched in the "Ansicht" menu); not saved. */
  editing = $state(false);

  readonly #known: PanelId[];
  readonly #fallback: (mode: Mode) => Layout;
  #saveTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(known: PanelId[], fallback: (mode: Mode) => Layout) {
    this.#known = known;
    this.#fallback = fallback;
    this.#layouts = { edit: fallback('edit'), show: fallback('show') };
  }

  get root(): LayoutNode {
    return this.#layouts[mode()].root;
  }

  get hidden(): PanelId[] {
    return this.#layouts[mode()].hidden;
  }

  get windows(): DetachedPanel[] {
    return this.#layouts[mode()].windows;
  }

  get #current(): Layout {
    return this.#layouts[mode()];
  }

  /** Takes the layouts from the settings (`null`: the defaults) without saving them again. */
  restore(stored: unknown) {
    const data = (typeof stored === 'object' && stored !== null ? stored : {}) as Partial<Record<Mode, unknown>>;
    const layouts = { ...this.#layouts };
    for (const mode of modes) layouts[mode] = normalized(data[mode], this.#known, this.#fallback(mode));
    this.#layouts = layouts;
  }

  visible(id: PanelId): boolean {
    return !this.hidden.includes(id);
  }

  /** The panel's window while it is in one. */
  window(id: PanelId): DetachedPanel | undefined {
    return this.windows.find((window) => window.panel === id);
  }

  placement(id: PanelId): Placement {
    if (this.hidden.includes(id)) return 'hidden';
    return this.window(id)?.kind ?? 'main';
  }

  /** Whether the panel can leave the main window: the main window keeps at least one. */
  canLeave(id: PanelId): boolean {
    return this.placement(id) !== 'main' || without(this.root, id) !== null;
  }

  /** Moves a panel to the main window (at the right edge), into a window of its own or switches it off. */
  place(id: PanelId, placement: Placement) {
    const from = this.placement(id);
    if (from === placement || !this.canLeave(id)) return;
    const { root, hidden, windows } = this.#current;
    if (inWindow(from) && inWindow(placement)) {
      // From one kind of window to the other: the window keeps its place.
      this.#set({ root, hidden, windows: windows.map((w) => (w.panel === id ? { ...w, kind: placement } : w)) });
      return;
    }
    // Take it out of where it is…
    const rest = {
      root: from === 'main' ? without(root, id)! : root,
      hidden: hidden.filter((other) => other !== id),
      windows: windows.filter((window) => window.panel !== id),
    };
    // …and put it where it goes; a window that only changes its kind keeps its place (above).
    if (placement === 'main') rest.root = added(rest.root, id);
    else if (placement === 'hidden') rest.hidden = [...rest.hidden, id];
    else rest.windows = [...rest.windows, { ...(this.window(id) ?? detachedPanel(id, placement)), kind: placement }];
    this.#set(rest);
  }

  /** The operator moved or resized a panel window; changes nothing that the panes depend on. */
  windowMoved({ panel, x, y, width, height }: PanelWindowRect) {
    this.#updateWindow(panel, { x, y, width, height });
  }

  setOpacity(id: PanelId, opacity: number) {
    this.#updateWindow(id, { opacity: clampOpacity(opacity) });
  }

  #updateWindow(id: PanelId, change: Partial<DetachedPanel>) {
    const layout = this.#current;
    if (!layout.windows.some((window) => window.panel === id)) return;
    // The tree stays the same object, so the panes are not rebuilt.
    this.#set({ ...layout, windows: layout.windows.map((w) => (w.panel === id ? { ...w, ...change } : w)) });
  }

  /** Opens and closes the panel windows to match the current mode (see `setPanelWindows`). */
  applyWindows() {
    const windows: PanelWindow[] = this.windows.map(({ panel, kind, x, y, width, height }) => ({
      panel,
      kind,
      x,
      y,
      width,
      height,
    }));
    void attempt(api.setPanelWindows(windows));
  }

  move(id: PanelId, target: PanelId, zone: DropZone) {
    this.#set({ ...this.#current, root: moved(this.root, id, target, zone) });
  }

  dock(id: PanelId, edge: Edge) {
    this.#set({ ...this.#current, root: docked(this.root, id, edge) });
  }

  /** Shows or hides a panel; the last one in the main window stays. A shown panel goes to the main window. */
  setVisible(id: PanelId, visible: boolean) {
    if (visible === this.visible(id)) return;
    this.place(id, visible ? 'main' : 'hidden');
  }

  /** New sizes of a split after dragging a handle. */
  resize(node: SplitNode, sizes: number[]) {
    if (sizes.length !== node.children.length) return;
    node.sizes = sizes;
    this.#save();
  }

  reset() {
    this.#set(this.#fallback(mode()));
  }

  #set(layout: Layout) {
    this.#layouts = { ...this.#layouts, [mode()]: layout };
    this.#save();
  }

  #save() {
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => {
      // A snapshot: `sizes` are changed in place while resizing.
      attempt(api.setLayout(JSON.parse(JSON.stringify(this.#layouts))));
    }, SAVE_DELAY_MS);
  }
}
