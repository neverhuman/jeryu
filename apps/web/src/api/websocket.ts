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
//     (§35.1.12). We rely on the browser-handled WS Ping/Pong; we
//     additionally send a JSON `Ping { nonce }` every 15 s so the server
//     observes liveness even through proxies that strip control frames.
//
// Reconnect policy: exponential backoff capped at 30 s, with full jitter.
// `lastSeq` is exposed so the store can persist it to sessionStorage and
// resume cleanly across page refresh.

import type {
  AgentControl,
  AgentControlClientMessage,
  ClientWsMessage,
  SubscriptionSpec,
  WebEvent,
} from './types';

/** A tap receives `event` frames for a single scope before they reach the
 *  zustand buffer. Used by the agent-terminal transport so high-frequency TTY
 *  bytes never pass through the rolling 200-event buffer. */
export type ScopeTap = (event: WebEvent) => void;
import {
  WS_PROTOCOL,
  bigintReplacer,
  cryptoRandomNonce,
  parseServerFrame,
} from './websocketProtocol';

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
  private taps = new Map<string, ScopeTap>();
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

  /**
   * Route `event` frames on `scope` directly to `cb` instead of the normal
   * `onEvent` path (the zustand rolling buffer). The agent terminal uses this
   * so raw TTY bytes bypass the bounded activity buffer entirely. At most one
   * tap per scope; re-tapping replaces the callback. The caller is still
   * responsible for `subscribe(scope)` so the server actually sends frames.
   */
  tapScope(scope: string, cb: ScopeTap): void {
    this.taps.set(scope, cb);
  }

  /** Remove a tap installed by {@link tapScope}. */
  untapScope(scope: string): void {
    this.taps.delete(scope);
  }

  /**
   * Drive a control command into a running agent's sandboxed TTY. Emits an
   * `agent_control` client frame keyed by `runId` (input bytes, interrupt,
   * resize, or resync).
   */
  sendControl(runId: string, control: AgentControl): void {
    this.sendFrame({ type: 'agent_control', run_id: runId, control });
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
    let parsed: unknown;
    try {
      parsed = JSON.parse(event.data);
    } catch {
      return;
    }
    const frame = parseServerFrame(parsed);
    if (!frame) return;
    switch (frame.type) {
      case 'hello': {
        // Reject frames from a runtime speaking a different protocol
        // revision than this client was built against (§35.1.12). Resuming
        // a cursor across protocol revisions is undefined, so we tear down
        // instead of trusting `current_seq`.
        if (frame.protocol !== WS_PROTOCOL) {
          this.handlers.onError?.(
            'ws_protocol_mismatch',
            `server protocol "${frame.protocol}" != expected "${WS_PROTOCOL}"`
          );
          try {
            this.socket?.close(4002, 'protocol-mismatch');
          } catch {
            // ignore close errors
          }
          return;
        }
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
        // Phase 1 fire-and-forget ack so the server can flush its window.
        this.send({ type: 'ack', seq: evt.seq });
        // Tapped scopes (agent TTY) are delivered straight to the tap and
        // never enter the zustand rolling buffer — return BEFORE `onEvent`.
        const tap = this.taps.get(evt.scope);
        if (tap) {
          tap(evt);
          break;
        }
        this.handlers.onEvent(evt);
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
        // Exhaustiveness check: `parseServerFrame` only returns the union
        // members handled above, so `frame` narrows to `never` here. The
        // assignment fails to compile if a new variant is added without a
        // matching case.
        const _exhaustive: never = frame;
        void _exhaustive;
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
    this.sendFrame(frame);
  }

  private sendFrame(frame: ClientWsMessage | AgentControlClientMessage): void {
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
