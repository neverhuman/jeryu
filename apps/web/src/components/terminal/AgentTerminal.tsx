// AgentTerminal.tsx — public live agent terminal (SSE + REST transport).
//
// Renders a compact toolbar (SSE connection status, byte budget, Ctrl-C)
// and the xterm surface. Output arrives via SSE; input is sent via REST POST.
// AgentTerminalImpl is imported directly (xterm itself is still dynamically
// imported at runtime inside the impl).

import { useState, useCallback } from 'react';
import { CircleStop, TerminalSquare } from 'lucide-react';

import { sendInterrupt } from './agentControlTransport';
import { AgentTerminalImpl } from './AgentTerminalImpl';

import './terminal.css';

export interface AgentTerminalProps {
  runId: string;
  label?: string;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

export function AgentTerminal({ runId, label }: AgentTerminalProps): JSX.Element {
  const [bytes, setBytes] = useState(0);
  const [sseStatus, setSseStatus] = useState<'connecting' | 'open' | 'closed' | 'error'>('connecting');
  const [exitCode, setExitCode] = useState<number | null>(null);

  const onSseStatus = useCallback((s: 'connecting' | 'open' | 'closed' | 'error') => setSseStatus(s), []);
  const onExit = useCallback((code: number) => setExitCode(code), []);

  const live = sseStatus === 'open';
  const statusVariant = live
    ? 'success'
    : sseStatus === 'connecting'
      ? 'warning'
      : 'danger';

  return (
    <section
      className="agent-terminal"
      data-testid="agent-terminal"
      data-run-id={runId}
      aria-label={`Agent terminal for run ${runId}`}
    >
      <div className="agent-terminal__toolbar" data-testid="agent-terminal-toolbar">
        <span className="agent-terminal__title">
          <TerminalSquare size={14} aria-hidden="true" />
          {label ?? `run ${runId}`}
        </span>
        {exitCode !== null && (
          <span
            className={`agent-terminal__pill agent-terminal__pill--${exitCode === 0 ? 'success' : 'danger'}`}
            data-testid="agent-terminal-exit"
          >
            exit {exitCode}
          </span>
        )}
        <span
          className={`agent-terminal__pill agent-terminal__pill--${statusVariant}`}
          data-testid="agent-terminal-status"
        >
          <span
            className={`agent-terminal__dot${live ? ' is-live' : ''}`}
            aria-hidden="true"
          />
          {sseStatus === 'open' ? 'Live' : sseStatus}
        </span>
        <span
          className="agent-terminal__budget"
          data-testid="agent-terminal-budget"
          title="Bytes streamed this session"
        >
          {formatBytes(bytes)}
        </span>
        <button
          type="button"
          className="agent-terminal__interrupt"
          data-testid="agent-terminal-interrupt"
          onClick={() => void sendInterrupt(runId)}
          title="Send interrupt (Ctrl-C)"
        >
          <CircleStop size={14} aria-hidden="true" /> Ctrl-C
        </button>
      </div>
      <AgentTerminalImpl
        runId={runId}
        onBytes={setBytes}
        onSseStatus={onSseStatus}
        onExit={onExit}
      />
    </section>
  );
}

export default AgentTerminal;
