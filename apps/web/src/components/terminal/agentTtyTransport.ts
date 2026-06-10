// agentTtyTransport.ts — SSE-based live TTY transport for agent runs.
//
// Opens an EventSource to GET /api/v1/agent-runs/{id}/tty/stream?after_seq=N
// and delivers decoded TTY output to a callback. Handles reconnection with
// gapless catchup via the after_seq cursor.

/** A single decoded TTY event from the SSE stream. */
export interface AgentStreamEvent {
  seq: number;
  stream: 'stdout' | 'stderr' | 'event';
  text: string | null;
  bytes_b64: string | null;
  exit_code: number | null;
}

export type TtyEventHandler = (event: AgentStreamEvent) => void;
export type TtyStatusHandler = (status: 'connecting' | 'open' | 'closed' | 'error') => void;

export interface AgentTtyConnection {
  close(): void;
}

/**
 * Open an SSE connection to a run's live TTY stream.
 *
 * Returns a handle with a `close()` method. The caller is responsible for
 * closing when unmounting.
 */
export function connectTtyStream(
  runId: string,
  onEvent: TtyEventHandler,
  onStatus?: TtyStatusHandler,
  afterSeq = 0,
): AgentTtyConnection {
  let lastSeq = afterSeq;
  let source: EventSource | null = null;
  let closed = false;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  function open(): void {
    if (closed) return;
    onStatus?.('connecting');
    const url = `/api/v1/agent-runs/${encodeURIComponent(runId)}/tty/stream?after_seq=${lastSeq}`;
    source = new EventSource(url);

    source.onopen = () => {
      onStatus?.('open');
    };

    source.onmessage = (msg) => {
      try {
        const evt: AgentStreamEvent = JSON.parse(msg.data);
        if (evt.seq > lastSeq) {
          lastSeq = evt.seq;
        }
        onEvent(evt);
      } catch {
        // malformed event — skip
      }
    };

    source.onerror = () => {
      // EventSource fires error on connection loss. It auto-reconnects in
      // some browsers, but we manually reconnect with the cursor to be safe.
      if (closed) return;
      onStatus?.('error');
      source?.close();
      source = null;
      // Reconnect with backoff.
      reconnectTimer = setTimeout(() => {
        if (!closed) open();
      }, 1500);
    };
  }

  open();

  return {
    close() {
      closed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      source?.close();
      source = null;
      onStatus?.('closed');
    },
  };
}
