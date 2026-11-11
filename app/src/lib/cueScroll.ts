// Scrolling the cue list to a row. Its sticky header and the scene marker stuck
// below it cover the top of the list, so `scrollIntoView` with a fixed scroll
// padding is not enough: what covers the row depends on where it ends up.

/** Height of the cue list's sticky header (`h-8`), including its border. */
export const LIST_HEADER_HEIGHT = 32;

/**
 * How far to scroll the list so `row` is fully visible: 0 if it is, negative
 * upwards. `below` is kept free under the row where the list is tall enough,
 * `air` above it.
 *
 * Scrolled up to the top edge, a cue row has the nearest scene marker above it
 * stuck over the list (the next marker is below the row, so it doesn't push
 * that one out yet); a scene marker sticks itself and is covered by the header only.
 */
export function rowScrollDelta(scroller: HTMLElement, row: HTMLElement, below = 0, air = 0): number {
  let cover = LIST_HEADER_HEIGHT;
  if (row.dataset.scene === undefined) {
    let previous = row.previousElementSibling;
    while (previous instanceof HTMLElement && previous.dataset.scene === undefined)
      previous = previous.previousElementSibling;
    if (previous instanceof HTMLElement) cover += previous.offsetHeight;
  }
  const rect = row.getBoundingClientRect();
  const viewport = scroller.getBoundingClientRect();
  const free = viewport.height > cover + air + rect.height + below ? below : 0;
  const top = viewport.top + cover + air;
  const bottom = viewport.bottom - free;
  if (rect.top < top) return rect.top - top;
  if (rect.bottom > bottom) return rect.bottom - bottom;
  return 0;
}

/** Scrolls the cue list to show `row`, like `scrollIntoView({ block: 'nearest' })`; rows of other lists just so. */
export function revealRow(row: HTMLElement, behavior: ScrollBehavior) {
  const scroller = row.closest<HTMLElement>('[data-slot="scroll-area-viewport"]');
  if (!scroller || !row.parentElement?.hasAttribute('data-cue-rows')) {
    row.scrollIntoView({ block: 'nearest', behavior });
    return;
  }
  const delta = rowScrollDelta(scroller, row, 12, 4);
  if (delta) scroller.scrollBy({ top: delta, behavior });
}
