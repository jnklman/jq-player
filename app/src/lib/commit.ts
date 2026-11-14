// Committing the field being typed in. Fields send what was typed only when they lose the focus (like `change`), so
// until then the backend does not know of it: a save, a show being replaced or the program closing would miss it.
// These commit it first, while the field keeps the focus.

import { editsAnswered } from './api';
import { isTextField } from './shortcuts';
import { whenShown } from './store.svelte';

/** Event that makes an editable field (the detail text) commit what was typed, as if it lost the focus. */
export const COMMIT_EVENT = 'jq-commit';

/**
 * Commits what is being typed in the focused field of this window; the edit is sent before this returns. An input
 * commits (`change`) while it loses the focus, so it gets the focus back at once, before anything is painted (no
 * flicker); the detail text (CodeMirror) notices a lost focus only later and commits on `COMMIT_EVENT` instead.
 */
export function commitTyping() {
  const field = document.activeElement;
  if (!(field instanceof HTMLElement) || !isTextField(field)) return;
  if (field.isContentEditable) {
    field.dispatchEvent(new Event(COMMIT_EVENT));
    return;
  }
  field.blur();
  if (field.isConnected && (document.activeElement === null || document.activeElement === document.body)) {
    field.focus({ preventScroll: true });
  }
}

/** Like `commitTyping`, and resolves once the backend has the edit and the view shows it (e.g. `dirty`). */
export async function commitFields(): Promise<void> {
  commitTyping();
  const revisions = await editsAnswered();
  if (revisions.length) await whenShown(Math.max(...revisions));
}
