import type { Attachment } from 'svelte/attachments';
import { SvelteMap } from 'svelte/reactivity';

import type { LayoutNode, PanelId } from '../../lib/layout.svelte';

// Panels stay mounted while the layout shows them, also when it is rebuilt
// (switching between edit and show mode, moving panels): building the cue
// list of a large show anew stalls the UI for a moment. The workspace renders
// each shown panel once, into an element of its own (`host`), and the
// layout's slots take these elements in (`slot`). A panel the layout no
// longer shows is unmounted, so hidden panels do no work.

interface Host {
  element: HTMLElement;
  /** Scroll offsets inside the panel, put back after a move (detaching loses them). */
  scroll: Map<Element, { top: number; left: number }>;
}

export class PanelHosts {
  #hosts = new SvelteMap<PanelId, Host>();

  /** On a panel's own element. */
  host(id: PanelId): Attachment<HTMLElement> {
    return (element) => {
      const scroll = new Map<Element, { top: number; left: number }>();
      const remember = (e: Event) => {
        if (e.target instanceof Element) scroll.set(e.target, { top: e.target.scrollTop, left: e.target.scrollLeft });
      };
      element.addEventListener('scroll', remember, { capture: true, passive: true });
      this.#hosts.set(id, { element, scroll });
      return () => {
        element.removeEventListener('scroll', remember, { capture: true });
        if (this.#hosts.get(id)?.element === element) this.#hosts.delete(id);
        // Svelte only removes the wrapper it rendered; the element itself sits in a slot.
        element.remove();
      };
    };
  }

  /** On a layout slot: shows panel `id` there once it is rendered. */
  slot(id: PanelId): Attachment<HTMLElement> {
    return (slot) => {
      const host = this.#hosts.get(id);
      if (!host) return;
      slot.append(host.element);
      for (const [element, offset] of host.scroll) element.scrollTo(offset);
      return () => {
        if (host.element.parentNode === slot) host.element.remove();
      };
    };
  }
}

/** The panels `root` shows, in the order of `known` (keeps the rendered list from reordering). */
export function shownPanels(root: LayoutNode, known: PanelId[]): PanelId[] {
  const shown = new Set<PanelId>();
  const walk = (node: LayoutNode) => {
    if (node.type === 'panel') shown.add(node.panel);
    else node.children.forEach(walk);
  };
  walk(root);
  return known.filter((id) => shown.has(id));
}
