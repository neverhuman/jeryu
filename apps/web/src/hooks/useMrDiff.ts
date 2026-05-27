// useMrDiff.ts — React Query hook for `GET /api/v1/repos/{id}/merge-requests/{iid}/diff`
// (W-FE-11).
//
// The diff payload can be heavy; we cache it for the duration the page is
// mounted and rely on WS events (`mr.diff_recomputed`) to invalidate.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { MergeRequestDiff } from '../api/types';

export function mrDiffQueryKey(
  repoId: string | null,
  iid: string | null
): readonly unknown[] {
  return ['mr', repoId, iid, 'diff'] as const;
}

export function useMrDiff(
  repoId: string | null,
  iid: string | null
): UseQueryResult<MergeRequestDiff, Error> {
  return useQuery({
    queryKey: mrDiffQueryKey(repoId, iid),
    queryFn: ({ signal }) =>
      apiGet<MergeRequestDiff>(
        endpoints.mergeRequestDiff(repoId as string, iid as string),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' &&
      repoId.length > 0 &&
      typeof iid === 'string' &&
      iid.length > 0,
    staleTime: 30_000,
  });
}
