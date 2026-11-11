// Simple formatting of a cue's detail text, kept as plain text with
// Markdown-like markers: **bold**, *italic*, __underlined__, ~~struck~~.
// The text stays readable as it is (cue list, show file); the playhead cue
// shows it formatted, the editor (richTextEditor.ts) with faint markers. Markers only pair up within a line; an unpaired one
// stays literal text.

import { tw } from './ui';

export type Format = 'bold' | 'italic' | 'underline' | 'strike';

/** The marker of each format, longest first per character, as the parser tries them. */
export const MARKERS: Record<Format, string> = {
  bold: '**',
  underline: '__',
  strike: '~~',
  italic: '*',
};

const ORDER: Format[] = ['bold', 'underline', 'strike', 'italic'];

export interface Segment {
  text: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
}

type Styles = Omit<Segment, 'text'>;

/** A piece of the text in one style, at `from`; `marker` for the markers themselves. */
export interface Piece extends Segment {
  from: number;
  marker?: boolean;
}

/**
 * Where the `marker` opened before `from` closes in `line`, or -1. Runs of the marker character count as a
 * whole: `**` inside `*…*` does not close it, and `***` closes both `**` and `*`.
 */
function closing(line: string, from: number, marker: string): number {
  const char = marker[0];
  for (let at = from + 1; at < line.length; at++) {
    if (line[at] !== char || line[at - 1] === char) continue;
    const count = runAfter(line, at, char);
    if (holds(marker, count)) return at + count - marker.length;
    at += count - 1;
  }
  return -1;
}

/** The pieces of `line`, which starts at `offset` in the whole text. */
function parseLine(line: string, offset: number, styles: Styles, out: Piece[]) {
  let plainFrom = 0;
  let index = 0;
  const flush = () => {
    if (index > plainFrom) out.push({ text: line.slice(plainFrom, index), from: offset + plainFrom, ...styles });
  };
  while (index < line.length) {
    const format = ORDER.find((candidate) => !styles[candidate] && line.startsWith(MARKERS[candidate], index));
    if (format) {
      const marker = MARKERS[format];
      const from = index + marker.length;
      const close = closing(line, from, marker);
      if (close > 0) {
        flush();
        out.push({ text: marker, from: offset + index, marker: true, ...styles });
        parseLine(line.slice(from, close), offset + from, { ...styles, [format]: true }, out);
        out.push({ text: marker, from: offset + close, marker: true, ...styles });
        index = close + marker.length;
        plainFrom = index;
        continue;
      }
    }
    index++;
  }
  flush();
}

/** Every piece of the text, markers included; line breaks are pieces of their own. */
export function parsePieces(text: string): Piece[] {
  const out: Piece[] = [];
  let offset = 0;
  text.split('\n').forEach((line, i) => {
    if (i > 0) out.push({ text: '\n', from: offset - 1 });
    parseLine(line, offset, {}, out);
    offset += line.length + 1;
  });
  return out;
}

/** The text in segments of one style each, without the markers; line breaks stay in the text. */
export function parseRichText(text: string): Segment[] {
  return parsePieces(text)
    .filter((piece) => !piece.marker)
    .map(({ text, bold, italic, underline, strike }) => {
      const segment: Segment = { text };
      Object.assign(segment, bold && { bold }, italic && { italic }, underline && { underline }, strike && { strike });
      return segment;
    });
}

/** The Tailwind classes of a piece's styles (also the markers', which are faint). */
export function pieceClass(piece: Styles & { marker?: boolean }): string {
  // Both lines in one class: `underline` and `line-through` set the same property.
  const lines =
    piece.underline && piece.strike
      ? tw`[text-decoration-line:underline_line-through]`
      : piece.underline
        ? tw`underline`
        : piece.strike
          ? tw`line-through`
          : '';
  return [piece.bold && tw`font-bold`, piece.italic && tw`italic`, lines, piece.marker && tw`opacity-40`]
    .filter(Boolean)
    .join(' ');
}

/** The text without its markers (for one-line previews). */
export function plainText(text: string): string {
  return parseRichText(text)
    .map((segment) => segment.text)
    .join('');
}

/** A text change: `text` replaces `from`..`to`, then `start`..`end` is selected (in the new text). */
export interface TextChange {
  from: number;
  to: number;
  text: string;
  start: number;
  end: number;
}

/** Whether a run of `count` marker characters holds `marker` (`***` holds both `**` and `*`). */
function holds(marker: string, count: number): boolean {
  return count === marker.length || (marker[0] === '*' && count === 3);
}

function runBefore(text: string, at: number, char: string): number {
  let count = 0;
  while (at - count > 0 && text[at - count - 1] === char) count++;
  return count;
}

function runAfter(text: string, at: number, char: string): number {
  let count = 0;
  while (text[at + count] === char) count++;
  return count;
}

/**
 * Switches `format` for the selection `start`..`end` of `value`: removes the markers around or at the edges
 * of the selection, or else puts them around it (per line, without the spaces at its ends). An empty
 * selection gets a pair of markers with the caret between them.
 */
export function toggleFormat(value: string, start: number, end: number, format: Format): TextChange {
  const marker = MARKERS[format];
  const size = marker.length;
  const char = marker[0];
  const selected = value.slice(start, end);

  // Markers just outside the selection.
  if (holds(marker, runBefore(value, start, char)) && holds(marker, runAfter(value, end, char))) {
    return { from: start - size, to: end + size, text: selected, start: start - size, end: end - size };
  }
  // Markers at the edges of the selection.
  if (
    selected.length > 2 * size &&
    holds(marker, runAfter(selected, 0, char)) &&
    holds(marker, runBefore(selected, selected.length, char))
  ) {
    const inner = selected.slice(size, -size);
    return { from: start, to: end, text: inner, start, end: start + inner.length };
  }
  if (start === end) {
    return { from: start, to: end, text: marker + marker, start: start + size, end: start + size };
  }
  const wrapped = selected
    .split('\n')
    .map((line) => {
      const match = /^(\s*)(.*?)(\s*)$/s.exec(line);
      const [, lead, body, trail] = match ?? ['', '', line, ''];
      return body ? `${lead}${marker}${body}${marker}${trail}` : line;
    })
    .join('\n');
  return { from: start, to: end, text: wrapped, start, end: start + wrapped.length };
}
