// AgentTerminal.test.tsx — component-level wiring test for the live terminal.
//
// Drives the REAL transport path: a stubbed `WebSocket` feeds `agent_run.{id}`
// event frames into `JeRyuWsClient`, which routes them through the tap →
// `realtimeStore` → `useAgentTty` → the rAF write loop → `term.write`. xterm
// itself is mocked so we can assert the exact bytes handed to `write` and
// invoke the captured `onData` handler. Covers:
//   * merged bytes: two frames in one rAF batch coalesce into one write;
//   * input: keystrokes become `input` controls with the right decoded bytes;
//   * interrupt: Ctrl-C (button + ETX keystroke) become `interrupt` controls;
//   * resync: a chunk_seq gap clears the screen and emits a `resync` control.

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { AgentTerminal } from '../AgentTerminal';
import { textToBase64, base64ToBytes } from '../agentTtyDecode';
import { useRealtimeStore } from '../../../stores/realtimeStore';

// ── xterm mocks ────────────────────────────────────────────────────────────
vi.mock('@xterm/xterm', () => {
  class Terminal {
    static last: Terminal | null = null;
    cols = 80;
    rows = 24;
    writes: Uint8Array[] = [];
    clears = 0;
    dataHandler: ((d: string) => void) | null = null;
    constructor() {
      Terminal.last = this;
    }
    loadAddon(): void {}
    open(): void {}
    onData(cb: (d: string) => void): { dispose(): void } {
      this.dataHandler = cb;
      return { dispose: () => {} };
    }
    write(data: Uint8Array): void {
      this.writes.push(data);
    }
    clear(): void {
      this.clears += 1;
    }
    dispose(): void {}
  }
  return { Terminal };
});

vi.mock('@xterm/addon-fit', () => {
  class FitAddon {
    fit(): void {}
  }
  return { FitAddon };
});

import { Terminal } from '@xterm/xterm';

/** Shape of the mocked Terminal we read in assertions. */
interface MockTerminal {
  writes: Uint8Array[];
  clears: number;
  dataHandler: ((d: string) => void) | null;
  cols: number;
  rows: number;
}

/** The `static last` slot exposed by the mock above. */
function lastTerm(): MockTerminal {
  return (Terminal as unknown as { last: MockTerminal }).last;
}

function lastTermOrNull(): MockTerminal | null {
  return (Terminal as unknown as { last: MockTerminal | null }).last;
}

function resetLastTerm(): void {
  (Terminal as unknown as { last: MockTerminal | null }).last = null;
}

// ── scriptable WebSocket double ─────────────────────────────────────────────
type Listener = (ev: unknown) => void;
class FakeWebSocket {
  static OPEN = 1;
  static instances: FakeWebSocket[] = [];
  readyState = FakeWebSocket.OPEN;
  sent: string[] = [];
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
  close(): void {
    this.readyState = 3;
  }
  emit(type: string, ev: unknown): void {
    for (const fn of this.listeners[type] ?? []) fn(ev);
  }
}

// ── controllable requestAnimationFrame ──────────────────────────────────────
let rafCallbacks: Array<FrameRequestCallback | undefined> = [];
function runRaf(): void {
  const cbs = rafCallbacks;
  rafCallbacks = [];
  for (const cb of cbs) cb?.(0);
}

const RUN_ID = 'run-1';
const SCOPE = `agent_run.${RUN_ID}`;

function helloFrame(): string {
  return JSON.stringify({
    type: 'hello',
    server_time: '2026-01-01T00:00:00Z',
    current_seq: 0,
    protocol: 'jeryu.ws.v1',
  });
}

function eventFrame(opts: {
  seq: number;
  chunk_seq: number;
  text: string;
  scope?: string;
}): string {
  return JSON.stringify({
    type: 'event',
    event: {
      seq: opts.seq,
      timestamp: '2026-01-01T00:00:00Z',
      scope: opts.scope ?? SCOPE,
      kind: 'agent.tty',
      entity: SCOPE,
      summary: 'tty',
      payload: {
        chunk_seq: opts.chunk_seq,
        stream: 'pty',
        bytes_b64: textToBase64(opts.text),
      },
    },
  });
}

function sentControls(socket: FakeWebSocket): Array<Record<string, unknown>> {
  return socket.sent
    .map((s): Record<string, unknown> => JSON.parse(s))
    .filter((f) => f.type === 'agent_control');
}

function decode(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes);
}

