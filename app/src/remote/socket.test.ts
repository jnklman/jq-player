import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { HEARTBEAT_TIMEOUT, type Identity, RemoteSocket, type SocketStatus } from './socket';

/** A WebSocket the test drives: what the page sent, and frames and closes from the "server". */
class MockSocket {
  static all: MockSocket[] = [];
  sent: unknown[] = [];
  closed = false;
  onmessage: ((message: { data: string }) => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;

  constructor(public url: string) {
    MockSocket.all.push(this);
  }

  send(text: string) {
    this.sent.push(JSON.parse(text));
  }

  close() {
    this.closed = true;
  }

  /** A frame from the server. */
  frame(frame: object) {
    this.onmessage?.({ data: JSON.stringify(frame) });
  }

  /** The server (or the network) closed it. */
  drop(code = 1006) {
    this.onclose?.({ code });
  }

  static get last(): MockSocket {
    return MockSocket.all[MockSocket.all.length - 1];
  }
}

const hello = (instance = 'run-1', role = 'view') => ({ type: 'hello', instance, client: 'c1', name: 'Tablet', role });

function setup(me: () => Promise<Identity | null> = async () => ({ id: 'c1', name: 'Tablet', role: 'view' })) {
  const statuses: SocketStatus[] = [];
  const hellos: boolean[] = [];
  const whoAmI = vi.fn(me);
  const socket = new RemoteSocket({
    url: 'ws://host:8642/ws',
    whoAmI,
    onStatus: (status) => statuses.push(status),
    onHello: (restarted) => hellos.push(restarted),
    offline: () => 'offline',
    WebSocket: MockSocket as unknown as typeof WebSocket,
  });
  socket.start();
  return { socket, statuses, hellos, whoAmI };
}

describe('RemoteSocket', () => {
  beforeEach(() => {
    MockSocket.all = [];
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('calls commands by id and hands events to their listeners', async () => {
    const { socket, statuses } = setup();
    await expect(socket.invoke('get_show')).rejects.toBe('offline');
    MockSocket.last.frame(hello());
    expect(statuses.at(-1)).toEqual({ connected: true, role: 'view', unknown: false, keys: null });

    const shown = socket.invoke('get_show');
    const refused = socket.invoke('action', { action: { action: 'go' } });
    expect(MockSocket.last.sent).toEqual([
      { type: 'call', id: 1, cmd: 'get_show', args: {} },
      { type: 'call', id: 2, cmd: 'action', args: { action: { action: 'go' } } },
    ]);
    MockSocket.last.frame({ type: 'reply', id: 2, err: 'Not allowed' });
    MockSocket.last.frame({ type: 'reply', id: 1, ok: { revision: 3, view: null } });
    await expect(shown).resolves.toEqual({ revision: 3, view: null });
    await expect(refused).rejects.toBe('Not allowed');

    const states: unknown[] = [];
    const unlisten = await socket.listen('state', (payload) => states.push(payload));
    MockSocket.last.frame({ type: 'event', name: 'state', payload: { playhead: 'a' } });
    unlisten();
    MockSocket.last.frame({ type: 'event', name: 'state', payload: { playhead: 'b' } });
    expect(states).toEqual([{ playhead: 'a' }]);

    MockSocket.last.frame({ type: 'role', role: 'control' });
    expect(statuses.at(-1)?.role).toBe('control');
    const keys = [{ chord: { code: 'F1', ctrl: false, shift: false, alt: false, label: '' }, action: 'go' }];
    MockSocket.last.frame({ type: 'keys', keys });
    expect(statuses.at(-1)?.keys).toEqual(keys);
  });

  it('counts the connection as lost when the pings stop, and reconnects', async () => {
    const { socket, statuses, whoAmI } = setup();
    MockSocket.last.frame(hello());
    const pending = socket.invoke('get_show');
    vi.advanceTimersByTime(HEARTBEAT_TIMEOUT - 500);
    MockSocket.last.frame({ type: 'ping' });
    vi.advanceTimersByTime(HEARTBEAT_TIMEOUT - 500);
    expect(statuses.at(-1)?.connected).toBe(true);

    vi.advanceTimersByTime(1500);
    expect(statuses.at(-1)?.connected).toBe(false);
    expect(MockSocket.last.closed).toBe(true);
    await expect(pending).rejects.toBe('offline');

    await vi.advanceTimersByTimeAsync(600);
    expect(whoAmI).toHaveBeenCalled();
    expect(MockSocket.all).toHaveLength(2);
    MockSocket.last.frame(hello());
    expect(statuses.at(-1)?.connected).toBe(true);
  });

  it('tells a restarted host from the same one', async () => {
    const { hellos } = setup();
    MockSocket.last.frame(hello('run-1'));
    MockSocket.last.drop();
    await vi.advanceTimersByTimeAsync(600);
    MockSocket.last.frame(hello('run-1'));
    MockSocket.last.drop();
    await vi.advanceTimersByTimeAsync(1100);
    MockSocket.last.frame(hello('run-2'));
    expect(hellos).toEqual([false, false, true]);
  });

  it('stops once the host removed the device or no longer knows it', async () => {
    const removed = setup();
    MockSocket.last.frame(hello());
    MockSocket.last.drop(4001);
    expect(removed.statuses.at(-1)).toEqual({ connected: false, role: null, unknown: true, keys: null });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(MockSocket.all).toHaveLength(1);
    expect(removed.whoAmI).not.toHaveBeenCalled();

    MockSocket.all = [];
    const forgotten = setup(async () => null);
    MockSocket.last.drop();
    await vi.advanceTimersByTimeAsync(600);
    expect(forgotten.whoAmI).toHaveBeenCalled();
    expect(forgotten.statuses.at(-1)?.unknown).toBe(true);
    expect(MockSocket.all).toHaveLength(1);
  });

  it('keeps trying while the host cannot be reached', async () => {
    const { whoAmI } = setup(async () => {
      throw new Error('network');
    });
    MockSocket.last.drop();
    await vi.advanceTimersByTimeAsync(600);
    await vi.advanceTimersByTimeAsync(1100);
    expect(whoAmI.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(MockSocket.all).toHaveLength(1);
  });
});
