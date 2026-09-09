// ChecksPanel.tsx — list of CI status checks (W-FE-11).
//
// Each row shows the check name, a status badge (success / failing /
// pending / skipped / cancelled / neutral), and a chevron link to
// `details_url` when present. The panel header doubles as a summary
// (e.g. "3 passing · 1 failing · 0 pending").

import {
  CheckCircle2,
  Circle,
  CircleAlert,
  CircleDashed,
  CircleOff,
  ExternalLink,
  Loader2,
  type LucideIcon,
} from 'lucide-react';

import type { PullRequestCheck, PullRequestChecks } from '../../api/types';

import './merge.css';

type Tone =
  | 'success'
  | 'failing'
  | 'pending'
  | 'skipped'
  | 'cancelled'
  | 'neutral';

function toneFor(check: PullRequestCheck): Tone {
  const status = check.status?.toLowerCase() ?? '';
  if (status === 'success' || status === 'passing') return 'success';
  if (status === 'failure' || status === 'failing' || status === 'error')
    return 'failing';
  if (status === 'pending' || status === 'running' || status === 'queued')
    return 'pending';
  if (status === 'skipped') return 'skipped';
  if (status === 'cancelled' || status === 'canceled') return 'cancelled';
  return 'neutral';
}

const TONE_ICONS: Record<Tone, LucideIcon> = {
  success: CheckCircle2,
  failing: CircleAlert,
  pending: Loader2,
  skipped: CircleOff,
  cancelled: CircleDashed,
  neutral: Circle,
};

const TONE_LABELS: Record<Tone, string> = {
  success: 'passing',
  failing: 'failing',
  pending: 'pending',
  skipped: 'skipped',
  cancelled: 'cancelled',
  neutral: 'neutral',
};

export interface ChecksPanelProps {
  checks: PullRequestChecks | null;
  isLoading?: boolean;
  className?: string;
}

export function ChecksPanel({
  checks,
  isLoading = false,
  className,
}: ChecksPanelProps): JSX.Element {
  if (isLoading) {
    return (
      <section
        className={`checks-panel ${className ?? ''}`.trim()}
        aria-label="Status checks"
      >
        <header className="checks-panel__header">
          <h3 className="checks-panel__title">Checks</h3>
          <p className="checks-panel__summary">Loading…</p>
        </header>
      </section>
    );
  }

  const list = checks?.checks ?? [];

  return (
    <section
      className={`checks-panel ${className ?? ''}`.trim()}
      aria-label="Status checks"
    >
      <header className="checks-panel__header">
        <h3 className="checks-panel__title">Checks</h3>
        {checks ? (
          <p className="checks-panel__summary">
            <span className="checks-panel__count checks-panel__count--success">
              {checks.passing} passing
            </span>
            <span aria-hidden="true"> · </span>
            <span className="checks-panel__count checks-panel__count--failing">
              {checks.failing} failing
            </span>
            <span aria-hidden="true"> · </span>
            <span className="checks-panel__count checks-panel__count--pending">
              {checks.pending} pending
            </span>
            {checks.skipped > 0 ? (
              <>
                <span aria-hidden="true"> · </span>
                <span className="checks-panel__count checks-panel__count--skipped">
                  {checks.skipped} skipped
                </span>
              </>
            ) : null}
          </p>
        ) : (
          <p className="checks-panel__summary">No checks reported.</p>
        )}
      </header>

      {list.length === 0 ? (
        <p className="checks-panel__empty">No checks have run on this head.</p>
      ) : (
        <ul className="checks-panel__list">
          {list.map((check) => {
            const tone = toneFor(check);
            const Icon = TONE_ICONS[tone];
            return (
              <li
                key={check.id}
                className="checks-panel__item"
                data-tone={tone}
              >
                <span
                  className={`checks-panel__badge checks-panel__badge--${tone}`}
                  aria-label={TONE_LABELS[tone]}
                >
                  <Icon aria-hidden="true" size={14} />
                </span>
                <div className="checks-panel__body">
                  <div className="checks-panel__name">{check.name}</div>
                  {check.description ? (
                    <div className="checks-panel__description">
                      {check.description}
                    </div>
                  ) : null}
                </div>
                {check.details_url ? (
                  <a
                    href={check.details_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="checks-panel__link"
                    aria-label={`Open ${check.name} details`}
                  >
                    <ExternalLink aria-hidden="true" size={12} />
                  </a>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
