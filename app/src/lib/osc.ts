// OSC arguments as typed in the inspector: `1` int, `1.0` or `0,5` float,
// anything else text; text with spaces (or that looks like a number) in quotes.

import type { OscArg } from './api';

const INT = /^-?\d+$/;
const FLOAT = /^-?(\d+[.,]\d*|[.,]\d+)(e[+-]?\d+)?$/i;

export function parseOscArgs(text: string): OscArg[] {
  const tokens = text.match(/"[^"]*"|\S+/g) ?? [];
  return tokens.map((token) => {
    if (token.startsWith('"') && token.endsWith('"') && token.length >= 2) return { string: token.slice(1, -1) };
    if (INT.test(token)) return { int: parseInt(token, 10) };
    if (FLOAT.test(token)) return { float: Number(token.replace(',', '.')) };
    return { string: token };
  });
}

/** A float that reads back as one: always with a decimal point, also in exponent form ("1.0e-7", "1.0e+21"). */
function floatText(value: number): string {
  const text = String(value);
  if (/e/i.test(text)) return text.replace(/^(-?\d+)(e)/i, '$1.0$2');
  return /^-?\d+$/.test(text) ? `${text}.0` : text;
}

export function formatOscArgs(args: OscArg[]): string {
  return args
    .map((arg) => {
      if ('int' in arg) return String(arg.int);
      if ('float' in arg) return floatText(arg.float);
      const plain = arg.string !== '' && !/\s|"/.test(arg.string) && !INT.test(arg.string) && !FLOAT.test(arg.string);
      return plain ? arg.string : `"${arg.string}"`;
    })
    .join(' ');
}
