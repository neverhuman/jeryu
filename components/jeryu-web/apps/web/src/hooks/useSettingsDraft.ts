// useSettingsDraft.ts — draft-state controller for the Phase 3 settings studio
// (W-FE-12).
//
// Owns the in-memory `DraftSettings` (the backend `SettingsPatch` plus
// in-memory copies of the nested editors), keeps it synced with the current
// snapshot until the user edits, and wires the preview / apply / discard /
// hash-drift recovery flow against the supplied mutations. The page module
// renders the result; this hook keeps the orchestration logic testable and
// off the component body.

import { useCallback, useEffect, useMemo, useState } from 'react';

import { ApiError } from '../api/client';
import type { useApplySettingsPatch } from './useApplySettingsPatch';
import type { usePreviewSettingsPatch } from './usePreviewSettingsPatch';
import type { useRepoSettings } from './useRepoSettings';
import type {
  AgentSettings,
  BranchProtectionRule,
  MergeSettings,
  RepositorySettings,
  SettingsPatch,
} from '../api/types';

export interface DraftSettings {
  /** Patch the backend currently understands (description, visibility, …). */
  patch: SettingsPatch;
  /** In-memory copies of nested editors so Phase 3 can validate the UX
   *  even before the backend exposes those patch fields. */
  merge: MergeSettings;
  branchProtection: BranchProtectionRule[];
  agents: AgentSettings;
}

export function emptyPatch(): SettingsPatch {
  return {
    description: null,
    homepage: null,
    visibility: null,
    default_branch: null,
    archived: null,
  };
}

export function deriveDraft(current: RepositorySettings): DraftSettings {
  return {
    patch: emptyPatch(),
    merge: current.merge,
    branchProtection: current.branch_protection,
    agents: current.agents,
  };
}

type SettingsQuery = ReturnType<typeof useRepoSettings>;
type PreviewMutation = ReturnType<typeof usePreviewSettingsPatch>;
type ApplyMutation = ReturnType<typeof useApplySettingsPatch>;

export interface UseSettingsDraftArgs {
  current: RepositorySettings | null;
  settings: SettingsQuery;
  previewMutation: PreviewMutation;
  applyMutation: ApplyMutation;
}

export interface UseSettingsDraftResult {
  draft: DraftSettings | null;
  setDraft: React.Dispatch<React.SetStateAction<DraftSettings | null>>;
  hasPendingPatch: boolean;
  setPatch: (next: Partial<SettingsPatch>) => void;
  handlePreview: () => void;
  handleApply: () => void;
  handleDiscard: () => void;
  handleRecoverFromHashDrift: () => void;
  hashDrift: boolean;
}

export function useSettingsDraft({
  current,
  settings,
  previewMutation,
  applyMutation,
}: UseSettingsDraftArgs): UseSettingsDraftResult {
  const [draft, setDraft] = useState<DraftSettings | null>(null);

  // Keep the draft synced with the current snapshot until the user starts
  // editing. After that, only an explicit "Discard" clears it.
  useEffect(() => {
    if (current && !draft) {
      setDraft(deriveDraft(current));
    }
  }, [current, draft]);

  const hasPendingPatch = useMemo(() => {
    if (!draft) return false;
    return Object.values(draft.patch).some((v) => v !== null);
  }, [draft]);

  const setPatch = useCallback((next: Partial<SettingsPatch>) => {
    setDraft((d) => {
      if (!d) return d;
      return { ...d, patch: { ...d.patch, ...next } };
    });
  }, []);

  const handlePreview = useCallback(() => {
    if (!draft) return;
    previewMutation.reset();
    previewMutation.mutate(draft.patch);
  }, [draft, previewMutation]);

  const handleApply = useCallback(() => {
    if (!draft) return;
    const preview = previewMutation.data;
    if (!preview) return;
    applyMutation.reset();
    applyMutation.mutate({
      patch: draft.patch,
      baseSettingsHash: preview.current_hash,
    });
  }, [draft, previewMutation.data, applyMutation]);

  const handleDiscard = useCallback(() => {
    previewMutation.reset();
    applyMutation.reset();
    if (current) setDraft(deriveDraft(current));
  }, [current, previewMutation, applyMutation]);

  const handleRecoverFromHashDrift = useCallback(() => {
    applyMutation.reset();
    previewMutation.reset();
    void settings.refetch();
    // Don't clear the draft automatically — let the user see what they had
    // and re-preview against the refreshed snapshot.
  }, [applyMutation, previewMutation, settings]);

  const hashDrift =
    applyMutation.error instanceof ApiError &&
    applyMutation.error.code === 'settings_hash_stale';

  return {
    draft,
    setDraft,
    hasPendingPatch,
    setPatch,
    handlePreview,
    handleApply,
    handleDiscard,
    handleRecoverFromHashDrift,
    hashDrift,
  };
}
