// Which show view a window has. The backend numbers every view it sends (`ShowEvent.revision`); a command that
// changes the show answers with the number of the view it caused, which arrives as a `show` event on its own (the
// view is sent only once). Event and reply may come in either order, so a caller that goes on with the new view
// (selecting an added cue, say) waits for it here.

import type { Revision } from './api';

interface Waiting {
  revision: Revision;
  resolve: () => void;
}

export class Revisions {
  #current = 0;
  #waiting: Waiting[] = [];

  /** The number of the view shown; 0 before the first. */
  get current(): Revision {
    return this.#current;
  }

  /**
   * Takes a view numbered `revision` if it is not older than the one shown (an older one arriving late is dropped;
   * the same number again, as `getShow` reports it, is taken) and wakes those waiting for it.
   */
  accept(revision: Revision): boolean {
    if (revision < this.#current) return false;
    this.#current = revision;
    const due = this.#waiting.filter((waiting) => waiting.revision <= revision);
    this.#waiting = this.#waiting.filter((waiting) => waiting.revision > revision);
    due.forEach((waiting) => waiting.resolve());
    return true;
  }

  /**
   * Resolves once the view numbered `revision` (or a later one) is shown: at once if it is already, at the latest
   * after `timeout` ms, so a lost event never hangs the caller (it then goes on with the view it has).
   */
  reached(revision: Revision, timeout: number): Promise<void> {
    if (revision <= this.#current) return Promise.resolve();
    return new Promise((resolve) => {
      const waiting: Waiting = {
        revision,
        resolve: () => {
          clearTimeout(timer);
          resolve();
        },
      };
      const timer = setTimeout(() => {
        this.#waiting = this.#waiting.filter((other) => other !== waiting);
        resolve();
      }, timeout);
      this.#waiting.push(waiting);
    });
  }
}
