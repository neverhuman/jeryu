// usePullRequest.ts — React Query hook for `GET /api/v1/repos/{id}/pulls/{number}`
// (W-FE-11).
//
// Returns the `PullRequestDetail` envelope used by the merge cockpit. The
// hook is enabled only when both `repoId` and `number` resolve; pages should
// gate on `useResolveRepo` before calling this so we don't fire a 404
// against the API for a URL the user is still typing.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { PullRequestDetail } from '../api/types';

export function pullRequestQueryKey(
  repoId: string | null,
  prNumber: string | null
): readonly unknown[] {
  return ['pull', repoId, prNumber] as const;
}

export function usePullRequest(
  repoId: string | null,
  prNumber: string | null
): UseQueryResult<PullRequestDetail, Error> {
  return useQuery({
    queryKey: pullRequestQueryKey(repoId, prNumber),
    queryFn: ({ signal }) =>
      apiGet<PullRequestDetail>(
        endpoints.pull(repoId as string, prNumber as string),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' &&
      repoId.length > 0 &&
      typeof prNumber === 'string' &&
      prNumber.length > 0,
    staleTime: 15_000,
  });
}
