// Numbers typed into fields: a German comma and a typographic minus count,
// empty means "no value".

/** The number in `text`; null if empty or not a number. */
export function parseNumber(text: string): number | null {
  const raw = text.trim().replace(',', '.').replace('−', '-');
  if (raw === '') return null;
  const value = Number(raw);
  return isFinite(value) ? value : null;
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
