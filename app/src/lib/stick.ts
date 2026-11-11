import type { Attachment } from 'svelte/attachments';

/** Distance from the bottom (px) that still counts as "at the bottom";
 * absorbs subpixel rounding of scrollTop. */
const SLACK = 8;

/** Keeps a scroll container pinned to its bottom while content is added,
 * like a terminal or chat log. Scrolling up releases the pin; scrolling
 * back down to the bottom engages it again.
 *
 * The pin state is derived from the scroll position on every scroll event,
 * so our own programmatic scrolls (which land at the bottom) keep it engaged
 * and any user scroll away from the bottom releases it. */
export const stickToBottom: Attachment<HTMLElement> = (node) => {
  let stuck = true;

  const atBottom = () => node.scrollHeight - node.scrollTop - node.clientHeight <= SLACK;
  const pin = () => {
    if (stuck) node.scrollTop = node.scrollHeight;
  };

  const onScroll = () => {
    stuck = atBottom();
  };

  // Rows added or removed anywhere below the container. Text updates
  // (timers, levels) are characterData changes and don't trigger this.
  const mutations = new MutationObserver(pin);
  mutations.observe(node, { childList: true, subtree: true });

  // The container itself got smaller (window resize, layout change).
  const resize = new ResizeObserver(pin);
  resize.observe(node);

  node.addEventListener('scroll', onScroll, { passive: true });
  pin();

  return () => {
    node.removeEventListener('scroll', onScroll);
    mutations.disconnect();
    resize.disconnect();
  };
};
