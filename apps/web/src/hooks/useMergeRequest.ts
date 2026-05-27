// useMergeRequest.ts — React Query hook for `GET /api/v1/repos/{id}/merge-requests/{iid}`
// (W-FE-11).
//
// Returns the `MergeRequestDetail` envelope used by the merge cockpit. The
// hook is enabled only when both `repoId` and `iid` resolve; pages should
// gate on `useResolveRepo` before calling this so we don't fire a 404
// against the BFF for a URL the user is still typing.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { MergeRequestDetail } from '../api/types';

export function mergeRequestQueryKey(
  repoId: string | null,
  iid: string | null
): readonly unknown[] {
  return ['mr', repoId, iid] as const;
}

export function useMergeRequest(
  repoId: string | null,
  iid: string | null
): UseQueryResult<MergeRequestDetail, Error> {
  return useQuery({
    queryKey: mergeRequestQueryKey(repoId, iid),
    queryFn: ({ signal }) =>
      apiGet<MergeRequestDetail>(
        endpoints.mergeRequest(repoId as string, iid as string),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' &&
      repoId.length > 0 &&
      typeof iid === 'string' &&
      iid.length > 0,
    staleTime: 15_000,
  });
}
