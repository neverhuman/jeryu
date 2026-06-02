// usePrDiff.ts — React Query hook for `GET /api/v1/repos/{id}/pulls/{number}/diff`
// (W-FE-11).
//
// The diff payload can be heavy; we cache it for the duration the page is
// mounted and rely on WS events (`pull.diff_recomputed`) to invalidate.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { PullRequestDiff } from '../api/types';

export function prDiffQueryKey(
  repoId: string | null,
  prNumber: string | null
): readonly unknown[] {
  return ['pull', repoId, prNumber, 'diff'] as const;
}

export function usePrDiff(
  repoId: string | null,
  prNumber: string | null
): UseQueryResult<PullRequestDiff, Error> {
  return useQuery({
    queryKey: prDiffQueryKey(repoId, prNumber),
    queryFn: ({ signal }) =>
      apiGet<PullRequestDiff>(
        endpoints.pullDiff(repoId as string, prNumber as string),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' &&
      repoId.length > 0 &&
      typeof prNumber === 'string' &&
      prNumber.length > 0,
    staleTime: 30_000,
  });
}
