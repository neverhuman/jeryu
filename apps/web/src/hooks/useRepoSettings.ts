// useRepoSettings.ts — React Query hook for `GET /api/v1/repos/{id}/settings`
// (W-FE-12).
//
// Fetches the canonical `RepositorySettings` envelope used by the settings
// studio. The current-hash field (used for optimistic concurrency on PATCH)
// lives on the preview response per §35.1.14 — this hook just returns the
// snapshot, and `usePreviewSettingsPatch` carries the hash needed for the
// follow-up apply.

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { RepositorySettings } from '../api/types';

export function repoSettingsQueryKey(
  repoId: string | null
): readonly unknown[] {
  return ['repo', repoId, 'settings'] as const;
}

export function useRepoSettings(
  repoId: string | null
): UseQueryResult<RepositorySettings, Error> {
  return useQuery({
    queryKey: repoSettingsQueryKey(repoId),
    queryFn: ({ signal }) =>
      apiGet<RepositorySettings>(endpoints.settings(repoId as string), {
        signal,
      }),
    enabled: typeof repoId === 'string' && repoId.length > 0,
    staleTime: 15_000,
  });
}
