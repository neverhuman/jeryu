// DashboardPage.tsx — root dashboard (W-FE-07 minimal).
//
// Phase 1 demonstrates the bootstrap query and the five required UX-QA
// states (loading / empty / error / permission denied / success). W-FE-07
// implements the full "What needs attention?" cards.

import { useState } from 'react';
import {
  Activity,
  FolderOpenDot,
  GitBranch,
  Sparkles,
} from 'lucide-react';

import { ActionButton } from '../components/action/ActionButton';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useBootstrap } from '../hooks/useBootstrap';
import { useRealtime } from '../hooks/useRealtime';
import { useRealtimeStore } from '../stores/realtimeStore';

import './page.css';

type DemoState = 'live' | 'loading' | 'empty' | 'error' | 'denied';

export function DashboardPage(): JSX.Element {
  const bootstrap = useBootstrap();
  const realtimeStatus = useRealtimeStore((s) => s.status);
  const [demoState, setDemoState] = useState<DemoState>('live');

  useRealtime(['global.activity', 'system.health']);

  if (demoState === 'loading' || bootstrap.isPending) {
    return (
      <div className="page" data-testid="dashboard-page">
        <DemoStateSwitcher value={demoState} onChange={setDemoState} />
        <LoadingState
          title="Loading dashboard"
          description="Fetching the bootstrap snapshot."
          variant="message"
        />
      </div>
    );
  }

  if (demoState === 'error' || bootstrap.isError) {
    return (
      <div className="page" data-testid="dashboard-page">
        <DemoStateSwitcher value={demoState} onChange={setDemoState} />
        <ErrorState
          title="Could not load the dashboard."
          description={
            demoState === 'error'
              ? 'Mock error — toggle the state switcher to recover.'
              : 'The bootstrap endpoint returned an error.'
          }
          error={bootstrap.error}
          action={
            <ActionButton
              variant="primary"
              onClick={() => {
                setDemoState('live');
                bootstrap.refetch();
              }}
            >
              Retry
            </ActionButton>
          }
        />
      </div>
    );
  }

  if (demoState === 'denied') {
    return (
      <div className="page" data-testid="dashboard-page">
        <DemoStateSwitcher value={demoState} onChange={setDemoState} />
        <PermissionDeniedState
          title="Dashboard requires sign-in"
          description="Mock permission-denied state to validate W-CC-02 wiring."
          missingPermission="dashboard.read"
          action={
            <ActionButton onClick={() => setDemoState('live')}>
              Back
            </ActionButton>
          }
        />
      </div>
    );
  }

  const data = bootstrap.data;
  const recent = demoState === 'empty' ? [] : data?.recent_repositories ?? [];
  const viewer = data?.viewer;
  const flags = data?.feature_flags;

  return (
    <div className="page" data-testid="dashboard-page">
      <header className="page__header">
        <div className="page__welcome">
          <h1 className="page__title">Welcome back</h1>
          {viewer ? (
            <span className="page__welcome-login">
              {viewer.display_name ?? viewer.login}
            </span>
          ) : null}
          <RealtimePill status={realtimeStatus} />
        </div>
        <p className="page__subtitle">
          The JeRyu Web Forge mission-control surface, showing your recent
          repositories and the active feature flags. The attention/blocker
          matrix is delivered by W-FE-07.
        </p>
        <DemoStateSwitcher value={demoState} onChange={setDemoState} />
      </header>

      <section className="page__section" aria-labelledby="recent-repos">
        <h2 className="page__section-title" id="recent-repos">
          Recent repositories
        </h2>
        {recent.length === 0 ? (
          <EmptyState
            title="No recent repositories yet"
            description="When you open a repository, it appears here for quick re-entry."
            icon={FolderOpenDot}
            action={
              <ActionButton variant="primary">Browse repositories</ActionButton>
            }
          />
        ) : (
          <div className="page__cards">
            {recent.map((repo) => (
              <article
                className="page__card"
                key={repo.id.id}
                aria-label={`Repository ${repo.id.owner}/${repo.id.name}`}
              >
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                  }}
                >
                  <h3 className="page__card-title">
                    {repo.id.owner}/{repo.id.name}
                  </h3>
                  <span className={`page__pill page__pill--${pillForHealth(repo.health)}`}>
                    {repo.health}
                  </span>
                </div>
                <p className="page__subtitle">
                  {repo.description ?? 'No description provided.'}
                </p>
                <dl className="page__meta-grid">
                  <dt>Default branch</dt>
                  <dd>
                    <GitBranch
                      aria-hidden="true"
                      size={12}
                      style={{ display: 'inline', verticalAlign: 'middle' }}
                    />{' '}
                    {repo.default_branch}
                  </dd>
                  <dt>Open PRs</dt>
                  <dd>{repo.open_pull_requests}</dd>
                  <dt>Failing checks</dt>
                  <dd>{repo.failing_checks}</dd>
                  <dt>Updated</dt>
                  <dd>{repo.updated_at}</dd>
                </dl>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="page__section" aria-labelledby="feature-flags">
        <h2 className="page__section-title" id="feature-flags">
          Feature flags
        </h2>
        <div className="page__feature-list">
          {flags
            ? Object.entries(flags).map(([key, value]) => (
                <span
                  className={`page__pill ${
                    value ? 'page__pill--success' : ''
                  }`}
                  key={key}
                >
                  {value ? <Sparkles size={10} aria-hidden="true" /> : null}
                  {key}: {value ? 'on' : 'off'}
                </span>
              ))
            : null}
        </div>
      </section>

      <p className="page__roadmap-note">
        The recent-repositories and feature-flag panels above are live. The
        attention/blocker matrix, PR cockpit summaries, and agent activity
        feed are delivered by W-FE-07.
      </p>
    </div>
  );
}

function pillForHealth(health: string): 'success' | 'warning' | 'danger' {
  if (health === 'healthy') return 'success';
  if (health === 'degraded') return 'warning';
  if (health === 'failing') return 'danger';
  return 'warning';
}

function RealtimePill({ status }: { status: string }): JSX.Element {
  const variant: 'success' | 'warning' | 'danger' =
    status === 'open'
      ? 'success'
      : status === 'connecting' || status === 'reconnecting'
        ? 'warning'
        : 'danger';
  return (
    <span className={`page__pill page__pill--${variant}`}>
      <Activity size={10} aria-hidden="true" /> {status}
    </span>
  );
}

interface DemoStateSwitcherProps {
  value: DemoState;
  onChange: (next: DemoState) => void;
}

function DemoStateSwitcher({
  value,
  onChange,
}: DemoStateSwitcherProps): JSX.Element {
  const options: { id: DemoState; label: string }[] = [
    { id: 'live', label: 'Live' },
    { id: 'loading', label: 'Loading' },
    { id: 'empty', label: 'Empty' },
    { id: 'error', label: 'Error' },
    { id: 'denied', label: 'Permission denied' },
  ];
  return (
    <div
      className="page__inline-actions"
      role="radiogroup"
      aria-label="Preview dashboard state"
    >
      {options.map((opt) => (
        <ActionButton
          key={opt.id}
          variant={value === opt.id ? 'primary' : 'default'}
          role="radio"
          aria-checked={value === opt.id}
          onClick={() => onChange(opt.id)}
        >
          {opt.label}
        </ActionButton>
      ))}
    </div>
  );
}
