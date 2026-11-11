import { describe, expect, it } from 'vitest';

import { type Format, parsePieces, parseRichText, plainText, toggleFormat } from './richText';

describe('parseRichText', () => {
  it('reads each marker', () => {
    expect(parseRichText('a **b** *c* __d__ ~~e~~')).toEqual([
      { text: 'a ' },
      { text: 'b', bold: true },
      { text: ' ' },
      { text: 'c', italic: true },
      { text: ' ' },
      { text: 'd', underline: true },
      { text: ' ' },
      { text: 'e', strike: true },
    ]);
  });

  it('nests formats', () => {
    expect(parseRichText('***a***')).toEqual([{ text: 'a', bold: true, italic: true }]);
    expect(parseRichText('*x **y** z*')).toEqual([
      { text: 'x ', italic: true },
      { text: 'y', italic: true, bold: true },
      { text: ' z', italic: true },
    ]);
  });

  it('leaves unpaired markers and markers across lines as text', () => {
    expect(parseRichText('2 * 3 = 6')).toEqual([{ text: '2 * 3 = 6' }]);
    expect(parseRichText('**a\nb**')).toEqual([{ text: '**a' }, { text: '\n' }, { text: 'b**' }]);
    expect(parseRichText('****')).toEqual([{ text: '****' }]);
    expect(parseRichText('datei_name.wav')).toEqual([{ text: 'datei_name.wav' }]);
  });

  it('gives the plain text', () => {
    expect(plainText('**Achtung:** __leise__ einblenden')).toBe('Achtung: leise einblenden');
  });
});

/** Applies the toggle to `text`, whose selection is marked with `[` and `]`, and marks the new one. */
function toggle(text: string, format: Format): string {
  const start = text.indexOf('[');
  const end = text.indexOf(']') - 1;
  const value = text.replace('[', '').replace(']', '');
  const change = toggleFormat(value, start, end, format);
  const next = value.slice(0, change.from) + change.text + value.slice(change.to);
  return next.slice(0, change.start) + '[' + next.slice(change.start, change.end) + ']' + next.slice(change.end);
}

describe('toggleFormat', () => {
  it('wraps the selection', () => {
    expect(toggle('ein [Wort] hier', 'bold')).toBe('ein [**Wort**] hier');
    expect(toggle('[ zwei \n\nZeilen ]', 'italic')).toBe('[ *zwei* \n\n*Zeilen* ]');
  });

  it('inserts a pair at the caret', () => {
    expect(toggle('a []b', 'underline')).toBe('a __[]__b');
  });

  it('removes markers around or inside the selection', () => {
    expect(toggle('ein **[Wort]** hier', 'bold')).toBe('ein [Wort] hier');
    expect(toggle('ein [~~Wort~~] hier', 'strike')).toBe('ein [Wort] hier');
    expect(toggle('a __[]__b', 'underline')).toBe('a []b');
  });

  it('keeps the other format of combined markers', () => {
    expect(toggle('***[a]***', 'bold')).toBe('*[a]*');
    expect(toggle('***[a]***', 'italic')).toBe('**[a]**');
    expect(toggle('**[a]**', 'italic')).toBe('**[*a*]**');
    expect(toggle('*[a]*', 'bold')).toBe('*[**a**]*');
  });
});

describe('parsePieces', () => {
  it('gives every piece with its place in the text, markers included', () => {
    expect(parsePieces('a\n**b**')).toEqual([
      { text: 'a', from: 0 },
      { text: '\n', from: 1 },
      { text: '**', from: 2, marker: true },
      { text: 'b', from: 4, bold: true },
      { text: '**', from: 5, marker: true },
    ]);
  });
});
