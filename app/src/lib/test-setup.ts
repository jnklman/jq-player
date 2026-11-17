// Runs before each test file: the fake Tauri backend (`src/test/backend.ts`), what jsdom lacks, and a clean state
// after every test.

import '@testing-library/jest-dom/vitest';
// Unmounts what `render` mounted after each test.
import '@testing-library/svelte/vitest';

import { cleanup } from '@testing-library/svelte';
import { afterEach, beforeEach } from 'vitest';

import { backend, installBackend } from '../test/backend';
import { resetUi } from '../test/store';
import { i18n } from './i18n/index.svelte';

installBackend();

// jsdom has no layout: these exist so components that measure or observe themselves can be mounted.
class NoResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= NoResizeObserver as unknown as typeof ResizeObserver;
globalThis.IntersectionObserver ??= NoResizeObserver as unknown as typeof IntersectionObserver;
window.matchMedia ??= ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addListener() {},
  removeListener() {},
  addEventListener() {},
  removeEventListener() {},
  dispatchEvent: () => false,
})) as typeof window.matchMedia;
Element.prototype.scrollIntoView ??= function () {};
Element.prototype.scrollTo ??= function (this: Element, options?: ScrollToOptions | number) {
  if (typeof options === 'object') {
    if (options.top !== undefined) this.scrollTop = options.top;
    if (options.left !== undefined) this.scrollLeft = options.left;
  }
} as Element['scrollTo'];
Element.prototype.scrollBy ??= function () {} as Element['scrollBy'];
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= function () {};
Element.prototype.releasePointerCapture ??= function () {};
Element.prototype.animate ??= function () {
  return { finished: Promise.resolve(), cancel() {}, play() {}, pause() {}, onfinish: null } as unknown as Animation;
};

// The UI starts in the system language (`navigator.languages`); tests expect English texts and numbers
// whatever the machine's locale is. A test of another language sets `i18n.locale` itself.
beforeEach(() => {
  i18n.locale = 'en';
});

afterEach(() => {
  // Unmounted first, so no component sees the show go away.
  cleanup();
  backend.reset();
  resetUi();
  document.body.innerHTML = '';
});
