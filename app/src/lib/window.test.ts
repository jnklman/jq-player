import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { settle } from '../test/store';
import {
  dragByTouch,
  hasKeys,
  loadWindowSupport,
  markPlatform,
  trackFocus,
  trackMaximized,
  windowState,
  windowSupport,
} from './window.svelte';

describe('window support', () => {
  it('takes what the backend tells, keeping the guess when it fails', async () => {
    backend.on('window_support', () => ({ overlay: false, native_frame: true }));
    loadWindowSupport();
    await settle();
    expect({ ...windowSupport }).toEqual({ overlay: false, native_frame: true });
    backend.fail('window_support');
    loadWindowSupport();
    await settle();
    expect(windowSupport.native_frame).toBe(true);
  });
});

describe('focus', () => {
  it('counts the keys as lost only when no window of the app has the focus', async () => {
    backend.on('app_focused', () => false);
    vi.spyOn(document, 'hasFocus').mockReturnValue(false);
    trackFocus();
    await settle();
    vi.restoreAllMocks();
    window.dispatchEvent(new Event('blur'));
    expect(windowState.focused).toBe(false);
    expect(hasKeys()).toBe(false);
    await backend.emit('app_focus', true);
    expect(hasKeys()).toBe(true);
    await backend.emit('app_focus', false);
    window.dispatchEvent(new Event('focus'));
    expect(hasKeys()).toBe(true);
  });
});

describe('maximized', () => {
  it('marks the root while the window is maximized, and checks again on resize', async () => {
    backend.on('plugin:window|is_maximized', () => true);
    trackMaximized();
    await settle();
    expect(windowState.maximized).toBe(true);
    expect(document.documentElement.hasAttribute('data-maximized')).toBe(true);
    backend.on('plugin:window|is_maximized', () => false);
    await backend.emit('tauri://resize', { width: 1, height: 1 });
    await settle();
    expect(document.documentElement.hasAttribute('data-maximized')).toBe(false);
  });
});

describe('platform', () => {
  it('marks Windows', () => {
    markPlatform();
    expect(document.documentElement.hasAttribute('data-windows')).toBe(navigator.userAgent.includes('Windows'));
  });
});

describe('dragByTouch', () => {
  dragByTouch();

  function touch(target: Element, pointerType = 'touch') {
    const e = new PointerEvent('pointerdown', {
      bubbles: true,
      cancelable: true,
      composed: true,
      pointerType,
      isPrimary: true,
    });
    target.dispatchEvent(e);
    return e;
  }

  const drags = () => backend.invoked('plugin:window|start_dragging').length;

  it('drags from a drag region by touch, not by mouse', async () => {
    document.body.innerHTML = '<header data-tauri-drag-region><span>title</span></header>';
    const header = document.querySelector('header')!;
    expect(touch(header).defaultPrevented).toBe(true);
    touch(header, 'mouse');
    await settle();
    expect(drags()).toBe(1);
  });

  it('drags from inside only a "deep" region', async () => {
    document.body.innerHTML =
      '<header data-tauri-drag-region><span id="plain">a</span></header><nav data-tauri-drag-region="deep"><span id="deep">b</span></nav>';
    touch(document.getElementById('plain')!);
    touch(document.getElementById('deep')!);
    await settle();
    expect(drags()).toBe(1);
  });

  it('never drags from controls or "false" regions', async () => {
    document.body.innerHTML =
      '<header data-tauri-drag-region="deep"><button>x</button><div role="tab">t</div><div tabindex="0">f</div><div data-tauri-drag-region="false"><i>n</i></div></header><p>outside</p>';
    for (const selector of ['button', '[role=tab]', '[tabindex]', 'i', 'p']) touch(document.querySelector(selector)!);
    await settle();
    expect(drags()).toBe(0);
  });
});
