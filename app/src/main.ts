import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import './styles.css';

import { mount } from 'svelte';

import App from './App.svelte';
import PanelWindow from './components/layout/PanelWindow.svelte';
import { events } from './lib/api';
import { openShowFile } from './lib/shows';
import { init, initLanguage } from './lib/store.svelte';
import { lockWebview } from './lib/webview';
import { dragByTouch, loadWindowSupport, markPlatform, trackFocus, trackMaximized } from './lib/window.svelte';
import { forwardDialogs, isMainWindow, serveDialogs, syncSelection, windowPanel } from './lib/windows.svelte';

await initLanguage();
lockWebview();
markPlatform();
trackMaximized();
trackFocus();
loadWindowSupport();
dragByTouch();
init(isMainWindow);
syncSelection();
if (isMainWindow) {
  events.onOpenRequested(openShowFile);
  serveDialogs();
} else {
  forwardDialogs();
}

const target = document.getElementById('app')!;

export default isMainWindow ? mount(App, { target }) : mount(PanelWindow, { target, props: { id: windowPanel! } });
