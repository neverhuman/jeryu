// useRepository.ts — React Query hook for `GET /api/v1/repos/{id}` (W-FE-09).
//
// The backend keys repositories by the opaque `RepositoryId.id` (§35.1.2).
// The SPA URLs use the human-readable `:provider/*fullName` triple, so pages
// resolve to an `id` via the list cache (`useRepositories`) before reaching
// here. Passing `enabled: false` until the resolution completes prevents an
// unnecessary 404 during navigation.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { RepositorySummary } from '../api/types';

export function repositoryQueryKey(repoId: string | null): readonly unknown[] {
  return ['repo', repoId] as const;
}

export function useRepository(
  repoId: string | null
): UseQueryResult<RepositorySummary, Error> {
  return useQuery({
    queryKey: repositoryQueryKey(repoId),
    queryFn: ({ signal }) =>
      apiGet<RepositorySummary>(endpoints.repo(repoId as string), { signal }),
    enabled: typeof repoId === 'string' && repoId.length > 0,
    staleTime: 30_000,
  });
}
