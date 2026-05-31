// websocketProtocol.ts — wire-protocol decoding for the realtime socket.
//
// This module owns the inbound-frame validation layer shared by
// `JeRyuWsClient` (`./websocket`): the runtime type guards that prove a
// `JSON.parse`d `unknown` matches the `ServerWsMessage` discriminated union
// before it reaches the client's message switch, plus the negotiated
// protocol identifier. Keeping the guards here lets the socket-lifecycle
// module stay focused on open/close/reconnect/heartbeat.

import type { ServerWsMessage, WebEvent } from './types';

/**
 * Wire protocol identifier negotiated in the server `hello` frame
 * (`crate::api::websocket`). The server stamps this into
 * `ServerWsMessage::Hello.protocol`; a mismatch means the runtime is talking
 * a different protocol revision than this client was built against, so we
 * surface it as an error and refuse to resume against a stale cursor.
 */
export const WS_PROTOCOL = 'jeryu.ws.v1';

/**
 * Runtime guard for inbound server frames. `JSON.parse` yields `unknown`;
 * we prove the discriminated-union shape (`type` plus the fields the matching
 * branch reads) before handing the frame to the switch, so no field is read
 * off an unvalidated value. Unknown / malformed frames are dropped.
 */
export function parseServerFrame(raw: unknown): ServerWsMessage | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const f = raw as Record<string, unknown>;
  switch (f.type) {
    case 'hello':
      return typeof f.protocol === 'string' &&
        typeof f.server_time === 'string' &&
        isSeq(f.current_seq)
        ? {
            type: 'hello',
            server_time: f.server_time,
            current_seq: toSeq(f.current_seq),
            protocol: f.protocol,
          }
        : null;
    case 'snapshot_required':
      return typeof f.reason === 'string' && isSeq(f.current_seq)
        ? {
            type: 'snapshot_required',
            reason: f.reason,
            current_seq: toSeq(f.current_seq),
          }
        : null;
    case 'event':
      return isWebEvent(f.event)
        ? { type: 'event', event: { ...f.event, seq: toSeq(f.event.seq) } }
        : null;
    case 'pong':
      return typeof f.nonce === 'string' && typeof f.server_time === 'string'
        ? { type: 'pong', nonce: f.nonce, server_time: f.server_time }
        : null;
    case 'error':
      return typeof f.code === 'string' && typeof f.message === 'string'
        ? { type: 'error', code: f.code, message: f.message }
        : null;
    default:
      return null;
  }
}

/** Accept the `seq` field as either a JSON number or a bigint (the server
 *  clamps u64 cursors to Number range on the wire; see `bigintReplacer`). */
export function isSeq(value: unknown): value is number | bigint {
  return typeof value === 'number' || typeof value === 'bigint';
}

export function toSeq(value: number | bigint): bigint {
  return typeof value === 'bigint' ? value : BigInt(Math.trunc(value));
}

export function isWebEvent(value: unknown): value is WebEvent {
  if (typeof value !== 'object' || value === null) return false;
  const e = value as Record<string, unknown>;
  return (
    isSeq(e.seq) &&
    typeof e.scope === 'string' &&
    typeof e.kind === 'string' &&
    typeof e.entity === 'string'
  );
}

/** Serialize `bigint` cursors as JSON numbers (the server accepts both). */
export function bigintReplacer(_key: string, value: unknown): unknown {
  if (typeof value === 'bigint') {
    // The wire protocol carries u64 seq numbers; clamp to Number range so
    // legacy clients without BigInt JSON support can still decode. The
    // server treats both equivalently.
    return Number(value);
  }
  return value;
}

export function cryptoRandomNonce(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
