// The editor of a detail text (CodeMirror): plain text with the formats of
// richText.ts shown as you type, the markers faint. CodeMirror keeps the
// text, its undo history, pasting (as plain text) and input methods.

import { history, historyKeymap, standardKeymap } from '@codemirror/commands';
import { Compartment, EditorState, Prec, RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  placeholder,
  ViewPlugin,
  type ViewUpdate,
} from '@codemirror/view';

import { type Format, parsePieces, pieceClass, toggleFormat } from './richText';
import { formatShortcut } from './shortcuts';
import { tw } from './ui';

function decorate(text: string): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const piece of parsePieces(text)) {
    const style = pieceClass(piece);
    if (style) builder.add(piece.from, piece.from + piece.text.length, Decoration.mark({ class: style }));
  }
  return builder.finish();
}

const formatting = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view.state.doc.toString());
    }
    update(update: ViewUpdate) {
      if (update.docChanged) this.decorations = decorate(update.state.doc.toString());
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

/** Switches `format` for the selection (see `toggleFormat`), as one undo step. */
export function applyFormat(view: EditorView, format: Format) {
  const { from, to } = view.state.selection.main;
  const change = toggleFormat(view.state.doc.toString(), from, to, format);
  view.dispatch({
    changes: { from: change.from, to: change.to, insert: change.text },
    selection: { anchor: change.start, head: change.end },
    userEvent: 'input.format',
  });
  view.focus();
}

// Looks of a text field (Textarea); the app's focus ring sits on the frame.
const theme = EditorView.theme({
  '&': { height: '100%' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { fontFamily: 'inherit', lineHeight: 'inherit' },
  '.cm-content': { padding: '0', caretColor: 'currentColor' },
  '.cm-line': { padding: '0' },
  '.cm-placeholder': { color: 'var(--color-dim)' },
});

export interface RichTextEditor {
  view: EditorView;
  /** Shows `text` as the field's new content, with a fresh undo history (another cue). */
  load(text: string): void;
  setPlaceholder(text: string): void;
  setDisabled(disabled: boolean): void;
  destroy(): void;
}

/**
 * An editor in `parent`; `onchange` gets the text when the field loses the focus after a change (as `change`), or
 * when the editor is destroyed with a change not sent yet.
 */
export function createRichTextEditor(
  parent: HTMLElement,
  options: { text: string; placeholder: string; disabled: boolean; label: string; onchange: (text: string) => void },
): RichTextEditor {
  const hint = new Compartment();
  const access = new Compartment();
  let committed = options.text;
  let currentHint = options.placeholder;
  let currentDisabled = options.disabled;

  const accessFor = (disabled: boolean) => [EditorView.editable.of(!disabled), EditorState.readOnly.of(disabled)];
  const stateFor = (text: string) =>
    EditorState.create({
      doc: text,
      extensions: [
        Prec.highest(
          EditorView.domEventHandlers({
            keydown(e, target) {
              const format = formatShortcut(e);
              if (!format) return false;
              e.preventDefault();
              applyFormat(target, format);
              return true;
            },
          }),
        ),
        history(),
        keymap.of([...historyKeymap, ...standardKeymap]),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({ 'aria-label': options.label, 'aria-multiline': 'true' }),
        EditorView.contentAttributes.of({ class: tw`outline-hidden` }),
        EditorView.updateListener.of((update) => {
          if (update.focusChanged && !update.view.hasFocus) flush();
        }),
        hint.of(placeholder(currentHint)),
        access.of(accessFor(currentDisabled)),
        formatting,
        theme,
      ],
    });

  const view = new EditorView({ parent, state: stateFor(options.text) });

  // Sends a change not sent yet; CodeMirror reports a lost focus only later, too late for an editor that goes away.
  const flush = () => {
    const current = view.state.doc.toString();
    if (current === committed) return;
    committed = current;
    options.onchange(current);
  };

  return {
    view,
    load(text) {
      committed = text;
      if (text !== view.state.doc.toString()) view.setState(stateFor(text));
    },
    setPlaceholder(text) {
      currentHint = text;
      view.dispatch({ effects: hint.reconfigure(placeholder(text)) });
    },
    setDisabled(disabled) {
      currentDisabled = disabled;
      view.dispatch({ effects: access.reconfigure(accessFor(disabled)) });
    },
    destroy() {
      flush();
      view.destroy();
    },
  };
}
