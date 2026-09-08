import { useMemo, useState } from 'react';

import type { EvidenceState } from '../api/types';
import { useControlPlane } from '../hooks/useControlPlane';
import { useEcosystem, useToolBuildClusters } from '../hooks/useToolingEvidence';
import { PullRequestListView } from './PullRequestListView';
import {
  DEFAULT_PULL_ROOM_FILTERS,
  filterPullRequests,
  fromControlPullRequest,
  groupPullRequests,
  rankToolBuildOpportunities,
  repoOptions,
  type PullRoomFilters,
} from './pullRoomModel';

import './page.css';
import './PullRoomPage.css';

const EVIDENCE_STATES: EvidenceState[] = [
  'fresh',
  'missing',
  'queued',
  'failed',
  'unknown',
];

export function PullRoomPage(): JSX.Element {
  const snapshot = useControlPlane();
  const toolClusters = useToolBuildClusters(8);
  const ecosystem = useEcosystem();
  const [filters, setFilters] = useState<PullRoomFilters>(
    DEFAULT_PULL_ROOM_FILTERS
  );

  const items = useMemo(
    () => snapshot.data?.pullRequests.map(fromControlPullRequest) ?? [],
    [snapshot.data]
  );
  const filtered = useMemo(
    () => filterPullRequests(items, filters),
    [filters, items]
  );
  const lanes = useMemo(() => groupPullRequests(filtered), [filtered]);
  const repos = useMemo(() => repoOptions(items), [items]);
  const opportunities = useMemo(
    () =>
      snapshot.data
        ? rankToolBuildOpportunities(
            snapshot.data,
            toolClusters.data?.clusters ?? []
          )
        : [],
    [snapshot.data, toolClusters.data]
  );

  if (snapshot.isLoading) {
    return (
      <div className="page pull-room" data-testid="pull-room-page">
        <header className="page__header">
          <h1 className="page__title">Pull Room</h1>
        </header>
        <p className="page__roadmap-note">Loading pull request control plane.</p>
      </div>
    );
  }

  if (snapshot.isError || !snapshot.data) {
    return (
      <div className="page pull-room" data-testid="pull-room-page">
        <header className="page__header">
          <h1 className="page__title">Pull Room</h1>
        </header>
        <p className="page__roadmap-note">
          {snapshot.error?.message ?? 'Pull Room control-plane snapshot unavailable.'}
        </p>
      </div>
    );
  }

  return (
    <div className="page page--full pull-room" data-testid="pull-room-page">
      <header className="page__header pull-room__header">
        <div>
          <h1 className="page__title">Pull Room</h1>
          <p className="page__subtitle">
            Cross-repo pull request cockpit from local control-plane truth.
          </p>
        </div>
        <div className="pull-room__summary">
          <Metric label="open" value={snapshot.data.summary.openPrCount} />
          <Metric label="missing checks" value={snapshot.data.summary.missingCheckPrCount} />
          <Metric label="failing checks" value={snapshot.data.summary.failingCheckCount} />
          <Metric label="tool clusters" value={snapshot.data.toolBuild.clusterCount} />
        </div>
      </header>

      <section className="pull-room__filters" aria-label="Pull Room filters">
        <label>
          Repo
          <select
            value={filters.repo}
            onChange={(event) =>
              setFilters((current) => ({ ...current, repo: event.target.value }))
            }
          >
            <option value="all">All repos</option>
            {repos.map((repo) => (
              <option key={repo} value={repo}>
                {repo}
              </option>
            ))}
          </select>
        </label>
        <label>
          State
          <select
            value={filters.state}
            onChange={(event) =>
              setFilters((current) => ({ ...current, state: event.target.value }))
            }
          >
            <option value="all">All states</option>
            <option value="draft">Draft</option>
            <option value="open">Open</option>
            <option value="mergeable">Mergeable</option>
            <option value="merged">Merged</option>
            <option value="closed">Closed</option>
          </select>
        </label>
        <label>
          Evidence
          <select
            value={filters.evidence}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                evidence: event.target.value,
              }))
            }
          >
            <option value="all">All evidence</option>
            {EVIDENCE_STATES.map((state) => (
              <option key={state} value={state}>
                {state}
              </option>
            ))}
          </select>
        </label>
        <label>
          Checks
          <select
            value={filters.checkPosture}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                checkPosture: event.target.value,
              }))
            }
          >
            <option value="all">All checks</option>
            <option value="missing">Missing</option>
            <option value="failing">Failing</option>
            <option value="queued">Queued</option>
            <option value="running">Running</option>
            <option value="passing">Passing</option>
          </select>
        </label>
        <label className="pull-room__search">
          Search
          <input
            value={filters.search}
            onChange={(event) =>
              setFilters((current) => ({ ...current, search: event.target.value }))
            }
            type="search"
            aria-label="Search pull requests"
          />
        </label>
      </section>

      <div className="pull-room__content">
        <PullRequestListView
          lanes={lanes}
          emptyMessage="No pull requests match the current filters."
        />
        <aside className="pull-room__rail" aria-labelledby="tooling-opportunities">
          <h2 id="tooling-opportunities">Tooling opportunities</h2>
          <div className="pull-room__rail-panel">
            <h3>Tool graph</h3>
            {ecosystem.isError ? (
              <p>Tool graph unavailable: {ecosystem.error.message}</p>
            ) : (
              <p>
                {ecosystem.data?.tools.length ?? 0} tools ·{' '}
                {ecosystem.data?.tools.filter((tool) => tool.conformance === 'mutating').length ?? 0}{' '}
                mutating · {ecosystem.data?.degradedReason || 'live'}
              </p>
            )}
          </div>
          <div className="pull-room__opportunities">
            {toolClusters.isError ? (
              <p className="pull-room__rail-note">
                Tool-build clusters unavailable: {toolClusters.error.message}
              </p>
            ) : opportunities.length === 0 ? (
              <p className="pull-room__rail-note">
                No tool-building clusters available.
              </p>
            ) : (
              opportunities.slice(0, 6).map((item) => (
                <article className="pull-room__opportunity" key={item.id}>
                  <div className="pull-room__opportunity-top">
                    <strong>{item.repo}</strong>
                    <span>{item.score}</span>
                  </div>
                  <p>{item.insight}</p>
                  <div className="pull-room__opportunity-meta">
                    <span>{item.occurrenceCount} occurrences</span>
                    <span>{item.fileCount} files</span>
                    {item.language ? <span>{item.language}</span> : null}
                  </div>
                  <code>{item.suggestedProofLane}</code>
                </article>
              ))
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }): JSX.Element {
  return (
    <div className="pull-room__metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
