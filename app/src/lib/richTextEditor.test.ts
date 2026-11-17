import { describe, expect, it, vi } from 'vitest';

import { COMMIT_EVENT } from './commit';
import { applyFormat, createRichTextEditor } from './richTextEditor';

function editor(text = 'hello world', extra: { disabled?: boolean } = {}) {
  const parent = document.createElement('div');
  document.body.append(parent);
  const onchange = vi.fn();
  const rich = createRichTextEditor(parent, {
    text,
    placeholder: 'Notes',
    disabled: extra.disabled ?? false,
    label: 'Detail text',
    onchange,
  });
  return { rich, parent, onchange, doc: () => rich.view.state.doc.toString() };
}

describe('createRichTextEditor', () => {
  it('shows the text with formats marked and the field labelled', () => {
    const { rich, parent } = editor('a **bold** word');
    expect(rich.view.contentDOM.getAttribute('aria-label')).toBe('Detail text');
    expect(parent.querySelector('.cm-content')?.textContent).toBe('a **bold** word');
    expect(parent.querySelectorAll('.cm-content span').length).toBeGreaterThan(0);
    rich.destroy();
  });

  it('formats the selection as one undo step', () => {
    const { rich, doc } = editor('hello world');
    rich.view.dispatch({ selection: { anchor: 0, head: 5 } });
    applyFormat(rich.view, 'bold');
    expect(doc()).toBe('**hello** world');
    rich.destroy();
  });

  it('takes the format keys', () => {
    const { rich, doc } = editor('hello');
    rich.view.dispatch({ selection: { anchor: 0, head: 5 } });
    const e = new KeyboardEvent('keydown', { key: 'i', code: 'KeyI', ctrlKey: true, bubbles: true, cancelable: true });
    rich.view.contentDOM.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(true);
    expect(doc()).not.toBe('hello');
    const plain = new KeyboardEvent('keydown', { key: 'q', code: 'KeyQ', bubbles: true, cancelable: true });
    rich.view.contentDOM.dispatchEvent(plain);
    expect(plain.defaultPrevented).toBe(false);
    rich.destroy();
  });

  it('sends a change on commit and on destroy, once', () => {
    const { rich, onchange } = editor('a');
    rich.view.contentDOM.dispatchEvent(new Event(COMMIT_EVENT));
    expect(onchange).not.toHaveBeenCalled();
    rich.view.dispatch({ changes: { from: 1, insert: 'b' } });
    rich.view.contentDOM.dispatchEvent(new Event(COMMIT_EVENT));
    expect(onchange).toHaveBeenCalledWith('ab');
    rich.view.dispatch({ changes: { from: 2, insert: 'c' } });
    rich.destroy();
    expect(onchange).toHaveBeenLastCalledWith('abc');
    expect(onchange).toHaveBeenCalledTimes(2);
  });

  it('loads another text without sending it', () => {
    const { rich, onchange, doc } = editor('first');
    rich.load('second');
    expect(doc()).toBe('second');
    rich.load('second');
    rich.destroy();
    expect(onchange).not.toHaveBeenCalled();
  });

  it('changes placeholder and access', () => {
    const { rich, parent } = editor('', { disabled: true });
    expect(rich.view.state.readOnly).toBe(true);
    rich.setDisabled(false);
    expect(rich.view.state.readOnly).toBe(false);
    rich.setPlaceholder('Other');
    expect(parent.textContent).toContain('Other');
    rich.destroy();
  });
});
