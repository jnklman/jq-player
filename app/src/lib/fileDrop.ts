// Files dragged onto the window from outside (Explorer, Finder, a file manager).
//
// Only Tauri's drag-drop event names the files' paths (an HTML5 drop carries their contents, not where they
// are), so it is on for the window (`dragDropEnabled`). Reordering lists doesn't use HTML5 drag and drop
// (`sortable` draws its own drag), so it is unaffected.

import { getCurrentWebview } from '@tauri-apps/api/webview';

export interface FileDropHandlers {
  /** Files are dragged over the point (`x`, `y`, in CSS pixels of the viewport). */
  over(x: number, y: number, paths: string[]): void;
  /** The drag left the window, was cancelled or ended in a drop. */
  leave(): void;
  /** The files were dropped at the point. */
  drop(x: number, y: number, paths: string[]): void;
}

/** Follows files dragged onto the window until the returned function is called. */
export function onFileDrop(handlers: FileDropHandlers): () => void {
  // `over` doesn't repeat the paths; they come with `enter`.
  let paths: string[] = [];
  let stopped = false;
  let unlisten: (() => void) | undefined;
  getCurrentWebview()
    .onDragDropEvent(({ payload }) => {
      if (payload.type === 'leave') {
        paths = [];
        handlers.leave();
        return;
      }
      // The webview fills the window, so its coordinates are the viewport's.
      const { x, y } = payload.position.toLogical(window.devicePixelRatio);
      if (payload.type === 'over') {
        handlers.over(x, y, paths);
        return;
      }
      paths = payload.paths;
      if (payload.type === 'enter') {
        handlers.over(x, y, paths);
        return;
      }
      paths = [];
      handlers.leave();
      handlers.drop(x, y, payload.paths);
    })
    .then((stop) => (stopped ? stop() : (unlisten = stop)))
    // Without the event there is just no dropping; nothing else depends on it.
    .catch(() => {});
  return () => {
    stopped = true;
    unlisten?.();
  };
}

/**
 * Where something dropped at height `y` (viewport) goes among the rows (the element children of
 * `container`): `index` is the row it goes before (the row count: after the last), `top` the gap's height
 * in `container`. The rows' heights count, not where they show, as a stuck (sticky) row sits elsewhere.
 */
export function dropSlot(container: HTMLElement, y: number): { index: number; top: number } {
  let top = 0;
  let index = 0;
  const offset = y - container.getBoundingClientRect().top;
  for (const row of container.children) {
    if (!(row instanceof HTMLElement)) continue;
    const height = row.offsetHeight;
    if (offset < top + height / 2) break;
    top += height;
    index++;
  }
  return { index, top };
}

/** How close to an edge (CSS px) a drag starts scrolling, and the fastest it goes (px per frame), reached at the
 * edge itself. In between the speed grows with the square of the depth: slow over most of the zone, fast only
 * right at the edge. */
const EDGE = 32;
const MAX_SPEED = 16;
const MIN_SPEED = 1;

/**
 * How fast to scroll for a drag at height `y` over a list spanning `top` to `bottom` (viewport): negative up,
 * positive down, faster closer to the edge; 0 in the middle or outside. The upper edge zone starts below a
 * sticky header of `inset`; over the header itself it scrolls fastest.
 */
export function edgeSpeed(top: number, bottom: number, y: number, inset = 0): number {
  if (y < top || y > bottom) return 0;
  const speed = (depth: number) => Math.round(MIN_SPEED + (MAX_SPEED - MIN_SPEED) * Math.min(1, depth / EDGE) ** 2);
  const fromTop = top + inset + EDGE - y;
  if (fromTop > 0) return -speed(fromTop);
  const fromBottom = y - (bottom - EDGE);
  return fromBottom > 0 ? speed(fromBottom) : 0;
}

/**
 * Scrolls `element` while a drag hovers near its top or bottom edge (`update` with the pointer's height, `null`
 * when it is off the list), every frame, also while the pointer holds still. `onScroll` runs after each step
 * that moved, e.g. to place a drop marker again.
 */
export function edgeScroller(element: HTMLElement, onScroll: () => void, inset = 0) {
  let speed = 0;
  let frame = 0;
  const step = () => {
    frame = 0;
    if (!speed) return;
    const before = element.scrollTop;
    element.scrollTop += speed;
    if (element.scrollTop !== before) onScroll();
    frame = requestAnimationFrame(step);
  };
  const stop = () => {
    speed = 0;
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
  };
  return {
    update(y: number | null) {
      if (y === null) return stop();
      const { top, bottom } = element.getBoundingClientRect();
      speed = edgeSpeed(top, bottom, y, inset);
      if (speed && !frame) frame = requestAnimationFrame(step);
    },
    stop,
  };
}
