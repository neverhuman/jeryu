// useMrChecks.ts — React Query hook for `GET /merge-requests/{iid}/checks`
// (W-FE-11).

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { MergeRequestChecks } from '../api/types';

export function mrChecksQueryKey(
  repoId: string | null,
  iid: string | null
): readonly unknown[] {
  return ['mr', repoId, iid, 'checks'] as const;
}

export function useMrChecks(
  repoId: string | null,
  iid: string | null
): UseQueryResult<MergeRequestChecks, Error> {
  return useQuery({
    queryKey: mrChecksQueryKey(repoId, iid),
    queryFn: ({ signal }) =>
      apiGet<MergeRequestChecks>(
        endpoints.mergeRequestChecks(repoId as string, iid as string),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' &&
      repoId.length > 0 &&
      typeof iid === 'string' &&
      iid.length > 0,
    staleTime: 10_000,
  });
}
