// useRepoTree.ts — React Query hook for `GET /api/v1/repos/{id}/tree`
// (W-FE-10).
//
// One query per `(repoId, ref, path)` triple so the virtualized file tree can
// lazy-load directory expansions without rebuilding the whole tree. The cache
// key intentionally includes the empty-path case so the root listing is
// shared with downstream expanders.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { TreeEntry } from '../api/types';

export function repoTreeQueryKey(
  repoId: string | null,
  ref: string,
  path: string
): readonly unknown[] {
  return ['repo', repoId, 'tree', ref, path] as const;
}

export function useRepoTree(
  repoId: string | null,
  ref: string,
  path: string
): UseQueryResult<TreeEntry[], Error> {
  return useQuery({
    queryKey: repoTreeQueryKey(repoId, ref, path),
    queryFn: ({ signal }) =>
      apiGet<TreeEntry[]>(
        endpoints.tree(repoId as string, { ref, path }),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' && repoId.length > 0 && ref.length > 0,
    staleTime: 30_000,
  });
}
