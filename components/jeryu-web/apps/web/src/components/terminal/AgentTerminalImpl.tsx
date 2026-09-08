// AgentTerminalImpl.tsx — live agent terminal surface (SSE + REST transport).
//
// On mount:
//   1. Dynamically imports xterm and boots it into the container div.
//   2. Opens an SSE connection to GET /api/v1/agent-runs/{runId}/tty/stream
//      for live TTY output.
//   3. Operator keystrokes are sent via POST /api/v1/agent-runs/{runId}/control.

import { useEffect, useRef, useCallback, useState } from 'react';

import { connectTtyStream, type AgentTtyConnection, type AgentStreamEvent } from './agentTtyTransport';
import { sendInput, sendInterrupt, sendResize } from './agentControlTransport';

export interface AgentTerminalImplProps {
  runId: string;
  onBytes?: (totalBytes: number) => void;
  onSseStatus?: (status: 'connecting' | 'open' | 'closed' | 'error') => void;
  onExit?: (exitCode: number) => void;
  resizeDebounceMs?: number;
}

const ETX = 0x03;

// Strip terminal escape sequences unsupported by xterm.js that can corrupt
// rendering. Specifically: Kitty keyboard protocol (CSI >Pm, CSI =Ps;Ps u,
// CSI ?u) and DECRQM mode queries (CSI ?Pd $p).
const UNSUPPORTED_SEQS = /\x1b\[(?:[>=][0-9;]*[mu]|\?[0-9]*\$p|\?u)/g;
const TCMALLOC_NOISE = /^\d+ third_party\/tcmalloc\/.*\n?/gm;
function sanitizeTty(text: string): string {
  return text.replace(UNSUPPORTED_SEQS, '').replace(TCMALLOC_NOISE, '');
}

export function AgentTerminalImpl({
  runId,
  onBytes,
  onSseStatus,
  onExit,
  resizeDebounceMs = 150,
}: AgentTerminalImplProps): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<any>(null);
  const fitRef = useRef<any>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [xtermReady, setXtermReady] = useState(false);

  const pendingRef = useRef<Uint8Array[]>([]);
  const rafRef = useRef<number | null>(null);
  const bytesRef = useRef(0);

  const onBytesRef = useRef(onBytes);
  useEffect(() => { onBytesRef.current = onBytes; }, [onBytes]);
  const onSseStatusRef = useRef(onSseStatus);
  useEffect(() => { onSseStatusRef.current = onSseStatus; }, [onSseStatus]);
  const onExitRef = useRef(onExit);
  useEffect(() => { onExitRef.current = onExit; }, [onExit]);

  // Named function expression so the retry path can re-schedule itself
  // without closing over the outer `flush` binding before it is declared.
  const flush = useCallback(function flushFrame(): void {
    rafRef.current = null;
    const term = termRef.current;
    if (!term) {
      if (pendingRef.current.length > 0) {
        rafRef.current = requestAnimationFrame(flushFrame);
      }
      return;
    }
    const chunks = pendingRef.current;
    pendingRef.current = [];
    if (chunks.length === 0) return;
    const total = chunks.reduce((n: number, c: Uint8Array) => n + c.length, 0);
    const merged = new Uint8Array(total);
    let offset = 0;
    for (const c of chunks) {
      merged.set(c, offset);
      offset += c.length;
    }
    term.write(merged);
  }, []);

  function enqueueBytes(bytes: Uint8Array): void {
    pendingRef.current.push(bytes);
    bytesRef.current += bytes.length;
    onBytesRef.current?.(bytesRef.current);
    if (rafRef.current === null) {
      rafRef.current = requestAnimationFrame(flush);
    }
  }

  function enqueueText(text: string): void {
    enqueueBytes(new TextEncoder().encode(text));
  }

  function handleStreamEvent(evt: AgentStreamEvent): void {
    if (evt.exit_code !== null && evt.exit_code !== undefined) {
      const exitLine = `\r\n\x1b[90m── process exited with code ${evt.exit_code} ──\x1b[0m\r\n`;
      enqueueText(exitLine);
      onExitRef.current?.(evt.exit_code);
      return;
    }

    if (evt.stream === 'event' && evt.text && evt.text !== 'started' && evt.text !== 'budget') {
      enqueueText(`\r\n\x1b[36m[${evt.text}]\x1b[0m\r\n`);
      return;
    }

    if (evt.stream === 'stdout' || evt.stream === 'stderr') {
      if (evt.text) {
        enqueueText(sanitizeTty(evt.text));
      } else if (evt.bytes_b64) {
        try {
          const binary = atob(evt.bytes_b64);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i) & 0xff;
          }
          enqueueBytes(bytes);
        } catch {
          // malformed
        }
      }
    }
  }

  // ── xterm + SSE lifecycle ─────────────────────────────────────────────────
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return () => {};

    let disposed = false;
    let dataSub: { dispose(): void } | null = null;
    let ro: ResizeObserver | null = null;
    let resizeTimer: ReturnType<typeof setTimeout> | null = null;
    let sseConn: AgentTtyConnection | null = null;

    // Start SSE immediately — don't wait for xterm to load.
    sseConn = connectTtyStream(
      runId,
      (evt) => handleStreamEvent(evt),
      (status) => onSseStatusRef.current?.(status),
    );

    // Load xterm asynchronously.
    (async () => {
      try {
        const [xtermModule, fitModule] = await Promise.all([
          import('@xterm/xterm'),
          import('@xterm/addon-fit'),
        ]);
        if (disposed || !containerRef.current) return;

        const { Terminal } = xtermModule;
        const { FitAddon } = fitModule;

        const term = new Terminal({
          convertEol: false,
          cursorBlink: true,
          fontFamily:
            '"JetBrains Mono", ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
          fontSize: 13,
          scrollback: 5000,
          // Match the TUI tokens (near-black surface, cyan cursor). xterm needs
          // concrete colors here, so these mirror --color-bg-0 / accent-primary.
          theme: {
            background: '#05070a',
            foreground: '#e8f0f7',
            cursor: '#22d3ee',
            cursorAccent: '#05070a',
            selectionBackground: 'rgba(34, 211, 238, 0.30)',
          },
        });
        const fit = new FitAddon();
        term.loadAddon(fit);
        term.open(containerRef.current);
        safeFit(fit);
        termRef.current = term;
        fitRef.current = fit;
        setXtermReady(true);

        // Send the initial PTY resize so the backend knows our dimensions
        // before any content is rendered.
        void sendResize(runId, term.cols, term.rows);

        // Keystrokes → REST control.
        dataSub = term.onData((data: string) => {
          if (data.length === 1 && data.charCodeAt(0) === ETX) {
            void sendInterrupt(runId);
            return;
          }
          void sendInput(runId, data);
        });

        // Resize → debounced fit + REST resize.
        ro = new ResizeObserver(() => {
          if (resizeTimer) clearTimeout(resizeTimer);
          resizeTimer = setTimeout(() => {
            safeFit(fit);
            const t = termRef.current;
            if (t) {
              void sendResize(runId, t.cols, t.rows);
            }
          }, resizeDebounceMs);
        });
        ro.observe(container);

        // Flush any SSE events that arrived before xterm was ready.
        if (rafRef.current === null && pendingRef.current.length > 0) {
          rafRef.current = requestAnimationFrame(flush);
        }
      } catch (err) {
        if (!disposed) {
          setLoadError(String(err));
        }
      }
    })();

    return () => {
      disposed = true;
      sseConn?.close();
      dataSub?.dispose();
      ro?.disconnect();
      if (resizeTimer) clearTimeout(resizeTimer);
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
      pendingRef.current = [];
      termRef.current?.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [runId, flush]);

  if (loadError) {
    return (
      <div className="agent-terminal__surface agent-terminal__error" data-testid="agent-terminal-surface">
        <p style={{ color: '#ef4444', padding: '1rem' }}>
          Failed to load terminal: {loadError}
        </p>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="agent-terminal__surface"
      data-testid="agent-terminal-surface"
      role="presentation"
      style={{ minHeight: '200px' }}
    />
  );
}

function safeFit(fit: any): void {
  try {
    fit.fit();
  } catch {
    // no measurable viewport yet
  }
}

export default AgentTerminalImpl;
