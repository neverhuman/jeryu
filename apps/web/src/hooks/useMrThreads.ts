// useMrThreads.ts — React Query hook for `GET /merge-requests/{iid}/threads`
// (W-FE-11).
//
// Invalidated on `mr.thread.*` WS events for this iid so a comment posted
// from another tab/session appears live. The invalidator is installed on
// mount and torn down on unmount via the realtime store's `addInvalidator`
// hook.

import { useQuery, useQueryClient, type UseQueryResult } from '@tanstack/react-query';
import { useEffect } from 'react';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import { useRealtimeStore } from '../stores/realtimeStore';
import type { MergeRequestThreadList } from '../api/types';

export function mrThreadsQueryKey(
  repoId: string | null,
  iid: string | null
): readonly unknown[] {
  return ['mr', repoId, iid, 'threads'] as const;
}

export function useMrThreads(
  repoId: string | null,
  iid: string | null
): UseQueryResult<MergeRequestThreadList, Error> {
  const queryClient = useQueryClient();
  const addInvalidator = useRealtimeStore((s) => s.addInvalidator);

  useEffect(() => {
    if (!repoId || !iid) return undefined;
    return addInvalidator((event) => {
      // Match `mr.thread.*` events scoped to this MR.
      if (!event.kind.startsWith('mr.thread.')) return;
      // `WebEvent.entity` is a free-form id; the backend formats MR-scoped
      // events as `entity: "mr:{repo_id}:{iid}"` per §35.1.15. We accept
      // either that or `scope: mr.{iid}` to stay forgiving.
      const matchesEntity = event.entity?.includes(`:${repoId}:${iid}`) ?? false;
      const matchesScope = event.scope === `mr.${iid}`;
      if (matchesEntity || matchesScope) {
        queryClient.invalidateQueries({
          queryKey: mrThreadsQueryKey(repoId, iid),
        });
      }
    });
  }, [repoId, iid, addInvalidator, queryClient]);

  return useQuery({
    queryKey: mrThreadsQueryKey(repoId, iid),
    queryFn: ({ signal }) =>
      apiGet<MergeRequestThreadList>(
        endpoints.mergeRequestThreads(repoId as string, iid as string),
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
