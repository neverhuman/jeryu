import type { Dispatch, SetStateAction } from 'react';

import type { WorkPrincipal } from '../../api/types';
import {
  WORK_KINDS,
  WORK_KIND_LABELS,
  WORK_PRIORITIES,
  WORK_PRIORITY_LABELS,
  WORK_STATUSES,
  WORK_STATUS_LABELS,
  displayPrincipal,
  type WorkFilters,
} from '../workModel';

export interface WorkFilterBarProps {
  filters: WorkFilters;
  setFilters: Dispatch<SetStateAction<WorkFilters>>;
  repoScoped: boolean;
  repos: string[];
  labels: string[];
  assignees: WorkPrincipal[];
}

export function WorkFilterBar({
  filters,
  setFilters,
  repoScoped,
  repos,
  labels,
  assignees,
}: WorkFilterBarProps): JSX.Element {
  return (
    <section className="work-page__filters" aria-label="Work filters">
      {!repoScoped ? (
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
      ) : null}
      <label>
        Status
        <select
          value={filters.status}
          onChange={(event) =>
            setFilters((current) => ({ ...current, status: event.target.value }))
          }
        >
          <option value="all">All statuses</option>
          {WORK_STATUSES.map((status) => (
            <option key={status} value={status}>
              {WORK_STATUS_LABELS[status]}
            </option>
          ))}
        </select>
      </label>
      <label>
        Kind
        <select
          value={filters.kind}
          onChange={(event) =>
            setFilters((current) => ({ ...current, kind: event.target.value }))
          }
        >
          <option value="all">All kinds</option>
          {WORK_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {WORK_KIND_LABELS[kind]}
            </option>
          ))}
        </select>
      </label>
      <label>
        Priority
        <select
          value={filters.priority}
          onChange={(event) =>
            setFilters((current) => ({ ...current, priority: event.target.value }))
          }
        >
          <option value="all">All priorities</option>
          {WORK_PRIORITIES.map((priority) => (
            <option key={priority} value={priority}>
              {WORK_PRIORITY_LABELS[priority]}
            </option>
          ))}
        </select>
      </label>
      <label>
        Assignee
        <select
          value={filters.assignee}
          onChange={(event) =>
            setFilters((current) => ({ ...current, assignee: event.target.value }))
          }
        >
          <option value="all">All assignees</option>
          {assignees.map((assignee) => (
            <option key={assignee.id} value={assignee.id}>
              {displayPrincipal(assignee)}
            </option>
          ))}
        </select>
      </label>
      <label>
        Label
        <select
          value={filters.label}
          onChange={(event) =>
            setFilters((current) => ({ ...current, label: event.target.value }))
          }
        >
          <option value="all">All labels</option>
          {labels.map((label) => (
            <option key={label} value={label}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <label className="work-page__search">
        Search
        <input
          type="search"
          value={filters.search}
          onChange={(event) =>
            setFilters((current) => ({ ...current, search: event.target.value }))
          }
          aria-label="Search work"
        />
      </label>
    </section>
  );
}
