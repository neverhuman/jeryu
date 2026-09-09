// RepoCard.tsx — compact repository card for the grid view (W-FE-08).
//
// Renders the bits a triage user needs at-a-glance: repo name, description,
// language + default branch, visibility, open PR count, failing-check count,
// active-agent count, updated_at, plus repo-role + health + jankurai score
// pills on the right. The meta row also carries the offsite mirror posture
// and a running-jobs indicator (only when something is actually running).
//
// Click navigates to the overview page. The whole card is an anchor so
// modifier-clicks (open in new tab) work like a native link without us
// reimplementing the contract.

import { Bot, GitBranch, GitMerge, Play, ShieldAlert } from 'lucide-react';
import { Link } from 'react-router-dom';

import type { RepositorySummary } from '../../api/types';

import { JankuraiScoreBadge } from './JankuraiScoreBadge';
import { MirrorStatusBadge } from './MirrorStatusBadge';
import { RepoHealthPill } from './RepoHealthPill';
import { RepoRoleBadge } from './RepoRoleBadge';
import { relativeTime } from './relativeTime';
import './repo.css';

export interface RepoCardProps {
  repo: RepositorySummary;
}

export function repoHref(repo: RepositorySummary): string {
  return `/repos/${encodeURIComponent(repo.id.host)}/${repo.id.owner}/${repo.id.name}`;
}

export function RepoCard({ repo }: RepoCardProps): JSX.Element {
  const description = repo.description ?? 'No description provided.';
  return (
    <Link
      to={repoHref(repo)}
      className="repo-card"
      aria-label={`Open repository ${repo.id.name}`}
    >
      <div className="repo-card__head">
        <h3 className="repo-card__title">{repo.id.name}</h3>
        <div className="repo-card__pills">
          <RepoRoleBadge role={repo.repo_role} />
          <JankuraiScoreBadge
            score={repo.jankurai_score}
            decision={repo.jankurai_decision}
            scoredAt={repo.jankurai_scored_at}
          />
          <RepoHealthPill health={repo.health} />
        </div>
      </div>
      <p className="repo-card__description">{description}</p>
      <div className="repo-card__meta">
        <span className="repo-card__meta-item" title="Default branch">
          <GitBranch size={12} aria-hidden="true" /> {repo.default_branch}
        </span>
        {repo.language ? (
          <span className="repo-card__meta-item" title="Language">
            {repo.language}
          </span>
        ) : null}
        <span className="repo-card__meta-item" title="Visibility">
          {repo.visibility}
        </span>
        <span
          className="repo-card__meta-item"
          title="Open pull requests"
          aria-label={`${repo.open_pull_requests} open pull requests`}
        >
          <GitMerge size={12} aria-hidden="true" /> {repo.open_pull_requests}
        </span>
        <span
          className="repo-card__meta-item"
          title="Failing checks"
          aria-label={`${repo.failing_checks} failing checks`}
        >
          <ShieldAlert size={12} aria-hidden="true" /> {repo.failing_checks}
        </span>
        {repo.running_jobs > 0 ? (
          <span
            className="repo-card__meta-item repo-card__meta-item--running"
            title="Running jobs"
            aria-label={`${repo.running_jobs} running jobs`}
          >
            <Play size={12} aria-hidden="true" /> {repo.running_jobs}
          </span>
        ) : null}
        <MirrorStatusBadge mirror={repo.mirror} />
        <span
          className="repo-card__meta-item"
          title={repo.updated_at}
        >
          {relativeTime(repo.updated_at)}
        </span>
      </div>
      <div className="repo-card__actions">
        <a
          href={`${repoHref(repo)}/agents`}
          className="repo-card__agents-link"
          data-testid={`repo-agents-link-${repo.id.owner}-${repo.id.name}`}
          onClick={(e) => e.stopPropagation()}
          aria-label={`Open agents for ${repo.id.name} (${repo.active_agents} active)`}
        >
          <Bot size={14} aria-hidden="true" />
          <span className="repo-card__agents-count">{repo.active_agents}</span>
          Agents
        </a>
      </div>
    </Link>
  );
}
