// usePrChecks.ts — React Query hook for `GET /pulls/{number}/checks`
// (W-FE-11).

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { PullRequestChecks } from '../api/types';

export function prChecksQueryKey(
  repoId: string | null,
  prNumber: string | null
): readonly unknown[] {
  return ['pull', repoId, prNumber, 'checks'] as const;
}

export function usePrChecks(
  repoId: string | null,
  prNumber: string | null
): UseQueryResult<PullRequestChecks, Error> {
  return useQuery({
    queryKey: prChecksQueryKey(repoId, prNumber),
    queryFn: ({ signal }) =>
      apiGet<PullRequestChecks>(
        endpoints.pullChecks(repoId as string, prNumber as string),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' &&
      repoId.length > 0 &&
      typeof prNumber === 'string' &&
      prNumber.length > 0,
    staleTime: 10_000,
  });
}
