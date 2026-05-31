// pullRequestDrift.test.ts — drift-signal extraction for the PR cockpit
// (Phase G).
//
// `extractDrift` translates the 409 error envelope a mutating approve/merge
// returns into the `HeadDriftInfo` the recovery banner renders. It must:
//   1. Recognise the three known drift codes (`merge_sha_stale`,
//      `merge_passport_stale`, `concurrency_conflict`) and nothing else.
//   2. Read BOTH the long-form (`expected_head_sha` / `current_head_sha`) and
//      the short-form (`expected_sha` / `current_sha` / `head_sha`) detail
//      keys the backend may send.
//   3. Degrade gracefully (empty strings) when details are absent.

import { describe, expect, it } from 'vitest';

import { ApiError } from '../../api/client';
import { extractDrift } from '../pullRequestDrift';

function err(code: string, details?: Record<string, unknown>): ApiError {
  return new ApiError(409, { code, message: code, details });
}

describe('extractDrift', () => {
  it('returns null for codes that are not drift errors', () => {
    expect(extractDrift(err('permission_denied'))).toBeNull();
    expect(extractDrift(err('not_found'))).toBeNull();
    expect(extractDrift(err('internal'))).toBeNull();
  });

  it('extracts the long-form expected/current head SHA keys', () => {
    const info = extractDrift(
      err('merge_sha_stale', {
        expected_head_sha: 'aaa',
        current_head_sha: 'bbb',
      })
    );
    expect(info).toEqual({ expected: 'aaa', current: 'bbb', code: 'merge_sha_stale' });
  });

  it('falls back to the short-form sha keys', () => {
    const info = extractDrift(
      err('concurrency_conflict', { expected_sha: 'old', current_sha: 'new' })
    );
    expect(info).toEqual({
      expected: 'old',
      current: 'new',
      code: 'concurrency_conflict',
    });
  });

  it('reads current from `head_sha` when no current_* key is present', () => {
    const info = extractDrift(
      err('merge_passport_stale', { expected_sha: 'x', head_sha: 'y' })
    );
    expect(info?.current).toBe('y');
    expect(info?.code).toBe('merge_passport_stale');
  });

  it('returns empty SHA strings when details are missing', () => {
    const info = extractDrift(err('merge_sha_stale'));
    expect(info).toEqual({ expected: '', current: '', code: 'merge_sha_stale' });
  });
});
