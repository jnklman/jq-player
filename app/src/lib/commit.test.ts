import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { view } from '../test/fixtures';
import { loadShow } from '../test/store';
import { api } from './api';
import { COMMIT_EVENT, commitFields, commitTyping } from './commit';
import { ui } from './store.svelte';

describe('commitTyping', () => {
  it('commits an input by blurring it and gives it the focus back', () => {
    document.body.innerHTML = '<input />';
    const input = document.querySelector('input')!;
    input.focus();
    const committed = vi.fn();
    input.addEventListener('blur', committed);
    commitTyping();
    expect(committed).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(input);
  });

  it('asks an editable field to commit itself', () => {
    document.body.innerHTML = '<div contenteditable="true" tabindex="0"></div>';
    const field = document.querySelector('div')!;
    Object.defineProperty(field, 'isContentEditable', { value: true });
    field.focus();
    const committed = vi.fn();
    field.addEventListener(COMMIT_EVENT, committed);
    commitTyping();
    expect(committed).toHaveBeenCalledOnce();
  });

  it('does nothing without a focused field', () => {
    document.body.innerHTML = '<button></button>';
    const button = document.querySelector('button')!;
    button.focus();
    const blurred = vi.fn();
    button.addEventListener('blur', blurred);
    commitTyping();
    expect(blurred).not.toHaveBeenCalled();
  });

  it('leaves the focus where the commit put it', () => {
    document.body.innerHTML = '<input id="a" /><input id="b" />';
    const [a, b] = document.querySelectorAll('input');
    a.focus();
    a.addEventListener('blur', () => b.focus());
    commitTyping();
    expect(document.activeElement).toBe(b);
  });
});

describe('commitFields', () => {
  it('waits until the view shows the committed edit', async () => {
    await loadShow(view([], { dirty: false }));
    document.body.innerHTML = '<input />';
    const input = document.querySelector('input')!;
    input.focus();
    backend.onEdit((_, current) => ({ ...current!, dirty: true }));
    input.addEventListener('blur', () => void api.edit({ op: 'renumber' }));
    await commitFields();
    expect(ui.view?.dirty).toBe(true);
  });

  it('returns at once without pending edits', async () => {
    await commitFields();
    expect(backend.edits).toEqual([]);
  });
});
