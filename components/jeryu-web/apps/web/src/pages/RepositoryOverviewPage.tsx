// RepositoryOverviewPage.tsx — repository overview (W-FE-09).
//
// Resolves `:provider/*fullName` to an opaque `repo_id` via the list cache
// (`useResolveRepo`) and renders:
//   * Top strip: title, visibility, default branch, clone URL popover, health
//   * Main: rendered README via <ReadmePanel>
//   * Sidebar: default branch + last-updated, open pull request / failing
//     check counts, and recent agent activity — all from `RepositorySummary`.
//
// The realtime store subscribes to `repo.${id}` once the resolution
// completes. (An earlier revision subscribed to a synthetic
// `repo.${provider}.${fullName}` scope before opaque ids were available.)

import { GitBranch } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link, Navigate, useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import {
  Breadcrumbs,
  BranchSelector,
  ReadmePanel,
} from '../components/browser';
import {
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { RepoDangerZone } from '../components/repo/RepoDangerZone';
import { RepoHealthPill } from '../components/repo/RepoHealthPill';
import { RepoRoleBadge } from '../components/repo/RepoRoleBadge';
import { useRealtime } from '../hooks/useRealtime';
import { useResolveRepo } from '../hooks/useResolveRepo';
import { useSelectionStore } from '../stores/selectionStore';

import { ClonePopover } from './repositoryOverviewParts';

import '../components/browser/browser.css';
import './page.css';

function fullNameFromParams(params: Record<string, string | undefined>): string {
  const main = params.fullName ?? '';
  const tail = params['*'] ?? '';
  if (tail) {
    return `${main}/${tail}`.replace(/\/+$/, '');
  }
  return main;
}

export interface RepositoryOverviewPageProps {
  provider?: string;
  fullName?: string;
}

export function RepositoryOverviewPage(props: RepositoryOverviewPageProps = {}): JSX.Element {
  const params = useParams();
  const provider = props.provider ?? params.provider ?? 'unknown';
  const fullName = props.fullName ?? fullNameFromParams(params);
  const resolved = useResolveRepo(provider, fullName);
  const setRepo = useSelectionStore((s) => s.setCurrentRepo);

  const repoId = resolved.data?.id ?? null;
  const summary = resolved.data?.summary ?? null;
  const defaultBranch = summary?.default_branch ?? '';
  const [activeRef, setActiveRef] = useState<string>('');

  useEffect(() => {
    setRepo(repoId);
    return () => setRepo(null);
  }, [repoId, setRepo]);

  useEffect(() => {
    if (defaultBranch && !activeRef) setActiveRef(defaultBranch);
  }, [defaultBranch, activeRef]);

  useRealtime(repoId ? [`repo.${repoId}`] : []);

  if (resolved.isPending) {
    return (
      <div className="page" data-testid="repo-overview-page">
        <LoadingState
          title="Loading repository…"
          variant="message"
          description="Resolving the repository from the list cache."
        />
      </div>
    );
  }
  if (resolved.error) {
    const err = resolved.error;
    if (err instanceof ApiError && err.status === 403) {
      return (
        <div className="page" data-testid="repo-overview-page">
          <PermissionDeniedState
            description="You do not have permission to view this repository."
            missingPermission="repo.read"
          />
        </div>
      );
    }
    return (
      <div className="page" data-testid="repo-overview-page">
        <ErrorState
          title="Could not load repository"
          error={resolved.error}
        />
      </div>
    );
  }
  if (!summary) {
    return (
      <div className="page" data-testid="repo-overview-page">
        <ErrorState
          title="Repository not found"
          description={`No repository named ${fullName} on ${provider}.`}
          action={
            <Link to="/repos">
              <ActionButton variant="primary">Back to repositories</ActionButton>
            </Link>
          }
        />
      </div>
    );
  }

  if (summary.family) {
    return (
      <Navigate
        to={`/repos/family/${encodeURIComponent(summary.family)}`}
        replace
      />
    );
  }

  return (
    <div className="page" data-testid="repo-overview-page">
      <Breadcrumbs
        segments={[
          { label: 'Repos', to: '/repos' },
          { label: provider, to: `/repos?host=${provider}` },
          { label: summary.id.owner },
          { label: summary.id.name },
        ]}
      />

      <header className="page__header">
        <div className="repo-overview__head">
          <h1 className="repo-overview__title">{summary.id.name}</h1>
          <RepoHealthPill health={summary.health} />
          <RepoRoleBadge role={summary.repo_role} />
          <span className="page__pill">{summary.visibility}</span>
          {summary.language ? (
            <span className="page__pill">{summary.language}</span>
          ) : null}
        </div>
        {summary.description ? (
          <p className="page__subtitle">{summary.description}</p>
        ) : null}
        <div className="repo-overview__strip">
          <BranchSelector
            repoId={repoId}
            value={activeRef}
            onSelect={setActiveRef}
          />
          <span>
            <GitBranch size={12} aria-hidden="true" /> {summary.default_branch}
          </span>
          <ClonePopover
            httpUrl={summary.clone_http_url}
            sshUrl={summary.clone_ssh_url}
          />
          <Link
            to={`/repos/${encodeURIComponent(provider)}/${fullName}/code`}
            aria-label="Browse code"
          >
            <ActionButton variant="default">Browse code</ActionButton>
          </Link>
          <Link
            to={`/repos/${encodeURIComponent(provider)}/${fullName}/agents`}
            aria-label="Open agents and start a new session"
          >
            <ActionButton variant="primary">Agents</ActionButton>
          </Link>
        </div>
      </header>

      <section className="repo-overview" aria-label="Repository overview">
        <div>
          <ReadmePanel repoId={repoId} ref={activeRef || defaultBranch} />
        </div>
        <aside className="repo-overview__sidebar" aria-label="Sidebar">
          <article className="repo-overview__sidebar-card">
            <h2 className="repo-overview__sidebar-title">Default branch</h2>
            <p className="text-muted">
              <code>{summary.default_branch}</code> · updated{' '}
              {summary.updated_at}
            </p>
            <Link
              to={`/repos/${encodeURIComponent(provider)}/${fullName}/code`}
            >
              Browse code
            </Link>
          </article>
          <article className="repo-overview__sidebar-card">
            <h2 className="repo-overview__sidebar-title">Open pull requests</h2>
            <p className="text-muted">
              {summary.open_pull_requests} open · {summary.failing_checks} failing
              checks
            </p>
            <Link
              to={`/repos/${encodeURIComponent(provider)}/${fullName}/pulls`}
            >
              View pull requests
            </Link>
          </article>
          <article className="repo-overview__sidebar-card">
            <h2 className="repo-overview__sidebar-title">Agents</h2>
            <p className="text-muted">
              {summary.active_agents} active agent
              {summary.active_agents === 1 ? '' : 's'} · updated{' '}
              {summary.updated_at}
            </p>
            <Link
              to={`/repos/${encodeURIComponent(provider)}/${fullName}/agents`}
            >
              View agents · New session
            </Link>
          </article>
        </aside>
      </section>

      <RepoDangerZone repo={summary} />
    </div>
  );
}
