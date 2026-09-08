import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { PullRequestListResponse } from '../api/types';
import { useResolveRepo } from '../hooks/useResolveRepo';
import { PullRequestListView } from './PullRequestListView';
import { fromPullRequestSummary, groupPullRequests } from './pullRoomModel';

import './page.css';
import './PullRoomPage.css';

export interface RepositoryPullRequestsPageProps {
  provider?: string;
  fullName?: string;
}

export function RepositoryPullRequestsPage(props: RepositoryPullRequestsPageProps = {}): JSX.Element {
  const params = useParams();
  const provider = props.provider ?? params.provider ?? 'unknown';
  const fullName = props.fullName ?? params.fullName ?? '';
  const resolved = useResolveRepo(provider, fullName);
  const repoId = resolved.data?.id ?? null;
  const pulls = useQuery({
    queryKey: ['repo-pulls', repoId],
    queryFn: ({ signal }) =>
      apiGet<PullRequestListResponse>(endpoints.pulls(repoId as string), {
        signal,
      }),
    enabled: typeof repoId === 'string' && repoId.length > 0,
    staleTime: 15_000,
  });
  const lanes = useMemo(
    () =>
      groupPullRequests(
        pulls.data?.items.map((item) => fromPullRequestSummary(item)) ?? []
      ),
    [pulls.data]
  );

  if (resolved.isPending) {
    return (
      <div className="page" data-testid="repo-pulls-page">
        <p className="page__roadmap-note">Resolving repository.</p>
      </div>
    );
  }

  if (resolved.error || !resolved.data) {
    return (
      <div className="page" data-testid="repo-pulls-page">
        <header className="page__header">
          <h1 className="page__title">Pull requests</h1>
        </header>
        <p className="page__roadmap-note">
          {resolved.error?.message ?? `No repository ${fullName}.`}
        </p>
      </div>
    );
  }

  return (
    <div className="page page--full pull-room" data-testid="repo-pulls-page">
      <header className="page__header pull-room__header">
        <div>
          <h1 className="page__title">Pull requests</h1>
          <p className="page__subtitle">
            {resolved.data.summary.id.owner}/{resolved.data.summary.id.name}
          </p>
        </div>
      </header>
      {pulls.isPending ? (
        <p className="page__roadmap-note">Loading pull requests.</p>
      ) : pulls.isError ? (
        <p className="page__roadmap-note">{pulls.error.message}</p>
      ) : (
        <PullRequestListView lanes={lanes} emptyMessage="No pull requests" />
      )}
    </div>
  );
}
