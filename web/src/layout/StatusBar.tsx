// StatusBar.tsx — bottom strip (W-FE-01).

import { useRealtimeStore } from '../stores/realtimeStore';

export function StatusBar(): JSX.Element {
  const status = useRealtimeStore((s) => s.status);
  const lastSeq = useRealtimeStore((s) => s.lastSeq);
  const subs = useRealtimeStore((s) => s.subscriptions);
  const lastError = useRealtimeStore((s) => s.lastError);

  return (
    <div className="status-bar" role="status" aria-live="polite">
      <span className="status-bar__group">
        <span className="status-bar__pill">
          <span
            className={`status-bar__dot status-bar__dot--${status}`}
            aria-hidden="true"
          />
          WS: {status}
        </span>
        <span className="status-bar__pill">
          seq {lastSeq === null ? '—' : String(lastSeq)}
        </span>
        <span className="status-bar__pill">subs {subs.size}</span>
      </span>
      <span className="status-bar__group">
        {lastError ? (
          <span className="status-bar__pill" title={lastError.message}>
            err: {lastError.code}
          </span>
        ) : null}
        <span className="status-bar__pill">JeRyu Web Forge · Phase 1</span>
      </span>
    </div>
  );
}
