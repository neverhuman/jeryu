// AgentTerminalImpl.tsx — the heavy xterm-backed terminal surface.
//
// Lazy-loaded by `AgentTerminal` (React.lazy), and the xterm core itself is
// pulled in at RUNTIME via dynamic `import('@xterm/xterm')` inside the mount
// effect — so the ~330 KB core ships in its own `xterm-vendor` chunk and is
// only fetched the first time a terminal actually mounts (out of the initial
// bundle, and out of this lazy chunk's static graph too). This component owns
// the live wiring:
//   * a real `Terminal` + `FitAddon`, opened into a container div;
//   * a requestAnimationFrame-batched write loop so a burst of byte frames
//     coalesces into ONE `term.write` per frame instead of thrashing the
//     renderer (frames that arrive before xterm finishes loading are buffered);
//   * `onData` → `input` controls (operator keystrokes), with Ctrl-C (ETX)
//     promoted to an explicit `interrupt` control;
//   * a `ResizeObserver` → debounced `FitAddon.fit()` → `resize` control;
//   * chunk_seq gap / reconnect detection → clear + `resync` control.
//
// All transport goes through `realtimeStore` (`subscribeTty` + `sendAgentControl`)
// so the bytes never touch the rolling activity buffer.

import { useEffect, useRef } from 'react';
import type { Terminal as XTerm } from '@xterm/xterm';
import type { FitAddon as XFitAddon } from '@xterm/addon-fit';

import { useRealtimeStore } from '../../stores/realtimeStore';
import { base64ToBytes, textToBase64 } from './agentTtyDecode';
import { useAgentTty } from './useAgentTty';

export interface AgentTerminalImplProps {
  runId: string;
  /** Reports the cumulative decoded byte count so the toolbar can show a byte
   *  budget. */
  onBytes?: (totalBytes: number) => void;
  /** Debounce window (ms) for resize controls. Exposed for tests. */
  resizeDebounceMs?: number;
}

/** Byte value of Ctrl-C (ETX). Promoted from raw input to an interrupt. */
const ETX = 0x03;

