// The panels of the main view. A new panel is one entry here: a component
// (built on `PanelFrame`) and an id; it then shows up in the "View" menu
// and is added to stored layouts on the next start. Each panel can also be
// taken out into a window of its own (`PanelWindow.svelte`).

import type { Component } from 'svelte';

import { t } from '../../lib/i18n/index.svelte';
import { type Layout, LayoutState, panel, split } from '../../lib/layout.svelte';
import type { Mode } from '../../lib/store.svelte';
import ActiveCues from '../active/ActiveCues.svelte';
import CueList from '../cuelist/CueList.svelte';
import Inspector from '../inspector/Inspector.svelte';
import PlayheadCue from '../PlayheadCue.svelte';
import StopwatchesPanel from '../StopwatchesPanel.svelte';
import Transport from '../Transport.svelte';
import { minWidths } from './fit.svelte';

export interface PanelDef {
  /** Stored in saved layouts; never rename. */
  id: string;
  /** Name in the "View" menu and while dragging (a getter: it follows the language). */
  title: string;
  component: Component;
  /** Least width (px) the layout gives the panel, where the window has room for it (`fit.svelte.ts`). */
  minWidth?: number;
}

export const panels: PanelDef[] = [
  {
    id: 'transport',
    get title() {
      return t('transport.title');
    },
    component: Transport,
  },
  {
    id: 'cues',
    get title() {
      return t('cuelist.title');
    },
    component: CueList,
    // Wide enough for the rows' columns (`cuelist/grid.ts`) without scrolling sideways.
    minWidth: 720,
  },
  {
    id: 'active',
    get title() {
      return t('active.title');
    },
    component: ActiveCues,
  },
  {
    id: 'inspector',
    get title() {
      return t('inspector.title');
    },
    component: Inspector,
  },
  {
    id: 'playhead',
    get title() {
      return t('playhead.title');
    },
    component: PlayheadCue,
  },
  {
    id: 'stopwatches',
    get title() {
      return t('stopwatches.title');
    },
    component: StopwatchesPanel,
  },
];

for (const def of panels) if (def.minWidth) minWidths.set(def.id, def.minWidth);

export const panelById = (id: string) => panels.find((def) => def.id === id);

// The transport across the top, as high as it needs (0 %, `fit.svelte.ts`);
// below it the cue list on the left, on the right the running cues above the
// inspector while editing, above the playhead cue in the show.
const defaultLayout = (mode: Mode): Layout => ({
  root: split('vertical', [
    [panel('transport'), 0],
    [
      split('horizontal', [
        [panel('cues'), 70],
        [
          split('vertical', [
            [panel('active'), 45],
            [panel(mode === 'edit' ? 'inspector' : 'playhead'), 55],
          ]),
          30,
        ],
      ]),
      100,
    ],
  ]),
  hidden: [mode === 'edit' ? 'playhead' : 'inspector', 'stopwatches'],
  windows: [],
});

export const layout = new LayoutState(
  panels.map((def) => def.id),
  defaultLayout,
);
