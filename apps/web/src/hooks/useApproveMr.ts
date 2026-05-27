// useApproveMr.ts — `POST /merge-requests/{iid}/approve` mutation (W-FE-11).
//
// Generates a per-attempt `Idempotency-Key` (§35.1.3) so retries collapse
// server-side. On success we invalidate the MR detail + threads so the
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
  MergeApproveRequest,
  MergeRequestDetail,
} from '../api/types';

import { mergeRequestQueryKey } from './useMergeRequest';
import { mrThreadsQueryKey } from './useMrThreads';

function newIdempotencyKey(): string {
  // crypto.randomUUID() is available in evergreen browsers + jsdom 22+.
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `mr-approve-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function useApproveMr(
  repoId: string | null,
  iid: string | null
): UseMutationResult<MergeRequestDetail, ApiError, MergeApproveRequest> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: MergeApproveRequest) => {
      if (!repoId || !iid) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository or merge request not resolved yet.',
        });
      }
      return apiSend<MergeRequestDetail>(
        endpoints.mergeRequestApprove(repoId, iid),
        body,
        { idempotencyKey: newIdempotencyKey() }
      );
    },
    onSuccess: (data) => {
      // Replace the cached detail with the server's new copy so the panel
      // reflects the updated review posture without an extra round-trip.
      queryClient.setQueryData(
        mergeRequestQueryKey(repoId, iid),
        data
      );
      queryClient.invalidateQueries({
        queryKey: mrThreadsQueryKey(repoId, iid),
      });
    },
  });
}
