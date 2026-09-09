// websocket.test.ts — JeRyuWsClient frame-validation + protocol-guard tests.
//
// These pin the HLT-019 / HLT-031 hardening on `apps/web/src/api/websocket.ts`:
//   * Inbound frames are validated at runtime before any field is read, so
//     malformed / unknown JSON is dropped rather than cast blindly.
//   * The server `hello` frame's `protocol` is checked against `jeryu.ws.v1`;
//     a mismatch surfaces an error and tears the socket down instead of
//     resuming against a foreign cursor.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  JeRyuWsClient,
  type JeRyuWsClientHandlers,
  type RealtimeStatus,
} from '../websocket';

type Listener = (ev: unknown) => void;

/** Minimal scriptable WebSocket double driving the client's event handlers. */
class FakeWebSocket {
  static OPEN = 1;
  static instances: FakeWebSocket[] = [];

  readyState = FakeWebSocket.OPEN;
  sent: string[] = [];
  closed: { code?: number; reason?: string } | null = null;
  private listeners: Record<string, Listener[]> = {};

  constructor(public url: string) {
    FakeWebSocket.instances.push(this);
  }

  addEventListener(type: string, fn: Listener): void {
    (this.listeners[type] ??= []).push(fn);
  }

  removeEventListener(): void {}

  send(data: string): void {
    this.sent.push(data);
  }

  close(code?: number, reason?: string): void {
    this.closed = { code, reason };
    this.readyState = 3;
  }

  emit(type: string, ev: unknown): void {
    for (const fn of this.listeners[type] ?? []) fn(ev);
  }
}

function makeHandlers(): {
  handlers: JeRyuWsClientHandlers;
  events: unknown[];
  errors: { code: string; message: string }[];
  statuses: RealtimeStatus[];
  hellos: bigint[];
} {
  const events: unknown[] = [];
  const errors: { code: string; message: string }[] = [];
  const statuses: RealtimeStatus[] = [];
  const hellos: bigint[] = [];
  return {
    events,
    errors,
    statuses,
    hellos,
    handlers: {
      onStatus: (s) => statuses.push(s),
      onEvent: (e) => events.push(e),
      onSnapshotRequired: () => {},
      onError: (code, message) => errors.push({ code, message }),
      onHello: (seq) => hellos.push(seq),
    },
  };
}

describe('JeRyuWsClient frame validation', () => {
  beforeEach(() => {
    FakeWebSocket.instances = [];
    vi.stubGlobal('WebSocket', FakeWebSocket as unknown as typeof WebSocket);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function connect(handlers: JeRyuWsClientHandlers): {
    client: JeRyuWsClient;
    socket: FakeWebSocket;
  } {
    const client = new JeRyuWsClient({
      url: 'wss://example.test/api/v1/ws',
      initialSubscriptions: [],
      resumeFrom: null,
      handlers,
    });
    client.connect();
    const socket = FakeWebSocket.instances[0];
    socket.emit('open', {});
    return { client, socket };
  }

  it('accepts a well-formed hello frame on the expected protocol', () => {
    const h = makeHandlers();
    const { socket } = connect(h.handlers);
    socket.emit('message', {
      data: JSON.stringify({
        type: 'hello',
        server_time: '2026-01-01T00:00:00Z',
        current_seq: 5,
        protocol: 'jeryu.ws.v1',
      }),
    });
    expect(h.hellos).toEqual([5n]);
    expect(socket.closed).toBeNull();
    expect(h.errors).toHaveLength(0);
  });

  it('rejects a hello frame on a mismatched protocol and closes', () => {
    const h = makeHandlers();
    const { socket } = connect(h.handlers);
    socket.emit('message', {
      data: JSON.stringify({
        type: 'hello',
        server_time: '2026-01-01T00:00:00Z',
        current_seq: 9,
        protocol: 'jeryu.ws.v2',
      }),
    });
    expect(h.hellos).toHaveLength(0);
    expect(h.errors[0]?.code).toBe('ws_protocol_mismatch');
    expect(socket.closed?.code).toBe(4002);
  });

  it('drops malformed JSON and unknown frame types without throwing', () => {
    const h = makeHandlers();
    const { socket } = connect(h.handlers);
    expect(() => socket.emit('message', { data: '{not json' })).not.toThrow();
    expect(() =>
      socket.emit('message', { data: JSON.stringify({ type: 'mystery' }) })
    ).not.toThrow();
    // A hello missing `protocol` fails validation and is dropped.
    socket.emit('message', {
      data: JSON.stringify({ type: 'hello', current_seq: 1 }),
    });
    expect(h.hellos).toHaveLength(0);
    expect(h.events).toHaveLength(0);
  });

  it('delivers a validated event frame and acks it', () => {
    const h = makeHandlers();
    const { socket } = connect(h.handlers);
    socket.sent.length = 0;
    socket.emit('message', {
      data: JSON.stringify({
        type: 'event',
        event: {
          seq: 42,
          timestamp: '2026-01-01T00:00:00Z',
          scope: 'global.activity',
          kind: 'repo.updated',
          entity: 'repo:1',
          summary: 'updated',
          payload: {},
        },
      }),
    });
    expect(h.events).toHaveLength(1);
    expect((h.events[0] as { seq: bigint }).seq).toBe(42n);
    // Fire-and-forget ack carrying the event seq.
    const ack = socket.sent.map((s) => JSON.parse(s)).find((f) => f.type === 'ack');
    expect(ack?.seq).toBe(42);
  });
});
