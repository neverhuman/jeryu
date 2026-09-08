import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { WorkItemListResponse } from '../api/types';
import { useResolveRepo } from '../hooks/useResolveRepo';
import {
  DEFAULT_WORK_FILTERS,
  assigneeOptions,
  filterWorkItems,
  groupWorkItems,
  labelOptions,
  repoOptions,
  type WorkFilters,
} from './workModel';
import {
  WorkCreateForm,
  WorkFilterBar,
  WorkLanes,
  WorkSummary,
} from './work';

import './page.css';
import './WorkPage.css';

export interface WorkPageProps {
  provider?: string;
  fullName?: string;
  alias?: 'issues';
}

interface WorkBoardProps {
  title: string;
  subtitle: string;
  queryKey: readonly unknown[];
  queryUrl: string;
  createUrl: string;
  repoScoped: boolean;
}

export function WorkPage(props: WorkPageProps = {}): JSX.Element {
  if (props.provider && props.fullName) {
    return <RepoWorkPage {...props} />;
  }
  return (
    <WorkBoard
      title="Work"
      subtitle="Split-wide work tracker for tasks, bugs, chores, docs, and CI follow-up."
      queryKey={['work']}
      queryUrl={endpoints.work()}
      createUrl={endpoints.work()}
      repoScoped={false}
    />
  );
}

function RepoWorkPage({ provider = 'unknown', fullName = '', alias }: WorkPageProps): JSX.Element {
  const resolved = useResolveRepo(provider, fullName);

  if (resolved.isPending) {
    return (
      <div className="page" data-testid="work-page">
        <p className="page__roadmap-note">Resolving repository.</p>
      </div>
    );
  }

  if (resolved.error || !resolved.data) {
    return (
      <div className="page" data-testid="work-page">
        <header className="page__header">
          <h1 className="page__title">Work</h1>
        </header>
        <p className="page__roadmap-note">
          {resolved.error?.message ?? `No repository ${fullName}.`}
        </p>
      </div>
    );
  }

  const repoName = `${resolved.data.summary.id.owner}/${resolved.data.summary.id.name}`;
  const subtitle =
    alias === 'issues'
      ? `${repoName} issue-compatible work items.`
      : `${repoName} work items.`;

  return (
    <WorkBoard
      title="Work"
      subtitle={subtitle}
      queryKey={['repo-work', resolved.data.id]}
      queryUrl={endpoints.repoWork(resolved.data.id)}
      createUrl={endpoints.repoWork(resolved.data.id)}
      repoScoped
    />
  );
}

function WorkBoard({
  title,
  subtitle,
  queryKey,
  queryUrl,
  createUrl,
  repoScoped,
}: WorkBoardProps): JSX.Element {
  const [filters, setFilters] = useState<WorkFilters>(DEFAULT_WORK_FILTERS);

  const work = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      apiGet<WorkItemListResponse>(queryUrl, { signal }),
    staleTime: 10_000,
  });

  const items = work.data?.items ?? [];
  const filtered = useMemo(
    () => filterWorkItems(items, filters),
    [filters, items]
  );
  const lanes = useMemo(() => groupWorkItems(filtered), [filtered]);
  const repos = useMemo(() => repoOptions(items), [items]);
  const labels = useMemo(() => labelOptions(items), [items]);
  const assignees = useMemo(() => assigneeOptions(items), [items]);
  const blocked = filtered.filter((item) => item.status === 'blocked').length;
  const inReview = filtered.filter((item) => item.status === 'in_review').length;

  return (
    <div className="page page--full work-page" data-testid="work-page">
      <header className="page__header work-page__header">
        <div>
          <h1 className="page__title">{title}</h1>
          <p className="page__subtitle">{subtitle}</p>
        </div>
        <WorkSummary
          shown={filtered.length}
          total={work.data?.total ?? items.length}
          blocked={blocked}
          inReview={inReview}
        />
      </header>

      <WorkFilterBar
        filters={filters}
        setFilters={setFilters}
        repoScoped={repoScoped}
        repos={repos}
        labels={labels}
        assignees={assignees}
      />

      <WorkCreateForm
        createUrl={createUrl}
        queryKey={queryKey}
        repoScoped={repoScoped}
      />

      {work.isPending ? (
        <p className="page__roadmap-note">Loading work.</p>
      ) : work.isError ? (
        <p className="page__roadmap-note">{work.error.message}</p>
      ) : filtered.length === 0 ? (
        <p className="page__roadmap-note">No work items match the current filters.</p>
      ) : (
        <WorkLanes lanes={lanes} />
      )}
    </div>
  );
}
