// types.ts — re-exports the generated DTO types (W-FE-03).
//
// All wire types live in `contracts/generated/*.ts` (produced by ts-rs from
// the Rust API surface). We re-export the subset used by the SPA so app code
// imports from `@/api/types` (logical boundary) rather than reaching into the
// generated tree directly.
//
// When a new DTO is needed, add a new export here — do not edit the generated
// files.

export type { WebBootstrap } from '../../../../contracts/generated/WebBootstrap';
export type { Viewer } from '../../../../contracts/generated/Viewer';
export type { WebFeatureFlags } from '../../../../contracts/generated/WebFeatureFlags';
export type { RepositorySummary } from '../../../../contracts/generated/RepositorySummary';
export type { RepositoryId } from '../../../../contracts/generated/RepositoryId';
export type { RepositoryVisibility } from '../../../../contracts/generated/RepositoryVisibility';
export type { RepositoryListResponse } from '../../../../contracts/generated/RepositoryListResponse';
export type { RefSelectorItem } from '../../../../contracts/generated/RefSelectorItem';
export type { RefKind } from '../../../../contracts/generated/RefKind';
export type { TreeEntry } from '../../../../contracts/generated/TreeEntry';
export type { TreeEntryKind } from '../../../../contracts/generated/TreeEntryKind';
export type { BlobResponse } from '../../../../contracts/generated/BlobResponse';
export type { BlobEncoding } from '../../../../contracts/generated/BlobEncoding';
export type { RenderedMarkdown } from '../../../../contracts/generated/RenderedMarkdown';
export type { MarkdownHeading } from '../../../../contracts/generated/MarkdownHeading';
export type { MarkdownLink } from '../../../../contracts/generated/MarkdownLink';
export type { MergeRequestSummary } from '../../../../contracts/generated/MergeRequestSummary';
export type { MergeRequestDetail } from '../../../../contracts/generated/MergeRequestDetail';
export type { MergeRequestState } from '../../../../contracts/generated/MergeRequestState';
export type { MergePassport } from '../../../../contracts/generated/MergePassport';
export type { MergePassportBlocker } from '../../../../contracts/generated/MergePassportBlocker';
export type { MergePassportStatus } from '../../../../contracts/generated/MergePassportStatus';
export type { Mergeability } from '../../../../contracts/generated/Mergeability';
export type { ReviewThread } from '../../../../contracts/generated/ReviewThread';
export type { ReviewComment } from '../../../../contracts/generated/ReviewComment';
export type { ReviewVerdict } from '../../../../contracts/generated/ReviewVerdict';
export type { ReviewPosture } from '../../../../contracts/generated/ReviewPosture';
export type { ReviewSuggestion } from '../../../../contracts/generated/ReviewSuggestion';
export type { SubmitReviewRequest } from '../../../../contracts/generated/SubmitReviewRequest';
export type { CreateReviewCommentRequest } from '../../../../contracts/generated/CreateReviewCommentRequest';
export type { CreateRepositoryRequest } from '../../../../contracts/generated/CreateRepositoryRequest';
export type { CreateRepositoryPreview } from '../../../../contracts/generated/CreateRepositoryPreview';
export type { IssueSummary } from '../../../../contracts/generated/IssueSummary';
export type { IssueState } from '../../../../contracts/generated/IssueState';
export type { AgentPosture } from '../../../../contracts/generated/AgentPosture';
export type { AgentSettings } from '../../../../contracts/generated/AgentSettings';
export type { AccessSettings } from '../../../../contracts/generated/AccessSettings';
export type { CheckPosture } from '../../../../contracts/generated/CheckPosture';
export type { CiSettings } from '../../../../contracts/generated/CiSettings';
export type { GeneralSettings } from '../../../../contracts/generated/GeneralSettings';
export type { FeatureSettings } from '../../../../contracts/generated/FeatureSettings';
export type { MergeSettings } from '../../../../contracts/generated/MergeSettings';
export type { NotificationSettings } from '../../../../contracts/generated/NotificationSettings';
export type { RepositorySettings } from '../../../../contracts/generated/RepositorySettings';
export type { RepositoryHostKind } from '../../../../contracts/generated/RepositoryHostKind';
export type { RepositoryFacets } from '../../../../contracts/generated/RepositoryFacets';
export type { RetentionSettings } from '../../../../contracts/generated/RetentionSettings';
export type { SecuritySettings } from '../../../../contracts/generated/SecuritySettings';
export type { BranchProtectionRule } from '../../../../contracts/generated/BranchProtectionRule';
export type { SettingsPatch } from '../../../../contracts/generated/SettingsPatch';
export type { SettingsDiffPreview } from '../../../../contracts/generated/SettingsDiffPreview';
export type { SettingsFieldChange } from '../../../../contracts/generated/SettingsFieldChange';
export type { ClientWsMessage } from '../../../../contracts/generated/ClientWsMessage';
export type { ServerWsMessage } from '../../../../contracts/generated/ServerWsMessage';
export type { WebEvent } from '../../../../contracts/generated/WebEvent';
export type { SubscriptionSpec } from '../../../../contracts/generated/SubscriptionSpec';

