// websocket.ts — low-level WebSocket transport (W-FE-04).
//
// This module owns the raw socket lifecycle (open/close/reconnect/heartbeat)
// and exposes a strongly-typed `JeRyuWsClient` to `realtimeStore`. The store
// holds React state; the client is plain JS and easier to unit-test.
//
// Protocol (`jeryu.ws.v1`, see `crate::api::websocket`):
//   * Client sends `Hello { resume_from, subscriptions }` immediately on
//     open. `resume_from` is the last seq we saw (or `null`).
//   * Server replies with `Hello { current_seq }`. We compare it against
//     `resume_from`; on gap (current_seq > resume_from + 1) we notify the
//     consumer so it can invalidate the bootstrap snapshot.
//   * Client sends `Subscribe` to register additional scopes, `Unsubscribe`
//     to drop them. Both can be sent any time after the initial Hello.
//   * Heartbeats: server pings every 15 s with a 30 s read timeout
//     (§35.1.12). We piggyback on the browser-handled WS Ping/Pong; we
//     additionally send a JSON `Ping { nonce }` every 15 s so the server
//     observes liveness even through proxies that strip control frames.
//
// Reconnect policy: exponential backoff capped at 30 s, with full jitter.
// `lastSeq` is exposed so the store can persist it to sessionStorage and
// resume cleanly across page refresh.

import type {
  ClientWsMessage,
  ServerWsMessage,
  SubscriptionSpec,
  WebEvent,
} from './types';

export type RealtimeStatus =
  | 'idle'
  | 'connecting'
  | 'open'
  | 'closed'
  | 'reconnecting';

export interface JeRyuWsClientHandlers {
  onStatus: (status: RealtimeStatus) => void;
  onEvent: (event: WebEvent) => void;
  /**
   * Fired whenever the server announces an out-of-band gap (either through
   * `SnapshotRequired` or because the resumed-from cursor is older than
   * `current_seq - 1`). The store responds by invalidating the bootstrap
   * query so React Query refetches the read model.
   */
  onSnapshotRequired: (reason: string, currentSeq: bigint) => void;
  onError?: (code: string, message: string) => void;
  /** Called when the server's `Hello` frame arrives, with `current_seq`. */
  onHello?: (currentSeq: bigint) => void;
}

export interface JeRyuWsClientOptions {
  /** Absolute or relative WS URL. Relative paths are resolved against
   *  `window.location`. The default `/api/v1/ws` matches `endpoints.ws()`. */
  url: string;
  /** Initial subscription set sent in the `Hello` frame. */
  initialSubscriptions: SubscriptionSpec[];
  /** Resume cursor; nullable on cold start. */
  resumeFrom: bigint | null;
  handlers: JeRyuWsClientHandlers;
}

const HEARTBEAT_INTERVAL_MS = 15_000;
const READ_TIMEOUT_MS = 30_000;
const MAX_BACKOFF_MS = 30_000;
const BASE_BACKOFF_MS = 500;

export class JeRyuWsClient {
  private socket: WebSocket | null = null;
  private status: RealtimeStatus = 'idle';
  private subscriptions = new Map<string, SubscriptionSpec>();
  private resumeFrom: bigint | null = null;
  private readonly handlers: JeRyuWsClientHandlers;
  private readonly url: string;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private readTimeoutTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private explicitlyClosed = false;

  constructor(options: JeRyuWsClientOptions) {
    this.url = options.url;
    this.handlers = options.handlers;
    this.resumeFrom = options.resumeFrom;
    for (const spec of options.initialSubscriptions) {
      this.subscriptions.set(spec.scope, spec);
    }
  }

  connect(): void {
    if (this.socket && this.status !== 'closed') return;
    this.explicitlyClosed = false;
    this.openSocket();
  }

  disconnect(): void {
    this.explicitlyClosed = true;
    this.clearTimers();
    if (this.socket) {
      try {
        this.socket.close(1000, 'client-disconnect');
      } catch {
        // ignore close errors
      }
      this.socket = null;
    }
    this.setStatus('closed');
  }

  subscribe(specs: SubscriptionSpec[]): void {
    const added: SubscriptionSpec[] = [];
    for (const spec of specs) {
      if (!this.subscriptions.has(spec.scope)) {
        this.subscriptions.set(spec.scope, spec);
        added.push(spec);
      }
    }
    if (added.length === 0) return;
    this.send({ type: 'subscribe', subscriptions: added });
  }

  unsubscribe(scopes: string[]): void {
    const removed: string[] = [];
    for (const scope of scopes) {
      if (this.subscriptions.delete(scope)) {
        removed.push(scope);
      }
    }
    if (removed.length === 0) return;
    this.send({ type: 'unsubscribe', scopes: removed });
  }

  /** Latest cursor observed from the server. */
  getResumeFrom(): bigint | null {
    return this.resumeFrom;
  }

  // ── internals ──────────────────────────────────────────────────────────

