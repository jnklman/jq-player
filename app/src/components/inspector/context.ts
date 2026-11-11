import { getContext, setContext } from 'svelte';

const LINKED = Symbol('linked');

/** Tells the inspector's fields whether the edited cue is linked with others (see `OwnMark`). */
export const setLinked = (linked: () => boolean) => setContext(LINKED, linked);
/** Call during component init; the getter is reactive. */
export const getLinked = () => getContext<(() => boolean) | undefined>(LINKED) ?? (() => false);