import type { ReviewThread } from '../../../../contracts/generated/ReviewThread';

// ── Phase 3 frontend-local wire types (W-FE-11). ────────────────────────
// The backend (W-B-* phase 3) emits diff/checks/threads payloads that are
// not yet exported via ts-rs. These mirror the documented contract (see
// `WEB_WORK_CLAUDE.md` §7.4 W-FE-11 / §35.2.4). When the backend lands its
// ts-rs export, these declarations move to `contracts/generated/` and the
// re-export here becomes a one-liner like the others.

/** Per-file diff status emitted by `GET /merge-requests/{iid}/diff`. */
export type MergeRequestFileStatus =
  | 'added'
  | 'modified'
  | 'removed'
  | 'renamed';

/** Risk tier the backend tags onto each changed file. */
export type MergeRequestFileRisk = 'low' | 'medium' | 'high' | 'critical';

/** Single hunk in a unified diff. */
export interface MergeRequestDiffHunk {
  header: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  /** Raw unified-diff body lines (prefixed with `+` / `-` / ` `). */
  lines: string[];
}

/** Per-file diff entry. */
export interface MergeRequestDiffFile {
  path: string;
  /** When `status === 'renamed'`, the previous path. */
  old_path: string | null;
  status: MergeRequestFileStatus;
  additions: number;
  deletions: number;
  risk: MergeRequestFileRisk | null;
  /** Binary diffs carry no hunks; viewer renders a notice. */
  is_binary: boolean;
  hunks: MergeRequestDiffHunk[];
}

/** Wire shape of `GET /merge-requests/{iid}/diff`. */
export interface MergeRequestDiff {
  head_sha: string;
  base_sha: string;
  files: MergeRequestDiffFile[];
  /** True when the server truncated due to size; UI renders a warning. */
  truncated: boolean;
}

/** One CI status check on a MR. */
export interface MergeRequestCheck {
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

/** Wire shape of `GET /merge-requests/{iid}/checks`. */
export interface MergeRequestChecks {
  total: number;
  passing: number;
  failing: number;
  pending: number;
  skipped: number;
  checks: MergeRequestCheck[];
}

/** Wire shape of `GET /merge-requests/{iid}/threads`. */
export interface MergeRequestThreadList {
  /** Re-uses the canonical `ReviewThread` type. */
  threads: ReviewThread[];
}

/** Body for `POST /merge-requests/{iid}/approve`. */
export interface MergeApproveRequest {
  expected_head_sha: string;
  body_markdown?: string | null;
}

/** Body for `POST /merge-requests/{iid}/merge`. */
export interface MergeMergeRequest {
  expected_head_sha: string;
  expected_passport_hash: string | null;
  merge_method: 'merge' | 'squash' | 'rebase';
  commit_title?: string | null;
  commit_message?: string | null;
}
