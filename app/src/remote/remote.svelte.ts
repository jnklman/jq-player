// The remote page's own state: whether this device is known to the host, the join request, and the connection that
// feeds the same store the app uses (`lib/store.svelte.ts`). The panels read `ui.remote` (role, connection, lock) to
// know whether their buttons operate the show (`canControl`).

import { t } from '../lib/i18n/index.svelte';
import { init, initLanguage, reloadShow, resetRevisions, ui } from '../lib/store.svelte';
import { useTransport } from '../lib/transport';
import { type Identity, RemoteSocket, type SocketStatus } from './socket';

export type Phase =
  /** Asking the host who this device is. */
  | 'starting'
  /** Unknown to the host: the device has to ask to join. */
  | 'join'
  /** Known (waiting, watching or operating): connected or reconnecting. */
  | 'live';

export const remote = $state({
  phase: 'starting' as Phase,
  /** The device's name as the host knows it. */
  name: '',
  /** The host could not be reached at all (starting or asking to join). */
  unreachable: false,
  /** Why the last join request failed, or that the host removed the device. */
  error: null as string | null,
  joining: false,
});

/** Where the lock of the controls is remembered on this device; a convenience, so it may be lost. */
const LOCK_KEY = 'jq-remote-locked';

function storedLock(): boolean {
  try {
    return localStorage.getItem(LOCK_KEY) === '1';
  } catch {
    return false;
  }
}

/** Locks or unlocks the controls on this device (against a GO from a pocket). */
export function setLocked(locked: boolean) {
  if (ui.remote) ui.remote = { ...ui.remote, locked };
  try {
    localStorage.setItem(LOCK_KEY, locked ? '1' : '0');
  } catch {
    // Not remembered; it still applies until the page is reloaded.
  }
}

/** Fetches `/api/me`: who the host takes this device for. Throws if the host does not answer. */
export async function whoAmI(): Promise<Identity | null> {
  const response = await fetch('/api/me', { cache: 'no-store' });
  if (!response.ok) throw new Error(response.statusText);
  return (await response.json()) as Identity | null;
}

let socket: RemoteSocket | null = null;
// The store is started once, when the device first sees the show; after that a new connection only asks again.
let started = false;
const RETRY_START = 2000;

/** Starts the page: asks who this device is and connects, or offers to join. */
export async function start(options: { WebSocket?: typeof WebSocket } = {}) {
  socket = new RemoteSocket({
    url: `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`,
    whoAmI,
    onStatus: statusChanged,
    onHello: (restarted) => void hello(restarted),
    offline: () => t('remote.offline'),
    WebSocket: options.WebSocket,
  });
  useTransport(socket);
  await identify();
}

async function identify() {
  let me: Identity | null;
  try {
    me = await whoAmI();
  } catch {
    remote.unreachable = true;
    setTimeout(() => void identify(), RETRY_START);
    return;
  }
  remote.unreachable = false;
  if (me) connect(me);
  else remote.phase = 'join';
}

function connect(me: Identity) {
  remote.name = me.name;
  remote.error = null;
  remote.phase = 'live';
  ui.remote = { role: me.role, connected: false, locked: storedLock() };
  socket?.start();
}

/** Asks the host to let this device in as `name`. */
export async function join(name: string) {
  remote.joining = true;
  remote.error = null;
  try {
    const response = await fetch('/api/join', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    });
    if (!response.ok) {
      remote.error = (await response.text()) || response.statusText;
      return;
    }
    remote.unreachable = false;
    connect((await response.json()) as Identity);
  } catch {
    remote.unreachable = true;
  } finally {
    remote.joining = false;
  }
}

function statusChanged(status: SocketStatus) {
  if (status.unknown) {
    // Removed by the host, or its address changed: ask again.
    socket?.stop();
    ui.remote = null;
    remote.phase = 'join';
    remote.error = t('remote.removed');
    return;
  }
  const was = ui.remote;
  ui.remote = {
    role: status.role ?? was?.role ?? 'pending',
    connected: status.connected,
    locked: was?.locked ?? false,
  };
  // Let in just now.
  if (was?.role === 'pending' && status.role !== 'pending' && status.connected) void showUp(false);
}

async function hello(restarted: boolean) {
  if (ui.remote?.role === 'pending') return;
  await showUp(restarted);
}

/** Gets the show once the device may see it: the store's first start, or the current view after a new connection
 * (events may have been missed meanwhile). */
async function showUp(restarted: boolean) {
  if (!started) {
    started = true;
    await initLanguage();
    await init(false);
    return;
  }
  // A restarted host counts its views from the start again.
  if (restarted) resetRevisions();
  await reloadShow();
}
