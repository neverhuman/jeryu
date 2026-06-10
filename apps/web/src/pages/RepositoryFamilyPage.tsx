// RepositoryFamilyPage.tsx — drill-down page for one repository family.
//
// Routed at `/repos/family/:family`. Fetches the member repositories via
// `useRepositories({ family })`, shows a rollup strip (member count,
// worst-of health, summed activity), and renders the members as plain
// repo cards inside a boxed panel.
//
// All five UX-QA states are wired: loading, 403 permission, error with
// retry, empty ("No repositories in this family"), success.

import { Boxes, GitMerge, Play, ShieldAlert } from 'lucide-react';
import { Link, useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import { Breadcrumbs } from '../components/browser';
import { RepoCard, aggregateFamily } from '../components/repo';
import { RepoHealthPill } from '../components/repo/RepoHealthPill';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useRepositories } from '../hooks/useRepositories';

import '../components/browser/browser.css';
import '../components/repo/repo.css';
import './page.css';

export function RepositoryFamilyPage(): JSX.Element {
  const params = useParams();
  const family = params.family ?? '';
  const list = useRepositories({ family });

  const repos = list.data?.repositories ?? [];
  const rollup = aggregateFamily(family, repos);

  let body: JSX.Element;
  if (list.isPending) {
    body = <LoadingState title="Loading family repositories…" rows={6} />;
  } else if (list.error) {
    const err = list.error;
    if (
      err instanceof ApiError &&
      (err.status === 403 || err.code === 'permission_denied')
    ) {
      body = (
        <PermissionDeniedState
          description="You do not have permission to view repositories."
          missingPermission="repo.read"
        />
      );
    } else {
      body = (
        <ErrorState
          title="Could not load family repositories"
          error={err}
          action={
            <ActionButton variant="primary" onClick={() => list.refetch()}>
              Retry
            </ActionButton>
          }
        />
      );
    }
  } else if (repos.length === 0) {
    body = (
      <EmptyState
        title="No repositories in this family"
        description={`No repositories carry the family label "${family}".`}
        icon={Boxes}
        action={
          <Link to="/repos">
            <ActionButton variant="primary">Back to repositories</ActionButton>
          </Link>
        }
      />
    );
  } else {
    body = (
      <section
        className="repo-family-panel"
        aria-label={`${family} repositories`}
      >
        <div className="page__cards">
          {repos.map((repo) => (
            <RepoCard key={repo.id.id} repo={repo} />
          ))}
        </div>
      </section>
    );
  }

  return (
    <div className="page page--wide" data-testid="repository-family-page">
      <Breadcrumbs
        segments={[{ label: 'Repos', to: '/repos' }, { label: family }]}
      />

      <header className="page__header">
        <div className="page__welcome">
          <h1 className="page__title">{family}</h1>
        </div>
        {!list.isPending && !list.error && repos.length > 0 ? (
          <div
            className="repo-family-strip"
            aria-label={`${family} rollup`}
          >
            <span className="repo-family-strip__item">
              {rollup.memberCount} repo{rollup.memberCount === 1 ? '' : 's'}
            </span>
            <RepoHealthPill health={rollup.health} />
            <span
              className="repo-family-strip__item"
              title="Open pull requests"
              aria-label={`${rollup.openPullRequests} open pull requests`}
            >
              <GitMerge size={12} aria-hidden="true" />{' '}
              {rollup.openPullRequests} open
            </span>
            <span
              className="repo-family-strip__item"
              title="Failing checks"
              aria-label={`${rollup.failingChecks} failing checks`}
            >
              <ShieldAlert size={12} aria-hidden="true" />{' '}
              {rollup.failingChecks} failing
            </span>
            <span
              className="repo-family-strip__item"
              title="Running jobs"
              aria-label={`${rollup.runningJobs} running jobs`}
            >
              <Play size={12} aria-hidden="true" /> {rollup.runningJobs}{' '}
              running
            </span>
          </div>
        ) : null}
      </header>

      {body}
    </div>
  );
}
