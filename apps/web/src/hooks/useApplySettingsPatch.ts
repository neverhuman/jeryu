// useApplySettingsPatch.ts — `PATCH /settings` mutation (W-FE-12).
//
// Sends:
//   * `Idempotency-Key`: per-attempt UUID (§35.1.3).
//   * `If-Match: "<base_settings_hash>"`: optimistic concurrency token
//     from the preview response (§35.1.14). The backend rejects with
//     `settings_hash_stale` if a concurrent write changed the snapshot.
//
// On success the cached `RepositorySettings` is replaced with the server's
// reply so the studio stays in sync without an extra round trip.

import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from '@tanstack/react-query';

import { apiPatch, ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { RepositorySettings, SettingsPatch } from '../api/types';

import { repoSettingsQueryKey } from './useRepoSettings';

export interface ApplySettingsInput {
  patch: SettingsPatch;
  /** Base hash from the most recent preview response. */
  baseSettingsHash: string;
}

function newIdempotencyKey(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `settings-apply-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function useApplySettingsPatch(
  repoId: string | null
): UseMutationResult<RepositorySettings, ApiError, ApplySettingsInput> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ patch, baseSettingsHash }: ApplySettingsInput) => {
      if (!repoId) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository not resolved yet.',
        });
      }
      return apiPatch<RepositorySettings>(
        endpoints.settings(repoId),
        patch,
        {
          idempotencyKey: newIdempotencyKey(),
          headers: {
            // RFC 9110 — If-Match takes a quoted strong validator.
            'If-Match': `"${baseSettingsHash}"`,
          },
        }
      );
    },
    onSuccess: (data) => {
      queryClient.setQueryData(
        repoSettingsQueryKey(repoId),
        data
      );
    },
  });
}
