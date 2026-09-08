// repositorySettingsPanels.tsx — shared panels/helpers for the settings page.
//
// Holds the human title lookup for the active left-nav section and the
// "Pending changes" panel that wires the preview→apply flow (preview button,
// diff render, apply button, success + error surfaces). Extracting these
// keeps `RepositorySettingsPage` focused on guards and draft orchestration.

import { Check } from 'lucide-react';

import { ActionButton } from '../components/action/ActionButton';
import { SettingsDiffPreview, SettingsSection } from '../components/settings';
import { ErrorState } from '../components/state';
import type {
  useApplySettingsPatch,
} from '../hooks/useApplySettingsPatch';
import type {
  usePreviewSettingsPatch,
} from '../hooks/usePreviewSettingsPatch';

const SECTION_TITLES: Record<string, string> = {
  general: 'General',
  'merge-policy': 'Merge policy',
  'branch-protection': 'Branch protection',
  features: 'Features',
  ci: 'CI / Workflows',
  agents: 'Agents',
  access: 'Access',
  security: 'Security',
  notifications: 'Notifications',
  retention: 'Retention',
  'danger-zone': 'Danger zone',
};

export function sectionTitleFor(section: string): string {
  return SECTION_TITLES[section] ?? 'General';
}

type PreviewMutation = ReturnType<typeof usePreviewSettingsPatch>;
type ApplyMutation = ReturnType<typeof useApplySettingsPatch>;

export interface PendingChangesPanelProps {
  hasPendingPatch: boolean;
  hashDrift: boolean;
  previewMutation: PreviewMutation;
  applyMutation: ApplyMutation;
  onPreview: () => void;
  onApply: () => void;
  onDiscard: () => void;
  onDone: () => void;
}

export function PendingChangesPanel({
  hasPendingPatch,
  hashDrift,
  previewMutation,
  applyMutation,
  onPreview,
  onApply,
  onDiscard,
  onDone,
}: PendingChangesPanelProps): JSX.Element {
  return (
    <SettingsSection
      title="Pending changes"
      description="Preview the diff and apply when you're confident in the blast radius."
      actions={
        <>
          <ActionButton
            variant="default"
            onClick={onDiscard}
            disabled={
              applyMutation.isPending ||
              previewMutation.isPending ||
              !hasPendingPatch
            }
          >
            Discard
          </ActionButton>
          <ActionButton
            variant="primary"
            onClick={onPreview}
            disabled={!hasPendingPatch || previewMutation.isPending}
          >
            {previewMutation.isPending ? 'Previewing…' : 'Preview changes'}
          </ActionButton>
        </>
      }
    >
      <SettingsDiffPreview
        preview={previewMutation.data ?? null}
        isLoading={previewMutation.isPending}
      />
      {previewMutation.error ? (
        <ErrorState title="Preview failed" error={previewMutation.error} />
      ) : null}
      {previewMutation.data && previewMutation.data.diffs.length > 0 ? (
        <div className="settings-section__actions">
          <ActionButton
            variant="primary"
            icon={<Check aria-hidden="true" size={12} />}
            onClick={onApply}
            disabled={applyMutation.isPending}
          >
            {applyMutation.isPending ? 'Applying…' : 'Apply changes'}
          </ActionButton>
        </div>
      ) : null}
      {applyMutation.isSuccess ? (
        <p className="settings-section__description">
          Changes applied — audit entry recorded.
        </p>
      ) : null}
      {applyMutation.error && !hashDrift ? (
        <ErrorState title="Apply failed" error={applyMutation.error} />
      ) : null}
      {applyMutation.isSuccess ? (
        <ActionButton variant="default" onClick={onDone}>
          Done
        </ActionButton>
      ) : null}
    </SettingsSection>
  );
}
