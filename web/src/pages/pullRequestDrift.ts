// pullRequestDrift.ts — drift detection for the PR review cockpit (W-FE-11).
//
// When an approve/merge mutation returns 409, the page surfaces a recovery
// banner. This module owns the shape of that signal (`HeadDriftInfo`) and
// the extraction of the previous/current SHA from the known drift error codes
// (`merge_sha_stale` / `merge_passport_stale` / `concurrency_conflict`).

import { ApiError } from '../api/client';

export interface HeadDriftInfo {
  /** SHA the user saw when they pressed the action. */
  expected: string;
  /** SHA the backend reported is current. */
  current: string;
  /** Last error code so we can word the banner accurately. */
  code: 'merge_sha_stale' | 'merge_passport_stale' | 'concurrency_conflict';
}

export function extractDrift(error: ApiError): HeadDriftInfo | null {
  const code = error.code;
  if (
    code !== 'merge_sha_stale' &&
    code !== 'merge_passport_stale' &&
    code !== 'concurrency_conflict'
  ) {
    return null;
  }
  const details = error.details ?? {};
  const expected =
    (details.expected_head_sha as string | undefined) ??
    (details.expected_sha as string | undefined) ??
    '';
  const current =
    (details.current_head_sha as string | undefined) ??
    (details.current_sha as string | undefined) ??
    (details.head_sha as string | undefined) ??
    '';
  return { expected, current, code };
}
