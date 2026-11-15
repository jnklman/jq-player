// The undo history of a dialog's draft: the dialog edits a copy of the show's
// values and applies it as one undo step at the end, so while it is open undo
// and redo step through the draft's own changes (see `claimUndo` in commands.ts).

/** Undo and redo over the states of a draft; it holds copies, never the draft itself. */
export class DraftHistory<T> {
  private past: string[] = [];
  private future: string[] = [];

  /** `limit`: the most steps kept back. */
  constructor(private readonly limit = 200) {}

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  /** The draft goes from `before` to `after`: one step back to `before` (none if nothing changed); redo is gone. */
  record(before: T, after: T) {
    const was = JSON.stringify(before);
    if (was === JSON.stringify(after)) return;
    this.past.push(was);
    if (this.past.length > this.limit) this.past.shift();
    this.future = [];
  }

  /** The state before `current`, or null with nothing to undo; `current` can be redone. */
  undo(current: T): T | null {
    const back = this.past.pop();
    if (back === undefined) return null;
    this.future.push(JSON.stringify(current));
    return JSON.parse(back) as T;
  }

  /** The state undone from `current`, or null with nothing to redo. */
  redo(current: T): T | null {
    const forward = this.future.pop();
    if (forward === undefined) return null;
    this.past.push(JSON.stringify(current));
    return JSON.parse(forward) as T;
  }
}
