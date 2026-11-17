import { describe, expect, it } from 'vitest';

import { lockWebview } from './webview';

lockWebview();

const key = (init: KeyboardEventInit, target: EventTarget = document.body) => {
  const e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  return e.defaultPrevented;
};

const menu = (target: EventTarget) => {
  const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
  target.dispatchEvent(e);
  return e.defaultPrevented;
};

describe('lockWebview', () => {
  it('blocks reloading, printing, finding and history navigation', () => {
    expect(key({ key: 'F5' })).toBe(true);
    expect(key({ key: 'BrowserBack' })).toBe(true);
    expect(key({ key: 'r', ctrlKey: true })).toBe(true);
    expect(key({ key: 'P', metaKey: true })).toBe(true);
    expect(key({ key: 'ArrowLeft', altKey: true })).toBe(true);
  });

  it('leaves other keys alone', () => {
    expect(key({ key: 'r' })).toBe(false);
    expect(key({ key: 's', ctrlKey: true })).toBe(false);
    expect(key({ key: 'ArrowLeft' })).toBe(false);
  });

  it('keeps the context menu only in text fields (outside development builds)', () => {
    document.body.innerHTML = '<input /><textarea></textarea><div></div>';
    const dev = import.meta.env.DEV;
    expect(menu(document.querySelector('div')!)).toBe(!dev);
    expect(menu(document.querySelector('input')!)).toBe(false);
    expect(menu(document.querySelector('textarea')!)).toBe(false);
  });
});
