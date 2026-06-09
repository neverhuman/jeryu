// RepositoryAgentsPage.tsx — per-repo "Active Agents" view + live terminal.
//
// Lists the agent runs attached to a repository (branch, runner, status, and a
// tty-live dot) from `GET /api/v1/repos/{id}/agent-runs`, and — when a run is
// selected — mounts the live `<AgentTerminal>` so the operator can watch and
// drive that run's sandboxed TTY.
//
// Route (see router.tsx): `/repos/:provider/:fullName/agents/*`; the
// splat tail is an optional run id so `/agents/{runId}` deep-links straight to
// a terminal. In-app row selection is COMPONENT STATE (rows are buttons, not
// links): React Router decodes the `%2F`-encoded `:fullName` segment during
// client-side navigation, which re-resolves the route to the wrong page — so
// selecting a run must not navigate. Deep links still work because a fresh
// document load matches against the (encoded) `window.location`.

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Bot, GitBranch, Plus, Radio, Server } from 'lucide-react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { RepoAgentRunsResponse, RepoAgentSummary } from '../api/types';
import { AgentTerminal } from '../components/terminal/AgentTerminal';
import { useResolveRepo } from '../hooks/useResolveRepo';
import { useRealtime } from '../hooks/useRealtime';
import { useCreateSession } from '../hooks/useCreateSession';

import './page.css';
import './RepositoryAgentsPage.css';

export function RepositoryAgentsPage(): JSX.Element {
  const params = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const provider = params.provider ?? 'unknown';
  const fullName = params.fullName ?? '';
  // Deep-link seed: the optional run id rides the `agents/*` splat tail.
  const splatRunId = (params['*'] ?? '').replace(/\/+$/, '') || null;
  const [selectedRunId, setSelectedRunId] = useState<string | null>(splatRunId);
  // Which coding agent the New Session button launches (Codex / Claude / Jekko).
  const [agentId, setAgentId] = useState('codex');

  const resolved = useResolveRepo(provider, fullName);
  const repoId = resolved.data?.id ?? null;

  // Keep the per-repo activity scope live so the list reflects new runs.
  useRealtime(repoId ? [`repo.${repoId}`] : []);

  const createSession = useCreateSession(repoId);

  // Launch an isolated session, then deep-link to its terminal route and mount
  // `<AgentTerminal>` on the returned run. The URL is rebuilt from the live
  // (still %2F-encoded) pathname so the `:fullName` segment is not re-decoded.
  function onNewSession(): void {
    createSession.mutate(agentId, {
      onSuccess: (created) => {
        setSelectedRunId(created.run_id);
        const agentsBase = location.pathname.replace(/\/agents(?:\/.*)?$/, '/agents');
        navigate(`${agentsBase}/${encodeURIComponent(created.run_id)}`);
      },
    });
  }

  const runs = useQuery({
    queryKey: ['repo-agent-runs', repoId],
    queryFn: ({ signal }) =>
      apiGet<RepoAgentRunsResponse>(endpoints.repoAgentRuns(repoId as string), {
        signal,
      }),
    enabled: typeof repoId === 'string' && repoId.length > 0,
    staleTime: 10_000,
  });

  if (resolved.isPending) {
    return (
      <div className="page" data-testid="repo-agents-page">
        <p className="page__roadmap-note">Resolving repository.</p>
      </div>
    );
  }

  if (resolved.error || !resolved.data) {
    return (
      <div className="page" data-testid="repo-agents-page">
        <header className="page__header">
          <h1 className="page__title">Active agents</h1>
        </header>
        <p className="page__roadmap-note">
          {resolved.error?.message ?? `No repository ${fullName}.`}
        </p>
      </div>
    );
  }

  const items = runs.data?.items ?? [];
  const selected = selectedRunId
    ? items.find((r: RepoAgentSummary) => r.run_id === selectedRunId) ?? null
    : null;

  return (
    <div className="page page--full agents" data-testid="repo-agents-page">
      <header className="page__header">
        <div>
          <h1 className="page__title">Active agents</h1>
          <p className="page__subtitle">
            {resolved.data.summary.id.owner}/{resolved.data.summary.id.name}
          </p>
        </div>
        <div className="agents__header-actions">
          <label className="agents__agent-pick">
            <span className="sr-only">Agent</span>
            <select
              className="agents__agent-select"
              value={agentId}
              onChange={(event) => setAgentId(event.target.value)}
              disabled={createSession.isPending}
              data-testid="new-session-agent"
              aria-label="Agent to launch"
            >
              <option value="codex">Codex</option>
              <option value="claude">Claude</option>
              <option value="jekko">Jekko</option>
            </select>
          </label>
          <button
            type="button"
            className="agents__new-session"
            data-testid="new-session-button"
            onClick={onNewSession}
            disabled={createSession.isPending || !repoId}
            aria-busy={createSession.isPending}
          >
            <Plus size={14} aria-hidden="true" />
            {createSession.isPending ? 'Starting…' : 'New Session'}
          </button>
        </div>
      </header>

      {createSession.isError ? (
        <p
          className="page__roadmap-note agents__new-session-error"
          role="alert"
          data-testid="new-session-error"
        >
          Could not start a session: {createSession.error.message}
        </p>
      ) : null}

      <div className="agents__layout">
        <section
          className="page__section agents__list-pane"
          aria-labelledby="agents-list"
        >
          <h2 className="page__section-title" id="agents-list">
            Runs
          </h2>
          {runs.isPending ? (
            <p className="page__roadmap-note">Loading agent runs.</p>
          ) : runs.isError ? (
            <p className="page__roadmap-note">{runs.error.message}</p>
          ) : items.length === 0 ? (
            <p className="page__roadmap-note" data-testid="agents-empty">
              No agent runs on this repository yet.
            </p>
          ) : (
            <ul className="agents__list" data-testid="agents-list">
              {items.map((run: RepoAgentSummary) => (
                <AgentRow
                  key={run.run_id}
                  run={run}
                  onSelect={() => setSelectedRunId(run.run_id)}
                  active={run.run_id === selectedRunId}
                />
              ))}
            </ul>
          )}
        </section>

        <section className="agents__terminal-pane" aria-label="Agent terminal">
          {selectedRunId ? (
            <AgentTerminal
              runId={selectedRunId}
              label={selected ? `${selected.branch} · ${selected.agent ?? 'agent'}` : undefined}
            />
          ) : (
            <p className="page__roadmap-note" data-testid="agents-no-selection">
              Choose a run to open its live terminal.
            </p>
          )}
        </section>
      </div>
    </div>
  );
}

