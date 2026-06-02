// usePreviewSettingsPatch.ts — `POST /settings/preview` mutation (W-FE-12).
//
// Returns a `SettingsDiffPreview` with field diffs, side effects, warnings,
// and the `current_hash` we feed to the follow-up PATCH for OCC.

import {
  useMutation,
  type UseMutationResult,
} from '@tanstack/react-query';

import { apiSend, ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { SettingsDiffPreview, SettingsPatch } from '../api/types';

export function usePreviewSettingsPatch(
  repoId: string | null
): UseMutationResult<SettingsDiffPreview, ApiError, SettingsPatch> {
  return useMutation({
    mutationFn: async (patch: SettingsPatch) => {
      if (!repoId) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository not resolved yet.',
        });
      }
      return apiSend<SettingsDiffPreview>(
        endpoints.settingsPreview(repoId),
        patch
      );
    },
  });
}
