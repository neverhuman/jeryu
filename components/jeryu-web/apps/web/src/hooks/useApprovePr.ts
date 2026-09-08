// useApprovePr.ts — `POST /pulls/{number}/approve` mutation (W-FE-11).
//
// Generates a per-attempt `Idempotency-Key` (§35.1.3) so retries collapse
// server-side. On success we invalidate the PR detail + threads so the
// approval count and timeline refresh. On 409 with `merge_sha_stale` the
// component shows a recovery banner using the error envelope's `details`
// (head_sha snapshot) per §35.1.11.

import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from '@tanstack/react-query';

import { apiSend, ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import type {
  PullApproveRequest,
  PullRequestDetail,
} from '../api/types';

import { pullRequestQueryKey } from './usePullRequest';
import { prThreadsQueryKey } from './usePrThreads';

function newIdempotencyKey(): string {
  // crypto.randomUUID() is available in evergreen browsers + jsdom 22+.
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `pull-approve-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function useApprovePr(
  repoId: string | null,
  prNumber: string | null
): UseMutationResult<PullRequestDetail, ApiError, PullApproveRequest> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: PullApproveRequest) => {
      if (!repoId || !prNumber) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository or pull request not resolved yet.',
        });
      }
      return apiSend<PullRequestDetail>(
        endpoints.pullApprove(repoId, prNumber),
        body,
        { idempotencyKey: newIdempotencyKey() }
      );
    },
    onSuccess: (data) => {
      // Replace the cached detail with the server's new copy so the panel
      // reflects the updated review posture without an extra round-trip.
      queryClient.setQueryData(
        pullRequestQueryKey(repoId, prNumber),
        data
      );
      queryClient.invalidateQueries({
        queryKey: prThreadsQueryKey(repoId, prNumber),
      });
    },
  });
}
