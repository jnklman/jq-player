// The remote page's way to the host: one WebSocket (`/ws`, the device's cookie goes along) carrying commands and
// events like Tauri's IPC does in the app (see `lib/transport.ts`); the frames are described in
// `app/src-tauri/src/remote/protocol.rs`. It reconnects on its own and counts the connection as lost when the host's
// pings stop, so the page never shows a frozen state as if it were live.

import type { KeyBinding } from '../lib/api';
import type { Transport, UnlistenFn } from '../lib/transport';

/** What a device may do; see `Role` in `protocol.rs`. */
export type Role = 'pending' | 'view' | 'control';

/** The device as the host knows it (`/api/me`, `/api/join`). */
export interface Identity {
  id: string;
  name: string;
  role: Role;
}

export interface SocketStatus {
  /** A connection is open and the host's pings come. */
  connected: boolean;
  /** Known to the host; null until the first hello. */
  role: Role | null;
  /** The host does not know this device (any more): it has to ask to join. */
  unknown: boolean;
  /** The device's own keys; null = the host's. */
  keys: KeyBinding[] | null;
}

type ServerFrame =
  | { type: 'hello'; instance: string; client: string; name: string; role: Role; keys: KeyBinding[] | null }
  | { type: 'role'; role: Role }
  | { type: 'keys'; keys: KeyBinding[] | null }
  | { type: 'event'; name: string; payload: unknown }
  | { type: 'reply'; id: number; ok?: unknown; err?: string }
  | { type: 'ping' };

/** Close code of a removed device (`CLOSE_REMOVED` in `server.rs`). */
const CLOSE_REMOVED = 4001;
/** Without a ping for this long the connection counts as lost (the host pings every second). */
export const HEARTBEAT_TIMEOUT = 3000;
const RETRY_FIRST = 500;
const RETRY_MAX = 5000;

export interface SocketOptions {
  /** `ws://host:port/ws`. */
  url: string;
  /** Asks the host who this device is (`/api/me`): null if unknown, throws if the host cannot be reached. */
  whoAmI: () => Promise<Identity | null>;
  /** The connection or the role changed. */
  onStatus: (status: SocketStatus) => void;
  /** A hello after the connection was (re)opened; `restarted`: the host is another program run than before. */
  onHello: (restarted: boolean) => void;
  /** Not lost to the page's timers in tests. */
  WebSocket?: typeof WebSocket;
  /** The message a call fails with while there is no connection. */
  offline: () => string;
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

export class RemoteSocket implements Transport {
  status: SocketStatus = { connected: false, role: null, unknown: false, keys: null };
  #options: SocketOptions;
  #socket: WebSocket | null = null;
  #handlers = new Map<string, Set<(payload: unknown) => void>>();
  #pending = new Map<number, Pending>();
  #nextId = 1;
  #instance: string | null = null;
  #lastFrame = 0;
  #retry = RETRY_FIRST;
  #retryTimer: ReturnType<typeof setTimeout> | undefined;
  #heartbeat: ReturnType<typeof setInterval> | undefined;
  #stopped = false;

  constructor(options: SocketOptions) {
    this.#options = options;
  }

  /** Opens the connection, and keeps it open until `stop`. */
  start() {
    this.#stopped = false;
    this.status = { ...this.status, unknown: false };
    this.#open();
  }

  stop() {
    this.#stopped = true;
    clearTimeout(this.#retryTimer);
    this.#close();
  }

  invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
    const socket = this.#socket;
    if (!socket || !this.status.connected) return Promise.reject(this.#options.offline());
    const id = this.#nextId++;
    return new Promise<T>((resolve, reject) => {
      this.#pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      socket.send(JSON.stringify({ type: 'call', id, cmd, args }));
    });
  }

  listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
    const set = this.#handlers.get(event) ?? new Set();
    this.#handlers.set(event, set);
    const wrapped = handler as (payload: unknown) => void;
    set.add(wrapped);
    return Promise.resolve(() => set.delete(wrapped));
  }

  #setStatus(change: Partial<SocketStatus>) {
    const next = { ...this.status, ...change };
    if (
      next.connected === this.status.connected &&
      next.role === this.status.role &&
      next.unknown === this.status.unknown &&
      next.keys === this.status.keys
    )
      return;
    this.status = next;
    this.#options.onStatus(next);
  }

  #open() {
    const Impl = this.#options.WebSocket ?? WebSocket;
    const socket = new Impl(this.#options.url);
    this.#socket = socket;
    // A socket that never opens or answers (host gone mid-way) counts as lost after the heartbeat time.
    this.#lastFrame = Date.now();
    this.#watch();
    socket.onmessage = (message) => {
      if (socket !== this.#socket) return;
      this.#lastFrame = Date.now();
      this.#receive(JSON.parse(String(message.data)) as ServerFrame);
    };
    socket.onclose = (event) => {
      if (socket !== this.#socket) return;
      this.#lost(event.code === CLOSE_REMOVED);
    };
  }

  #receive(frame: ServerFrame) {
    switch (frame.type) {
      case 'hello': {
        const restarted = this.#instance !== null && this.#instance !== frame.instance;
        this.#instance = frame.instance;
        this.#retry = RETRY_FIRST;
        this.#setStatus({ connected: true, role: frame.role, unknown: false, keys: frame.keys ?? null });
        this.#options.onHello(restarted);
        break;
      }
      case 'role':
        this.#setStatus({ role: frame.role });
        break;
      case 'keys':
        this.#setStatus({ keys: frame.keys ?? null });
        break;
      case 'event':
        this.#handlers.get(frame.name)?.forEach((handler) => handler(frame.payload));
        break;
      case 'reply': {
        const pending = this.#pending.get(frame.id);
        this.#pending.delete(frame.id);
        if (frame.err !== undefined) pending?.reject(frame.err);
        else pending?.resolve(frame.ok ?? null);
        break;
      }
      case 'ping':
        break;
    }
  }

  /** Counts the connection as lost once the pings stop. */
  #watch() {
    clearInterval(this.#heartbeat);
    this.#heartbeat = setInterval(() => {
      if (Date.now() - this.#lastFrame > HEARTBEAT_TIMEOUT) this.#lost(false);
    }, HEARTBEAT_TIMEOUT / 6);
  }

  #close() {
    clearInterval(this.#heartbeat);
    const socket = this.#socket;
    this.#socket = null;
    if (socket) {
      socket.onmessage = null;
      socket.onclose = null;
      socket.close();
    }
    const offline = this.#options.offline();
    this.#pending.forEach((pending) => pending.reject(offline));
    this.#pending.clear();
  }

  /** The connection broke (or the device was removed): calls fail, and it is opened again unless the host no
   * longer knows the device. */
  #lost(removed: boolean) {
    this.#close();
    this.#setStatus({ connected: false });
    if (this.#stopped) return;
    if (removed) {
      this.#setStatus({ role: null, unknown: true, keys: null });
      return;
    }
    clearTimeout(this.#retryTimer);
    this.#retryTimer = setTimeout(() => void this.#reconnect(), this.#retry);
    this.#retry = Math.min(this.#retry * 2, RETRY_MAX);
  }

  async #reconnect() {
    if (this.#stopped) return;
    // A WebSocket refused for its cookie looks like any other failure: ask first.
    let me: Identity | null;
    try {
      me = await this.#options.whoAmI();
    } catch {
      this.#lost(false);
      return;
    }
    if (this.#stopped) return;
    if (!me) {
      this.#setStatus({ role: null, unknown: true, keys: null });
      return;
    }
    this.#open();
  }
}
