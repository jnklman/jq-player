// Operating every control of a form once, as a person would, for tests that check each field reaches the backend.

import { EditorView } from '@codemirror/view';
import { fireEvent, screen, within } from '@testing-library/svelte';
import type { UserEvent } from '@testing-library/user-event';

import { COMMIT_EVENT } from '$lib/commit';

import { settle } from './store';

/** Picks the option after the current one (or the first) in a select. */
export async function choose(user: UserEvent, trigger: HTMLElement) {
  await user.click(trigger);
  await settle();
  const options = screen.queryAllByRole('option').filter((option) => !option.hasAttribute('data-disabled'));
  if (options.length === 0) return;
  const current = options.findIndex((option) => option.getAttribute('aria-selected') === 'true');
  await user.click(options[(current + 1) % options.length]);
  await settle();
}

/** The enabled, editable controls in `root`, by role. */
export function controls(root: HTMLElement): HTMLElement[] {
  const roles = ['spinbutton', 'textbox', 'switch', 'checkbox', 'combobox', 'slider', 'radio'];
  return roles
    .flatMap((role) => within(root).queryAllByRole(role))
    .filter(
      (element) =>
        !(element as HTMLInputElement).disabled &&
        !(element as HTMLInputElement).readOnly &&
        !element.hasAttribute('data-disabled') &&
        element.getAttribute('aria-readonly') !== 'true',
    );
}

/**
 * Changes `element` once: a number gets another value and is then emptied (for optional fields), text is typed, a
 * rich text field gets a letter, a select its next option, a slider one step; anything else is clicked.
 */
export async function operate(user: UserEvent, element: HTMLElement) {
  const role = element.getAttribute('role') ?? (element as HTMLInputElement).type;
  if (!element.isConnected) return;
  if (element.matches('input[type="number"]') || role === 'spinbutton') {
    const input = element as HTMLInputElement;
    const next = input.value === '' ? '1' : String(Number(input.value) === 1 ? 2 : 1);
    await fireEvent.change(input, { target: { value: next } });
    await fireEvent.blur(input);
    if (input.isConnected) {
      await fireEvent.change(input, { target: { value: '' } });
      await fireEvent.blur(input);
    }
  } else if (element.getAttribute('contenteditable') === 'true') {
    // jsdom has no `isContentEditable` and no editing: the change goes through CodeMirror, committed as on Ctrl+S.
    EditorView.findFromDOM(element)?.dispatch({ changes: { from: 0, insert: 'x' } });
    element.dispatchEvent(new Event(COMMIT_EVENT));
  } else if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
    element.focus();
    if (document.activeElement === element) {
      await user.clear(element);
      await user.type(element, '7');
      await user.tab();
    } else {
      // Another layer holds the focus (a closing popover): the value changes all the same.
      await fireEvent.input(element, { target: { value: '7' } });
    }
    await fireEvent.change(element);
  } else if (role === 'combobox') {
    await choose(user, element);
  } else if (role === 'slider') {
    element.focus();
    await user.keyboard('{ArrowRight}');
    await fireEvent.pointerUp(element);
  } else {
    await user.click(element);
  }
  await settle();
}
