// The remote page (`remote.html`): served by the app's remote server to browsers on other devices. It shares the
// store and the panels with the app, but never Tauri: everything goes over the WebSocket (`socket.ts`). What it must
// not import is kept out by ESLint (`no-restricted-imports` for `src/remote`).

import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import '../styles.css';

import { mount } from 'svelte';

import { trackKeyboard } from '../lib/keys.svelte';
import { start } from './remote.svelte';
import RemoteApp from './RemoteApp.svelte';

trackKeyboard();
void start();

export default mount(RemoteApp, { target: document.getElementById('app')! });
