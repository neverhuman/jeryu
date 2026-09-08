// AgentTerminal.test.tsx — component-level wiring test for the live terminal.
//
// Drives the REAL transport path of the SSE + REST architecture: a stubbed
// `EventSource` feeds `AgentStreamEvent` frames through `connectTtyStream`
// → the rAF write loop → `term.write`, and operator input flows out via
// POST /api/v1/agent-runs/{id}/control (captured by a `fetch` spy). xterm
// itself is mocked so we can assert the exact bytes handed to `write` and
// invoke the captured `onData` handler. Covers:
//   * merged bytes: two frames in one rAF batch coalesce into one write;
//   * verbatim rendering of scripted text and base64-encoded PTY bytes;
//   * sanitizer: unsupported escape sequences and tcmalloc noise are stripped;
//   * input: keystrokes become `send_input` controls with the typed text;
//   * interrupt: Ctrl-C (button + ETX keystroke) become `interrupt` controls;
//   * exit: an exit_code event surfaces the exit pill and the exit line;
//   * isolation: SSE bytes never touch the realtime rolling event buffer.

import { act, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { AgentTerminal } from '../AgentTerminal';
import { textToBase64 } from '../agentTtyDecode';
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

// ── scriptable EventSource double ───────────────────────────────────────────
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((msg: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  constructor(public url: string) {
    FakeEventSource.instances.push(this);
  }
  close(): void {
    this.closed = true;
  }
}

// ── controllable requestAnimationFrame ──────────────────────────────────────
let rafCallbacks: Array<FrameRequestCallback | undefined> = [];
function runRaf(): void {
  const cbs = rafCallbacks;
  rafCallbacks = [];
  act(() => {
    for (const cb of cbs) cb?.(0);
  });
}

const RUN_ID = 'run-1';
const CONTROL_PATH = `/api/v1/agent-runs/${RUN_ID}/control`;

interface SseEventOpts {
  seq: number;
  stream?: 'stdout' | 'stderr' | 'event';
  text?: string | null;
  bytesB64?: string | null;
  exitCode?: number | null;
}

function sseFrame(opts: SseEventOpts): string {
  return JSON.stringify({
    seq: opts.seq,
    stream: opts.stream ?? 'stdout',
    text: opts.text ?? null,
    bytes_b64: opts.bytesB64 ?? null,
    exit_code: opts.exitCode ?? null,
  });
}

function emit(source: FakeEventSource, frame: string): void {
  act(() => {
    source.onmessage?.({ data: frame });
  });
}

let fetchSpy: ReturnType<typeof vi.spyOn>;

/** Control POSTs captured by the fetch spy, parsed. */
function sentControls(): Array<Record<string, unknown>> {
  const calls = fetchSpy.mock.calls as Array<[unknown, RequestInit | undefined]>;
  return calls
    .filter(
      ([url, init]) =>
        String(url).includes(CONTROL_PATH) &&
        (init?.method ?? 'GET').toUpperCase() === 'POST'
    )
    .map(([, init]): Record<string, unknown> => JSON.parse(String(init?.body)));
}

function controlsOfKind(kind: string): Array<Record<string, unknown>> {
  return sentControls().filter((c) => c.kind === kind);
}

function decode(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes);
}

async function mountAndStream(): Promise<{
  source: FakeEventSource;
  term: () => MockTerminal;
}> {
  render(<AgentTerminal runId={RUN_ID} />);
  const source = FakeEventSource.instances[0];
  expect(source.url).toContain(`/api/v1/agent-runs/${RUN_ID}/tty/stream`);
  act(() => {
    source.onopen?.();
  });
  // Wait for the lazy xterm surface + the dynamic import to finish.
  await screen.findByTestId('agent-terminal-surface');
  await waitFor(() => expect(lastTermOrNull()).not.toBeNull());
  // The mount-time resize control may still be in flight; drain it so the
  // per-test control assertions start from a clean slate.
  fetchSpy.mockClear();
  return { source, term: lastTerm };
}

beforeEach(() => {
  FakeEventSource.instances = [];
  rafCallbacks = [];
  resetLastTerm();
  vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource);
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
  fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
    return new Response(JSON.stringify({ accepted: true }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
  useRealtimeStore.setState({ status: 'idle', events: [] });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('AgentTerminal live wiring', () => {
  it('merges two TTY frames in one rAF batch into a single write', async () => {
    const { source, term } = await mountAndStream();
    emit(source, sseFrame({ seq: 1, text: 'A' }));
    emit(source, sseFrame({ seq: 2, text: 'B' }));
    runRaf();
    const t = term();
    expect(t.writes).toHaveLength(1);
    expect(decode(t.writes[0])).toBe('AB');
  });

  it('renders the scripted prompt text verbatim', async () => {
    const { source, term } = await mountAndStream();
    emit(source, sseFrame({ seq: 1, text: '$ cargo test\r\n' }));
    runRaf();
    expect(decode(term().writes[0])).toBe('$ cargo test\r\n');
  });

  it('renders base64-encoded PTY bytes verbatim', async () => {
    const { source, term } = await mountAndStream();
    emit(
      source,
      sseFrame({ seq: 1, bytesB64: textToBase64('$ cargo test\r\n') })
    );
    runRaf();
    expect(decode(term().writes[0])).toBe('$ cargo test\r\n');
  });

  it('strips terminal sequences xterm cannot render cleanly', async () => {
    const { source, term } = await mountAndStream();
    emit(
      source,
      sseFrame({
        seq: 1,
        text: '\x1b[>4mkeep\x1b[?2026$p\n123 third_party/tcmalloc/noise\n',
      })
    );
    runRaf();
    expect(decode(term().writes[0])).toBe('keep\n');
  });

  it('sends a send_input control with the typed text when the operator types', async () => {
    const { term } = await mountAndStream();
    term().dataHandler?.('x');
    const inputs = controlsOfKind('send_input');
    expect(inputs).toHaveLength(1);
    expect(inputs[0].text).toBe('x');
  });

  it('promotes a Ctrl-C keystroke (ETX) to an interrupt control', async () => {
    const { term } = await mountAndStream();
    term().dataHandler?.('\x03');
    expect(controlsOfKind('interrupt')).toHaveLength(1);
    expect(controlsOfKind('send_input')).toHaveLength(0);
  });

  it('sends an interrupt when the Ctrl-C toolbar button is clicked', async () => {
    await mountAndStream();
    fireEvent.click(screen.getByTestId('agent-terminal-interrupt'));
    expect(controlsOfKind('interrupt')).toHaveLength(1);
  });

  it('surfaces the exit pill and writes the exit line on an exit_code event', async () => {
    const { source, term } = await mountAndStream();
    emit(source, sseFrame({ seq: 1, stream: 'event', exitCode: 0 }));
    const pill = await screen.findByTestId('agent-terminal-exit');
    expect(pill).toHaveTextContent('exit 0');
    runRaf();
    expect(decode(term().writes[0])).toContain('process exited with code 0');
  });

  it('streams agent bytes WITHOUT touching the rolling event buffer', async () => {
    const { source, term } = await mountAndStream();
    emit(source, sseFrame({ seq: 1, text: 'quiet bytes' }));
    runRaf();
    expect(decode(term().writes[0])).toBe('quiet bytes');
    expect(useRealtimeStore.getState().events).toHaveLength(0);
  });
});
