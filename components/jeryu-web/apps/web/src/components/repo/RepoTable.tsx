// RepoTable.tsx — TanStack Table view of repositories (W-FE-08).
//
// Renders one row per `RepositorySummary` with a click target that navigates
// to the overview page. The header carries `aria-sort` so screen readers can
// announce the current sort direction; the sort itself runs against the
// table's data so it stays in sync with column clicks.

import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
  type ColumnDef,
  type SortingState,
} from '@tanstack/react-table';
import { Play } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';

import type { RepositorySummary } from '../../api/types';

import { JankuraiScoreBadge } from './JankuraiScoreBadge';
import { MirrorStatusBadge } from './MirrorStatusBadge';
import { RepoHealthPill } from './RepoHealthPill';
import { RepoRoleBadge } from './RepoRoleBadge';
import { repoHref } from './RepoCard';
import { familyHref } from './RepoFamilyCard';
import { formatFamilyName } from './familyRollup';

import './repo.css';

export interface RepoTableProps {
  repos: RepositorySummary[];
}

export function RepoTable({ repos }: RepoTableProps): JSX.Element {
  const navigate = useNavigate();
  const [sorting, setSorting] = useState<SortingState>([
    { id: 'name', desc: false },
  ]);

  const columns = useMemo<ColumnDef<RepositorySummary>[]>(
    () => [
      {
        id: 'name',
        header: 'Repository',
        accessorFn: (row) => row.id.name,
        cell: ({ row }) => (
          <span className="repo-table__repo-cell">
            <strong>{row.original.id.name}</strong>
            <RepoRoleBadge role={row.original.repo_role} />
          </span>
        ),
      },
      {
        id: 'family',
        header: 'Family',
        accessorFn: (row) => row.family ?? '',
        cell: ({ row }) => {
          const family = row.original.family;
          if (!family) return null;
          return (
            <Link
              to={familyHref(family)}
              className="repo-table__family-link"
              onClick={(e) => e.stopPropagation()}
              aria-label={`Open family ${formatFamilyName(family)}`}
            >
              {formatFamilyName(family)}
            </Link>
          );
        },
      },
      {
        id: 'description',
        header: 'Description',
        accessorFn: (row) => row.description ?? '',
        cell: ({ row }) =>
          row.original.description ?? (
            <span className="text-muted">No description</span>
          ),
      },
      {
        id: 'posture',
        header: 'Posture',
        accessorFn: (row) => row.health,
        cell: ({ row }) => <RepoHealthPill health={row.original.health} />,
      },
      {
        id: 'score',
        header: 'Score',
        // Unscored repos sort below every real score (worst-first when
        // ascending) instead of throwing the comparator off with nulls.
        accessorFn: (row) => row.jankurai_score ?? -1,
        cell: ({ row }) => (
          <JankuraiScoreBadge
            score={row.original.jankurai_score}
            decision={row.original.jankurai_decision}
            scoredAt={row.original.jankurai_scored_at}
          />
        ),
      },
      {
        id: 'mirror',
        header: 'Mirror',
        enableSorting: false,
        cell: ({ row }) => <MirrorStatusBadge mirror={row.original.mirror} />,
      },
      {
        id: 'open_prs',
        header: 'Open PRs',
        accessorFn: (row) => row.open_pull_requests,
      },
      {
        id: 'failing_checks',
        header: 'Checks',
        accessorFn: (row) => row.failing_checks,
        cell: ({ row }) => (
          <span className="repo-table__checks">
            {row.original.failing_checks}
            {row.original.running_jobs > 0 ? (
              <span
                className="repo-table__running"
                title="Running jobs"
                aria-label={`${row.original.running_jobs} running jobs`}
              >
                <Play size={12} aria-hidden="true" />
                {row.original.running_jobs}
              </span>
            ) : null}
          </span>
        ),
      },
      {
        id: 'updated_at',
        header: 'Updated',
        accessorFn: (row) => row.updated_at,
      },
    ],
    []
  );

  const table = useReactTable({
    data: repos,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  return (
    <table
      className="repo-table"
      role="grid"
      aria-label="Repositories"
    >
      <thead>
        {table.getHeaderGroups().map((headerGroup) => (
          <tr key={headerGroup.id}>
            {headerGroup.headers.map((header) => {
              const canSort = header.column.getCanSort();
              const direction = header.column.getIsSorted();
              const ariaSort: 'none' | 'ascending' | 'descending' =
                direction === 'asc'
                  ? 'ascending'
                  : direction === 'desc'
                    ? 'descending'
                    : 'none';
              return (
                <th
                  key={header.id}
                  scope="col"
                  aria-sort={ariaSort}
                  className={canSort ? 'repo-table__th--sortable' : undefined}
                  onClick={
                    canSort
                      ? header.column.getToggleSortingHandler()
                      : undefined
                  }
                >
                  {flexRender(
                    header.column.columnDef.header,
                    header.getContext()
                  )}
                </th>
              );
            })}
          </tr>
        ))}
      </thead>
      <tbody>
        {table.getRowModel().rows.map((row) => {
          const repo = row.original;
          return (
            <tr
              key={repo.id.id}
              tabIndex={0}
              role="row"
              aria-label={`Open ${repo.id.name}`}
              onClick={() => navigate(repoHref(repo))}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  navigate(repoHref(repo));
                }
              }}
            >
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id}>
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              ))}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
