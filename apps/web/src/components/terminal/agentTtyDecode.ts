// agentTtyDecode.ts — pure decode + validation for live agent TTY frames.
//
// The realtime socket streams `AgentTtyEvent`s on the `agent_run.{id}` scope;
// each `WebEvent.payload` is an `AgentTtyFrame` carrying the raw PTY bytes
// base64-encoded (so arbitrary control sequences survive the JSON hop). This
// module is the trust boundary: it proves an `unknown` payload is a valid
// frame and turns the base64 back into the exact bytes to feed `term.write`.
//
// Everything here is pure and DOM-free (uses `atob`, available in both the
// browser and jsdom), so it is exhaustively unit-testable without xterm or a
// socket.

import type { AgentTtyFrame } from '../../api/types';

/** Canonical base64 alphabet, optionally padded. We reject any other shape
 *  (whitespace, URL-safe `-_`, bad length) before touching `atob` so a
 *  malformed frame can never smuggle junk into the terminal. */
const BASE64_RE = /^[A-Za-z0-9+/]*={0,2}$/;

/** Logical output streams a frame may carry. */
const STREAMS: ReadonlySet<string> = new Set([
  'pty',
  'stdout',
  'stderr',
  'event',
]);

/**
 * Decode canonical base64 into the exact byte sequence it encodes. Pure and
 * lossless across all 256 byte values. Throws `RangeError` on malformed input
 * (bad alphabet, bad length, or stray padding) rather than silently producing
 * garbage — callers that want a soft failure should use {@link tryDecodeBase64}
 * or {@link parseTtyFrame}.
 */
export function base64ToBytes(b64: string): Uint8Array {
  if (typeof b64 !== 'string' || !BASE64_RE.test(b64)) {
    throw new RangeError('malformed base64');
  }
  // A valid base64 string (sans padding stripping) is always a multiple of 4.
  if (b64.length % 4 !== 0) {
    throw new RangeError('base64 length not a multiple of 4');
  }
  let binary: string;
  try {
    binary = atob(b64);
  } catch {
    throw new RangeError('base64 decode failed');
  }
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    out[i] = binary.charCodeAt(i) & 0xff;
  }
  return out;
}

/** Soft variant of {@link base64ToBytes}: returns `null` instead of throwing. */
export function tryDecodeBase64(b64: string): Uint8Array | null {
  try {
    return base64ToBytes(b64);
  } catch {
    return null;
  }
}

/** Encode bytes to canonical base64. Inverse of {@link base64ToBytes}; used by
 *  the input path (operator keystrokes → `agent_control` frames) and by tests. */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

/** Encode a UTF-8 string to base64 (operator keystrokes → input control). */
export function textToBase64(text: string): string {
  return bytesToBase64(new TextEncoder().encode(text));
}

/**
 * Validate an untrusted `WebEvent.payload` as an {@link AgentTtyFrame}. Returns
 * the typed frame on success or `null` for anything malformed (missing /
 * wrong-typed fields, unknown stream, or non-decodable base64). No field is
 * ever read off an unvalidated value.
 */
export function parseTtyFrame(payload: unknown): AgentTtyFrame | null {
  if (typeof payload !== 'object' || payload === null) return null;
  const p = payload as Record<string, unknown>;
  if (typeof p.chunk_seq !== 'number' || !Number.isFinite(p.chunk_seq)) {
    return null;
  }
  if (typeof p.stream !== 'string' || !STREAMS.has(p.stream)) return null;
  if (typeof p.bytes_b64 !== 'string') return null;
  // Prove the bytes actually decode so downstream `term.write` never chokes.
  if (tryDecodeBase64(p.bytes_b64) === null) return null;
  return {
    chunk_seq: p.chunk_seq,
    stream: p.stream as AgentTtyFrame['stream'],
    bytes_b64: p.bytes_b64,
  };
}
