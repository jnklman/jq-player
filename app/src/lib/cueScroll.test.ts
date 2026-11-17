import { describe, expect, it, vi } from 'vitest';

import { LIST_HEADER_HEIGHT, revealRow, rowScrollDelta } from './cueScroll';

/** Gives `el` a fixed box (jsdom has no layout). */
function box(el: Element, top: number, height: number) {
  vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({ top, bottom: top + height, height } as DOMRect);
  Object.defineProperty(el, 'offsetHeight', { value: height, configurable: true });
}

function list(html: string) {
  document.body.innerHTML = `<div data-slot="scroll-area-viewport"><div data-cue-rows>${html}</div></div>`;
  const scroller = document.querySelector<HTMLElement>('[data-slot="scroll-area-viewport"]')!;
  box(scroller, 0, 400);
  return scroller;
}

describe('rowScrollDelta', () => {
  it('is 0 for a row in view', () => {
    const scroller = list('<div id="r"></div>');
    const row = document.getElementById('r')!;
    box(row, 100, 30);
    expect(rowScrollDelta(scroller, row)).toBe(0);
  });

  it('scrolls up past the header and the scene marker stuck under it', () => {
    const scroller = list('<div data-scene id="s"></div><div id="x"></div><div id="r"></div>');
    box(document.getElementById('s')!, -100, 24);
    const row = document.getElementById('r')!;
    box(row, 40, 30);
    expect(rowScrollDelta(scroller, row)).toBe(40 - (LIST_HEADER_HEIGHT + 24));
    // A scene marker itself is covered by the header only.
    const scene = document.getElementById('s')!;
    box(scene, 10, 24);
    expect(rowScrollDelta(scroller, scene, 0, 4)).toBe(10 - (LIST_HEADER_HEIGHT + 4));
  });

  it('scrolls down to show the row and some room below it where the list is tall enough', () => {
    const scroller = list('<div id="r"></div>');
    const row = document.getElementById('r')!;
    box(row, 390, 30);
    expect(rowScrollDelta(scroller, row, 12)).toBe(390 + 30 - (400 - 12));
    box(scroller, 0, 60);
    box(row, 50, 30);
    expect(rowScrollDelta(scroller, row, 12)).toBe(50 + 30 - 60);
  });
});

describe('revealRow', () => {
  it('scrolls the cue list by the delta, and other rows into view', () => {
    const scroller = list('<div id="r"></div>');
    const row = document.getElementById('r')!;
    box(row, 500, 30);
    scroller.scrollBy = vi.fn();
    revealRow(row, 'smooth');
    expect(scroller.scrollBy).toHaveBeenCalledWith({ top: 500 + 30 - (400 - 12), behavior: 'smooth' });

    box(row, 100, 30);
    revealRow(row, 'auto');
    expect(scroller.scrollBy).toHaveBeenCalledOnce();

    document.body.innerHTML = '<ul><li></li></ul>';
    const item = document.querySelector('li')!;
    item.scrollIntoView = vi.fn();
    revealRow(item, 'auto');
    expect(item.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest', behavior: 'auto' });
  });
});
