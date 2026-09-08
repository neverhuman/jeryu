// types/generated.ts — re-exports the generated DTO types (W-FE-03).
//
// All wire types live in `contracts/generated/*.ts` (produced by ts-rs from
// the Rust API surface). We re-export the subset used by the SPA so app code
// imports from `@/api/types` (logical boundary) rather than reaching into the
// generated tree directly.
//
// When a new DTO is needed, add a new export here — do not edit the generated
// files.

export type { WebBootstrap } from '../../../../../contracts/generated/WebBootstrap';
export type { Viewer } from '../../../../../contracts/generated/Viewer';
export type { WebFeatureFlags } from '../../../../../contracts/generated/WebFeatureFlags';
export type { RepositorySummary } from '../../../../../contracts/generated/RepositorySummary';
export type { RepositoryId } from '../../../../../contracts/generated/RepositoryId';
export type { RepositoryRole } from '../../../../../contracts/generated/RepositoryRole';
export type { RepositoryVisibility } from '../../../../../contracts/generated/RepositoryVisibility';
export type { RepositoryListResponse } from '../../../../../contracts/generated/RepositoryListResponse';
export type { ToolFleetResponse } from '../../../../../contracts/generated/ToolFleetResponse';
export type { ToolFleetEntry } from '../../../../../contracts/generated/ToolFleetEntry';
export type { RepositoryMirrorStatus } from '../../../../../contracts/generated/RepositoryMirrorStatus';
export type { DeleteRepositoryRequest } from '../../../../../contracts/generated/DeleteRepositoryRequest';
export type { DeleteRepositoryReceipt } from '../../../../../contracts/generated/DeleteRepositoryReceipt';
export type { DeletedCount } from '../../../../../contracts/generated/DeletedCount';
export type { RefSelectorItem } from '../../../../../contracts/generated/RefSelectorItem';
export type { RefKind } from '../../../../../contracts/generated/RefKind';
export type { TreeEntry } from '../../../../../contracts/generated/TreeEntry';
export type { TreeEntryKind } from '../../../../../contracts/generated/TreeEntryKind';
export type { BlobResponse } from '../../../../../contracts/generated/BlobResponse';
export type { BlobEncoding } from '../../../../../contracts/generated/BlobEncoding';
export type { RenderedMarkdown } from '../../../../../contracts/generated/RenderedMarkdown';
export type { MarkdownHeading } from '../../../../../contracts/generated/MarkdownHeading';
export type { MarkdownLink } from '../../../../../contracts/generated/MarkdownLink';
export type { PullRequestSummary } from '../../../../../contracts/generated/PullRequestSummary';
export type { PullRequestDetail } from '../../../../../contracts/generated/PullRequestDetail';
export type { PullRequestState } from '../../../../../contracts/generated/PullRequestState';
export type { MergePassport } from '../../../../../contracts/generated/MergePassport';
export type { MergePassportBlocker } from '../../../../../contracts/generated/MergePassportBlocker';
export type { MergePassportStatus } from '../../../../../contracts/generated/MergePassportStatus';
export type { Mergeability } from '../../../../../contracts/generated/Mergeability';
export type { ReviewThread } from '../../../../../contracts/generated/ReviewThread';
export type { ReviewComment } from '../../../../../contracts/generated/ReviewComment';
export type { ReviewVerdict } from '../../../../../contracts/generated/ReviewVerdict';
export type { ReviewPosture } from '../../../../../contracts/generated/ReviewPosture';
export type { ReviewSuggestion } from '../../../../../contracts/generated/ReviewSuggestion';
export type { SubmitReviewRequest } from '../../../../../contracts/generated/SubmitReviewRequest';
export type { CreateReviewCommentRequest } from '../../../../../contracts/generated/CreateReviewCommentRequest';
export type { CreateRepositoryRequest } from '../../../../../contracts/generated/CreateRepositoryRequest';
export type { CreateRepositoryPreview } from '../../../../../contracts/generated/CreateRepositoryPreview';
export type { IssueSummary } from '../../../../../contracts/generated/IssueSummary';
export type { IssueState } from '../../../../../contracts/generated/IssueState';
export type { CreateWorkCommentRequest } from '../../../../../contracts/generated/CreateWorkCommentRequest';
export type { CreateWorkItemRequest } from '../../../../../contracts/generated/CreateWorkItemRequest';
export type { CreateWorkLinkRequest } from '../../../../../contracts/generated/CreateWorkLinkRequest';
export type { UpdateWorkItemRequest } from '../../../../../contracts/generated/UpdateWorkItemRequest';
export type { WorkComment } from '../../../../../contracts/generated/WorkComment';
export type { WorkFilter } from '../../../../../contracts/generated/WorkFilter';
export type { WorkIssueLink } from '../../../../../contracts/generated/WorkIssueLink';
export type { WorkItem } from '../../../../../contracts/generated/WorkItem';
export type { WorkItemDetail } from '../../../../../contracts/generated/WorkItemDetail';
export type { WorkItemKind } from '../../../../../contracts/generated/WorkItemKind';
export type { WorkItemListResponse } from '../../../../../contracts/generated/WorkItemListResponse';
export type { WorkPrincipal } from '../../../../../contracts/generated/WorkPrincipal';
export type { WorkPrincipalKind } from '../../../../../contracts/generated/WorkPrincipalKind';
export type { WorkPriority } from '../../../../../contracts/generated/WorkPriority';
export type { WorkPullRequestLink } from '../../../../../contracts/generated/WorkPullRequestLink';
export type { WorkRepository } from '../../../../../contracts/generated/WorkRepository';
export type { WorkStatus } from '../../../../../contracts/generated/WorkStatus';
export type { AgentPosture } from '../../../../../contracts/generated/AgentPosture';
export type { AgentSettings } from '../../../../../contracts/generated/AgentSettings';
export type { AccessSettings } from '../../../../../contracts/generated/AccessSettings';
export type { CheckPosture } from '../../../../../contracts/generated/CheckPosture';
export type { CiSettings } from '../../../../../contracts/generated/CiSettings';
export type { GeneralSettings } from '../../../../../contracts/generated/GeneralSettings';
export type { FeatureSettings } from '../../../../../contracts/generated/FeatureSettings';
export type { MergeSettings } from '../../../../../contracts/generated/MergeSettings';
export type { NotificationSettings } from '../../../../../contracts/generated/NotificationSettings';
export type { RepositorySettings } from '../../../../../contracts/generated/RepositorySettings';
export type { RepositoryHostKind } from '../../../../../contracts/generated/RepositoryHostKind';
export type { RepositoryFacets } from '../../../../../contracts/generated/RepositoryFacets';
export type { RetentionSettings } from '../../../../../contracts/generated/RetentionSettings';
export type { SecuritySettings } from '../../../../../contracts/generated/SecuritySettings';
export type { BranchProtectionRule } from '../../../../../contracts/generated/BranchProtectionRule';
export type { SettingsPatch } from '../../../../../contracts/generated/SettingsPatch';
export type { SettingsDiffPreview } from '../../../../../contracts/generated/SettingsDiffPreview';
export type { SettingsFieldChange } from '../../../../../contracts/generated/SettingsFieldChange';
export type { ClientWsMessage } from '../../../../../contracts/generated/ClientWsMessage';
export type { ServerWsMessage } from '../../../../../contracts/generated/ServerWsMessage';
export type { WebEvent } from '../../../../../contracts/generated/WebEvent';
export type { SubscriptionSpec } from '../../../../../contracts/generated/SubscriptionSpec';

// ── Reusable-tool registry summary. ──────────────────────────────────────
// Wire shape of `GET /api/v1/tools/registry/summary`, powering the gold "tool
// control plane" box at the top of the repositories grid. Owned by the Rust
// read-model exporter (ts-rs), re-exported here like the other generated DTOs.
export type { ToolRegistrySummary } from '../../../../../contracts/generated/ToolRegistrySummary';
export type { ToolRegistryEntry } from '../../../../../contracts/generated/ToolRegistryEntry';
export type { ToolFinderDashboard } from '../../../../../contracts/generated/ToolFinderDashboard';
export type { ToolFinderPatternFamily } from '../../../../../contracts/generated/ToolFinderPatternFamily';
export type { ToolFinderCluster } from '../../../../../contracts/generated/ToolFinderCluster';
export type { ToolFinderOccurrence } from '../../../../../contracts/generated/ToolFinderOccurrence';
export type { ToolFinderScanStatus } from '../../../../../contracts/generated/ToolFinderScanStatus';
export type { ToolFinderScanMeta } from '../../../../../contracts/generated/ToolFinderScanMeta';
export type { ToolFinderProposeReceipt } from '../../../../../contracts/generated/ToolFinderProposeReceipt';
