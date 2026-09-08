// tools/ScanProgressPanel.tsx — live scan progress: phase, repo counter bar,
// and the event feed, driven by the `tool_finder.scan` WebSocket scope.

import type { ToolFinderScanStatus, WebEvent } from '../../api/types';
import { formatCount } from './toolsHelpers';

/** Live scan progress: phase, repo counter bar, and the event feed. */
export function ScanProgressPanel({
  status,
  feed,
}: {
  status: ToolFinderScanStatus;
  feed: WebEvent[];
}): JSX.Element {
  const fraction =
    status.repos_total > 0 ? status.repos_done / status.repos_total : 0;
  return (
    <section
      className="tools-scan page__card"
      data-testid="scan-progress-panel"
      aria-live="polite"
    >
      <div className="tools-scan__head">
        <span className={`page__pill tools-scan__phase--${status.phase}`}>
          {status.phase}
        </span>
        <span className="tools-scan__counts">
          {status.repos_done}/{status.repos_total} repos ·{' '}
          {formatCount(status.files_scanned)} files ·{' '}
          {formatCount(status.clusters_found)} clusters
        </span>
        {status.current_repo ? (
          <span className="tools-scan__repo">{status.current_repo}</span>
        ) : null}
      </div>
      <div
        className="tools-scan__bar"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={status.repos_total}
        aria-valuenow={status.repos_done}
      >
        <div
          className="tools-scan__bar-fill"
          style={{ width: `${Math.round(fraction * 100)}%` }}
        />
      </div>
      {status.error ? (
        <p className="tools-scan__error">{status.error}</p>
      ) : null}
      <ul className="tools-scan__feed">
        {feed.slice(0, 6).map((event) => (
          <li key={String(event.seq)} className="tools-scan__feed-item">
            {event.summary}
          </li>
        ))}
      </ul>
    </section>
  );
}