  private openSocket(): void {
    this.setStatus(this.reconnectAttempt === 0 ? 'connecting' : 'reconnecting');
    const absoluteUrl = this.resolveUrl(this.url);
    let socket: WebSocket;
    try {
      socket = new WebSocket(absoluteUrl);
    } catch (err) {
      this.handlers.onError?.(
        'ws_open_failed',
        err instanceof Error ? err.message : 'WebSocket open failed'
      );
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;

    socket.addEventListener('open', () => this.onOpen());
    socket.addEventListener('message', (ev) => this.onMessage(ev));
    socket.addEventListener('close', () => this.onClose());
    socket.addEventListener('error', () => {
      // The `error` event is followed by `close`; we'll reconnect there.
      this.handlers.onError?.('ws_error', 'WebSocket transport error');
    });
  }

  private onOpen(): void {
    this.reconnectAttempt = 0;
    this.setStatus('open');
    this.send({
      type: 'hello',
      resume_from: this.resumeFrom,
      subscriptions: Array.from(this.subscriptions.values()),
    });
    this.startHeartbeat();
    this.resetReadTimeout();
  }

  private onMessage(event: MessageEvent<unknown>): void {
    this.resetReadTimeout();
    if (typeof event.data !== 'string') return;
    let frame: ServerWsMessage;
    try {
      frame = JSON.parse(event.data) as ServerWsMessage;
    } catch {
      return;
    }
    switch (frame.type) {
      case 'hello': {
        const expected = this.resumeFrom;
        this.resumeFrom = frame.current_seq;
        this.handlers.onHello?.(frame.current_seq);
        // Gap detection (§6.13 + §35.1.13): if we resumed from a known
        // cursor and the server is ahead by more than one tick, ask the
        // consumer to refetch the bootstrap snapshot.
        if (
          expected !== null &&
          frame.current_seq > expected + BigInt(1)
        ) {
          this.handlers.onSnapshotRequired('gap', frame.current_seq);
        }
        break;
      }
      case 'snapshot_required': {
        this.resumeFrom = frame.current_seq;
        this.handlers.onSnapshotRequired(frame.reason, frame.current_seq);
        break;
      }
      case 'event': {
        const evt = frame.event;
        this.resumeFrom = evt.seq;
        this.handlers.onEvent(evt);
        // Phase 1 fire-and-forget ack so the server can flush its window.
        this.send({ type: 'ack', seq: evt.seq });
        break;
      }
      case 'pong': {
        // Heartbeat round-trip; no-op besides resetReadTimeout above.
        break;
      }
      case 'error': {
        this.handlers.onError?.(frame.code, frame.message);
        break;
      }
      default: {
        // Discriminated-union safety: TypeScript treats `frame` as `never`
        // when all cases handled. Runtime fallthrough is intentional.
        break;
      }
    }
  }

  private onClose(): void {
    this.clearTimers();
    this.socket = null;
    if (this.explicitlyClosed) {
      this.setStatus('closed');
      return;
    }
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    this.setStatus('reconnecting');
    this.reconnectAttempt += 1;
    const backoff = Math.min(
      MAX_BACKOFF_MS,
      BASE_BACKOFF_MS * 2 ** Math.min(this.reconnectAttempt, 6)
    );
    const jitter = Math.floor(Math.random() * backoff);
    this.reconnectTimer = setTimeout(() => this.openSocket(), jitter);
  }

  private startHeartbeat(): void {
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
    this.heartbeatTimer = setInterval(() => {
      this.send({ type: 'ping', nonce: cryptoRandomNonce() });
    }, HEARTBEAT_INTERVAL_MS);
  }

  private resetReadTimeout(): void {
    if (this.readTimeoutTimer) clearTimeout(this.readTimeoutTimer);
    this.readTimeoutTimer = setTimeout(() => {
      // Server stopped sending frames; tear down so the close handler
      // schedules a reconnect.
      try {
        this.socket?.close(4000, 'read-timeout');
      } catch {
        // ignore
      }
    }, READ_TIMEOUT_MS);
  }

  private clearTimers(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.readTimeoutTimer) {
      clearTimeout(this.readTimeoutTimer);
      this.readTimeoutTimer = null;
    }
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private setStatus(status: RealtimeStatus): void {
    if (this.status === status) return;
    this.status = status;
    this.handlers.onStatus(status);
  }

  private send(frame: ClientWsMessage): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) return;
    try {
      this.socket.send(JSON.stringify(frame, bigintReplacer));
    } catch (err) {
      this.handlers.onError?.(
        'ws_send_failed',
        err instanceof Error ? err.message : 'WebSocket send failed'
      );
    }
  }

  private resolveUrl(input: string): string {
    if (/^wss?:/i.test(input)) return input;
    if (typeof window === 'undefined') return input;
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    const path = input.startsWith('/') ? input : `/${input}`;
    return `${proto}//${host}${path}`;
  }
}

function cryptoRandomNonce(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

/** Serialize `bigint` cursors as JSON numbers (the server accepts both). */
function bigintReplacer(_key: string, value: unknown): unknown {
  if (typeof value === 'bigint') {
    // The wire protocol carries u64 seq numbers; clamp to Number range so
    // legacy clients without BigInt JSON support can still decode. The
    // server treats both equivalently.
    return Number(value);
  }
  return value;
}
