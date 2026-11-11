// Draws the visible part of the waveform dialog's waveform onto a canvas as
// wide as the view: one column per pixel, mirrored around the middle, with the
// fades' gain envelope over it.

import { type FadeSegment, levelOf, type Stretch } from './waveform';

export interface WaveformPicture {
  /** CSS pixels of the view. */
  width: number;
  height: number;
  /** Height of the ruler above the waveform, left blank. */
  ruler: number;
  pxPerSec: number;
  /** Content pixels scrolled out to the left. */
  scrollLeft: number;
  /** Peaks of the whole file. */
  overview: Stretch;
  /** A detail tile covering a time, where one is loaded. */
  detail: (secs: number) => Stretch | undefined;
  fades: FadeSegment[];
}

export function drawWaveform(canvas: HTMLCanvasElement, picture: WaveformPicture) {
  const { width, ruler, pxPerSec, scrollLeft } = picture;
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.round(width * ratio);
  canvas.height = Math.round(picture.height * ratio);
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  ctx.clearRect(0, 0, width, picture.height);
  const style = getComputedStyle(canvas);
  const color = (name: string) => style.getPropertyValue(name).trim();
  const height = picture.height - ruler;
  const middle = ruler + height / 2;

  // The loudest peak under each pixel, from a detail tile where one is loaded.
  ctx.fillStyle = color('--color-fg-2');
  for (let column = 0; column < width; column++) {
    const since = (scrollLeft + column) / pxPerSec;
    const until = since + 1 / pxPerSec;
    const peak = levelOf(picture.detail(since) ?? picture.overview, since, until);
    const half = Math.max(0.5, peak * (height / 2 - 4));
    ctx.fillRect(column, middle - half, 1, half * 2);
  }

  // What a fade takes away is shaded, its curve drawn over it.
  const xOf = (secs: number) => secs * pxPerSec - scrollLeft;
  const yOf = (gain: number) => ruler + (1 - gain) * height;
  ctx.strokeStyle = color('--color-fade');
  ctx.lineWidth = 1.5;
  for (const [from, to, gain] of picture.fades) {
    const steps = Math.max(8, Math.min(400, Math.ceil(((to - from) * pxPerSec) / 4)));
    const curve = Array.from({ length: steps + 1 }, (_, i) => {
      const pos = i / steps;
      return [xOf(from + pos * (to - from)), yOf(gain(pos))] as const;
    });
    ctx.beginPath();
    ctx.moveTo(xOf(from), ruler);
    for (const point of curve) ctx.lineTo(...point);
    ctx.lineTo(xOf(to), ruler);
    ctx.closePath();
    ctx.fillStyle = 'rgb(0 0 0 / 0.45)';
    ctx.fill();
    ctx.beginPath();
    for (const point of curve) ctx.lineTo(...point);
    ctx.stroke();
  }
}
