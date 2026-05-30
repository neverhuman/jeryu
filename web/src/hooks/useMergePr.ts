// useMergePr.ts — `POST /pulls/{number}/merge` mutation (W-FE-11).
//
// Symmetric to `useApprovePr`. The merge body must carry the Passport hash
// the UI saw when the button was rendered so the backend can reject if the
// gate snapshot drifted (§35.2.4).

import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from '@tanstack/react-query';

import { apiSend, ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import type {
  MergePullRequest,
  PullRequestDetail,
} from '../api/types';

import { pullRequestQueryKey } from './usePullRequest';

function newIdempotencyKey(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `pull-merge-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function useMergePr(
  repoId: string | null,
  prNumber: string | null
): UseMutationResult<PullRequestDetail, ApiError, MergePullRequest> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: MergePullRequest) => {
      if (!repoId || !prNumber) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository or pull request not resolved yet.',
        });
      }
      return apiSend<PullRequestDetail>(
        endpoints.pullMerge(repoId, prNumber),
        body,
        { idempotencyKey: newIdempotencyKey() }
      );
    },
    onSuccess: (data) => {
      queryClient.setQueryData(
        pullRequestQueryKey(repoId, prNumber),
        data
      );
    },
  });
}
