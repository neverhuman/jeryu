// useRefs.ts — React Query hook for `GET /api/v1/repos/{id}/refs` (W-FE-09).
//
// Returns the branches + tags for the branch selector. The backend ships a
// flat `Array<RefSelectorItem>` whose `kind` field marks branches vs tags.
// We sort defensively here so the selector reads consistently regardless of
// the host ordering so the selector is stable.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { RefSelectorItem } from '../api/types';

export function refsQueryKey(repoId: string | null): readonly unknown[] {
  return ['repo', repoId, 'refs'] as const;
}

export function useRefs(
  repoId: string | null
): UseQueryResult<RefSelectorItem[], Error> {
  return useQuery({
    queryKey: refsQueryKey(repoId),
    queryFn: async ({ signal }) => {
      const items = await apiGet<RefSelectorItem[]>(
        endpoints.refs(repoId as string),
        { signal }
      );
      return [...items].sort((a, b) => {
        if (a.kind !== b.kind) {
          // branches before tags before commits.
          const order = { branch: 0, tag: 1, commit: 2 } as const;
          return order[a.kind] - order[b.kind];
        }
        return a.name.localeCompare(b.name);
      });
    },
    enabled: typeof repoId === 'string' && repoId.length > 0,
    staleTime: 30_000,
  });
}
