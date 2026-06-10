// useDeleteRepository.ts — repository-removal mutation (HTTP DELETE, `/api/v1/repos/{id}`).
//
// The endpoint is two-tier: `delete_storage: false` removes the registry
// entry only (bare storage on disk is kept), `true` also removes the managed
// bare directory. Both tiers require:
//   * `confirm_full_name` byte-matching the repository's `owner/name`
//     (422 `confirm_mismatch` otherwise), and
//   * an `Idempotency-Key` header so retries are safe.
//
// On success the repos list cache is invalidated (every filtered slot hangs
// off the `['repos', ...]` prefix) and the SPA navigates back to `/repos` —
// every repo-scoped route under the deleted id is now a 404.

import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';

import { apiDeleteWithBody, ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import type {
  DeleteRepositoryReceipt,
  DeleteRepositoryRequest,
} from '../api/types';

import { newIdempotencyKey } from './useApplySettingsPatch';

export interface DeleteRepositoryInput {
  /** Must byte-match the repository's `owner/name`. */
  confirmFullName: string;
  /** Second tier: also remove the managed bare directory on disk. */
  deleteStorage: boolean;
}

export function useDeleteRepository(
  repoId: string | null
): UseMutationResult<DeleteRepositoryReceipt, ApiError, DeleteRepositoryInput> {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return useMutation({
    mutationFn: async ({
      confirmFullName,
      deleteStorage,
    }: DeleteRepositoryInput) => {
      if (!repoId) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository not resolved yet.',
        });
      }
      const body: DeleteRepositoryRequest = {
        confirm_full_name: confirmFullName,
        delete_storage: deleteStorage,
      };
      return apiDeleteWithBody<DeleteRepositoryReceipt>(
        endpoints.repo(repoId),
        body,
        { idempotencyKey: newIdempotencyKey() }
      );
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['repos'] });
      navigate('/repos');
    },
  });
}
