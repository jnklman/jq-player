import { getContext, setContext, type Snippet } from 'svelte';

import type { PanelId } from '../../lib/layout.svelte';
import type { PanelHosts } from './hosts';

/** Lets a panel's header start moving the panel (provided by the workspace). */
export interface Arrange {
  enabled: () => boolean;
  start: (panel: PanelId, e: PointerEvent) => void;
}

const ARRANGE = Symbol('arrange');
const PANEL = Symbol('panel');

export const setArrange = (arrange: Arrange) => setContext(ARRANGE, arrange);
/** Undefined outside the workspace, e.g. for the inspector in the group dialog. */
export const getArrange = () => getContext<Arrange | undefined>(ARRANGE);

export const setPanelId = (id: PanelId) => setContext(PANEL, id);
export const getPanelId = () => getContext<PanelId | undefined>(PANEL);

const HOSTS = Symbol('hosts');

export const setPanelHosts = (hosts: PanelHosts) => setContext(HOSTS, hosts);
export const getPanelHosts = () => getContext<PanelHosts>(HOSTS);

const CHROME = Symbol('chrome');

/** In a panel window: its own controls at the end of the panel's title row, which also moves the window (`drag`)
 * unless the window has the system's title bar. */
export interface WindowChrome {
  controls: Snippet;
  drag: boolean;
}

export const setWindowChrome = (chrome: WindowChrome) => setContext(CHROME, chrome);
export const getWindowChrome = () => getContext<WindowChrome | undefined>(CHROME);
