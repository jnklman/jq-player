// OSC arguments as typed in the inspector: `1` int, `1.0` or `0,5` float,
// anything else text; text with spaces (or that looks like a number) in quotes.

import type { OscArg } from './api';

const INT = /^-?\d+$/;
const FLOAT = /^-?(\d+[.,]\d*|[.,]\d+)(e-?\d+)?$/i;

export function parseOscArgs(text: string): OscArg[] {
  const tokens = text.match(/"[^"]*"|\S+/g) ?? [];
  return tokens.map((token) => {
    if (token.startsWith('"') && token.endsWith('"') && token.length >= 2) return { string: token.slice(1, -1) };
    if (INT.test(token)) return { int: parseInt(token, 10) };
    if (FLOAT.test(token)) return { float: Number(token.replace(',', '.')) };
    return { string: token };
  });
}

export function formatOscArgs(args: OscArg[]): string {
  return args
    .map((arg) => {
      if ('int' in arg) return String(arg.int);
      if ('float' in arg) return Number.isInteger(arg.float) ? arg.float.toFixed(1) : String(arg.float);
      const plain = arg.string !== '' && !/\s|"/.test(arg.string) && !INT.test(arg.string) && !FLOAT.test(arg.string);
      return plain ? arg.string : `"${arg.string}"`;
    })
    .join(' ');
}
