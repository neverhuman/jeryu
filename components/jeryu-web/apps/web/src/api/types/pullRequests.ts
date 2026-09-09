import type { PullRequestSummary } from '../../../../../contracts/generated/PullRequestSummary';
import type { ReviewThread } from '../../../../../contracts/generated/ReviewThread';

// ── Phase 3 frontend-local wire types (W-FE-11). ────────────────────────
// The backend (W-B-* phase 3) emits diff/checks/threads payloads that are
// not yet exported via ts-rs. These mirror the documented contract (see
// the web work spec §7.4 W-FE-11 / §35.2.4). When the backend lands its
// ts-rs export, these declarations move to `contracts/generated/` and the
// re-export here becomes a one-liner like the others.

/** Per-file diff status emitted by `GET /pulls/{number}/diff`. */
export type PullRequestFileStatus =
  | 'added'
  | 'modified'
  | 'removed'
  | 'renamed';

/** Risk tier the backend tags onto each changed file. */
export type PullRequestFileRisk = 'low' | 'medium' | 'high' | 'critical';

/** Single hunk in a unified diff. */
export interface PullRequestDiffHunk {
  header: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  /** Raw unified-diff body lines (prefixed with `+` / `-` / ` `). */
  lines: string[];
}

/** Per-file diff entry. */
export interface PullRequestDiffFile {
  path: string;
  /** When `status === 'renamed'`, the previous path. */
  old_path: string | null;
  status: PullRequestFileStatus;
  additions: number;
  deletions: number;
  risk: PullRequestFileRisk | null;
  /** Binary diffs carry no hunks; viewer renders a notice. */
  is_binary: boolean;
  hunks: PullRequestDiffHunk[];
}

/** Wire shape of `GET /pulls/{number}/diff`. */
export interface PullRequestDiff {
  head_sha: string;
  base_sha: string;
  files: PullRequestDiffFile[];
  /** True when the server truncated due to size; UI renders a warning. */
  truncated: boolean;
}

/** One CI check run on a PR. */
export interface PullRequestCheck {
  id: string;
  name: string;
  /** `success`, `failure`, `pending`, `skipped`, `cancelled`, `neutral`. */
  status: string;
  conclusion: string | null;
  details_url: string | null;
  description: string | null;
  /** RFC3339 timestamps. */
  started_at: string | null;
  completed_at: string | null;
}

/** Wire shape of `GET /pulls/{number}/checks`. */
export interface PullRequestChecks {
  total: number;
  passing: number;
  failing: number;
  pending: number;
  skipped: number;
  checks: PullRequestCheck[];
}

/** Wire shape of `GET /pulls/{number}/threads`. */
export interface PullRequestThreadList {
  /** Re-uses the canonical `ReviewThread` type. */
  threads: ReviewThread[];
}

/** Wire shape of `GET /api/v1/repos/{id}/pulls`. */
export interface PullRequestListResponse {
  items: PullRequestSummary[];
  total: number;
}

/** Body for `POST /pulls/{number}/approve`. */
export interface PullApproveRequest {
  expected_head_sha: string;
  body_markdown?: string | null;
}

/** Body for `POST /pulls/{number}/merge`. */
export interface MergePullRequest {
  expected_head_sha: string;
  expected_passport_hash: string | null;
  merge_method: 'merge' | 'squash' | 'rebase';
  commit_title?: string | null;
  commit_message?: string | null;
}
