// How the UI reaches the backend: `invoke` for commands, `listen` for events. In the app this is Tauri's IPC; the
// remote page (`src/remote`, a browser on another device) puts its WebSocket in place with `useTransport` before
// anything is sent. `api.ts` goes through here only, so both use the same commands and events.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';

export type UnlistenFn = () => void;

export interface Transport {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  /** Calls `handler` with the payload of every `event` until the returned function is called. */
  listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn>;
}

const tauri: Transport = {
  invoke: (cmd, args) => tauriInvoke(cmd, args),
  listen: (event, handler) => tauriListen(event, (e) => handler(e.payload as never)),
};

let current: Transport = tauri;

/** Sends commands and takes events through `transport` from now on; call it before the store starts. */
export function useTransport(transport: Transport) {
  current = transport;
}

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return current.invoke<T>(cmd, args);
}

export function listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return current.listen<T>(event, handler);
}
