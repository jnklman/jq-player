import { tw } from '../../lib/ui';

// Columns of the cue list, shared by its header, rows and end row. The first
// column holds the chain arrows; the last one the drag handle (edit mode).
export const cueGrid = tw`grid min-w-175 grid-cols-[44px_28px_52px_minmax(0,1.4fr)_minmax(0,1.2fr)_70px_200px_16px] items-center gap-3 pr-4 pl-2`;

/** Row heights in px, including the border: cue rows (and the end row) and the lower scene markers. */
export const CUE_ROW_HEIGHT = 48;
export const SCENE_ROW_HEIGHT = 32;

const row = tw`cursor-default border-b border-row-line py-1.5 focus:outline-hidden`;

/** A row of the list, fixed height so the list never jumps. */
export const cueRow = `${row} ${tw`h-12`}`;

/** A scene marker's row, a little lower than a cue's. */
export const sceneRow = `${row} ${tw`h-8`}`;

/** The row at the playhead. */
export const headRow = tw`bg-row-next inset-ring inset-ring-primary`;

/** While dragging: the gap in the list is tinted, the row under the pointer floats. */
const dragSingle = tw`[&.sortable-drag]:rounded-md [&.sortable-drag]:bg-panel-2 [&.sortable-drag]:shadow-2xl [&.sortable-drag]:ring-1 [&.sortable-drag]:ring-primary/60 [&.sortable-ghost]:bg-primary/10 [&.sortable-ghost>*]:opacity-30`;

/** Dragging several cues (`data-drag-count` holds the label, the other rows
 * are hidden): the gap and the floating row show only the label, the
 * floating row as a stack of cards. */
const dragGroupGap = tw`[&.sortable-ghost]:data-drag-count:relative [&.sortable-ghost]:data-drag-count:after:absolute [&.sortable-ghost]:data-drag-count:after:inset-0 [&.sortable-ghost]:data-drag-count:after:flex [&.sortable-ghost]:data-drag-count:after:items-center [&.sortable-ghost]:data-drag-count:after:justify-center [&.sortable-ghost]:data-drag-count:after:text-[13px] [&.sortable-ghost]:data-drag-count:after:font-semibold [&.sortable-ghost]:data-drag-count:after:text-primary [&.sortable-ghost]:data-drag-count:after:content-[attr(data-drag-count)] [&.sortable-ghost[data-drag-count]>*]:opacity-0`;
const dragGroupFloat = tw`[&.sortable-drag]:data-drag-count:shadow-[5px_5px_0_-1px_var(--color-panel-2),5px_5px_0_0_var(--color-primary),10px_10px_0_-1px_var(--color-panel-2),10px_10px_0_0_var(--color-primary),0_25px_50px_-12px_rgb(0_0_0/0.5)] [&.sortable-drag]:data-drag-count:after:absolute [&.sortable-drag]:data-drag-count:after:inset-0 [&.sortable-drag]:data-drag-count:after:flex [&.sortable-drag]:data-drag-count:after:items-center [&.sortable-drag]:data-drag-count:after:justify-center [&.sortable-drag]:data-drag-count:after:text-[13px] [&.sortable-drag]:data-drag-count:after:font-semibold [&.sortable-drag]:data-drag-count:after:text-primary [&.sortable-drag]:data-drag-count:after:content-[attr(data-drag-count)] [&.sortable-drag[data-drag-count]>*]:opacity-0`;

export const dragRow = [dragSingle, dragGroupGap, dragGroupFloat].join(' ');
