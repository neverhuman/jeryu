// websocket.ts — public entry for the realtime WebSocket client (W-FE-04).
//
// This is a thin composition shell: consumers keep importing `JeRyuWsClient`
// and its types from `@/api/websocket` while the implementation is split into
// cohesive sibling modules:
//   * `./websocket.client`   — the raw socket lifecycle (open/close/reconnect/
//     heartbeat) and the `JeRyuWsClient` transport class.
//   * `./websocket.types`    — the public type surface (status, handlers,
//     options, scope taps).
//   * `./websocketProtocol`  — inbound wire-frame decoding / runtime guards.
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
// `lastSeq` is exposed so the store can persist it through the storage adapter
// and resume cleanly across page refresh.

export { JeRyuWsClient } from './websocket.client';
export type {
  JeRyuWsClientHandlers,
  JeRyuWsClientOptions,
  RealtimeStatus,
  ScopeTap,
} from './websocket.types';
