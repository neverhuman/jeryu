// agentTtyDecode.test.ts — base64 round-trip + TTY frame validation.
//
// These pin the trust boundary between the realtime socket and xterm: the
// base64 codec must be lossless across ALL 256 byte values (terminal output is
// arbitrary bytes — ANSI escapes, NULs, UTF-8 multibyte), and `parseTtyFrame`
// must reject every malformed payload shape so junk can never reach
// `term.write`.

import { describe, expect, it } from 'vitest';

import {
  base64ToBytes,
  bytesToBase64,
  parseTtyFrame,
  textToBase64,
  tryDecodeBase64,
} from '../agentTtyDecode';

describe('base64 codec', () => {
  it('round-trips every single byte value 0..255', () => {
    for (let b = 0; b <= 255; b += 1) {
      const bytes = new Uint8Array([b]);
      const decoded = base64ToBytes(bytesToBase64(bytes));
      expect(Array.from(decoded)).toEqual([b]);
    }
  });

  it('round-trips a full 0..255 sweep in one buffer (lengths %3 == 0/1/2)', () => {
    const all = new Uint8Array(256);
    for (let i = 0; i < 256; i += 1) all[i] = i;
    for (const len of [255, 256, 254]) {
      const slice = all.slice(0, len);
      const decoded = base64ToBytes(bytesToBase64(slice));
      expect(decoded.length).toBe(len);
      expect(Array.from(decoded)).toEqual(Array.from(slice));
    }
  });

  it('round-trips an empty buffer', () => {
    expect(base64ToBytes(bytesToBase64(new Uint8Array(0))).length).toBe(0);
  });

  it('decodes a known terminal chunk to exact bytes', () => {
    // "$ cargo test\r\n" — the CR + LF must survive intact.
    const b64 = textToBase64('$ cargo test\r\n');
    const bytes = base64ToBytes(b64);
    expect(new TextDecoder().decode(bytes)).toBe('$ cargo test\r\n');
    expect(bytes[bytes.length - 2]).toBe(0x0d); // CR
    expect(bytes[bytes.length - 1]).toBe(0x0a); // LF
  });

  it('preserves a raw ANSI color escape sequence', () => {
    const ansi = '[32mgreen[0m';
    expect(new TextDecoder().decode(base64ToBytes(textToBase64(ansi)))).toBe(
      ansi
    );
  });

  it('throws on malformed base64 and tryDecodeBase64 returns null', () => {
    for (const bad of ['not base64!', 'AAA', 'A===', '====', 'AB CD', '@@@@']) {
      expect(() => base64ToBytes(bad)).toThrow();
      expect(tryDecodeBase64(bad)).toBeNull();
    }
  });
});

describe('parseTtyFrame', () => {
  const valid = {
    chunk_seq: 7,
    stream: 'pty',
    bytes_b64: textToBase64('hi'),
  };

  it('accepts a well-formed frame and echoes the typed fields', () => {
    const frame = parseTtyFrame(valid);
    expect(frame).not.toBeNull();
    expect(frame?.chunk_seq).toBe(7);
    expect(frame?.stream).toBe('pty');
    expect(new TextDecoder().decode(base64ToBytes(frame!.bytes_b64))).toBe('hi');
  });

  it('accepts each known stream name', () => {
    for (const stream of ['pty', 'stdout', 'stderr', 'event']) {
      expect(parseTtyFrame({ ...valid, stream })).not.toBeNull();
    }
  });

  it('rejects malformed payloads', () => {
    const cases: unknown[] = [
      null,
      undefined,
      42,
      'string',
      {},
      { ...valid, chunk_seq: '7' }, // wrong type
      { ...valid, chunk_seq: Number.NaN }, // non-finite
      { ...valid, stream: 'sideband' }, // unknown stream
      { ...valid, stream: 5 }, // wrong type
      { ...valid, bytes_b64: 123 }, // wrong type
      { ...valid, bytes_b64: 'not base64!' }, // non-decodable
      { chunk_seq: 1, stream: 'pty' }, // missing bytes_b64
    ];
    for (const payload of cases) {
      expect(parseTtyFrame(payload)).toBeNull();
    }
  });
});
