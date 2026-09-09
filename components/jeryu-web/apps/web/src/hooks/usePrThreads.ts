// usePrThreads.ts — React Query hook for `GET /pulls/{number}/threads`
// (W-FE-11).
//
// Invalidated on `pull.thread.*` WS events for this PR so a comment posted
// from another tab/session appears live. The invalidator is installed on
// mount and torn down on unmount via the realtime store's `addInvalidator`
// hook.

import { useQuery, useQueryClient, type UseQueryResult } from '@tanstack/react-query';
import { useEffect } from 'react';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import { useRealtimeStore } from '../stores/realtimeStore';
import type { PullRequestThreadList } from '../api/types';

export function prThreadsQueryKey(
  repoId: string | null,
  prNumber: string | null
): readonly unknown[] {
  return ['pull', repoId, prNumber, 'threads'] as const;
}

export function usePrThreads(
  repoId: string | null,
  prNumber: string | null
): UseQueryResult<PullRequestThreadList, Error> {
  const queryClient = useQueryClient();
  const addInvalidator = useRealtimeStore((s) => s.addInvalidator);

  useEffect(() => {
    if (!repoId || !prNumber) return () => {};
    return addInvalidator((event) => {
      // Match `pull.thread.*` events scoped to this PR.
      if (!event.kind.startsWith('pull.thread.')) return;
      // `WebEvent.entity` is a free-form id; the backend formats PR-scoped
      // events as `entity: "pull:{repo_id}:{number}"` per §35.1.15. We accept
      // either that or `scope: pull.{number}` to stay forgiving.
      const matchesEntity =
        event.entity?.includes(`:${repoId}:${prNumber}`) ?? false;
      const matchesScope = event.scope === `pull.${prNumber}`;
      if (matchesEntity || matchesScope) {
        queryClient.invalidateQueries({
          queryKey: prThreadsQueryKey(repoId, prNumber),
        });
      }
    });
  }, [repoId, prNumber, addInvalidator, queryClient]);

  return useQuery({
    queryKey: prThreadsQueryKey(repoId, prNumber),
    queryFn: ({ signal }) =>
      apiGet<PullRequestThreadList>(
        endpoints.pullThreads(repoId as string, prNumber as string),
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
