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
import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import type { RepositorySummary } from '../../api/types';

import { RepoHealthPill } from './RepoHealthPill';
import { repoHref } from './RepoCard';

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
        accessorFn: (row) => `${row.id.owner}/${row.id.name}`,
        cell: ({ row }) => {
          const repo = row.original;
          return (
            <span>
              <span className="text-muted">{repo.id.owner}/</span>
              <strong>{repo.id.name}</strong>
            </span>
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
        id: 'open_prs',
        header: 'Open PRs',
        accessorFn: (row) => row.open_pull_requests,
      },
      {
        id: 'failing_checks',
        header: 'Failing',
        accessorFn: (row) => row.failing_checks,
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
              aria-label={`Open ${repo.id.owner}/${repo.id.name}`}
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
