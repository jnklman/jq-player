import Sortable from 'sortablejs';
import type { Attachment } from 'svelte/attachments';

import { grabbing } from './ui';

/** Makes the children of a container reorderable by dragging (SortableJS).
 *
 * Svelte owns the DOM, so the move SortableJS made is reverted as soon as
 * the drag ends and only reported through `onMove`, with the dragged element
 * and its new index among its siblings. The list then re-renders in the new
 * order from state, like after any other edit.
 *
 * Styling hooks: the placeholder in the list carries `sortable-ghost`, the
 * element following the pointer `sortable-drag`.
 *
 * `group` names the elements that move along with the dragged one (it is up
 * to `onMove` to move them too). While dragging they are hidden, and the
 * placeholder and the element following the pointer carry `data-drag-count`
 * with `label`. A group drag is reported even when the dragged element stays
 * where it was, since the others still join it. */
export function sortable(
  onMove: (item: HTMLElement, to: number) => void,
  options: Sortable.Options = {},
  group?: (item: HTMLElement) => { others: HTMLElement[]; label: string } | null,
): Attachment<HTMLElement> {
  return (node) => {
    // The node after the dragged element before the drag. Restoring by element
    // index isn't enough: Svelte keeps an anchor text node after the last
    // item, and the last item must go back before it, not after.
    let next: Node | null = null;
    // The hidden elements of a group drag. They stay in the DOM, so the
    // indices SortableJS reports still count them.
    let hidden: HTMLElement[] = [];
    let isGroup = false;
    const instance = Sortable.create(node, {
      animation: 150,
      // Draw the drag ourselves instead of using native HTML5 drag and drop:
      // looks the same on every webview and doesn't depend on Tauri's
      // drag-and-drop handling.
      forceFallback: true,
      fallbackOnBody: true,
      // A click that jitters is no drag (a group drag moves even in place).
      fallbackTolerance: 3,
      ...options,
      onStart: ({ item }) => {
        next = item.nextSibling;
        grabbing(true);
        const moving = group?.(item);
        isGroup = !!moving;
        if (!moving) return;
        hidden = moving.others;
        for (const element of hidden) element.hidden = true;
        item.dataset.dragCount = moving.label;
        // The floating copy exists by now (forceFallback); it was cloned before the label was set.
        if (Sortable.ghost) Sortable.ghost.dataset.dragCount = moving.label;
      },
      onEnd: ({ item, from, oldIndex, newIndex }) => {
        grabbing(false);
        for (const element of hidden) element.hidden = false;
        hidden = [];
        delete item.dataset.dragCount;
        const wasGroup = isGroup;
        isGroup = false;
        if (oldIndex == null || newIndex == null || (oldIndex === newIndex && !wasGroup)) return;
        from.insertBefore(item, next);
        onMove(item, newIndex);
      },
    });
    return () => {
      for (const element of hidden) element.hidden = false;
      instance.destroy();
      grabbing(false);
    };
  };
}
