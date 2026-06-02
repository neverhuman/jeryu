// repositorySettingsSections.tsx — editable section renderers for the Phase 3
// settings studio (W-FE-12).
//
// Each component renders one left-nav category against the in-memory
// `RepositorySettings` snapshot. The editable sections here (General / Merge
// policy / Branch protection / Agents) stage their edits through the page's
// draft state; the read-only surfaces live in
// `./repositorySettingsReadonlySections` and are re-exported below so import
// sites see a single module.
//
// These live alongside `RepositorySettingsPage.tsx` so the page module stays
// focused on orchestration (guards, draft wiring, preview/apply flow).

import {
  AgentPolicyEditor,
  BranchProtectionEditor,
  MergePolicyEditor,
  SettingsSection,
} from '../components/settings';
import type {
  AgentSettings,
  BranchProtectionRule,
  MergeSettings,
  RepositorySettings,
  RepositoryVisibility,
  SettingsPatch,
} from '../api/types';

export {
  AccessSectionView,
  CiSectionView,
  FeaturesSectionView,
  NotificationsSectionView,
  RetentionSectionView,
  SecuritySectionView,
} from './repositorySettingsReadonlySections';

export function GeneralSectionView({
  current,
  patch,
  setPatch,
  disabled,
}: {
  current: RepositorySettings;
  patch: SettingsPatch;
  setPatch: (next: Partial<SettingsPatch>) => void;
  disabled: boolean;
}): JSX.Element {
  return (
    <SettingsSection
      title="General"
      description="Name, description, visibility, default branch."
    >
      <div className="settings-section__row">
        <div className="settings-section__field">
          <label htmlFor="settings-name">Name</label>
          <input
            id="settings-name"
            type="text"
            value={current.general.name}
            disabled
            readOnly
          />
        </div>
        <div className="settings-section__field">
          <label htmlFor="settings-default-branch">Default branch</label>
          <input
            id="settings-default-branch"
            type="text"
            value={patch.default_branch ?? current.general.default_branch}
            onChange={(e) => setPatch({ default_branch: e.target.value })}
            disabled={disabled}
          />
        </div>
      </div>
      <div className="settings-section__field">
        <label htmlFor="settings-description">Description</label>
        <input
          id="settings-description"
          type="text"
          value={patch.description ?? current.general.description ?? ''}
          onChange={(e) => setPatch({ description: e.target.value })}
          disabled={disabled}
        />
      </div>
      <div className="settings-section__field">
        <label htmlFor="settings-homepage">Homepage URL</label>
        <input
          id="settings-homepage"
          type="text"
          value={patch.homepage ?? current.general.homepage ?? ''}
          onChange={(e) => setPatch({ homepage: e.target.value })}
          disabled={disabled}
        />
      </div>
      <div className="settings-section__field">
        <label htmlFor="settings-visibility">Visibility</label>
        <select
          id="settings-visibility"
          value={patch.visibility ?? current.general.visibility}
          onChange={(e) =>
            setPatch({
              visibility: e.target.value as RepositoryVisibility,
            })
          }
          disabled={disabled}
        >
          <option value="public">public</option>
          <option value="internal">internal</option>
          <option value="private">private</option>
        </select>
      </div>
      <label className="settings-section__checkbox">
        <input
          type="checkbox"
          checked={patch.archived ?? current.general.archived}
          onChange={(e) => setPatch({ archived: e.target.checked })}
          disabled={disabled}
        />
        Archived
      </label>
    </SettingsSection>
  );
}

export function MergePolicyView({
  value,
  onChange,
  disabled,
}: {
  value: MergeSettings;
  onChange: (next: MergeSettings) => void;
  disabled: boolean;
}): JSX.Element {
  return (
    <SettingsSection
      title="Merge policy"
      description="Allowed merge methods, approvals, Merge Passport."
    >
      <MergePolicyEditor
        value={value}
        onChange={onChange}
        disabled={disabled}
      />
    </SettingsSection>
  );
}

export function BranchProtectionView({
  rules,
  onChange,
  disabled,
}: {
  rules: BranchProtectionRule[];
  onChange: (rules: BranchProtectionRule[]) => void;
  disabled: boolean;
}): JSX.Element {
  return (
    <SettingsSection
      title="Branch protection"
      description="Per-pattern rules for required checks, approvals, and force-push behavior."
    >
      <BranchProtectionEditor
        rules={rules}
        onChange={onChange}
        disabled={disabled}
      />
    </SettingsSection>
  );
}

export function AgentSectionView({
  value,
  onChange,
  disabled,
}: {
  value: AgentSettings;
  onChange: (next: AgentSettings) => void;
  disabled: boolean;
}): JSX.Element {
  return (
    <SettingsSection
      title="Agents"
      description="Policy for autonomous coding agents."
    >
      <AgentPolicyEditor
        value={value}
        onChange={onChange}
        disabled={disabled}
      />
    </SettingsSection>
  );
}
