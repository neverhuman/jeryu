// repositoriesPageParts.tsx — presentational pieces for `RepositoriesPage`.
//
// Holds the filter-chip control, the body renderer that wires the five
// UX-QA states (loading / empty / error / permission / success), and the
// pure helpers (`FilterState`, `DEFAULT_FILTER`, `groupByFamily`) the page
// uses to derive its query and group the result set. Splitting these out
// keeps the page module focused on state + routing orchestration.

import { Search } from 'lucide-react';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import { RepoCard, RepoFamilyGroup, RepoTable } from '../components/repo';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import type { RepoSort } from '../hooks/useRepositories';
import type { RepositorySummary } from '../api/types';

export interface FilterState {
  search: string;
  host?: string;
  visibility?: 'public' | 'internal' | 'private';
  family?: string;
  archived: boolean;
  sort: RepoSort;
}

export const DEFAULT_FILTER: FilterState = {
  search: '',
  archived: false,
  sort: 'recent_activity',
};

export function groupByFamily(
  repos: RepositorySummary[]
): Array<{ title: string; repos: RepositorySummary[] }> {
  // Map preserves insertion order; "ungrouped" is collected last so the
  // grouped families surface first.
  const buckets = new Map<string, RepositorySummary[]>();
  const ungrouped: RepositorySummary[] = [];
  for (const repo of repos) {
    if (repo.family) {
      const arr = buckets.get(repo.family) ?? [];
      arr.push(repo);
      buckets.set(repo.family, arr);
    } else {
      ungrouped.push(repo);
    }
  }
  const out: Array<{ title: string; repos: RepositorySummary[] }> = [];
  for (const [family, list] of buckets) {
    out.push({ title: family, repos: list });
  }
  if (ungrouped.length > 0) {
    out.push({ title: 'Other', repos: ungrouped });
  }
  return out;
}

interface FilterChipsProps<T extends string> {
  label: string;
  value: T | undefined;
  options: readonly T[];
  onChange: (next: T | undefined) => void;
  ariaLabel: string;
}

export function FilterChips<T extends string>({
  label,
  value,
  options,
  onChange,
  ariaLabel,
}: FilterChipsProps<T>): JSX.Element {
  return (
    <div
      className="page__inline-actions"
      role="group"
      aria-label={ariaLabel}
    >
      <span className="text-muted" aria-hidden="true">
        {label}:
      </span>
      {options.map((opt) => {
        const pressed = value === opt;
        return (
          <button
            key={opt}
            type="button"
            className="repo-toolbar__chip"
            aria-pressed={pressed}
            aria-label={`${label}: ${opt}`}
            onClick={() => onChange(pressed ? undefined : opt)}
          >
            {opt}
          </button>
        );
      })}
    </div>
  );
}

interface RepositoriesBodyProps {
  loading: boolean;
  error: Error | null;
  repos: RepositorySummary[];
  view: 'card' | 'table';
  onClearFilters: () => void;
}

export function RepositoriesBody({
  loading,
  error,
  repos,
  view,
  onClearFilters,
}: RepositoriesBodyProps): JSX.Element {
  if (loading) {
    return <LoadingState title="Loading repositories…" rows={6} />;
  }
  if (error) {
    if (
      error instanceof ApiError &&
      (error.status === 403 || error.code === 'permission_denied')
    ) {
      return (
        <PermissionDeniedState
          description="You do not have permission to view repositories."
          missingPermission="repo.read"
        />
      );
    }
    return (
      <ErrorState
        title="Could not load repositories"
        error={error}
        action={
          <ActionButton variant="primary" onClick={onClearFilters}>
            Reset filters
          </ActionButton>
        }
      />
    );
  }
  if (repos.length === 0) {
    return (
      <EmptyState
        title="No repositories match"
        description="Try adjusting your filters or search."
        icon={Search}
        action={
          <ActionButton variant="ghost" onClick={onClearFilters}>
            Clear filters
          </ActionButton>
        }
      />
    );
  }

  if (view === 'table') {
    return <RepoTable repos={repos} />;
  }

  const groups = groupByFamily(repos);
  return (
    <div className="page__section">
      {groups.map((group) => (
        <RepoFamilyGroup
          key={group.title}
          title={group.title}
          count={group.repos.length}
        >
          <div className="page__cards">
            {group.repos.map((repo) => (
              <RepoCard key={repo.id.id} repo={repo} />
            ))}
          </div>
        </RepoFamilyGroup>
      ))}
    </div>
  );
}
