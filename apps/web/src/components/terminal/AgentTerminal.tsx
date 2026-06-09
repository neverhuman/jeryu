// AgentTerminal.tsx — public live agent terminal (`<AgentTerminal runId>`).
//
// Renders a toolbar (connection status, a Ctrl-C interrupt button, and a
// running byte-budget) above the lazily-loaded xterm surface. The terminal
// surface lives in `AgentTerminalImpl`, pulled in via `React.lazy`; the heavy
// xterm core itself is dynamically imported at runtime inside that impl, so it
// ships in the `xterm-vendor` chunk and never weighs down the initial bundle.
//
// The operator can both WATCH and DRIVE the run: keystrokes inside the surface
// become `input` controls, Ctrl-C (button or keyboard) becomes an `interrupt`,
// and viewport changes become debounced `resize` controls — all over the
// realtime socket via `realtimeStore`.

import { Suspense, lazy, useState } from 'react';
import { CircleStop, TerminalSquare } from 'lucide-react';

import { useRealtimeStore } from '../../stores/realtimeStore';

import './terminal.css';

// Lazy: the impl (and the xterm core it dynamically imports) stay out of the
// initial bundle. The impl module's default export is the component.
const AgentTerminalImpl = lazy(() => import('./AgentTerminalImpl'));

export interface AgentTerminalProps {
  /** The agent run whose TTY to attach to (`agent_run.{runId}` scope). */
  runId: string;
  /** Optional label shown in the toolbar (e.g. branch or agent name). */
  label?: string;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

export function AgentTerminal({ runId, label }: AgentTerminalProps): JSX.Element {
  const status = useRealtimeStore((s) => s.status);
  const sendAgentControl = useRealtimeStore((s) => s.sendAgentControl);
  const [bytes, setBytes] = useState(0);

  const live = status === 'open';
  const statusVariant = live
    ? 'success'
    : status === 'connecting' || status === 'reconnecting'
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
        <span
          className={`agent-terminal__pill agent-terminal__pill--${statusVariant}`}
          data-testid="agent-terminal-status"
        >
          <span
            className={`agent-terminal__dot${live ? ' is-live' : ''}`}
            aria-hidden="true"
          />
          {status}
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
          onClick={() => sendAgentControl(runId, { kind: 'interrupt' })}
          title="Send interrupt (Ctrl-C)"
        >
          <CircleStop size={14} aria-hidden="true" /> Ctrl-C
        </button>
      </div>
      <Suspense
        fallback={
          <div
            className="agent-terminal__loading"
            data-testid="agent-terminal-loading"
          >
            Loading terminal…
          </div>
        }
      >
        <AgentTerminalImpl runId={runId} onBytes={setBytes} />
      </Suspense>
    </section>
  );
}

export default AgentTerminal;
