// useApprovePr.ts — `POST /pulls/{number}/approve` mutation (W-FE-11).
//
// Core binds retries to the same challenge, credential and exact request bytes.
// Keep that challenge on an uncertain response; never mint a new review on retry.

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

export function useApprovePr(
  repoId: string | null,
  prNumber: string | null
): UseMutationResult<PullRequestDetail, ApiError, PullApproveRequest> {
  const queryClient = useQueryClient();
  return useMutation({
    gcTime: 0,
    retry: false,
    mutationFn: async (body: PullApproveRequest) => {
      if (!repoId || !prNumber) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository or pull request not resolved yet.',
        });
      }
      return apiSend<PullRequestDetail>(
        endpoints.pullApprove(repoId, prNumber),
        body
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