export function AgentTerminalImpl({
  runId,
  onBytes,
  resizeDebounceMs = 150,
}: AgentTerminalImplProps): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XTerm | null>(null);
  const fitRef = useRef<XFitAddon | null>(null);

  // rAF write-loop state.
  const pendingRef = useRef<Uint8Array[]>([]);
  const rafRef = useRef<number | null>(null);
  const bytesRef = useRef(0);

  // Stream-continuity state.
  const lastChunkSeqRef = useRef<number | null>(null);
  const staleRef = useRef(false);
  const everOpenRef = useRef(false);

  const onBytesRef = useRef(onBytes);
  useEffect(() => {
    onBytesRef.current = onBytes;
  }, [onBytes]);

  const sendAgentControl = useRealtimeStore((s) => s.sendAgentControl);
  const status = useRealtimeStore((s) => s.status);

  // ── rAF-batched write loop ────────────────────────────────────────────────
  function flush(): void {
    rafRef.current = null;
    const term = termRef.current;
    // xterm may not have finished loading yet — keep the pending bytes and try
    // again next frame rather than dropping output.
    if (!term) {
      if (pendingRef.current.length > 0) {
        rafRef.current = requestAnimationFrame(flush);
      }
      return;
    }
    const chunks = pendingRef.current;
    pendingRef.current = [];
    if (chunks.length === 0) return;
    const total = chunks.reduce((n, c) => n + c.length, 0);
    const merged = new Uint8Array(total);
    let offset = 0;
    for (const c of chunks) {
      merged.set(c, offset);
      offset += c.length;
    }
    term.write(merged);
  }

  function scheduleFlush(): void {
    if (rafRef.current === null) {
      rafRef.current = requestAnimationFrame(flush);
    }
  }

  function enqueue(bytes: Uint8Array): void {
    pendingRef.current.push(bytes);
    bytesRef.current += bytes.length;
    onBytesRef.current?.(bytesRef.current);
    scheduleFlush();
  }

  function clearTerminal(): void {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    pendingRef.current = [];
    termRef.current?.clear();
  }

  // ── xterm lifecycle (xterm loaded at runtime) ─────────────────────────────
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return undefined;

    let disposed = false;
    let dataSub: { dispose(): void } | null = null;
    let ro: ResizeObserver | null = null;
    let resizeTimer: ReturnType<typeof setTimeout> | null = null;

    void (async () => {
      const [{ Terminal }, { FitAddon }] = await Promise.all([
        import('@xterm/xterm'),
        import('@xterm/addon-fit'),
      ]);
      // xterm's stylesheet is pulled in via `terminal.css` (`@import`), so it
      // rides the agents-route CSS chunk rather than the global bundle.
      if (disposed || !containerRef.current) return;

      const term = new Terminal({
        convertEol: false,
        cursorBlink: true,
        fontFamily:
          'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
        fontSize: 13,
        scrollback: 5000,
        theme: { background: '#0b0f17' },
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.open(containerRef.current);
      safeFit(fit);
      termRef.current = term;
      fitRef.current = fit;

      // Operator keystrokes. Ctrl-C is promoted to an explicit interrupt; all
      // other data is forwarded verbatim into the agent input (already a typed string).
      dataSub = term.onData((data: string) => {
        if (data.length === 1 && data.charCodeAt(0) === ETX) {
          sendAgentControl(runId, { kind: 'interrupt' });
          return;
        }
        sendAgentControl(runId, {
          kind: 'input',
          bytes_b64: textToBase64(data),
        });
      });

      // Resize → debounced fit + resize control.
      ro = new ResizeObserver(() => {
        if (resizeTimer) clearTimeout(resizeTimer);
        resizeTimer = setTimeout(() => {
          safeFit(fit);
          const t = termRef.current;
          if (t) {
            sendAgentControl(runId, {
              kind: 'resize',
              cols: t.cols,
              rows: t.rows,
            });
          }
        }, resizeDebounceMs);
      });
      ro.observe(container);

      // Drain anything that streamed in before xterm was ready.
      scheduleFlush();
    })();

    return () => {
      disposed = true;
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
    // runId / debounce are stable for a mounted terminal; sendAgentControl is a
    // stable store action.
  }, [runId]);

  // ── live stream ───────────────────────────────────────────────────────────
  useAgentTty(runId, (frame) => {
    const prev = lastChunkSeqRef.current;
    // A gap in the per-run chunk cursor means we lost bytes — the screen is no
    // longer trustworthy, so clear and ask the backend for a fresh snapshot.
    if (prev !== null && frame.chunk_seq !== prev + 1) {
      clearTerminal();
      lastChunkSeqRef.current = frame.chunk_seq;
      sendAgentControl(runId, { kind: 'resync' });
      // Still render the bytes we did receive after the resync request.
    } else {
      lastChunkSeqRef.current = frame.chunk_seq;
    }
    enqueue(base64ToBytes(frame.bytes_b64));
  });

  // Reconnect handling: while the socket is down the byte stream pauses, so on
  // return-to-open we clear the (now stale) screen and resync. We only treat a
  // drop as stale once we have already been open, so the INITIAL connect
  // (idle → connecting → open) never fires a spurious resync.
  useEffect(() => {
    if (status === 'open') {
      if (staleRef.current) {
        staleRef.current = false;
        lastChunkSeqRef.current = null;
        clearTerminal();
        sendAgentControl(runId, { kind: 'resync' });
      }
      everOpenRef.current = true;
      return;
    }
    if (
      (status === 'reconnecting' || status === 'closed') &&
      everOpenRef.current
    ) {
      staleRef.current = true;
    }
  }, [status, runId]);

  return (
    <div
      ref={containerRef}
      className="agent-terminal__surface"
      data-testid="agent-terminal-surface"
      role="presentation"
    />
  );
}

/** `FitAddon.fit()` throws if the container has no layout yet (0×0, e.g. in a
 *  unit test). Swallow that — the ResizeObserver will fit again once sized. */
function safeFit(fit: XFitAddon): void {
  try {
    fit.fit();
  } catch {
    // no measurable viewport yet
  }
}

export default AgentTerminalImpl;
