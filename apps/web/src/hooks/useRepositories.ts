// useRepositories.ts — React Query hook for `GET /api/v1/repos` (W-FE-08).
//
// Accepts a `RepositoriesQuery` describing the active filters / sort / search
// state and renders a stable React Query key from it so two callers with the
// same filter set share the cache slot. The query string is built here so
// pages do not assemble URL fragments by hand.
//
// The backend wire shape is `RepositoryListResponse` (`generated_at`,
// `total`, `repositories`, `facets`); we re-export it so consumers can type
// downstream selectors.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { RepositoryListResponse } from '../api/types';

export type RepoSort =
  | 'recent_activity'
  | 'name'
  | 'open_prs'
  | 'failing_checks';

export interface RepositoriesQuery {
  /** Free-text search; debounced upstream. */
  search?: string;
  /** Wire-format host kind (`jeryu` / `local`). */
  host?: string;
  /** Visibility filter. */
  visibility?: 'public' | 'internal' | 'private';
  /** Repository family (e.g. `veox-*`). */
  family?: string;
  /** When true, only archived repositories are returned. */
  archived?: boolean;
  /** Sort key. Defaults to `recent_activity` on the backend. */
  sort?: RepoSort;
}

function buildUrl(query: RepositoriesQuery): string {
  const params = new URLSearchParams();
  if (query.search) params.set('q', query.search);
  if (query.host) params.set('host', query.host);
  if (query.visibility) params.set('visibility', query.visibility);
  if (query.family) params.set('family', query.family);
  if (query.archived) params.set('archived', '1');
  if (query.sort) params.set('sort', query.sort);
  const qs = params.toString();
  return qs ? `${endpoints.repos()}?${qs}` : endpoints.repos();
}

/**
 * Stable React Query key. Empty values are dropped so two callers with the
 * same logical filter (one omitting an empty string, one passing `undefined`)
 * share a cache slot.
 */
export function repositoriesQueryKey(
  query: RepositoriesQuery
): readonly unknown[] {
  const compact: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(query)) {
    if (v === undefined || v === '' || v === false) continue;
    compact[k] = v;
  }
  return ['repos', compact] as const;
}

export function useRepositories(
  query: RepositoriesQuery
): UseQueryResult<RepositoryListResponse, Error> {
  return useQuery({
    queryKey: repositoriesQueryKey(query),
    queryFn: ({ signal }) =>
      apiGet<RepositoryListResponse>(buildUrl(query), { signal }),
    staleTime: 30_000,
  });
}
