// LiveActivityDock.tsx — right-side activity rail (W-FE-01).
//
// Subscribes to `global.activity` and renders the most recent events as a
// chronological list. The dock is the only place a WebSocket event surfaces
// without a dedicated page; per-page hooks (W-FE-06) add their own subs.

import { useRealtimeStore } from '../stores/realtimeStore';
import { useRealtime } from '../hooks/useRealtime';

export function LiveActivityDock(): JSX.Element {
  useRealtime(['global.activity']);
  const events = useRealtimeStore((s) => s.events);

  return (
    <div className="activity-dock">
      <header className="activity-dock__header">
        <h2 className="activity-dock__title">Live activity</h2>
        <span className="activity-dock__count" aria-label="Event count">
          {events.length}
        </span>
      </header>
      <div className="activity-dock__body" role="log" aria-live="polite">
        {events.length === 0 ? (
          <p className="activity-dock__empty">
            Waiting for the first event…
          </p>
        ) : (
          events.map((evt) => (
            <article
              key={String(evt.seq)}
              className="activity-dock__item"
              aria-label={`${evt.kind} on ${evt.entity}`}
            >
              <span className="activity-dock__scope">{evt.scope}</span>
              <span className="activity-dock__summary">{evt.summary}</span>
              <span className="activity-dock__meta">
                {formatTime(evt.timestamp)} · seq {String(evt.seq)}
              </span>
            </article>
          ))
        )}
      </div>
    </div>
  );
}

function formatTime(input: string): string {
  try {
    const dt = new Date(input);
    return dt.toLocaleTimeString();
  } catch {
    return input;
  }
}
