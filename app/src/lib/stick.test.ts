import { describe, expect, it } from 'vitest';

import { stickToBottom } from './stick';

/** A scroll box with fixed sizes (jsdom has no layout). */
function scroller(scrollHeight: number, clientHeight: number) {
  const node = document.createElement('div');
  let height = scrollHeight;
  Object.defineProperty(node, 'scrollHeight', { get: () => height });
  Object.defineProperty(node, 'clientHeight', { value: clientHeight });
  document.body.append(node);
  return { node, grow: (by: number) => (height += by) };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('stickToBottom', () => {
  it('pins to the bottom as rows are added, until scrolled up', async () => {
    const { node, grow } = scroller(500, 100);
    const detach = stickToBottom(node)!;
    expect(node.scrollTop).toBe(500);

    grow(50);
    node.append(document.createElement('p'));
    await flush();
    expect(node.scrollTop).toBe(550);

    node.scrollTop = 100;
    node.dispatchEvent(new Event('scroll'));
    grow(50);
    node.append(document.createElement('p'));
    await flush();
    expect(node.scrollTop).toBe(100);

    // Back at the bottom (within the slack) it pins again.
    node.scrollTop = 600 - 100 - 5;
    node.dispatchEvent(new Event('scroll'));
    grow(50);
    node.append(document.createElement('p'));
    await flush();
    expect(node.scrollTop).toBe(650);

    detach();
    grow(50);
    node.append(document.createElement('p'));
    await flush();
    expect(node.scrollTop).toBe(650);
  });
});
