// Numbers typed into fields: a German comma and a typographic minus count,
// empty means "no value".

/** The number in `text`; null if empty or not a number. */
export function parseNumber(text: string): number | null {
  const raw = text.trim().replace(',', '.').replace('−', '-');
  // Plain decimal notation (and an exponent, as a number field accepts it), not what `Number` also takes ("0x10", "1_0").
  if (!/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(raw)) return null;
  const value = Number(raw);
  return isFinite(value) ? value : null;
}

/** A number as a field shows it: without floating-point noise ("0.30000000000000004" → "0.3"), always with a point. */
export function plainNumber(value: number): string {
  return String(Number(value.toPrecision(12)));
}

/** The number in a field's change or input event; null if empty or not a number. */
export function numberOf(e: Event): number | null {
  return parseNumber((e.currentTarget as HTMLInputElement).value);
}

/** `value` within `min`…`max` (either may be left out). */
export function clamp(value: number, min = -Infinity, max = Infinity): number {
  return Math.max(min, Math.min(max, value));
}

/** To the millisecond, so times stay readable. */
export const roundMs = (secs: number) => Math.round(secs * 1000) / 1000;

/** A duration typed as "h:mm:ss", "m:ss" or whole minutes ("15"); null if empty or unreadable. */
export function parseClock(text: string): number | null {
  const parts = text.trim().split(':');
  if (parts.length > 3 || parts.some((part) => part.trim() === '')) return null;
  const numbers = parts.map(parseNumber);
  if (numbers.some((n) => n === null || n < 0)) return null;
  const [a, b, c] = numbers as number[];
  if (parts.length === 1) return a * 60;
  if (parts.length === 2) return a * 60 + b;
  return a * 3600 + b * 60 + c;
}