async function mountAndStream(): Promise<{
  socket: FakeWebSocket;
  term: () => MockTerminal;
}> {
  render(<AgentTerminal runId={RUN_ID} />);
  useRealtimeStore.getState().connect();
  const socket = FakeWebSocket.instances[0];
  socket.emit('open', {});
  socket.emit('message', { data: helloFrame() });
  // Wait for the lazy xterm surface + the subscribe/tap effect to run.
  await screen.findByTestId('agent-terminal-surface');
  await waitFor(() => expect(lastTermOrNull()).not.toBeNull());
  return { socket, term: lastTerm };
}

beforeEach(() => {
  FakeWebSocket.instances = [];
  rafCallbacks = [];
  resetLastTerm();
  vi.stubGlobal('WebSocket', FakeWebSocket as unknown as typeof WebSocket);
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    }
  );
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
    rafCallbacks.push(cb);
    return rafCallbacks.length;
  });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => {
    rafCallbacks[id - 1] = undefined;
  });
  useRealtimeStore.setState({ status: 'idle', events: [] });
});

afterEach(() => {
  useRealtimeStore.getState().disconnect();
  vi.unstubAllGlobals();
});

describe('AgentTerminal live wiring', () => {
  it('merges two TTY frames in one rAF batch into a single write', async () => {
    const { socket, term } = await mountAndStream();
    socket.emit('message', { data: eventFrame({ seq: 1, chunk_seq: 1, text: 'A' }) });
    socket.emit('message', { data: eventFrame({ seq: 2, chunk_seq: 2, text: 'B' }) });
    runRaf();
    const t = term();
    expect(t.writes).toHaveLength(1);
    expect(decode(t.writes[0])).toBe('AB');
  });

  it('renders the scripted prompt bytes verbatim', async () => {
    const { socket, term } = await mountAndStream();
    socket.emit('message', {
      data: eventFrame({ seq: 1, chunk_seq: 1, text: '$ cargo test\r\n' }),
    });
    runRaf();
    expect(decode(term().writes[0])).toBe('$ cargo test\r\n');
  });

  it('sends an input control with the decoded bytes when the operator types', async () => {
    const { socket } = await mountAndStream();
    socket.sent.length = 0;
    lastTerm().dataHandler?.('x');
    const controls = sentControls(socket);
    expect(controls).toHaveLength(1);
    const control = controls[0].control as { kind: string; bytes_b64: string };
    expect(controls[0].run_id).toBe(RUN_ID);
    expect(control.kind).toBe('input');
    expect(decode(base64ToBytes(control.bytes_b64))).toBe('x');
  });

  it('promotes a Ctrl-C keystroke (ETX) to an interrupt control', async () => {
    const { socket } = await mountAndStream();
    socket.sent.length = 0;
    lastTerm().dataHandler?.('\x03');
    const controls = sentControls(socket);
    expect(controls).toHaveLength(1);
    expect((controls[0].control as { kind: string }).kind).toBe('interrupt');
  });

  it('sends an interrupt when the Ctrl-C toolbar button is clicked', async () => {
    const { socket } = await mountAndStream();
    socket.sent.length = 0;
    fireEvent.click(screen.getByTestId('agent-terminal-interrupt'));
    const controls = sentControls(socket);
    expect(controls).toHaveLength(1);
    expect((controls[0].control as { kind: string }).kind).toBe('interrupt');
  });

  it('clears the screen and resyncs on a chunk_seq gap', async () => {
    const { socket, term } = await mountAndStream();
    socket.emit('message', { data: eventFrame({ seq: 1, chunk_seq: 1, text: 'A' }) });
    runRaf();
    socket.sent.length = 0;
    // Gap: chunk_seq jumps 1 -> 3.
    socket.emit('message', { data: eventFrame({ seq: 2, chunk_seq: 3, text: 'C' }) });
    expect(term().clears).toBeGreaterThanOrEqual(1);
    const controls = sentControls(socket);
    expect(controls.some((c) => (c.control as { kind: string }).kind === 'resync')).toBe(
      true
    );
  });

  it('streams agent bytes WITHOUT touching the rolling event buffer', async () => {
    const { socket } = await mountAndStream();
    socket.emit('message', { data: eventFrame({ seq: 1, chunk_seq: 1, text: 'A' }) });
    runRaf();
    // The tap intercepts the frame before the zustand buffer — it stays empty.
    expect(useRealtimeStore.getState().events).toHaveLength(0);
  });
});