function AgentRow({
  run,
  onSelect,
  active,
}: {
  run: RepoAgentSummary;
  onSelect: () => void;
  active: boolean;
}): JSX.Element {
  return (
    <li>
      <button
        type="button"
        onClick={onSelect}
        className={`agents__row${active ? ' is-active' : ''}`}
        data-testid={`agent-row-${run.run_id}`}
        aria-pressed={active}
      >
        <span className="agents__row-branch">
          <GitBranch size={13} aria-hidden="true" /> {run.branch}
        </span>
        <span className="agents__row-runner">
          <Server size={13} aria-hidden="true" /> {run.runner}
        </span>
        <span className="agents__row-agent">
          <Bot size={13} aria-hidden="true" /> {run.agent ?? 'agent'}
        </span>
        <span
          className={`agents__row-status agents__row-status--${statusVariant(run.status)}`}
          data-testid={`agent-status-${run.run_id}`}
        >
          {run.status}
        </span>
        <span
          className={`agents__tty-dot${run.tty_live ? ' is-live' : ''}`}
          data-testid={`agent-tty-${run.run_id}`}
          title={run.tty_live ? 'TTY streaming' : 'No live TTY'}
        >
          <Radio size={12} aria-hidden="true" />
          {run.tty_live ? 'live' : 'idle'}
        </span>
      </button>
    </li>
  );
}

function statusVariant(status: string): 'success' | 'warning' | 'danger' | 'muted' {
  switch (status) {
    case 'running':
      return 'success';
    case 'blocked':
    case 'queued':
      return 'warning';
    case 'failed':
    case 'errored':
      return 'danger';
    default:
      return 'muted';
  }
}
