import { afterEach, describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { settle } from '../test/store';
import { dropSlot, edgeScroller, edgeSpeed, onFileDrop } from './fileDrop';

/** A container at viewport height `top` with rows of the given heights (jsdom lays nothing out). */
function rows(top: number, heights: number[]): HTMLElement {
  const container = document.createElement('div');
  container.getBoundingClientRect = () => ({ top }) as DOMRect;
  for (const height of heights) {
    const row = document.createElement('div');
    Object.defineProperty(row, 'offsetHeight', { value: height });
    container.append(row);
  }
  return container;
}

describe('dropSlot', () => {
  it('goes before the row whose upper half is hit, after it from its lower half on', () => {
    const container = rows(100, [40, 30, 40]);
    expect(dropSlot(container, 90)).toEqual({ index: 0, top: 0 });
    expect(dropSlot(container, 119)).toEqual({ index: 0, top: 0 });
    expect(dropSlot(container, 120)).toEqual({ index: 1, top: 40 });
    expect(dropSlot(container, 154)).toEqual({ index: 1, top: 40 });
    expect(dropSlot(container, 155)).toEqual({ index: 2, top: 70 });
    expect(dropSlot(container, 500)).toEqual({ index: 3, top: 110 });
  });

  it('goes first into an empty list', () => {
    expect(dropSlot(rows(0, []), 50)).toEqual({ index: 0, top: 0 });
  });
});

describe('onFileDrop', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('reports hovering in CSS pixels, with the paths from the start, then the drop', async () => {
    vi.stubGlobal('devicePixelRatio', 2);
    const over = vi.fn();
    const leave = vi.fn();
    const drop = vi.fn();
    const stop = onFileDrop({ over, leave, drop });
    await settle();
    await backend.emit('tauri://drag-enter', { paths: ['/a.wav'], position: { x: 20, y: 40 } });
    await backend.emit('tauri://drag-over', { position: { x: 30, y: 60 } });
    expect(over.mock.calls).toEqual([
      [10, 20, ['/a.wav']],
      [15, 30, ['/a.wav']],
    ]);
    await backend.emit('tauri://drag-drop', { paths: ['/a.wav'], position: { x: 30, y: 60 } });
    expect(drop).toHaveBeenCalledWith(15, 30, ['/a.wav']);
    expect(leave).toHaveBeenCalledTimes(1);
    await backend.emit('tauri://drag-leave', null);
    expect(leave).toHaveBeenCalledTimes(2);

    stop();
    await backend.emit('tauri://drag-drop', { paths: ['/b.wav'], position: { x: 0, y: 0 } });
    expect(drop).toHaveBeenCalledTimes(1);
  });

  it('stops listening when stopped before the listener is in place', async () => {
    const drop = vi.fn();
    onFileDrop({ over: vi.fn(), leave: vi.fn(), drop })();
    await settle();
    await backend.emit('tauri://drag-drop', { paths: ['/a.wav'], position: { x: 0, y: 0 } });
    expect(drop).not.toHaveBeenCalled();
  });
});

describe('edgeSpeed', () => {
  it('scrolls up near the top (below the header) and down near the bottom, faster closer to the edge', () => {
    // A list from 100 to 600 with a 30 px header: the upper zone ends at 162, the lower one starts at 568.
    expect(edgeSpeed(100, 600, 300, 30)).toBe(0);
    expect(edgeSpeed(100, 600, 162, 30)).toBe(0);
    // A quarter into the zone: a sixteenth of the way to full speed.
    expect(edgeSpeed(100, 600, 154, 30)).toBe(-2);
    expect(edgeSpeed(100, 600, 130, 30)).toBe(-16);
    expect(edgeSpeed(100, 600, 110, 30)).toBe(-16);
    expect(edgeSpeed(100, 600, 568, 30)).toBe(0);
    // Halfway: a quarter of the way.
    expect(edgeSpeed(100, 600, 584, 30)).toBe(5);
    expect(edgeSpeed(100, 600, 600, 30)).toBe(16);
  });

  it('stays still off the list', () => {
    expect(edgeSpeed(100, 600, 99)).toBe(0);
    expect(edgeSpeed(100, 600, 601)).toBe(0);
  });
});

describe('edgeScroller', () => {
  afterEach(() => vi.unstubAllGlobals());

  /** A 500 px high list at 100 that can scroll 1000 px, with animation frames run by hand. */
  function setup() {
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => frames.push(callback));
    vi.stubGlobal('cancelAnimationFrame', () => frames.splice(0));
    const element = document.createElement('div');
    element.getBoundingClientRect = () => ({ top: 100, bottom: 600 }) as DOMRect;
    let scrollTop = 0;
    Object.defineProperty(element, 'scrollTop', {
      get: () => scrollTop,
      set: (value: number) => (scrollTop = Math.max(0, Math.min(1000, value))),
    });
    const onScroll = vi.fn();
    const frame = () => frames.splice(0).forEach((callback) => callback(0));
    return { element, onScroll, frames, frame, scroller: edgeScroller(element, onScroll) };
  }

  it('keeps scrolling every frame while the pointer stays near an edge', () => {
    const { element, onScroll, frame, scroller } = setup();
    scroller.update(600);
    frame();
    frame();
    expect(element.scrollTop).toBe(32);
    expect(onScroll).toHaveBeenCalledTimes(2);
    // A second update doesn't start a second loop.
    scroller.update(600);
    frame();
    expect(element.scrollTop).toBe(48);
  });

  it('stops in the middle, off the list, when stopped and at the end of the list', () => {
    const { element, onScroll, frames, frame, scroller } = setup();
    scroller.update(600);
    frame();
    scroller.update(300);
    frame();
    expect(frames).toEqual([]);
    scroller.update(600);
    scroller.update(null);
    expect(frames).toEqual([]);
    scroller.update(100);
    frame();
    expect(element.scrollTop).toBe(0);
    expect(onScroll).toHaveBeenCalledTimes(2);
    scroller.stop();
    expect(frames).toEqual([]);
    expect(element.scrollTop).toBe(0);
  });
});
