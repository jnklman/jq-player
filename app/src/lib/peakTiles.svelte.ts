// Detail peaks of one file for the waveform dialog, zoomed in past what the
// overview has: tiles of `TILE` peaks, a tile's density a power of two per
// second (its level), fetched around the view and evicted least recently used.

import { api } from './api';
import type { Stretch } from './waveform';

/** Peaks per detail tile. */
const TILE = 2048;
/** Detail tiles kept (about 8 kB each); the least recently used go first. */
const MAX_TILES = 128;
/** Detail tiles requested at a time; the rest follow, nearest to the view first. */
const MAX_LOADING = 3;

const tileLength = (level: number) => TILE / 2 ** level;

export class PeakTiles {
  // Keyed "level:index". Plain (not reactive): `changed` counts arrivals;
  // `generation` drops answers for a file read anew.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  private readonly tiles = new Map<string, Stretch>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  private readonly loading = new Set<string>();
  private generation = 0;
  private mediaDir = '';
  private file = '';
  /** Grows with every tile that arrives; read it to redraw. */
  changed = $state(0);

  /** Forgets all tiles; the next ones come from `file`. */
  reset(mediaDir: string, file: string) {
    this.tiles.clear();
    this.loading.clear();
    this.generation++;
    this.mediaDir = mediaDir;
    this.file = file;
  }

  /**
   * Asks for the tiles of `level` over the view (`from`, `span` seconds) and
   * what lies around it: two views ahead, one behind in `heading` (1 = later),
   * so the detail is there before it comes into view. Call again when a tile
   * arrives (`changed`), for the next ones.
   */
  request(level: number, from: number, span: number, heading: number, duration: number) {
    const middle = from + span / 2;
    const since = Math.max(0, from - (heading > 0 ? span : 2 * span));
    const until = Math.min(duration, from + span + (heading > 0 ? 2 * span : span));
    const length = tileLength(level);
    const wanted: number[] = [];
    for (let index = Math.floor(since / length); index * length < until; index++) wanted.push(index);
    const distance = (index: number) => Math.abs((index + 0.5) * length - middle);
    wanted.sort((left, right) => distance(left) - distance(right));
    for (const index of wanted) {
      const key = `${level}:${index}`;
      const tile = this.tiles.get(key);
      if (tile) {
        // Used: to the back of the eviction order.
        this.tiles.delete(key);
        this.tiles.set(key, tile);
      } else if (!this.loading.has(key) && this.loading.size < MAX_LOADING) {
        this.load(level, index);
      }
    }
  }

  /** The tile covering `secs`: of `level`, else the nearest finer or coarser one loaded. */
  at(level: number, secs: number): Stretch | undefined {
    for (const offset of [0, 1, -1, 2, -2, 3, -3]) {
      const tile = this.tiles.get(`${level + offset}:${Math.floor(secs / tileLength(level + offset))}`);
      if (tile && tile.peaks.length > 0 && tile.from <= secs && secs < tile.to) return tile;
    }
    return undefined;
  }

  private load(level: number, index: number) {
    const key = `${level}:${index}`;
    const from = index * tileLength(level);
    const current = this.generation;
    this.loading.add(key);
    api
      .waveformRange(this.mediaDir, this.file, from, from + tileLength(level), TILE)
      .then(
        (range) =>
          current === this.generation && this.tiles.set(key, { from, to: from + range.duration, peaks: range.peaks }),
        // Not asked for again; the overview stays there.
        () => current === this.generation && this.tiles.set(key, { from, to: from, peaks: [] }),
      )
      .finally(() => {
        if (current !== this.generation) return;
        this.loading.delete(key);
        // The oldest first: Map keeps insertion order, use re-inserts.
        for (const old of this.tiles.keys()) {
          if (this.tiles.size <= MAX_TILES) break;
          this.tiles.delete(old);
        }
        this.changed++;
      });
  }
}
