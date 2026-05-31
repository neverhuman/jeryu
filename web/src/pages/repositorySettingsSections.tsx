// repositorySettingsSections.tsx — per-section renderers for the Phase 3
// settings studio (W-FE-12).
//
// Each component renders one left-nav category against the in-memory
// `RepositorySettings` snapshot. Editable sections (General / Merge policy /
// Branch protection / Agents) stage their edits through the page's draft
// state; the remaining sections are read-only surfaces for Phase 3.
//
// These live alongside `RepositorySettingsPage.tsx` so the page module stays
// focused on orchestration (guards, draft wiring, preview/apply flow).

import {
  AgentPolicyEditor,
  BranchProtectionEditor,
  MergePolicyEditor,
  SecretsMetadataTable,
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

export function FeaturesSectionView({
  current,
}: {
  current: RepositorySettings;
}): JSX.Element {
  const f = current.features;
  return (
    <SettingsSection
      title="Features"
      description="Toggle product features for this repository (read-only in this BFF surface for Phase 3)."
    >
      <ul className="settings-section__body">
        {Object.entries(f).map(([key, value]) => (
          <li key={key} className="settings-section__checkbox">
            <input type="checkbox" checked={value} disabled readOnly />
            <span>{key.replaceAll('_', ' ')}</span>
          </li>
        ))}
      </ul>
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

export function SecuritySectionView({
  current,
}: {
  current: RepositorySettings;
}): JSX.Element {
  const s = current.security;
  return (
    <SettingsSection
      title="Security"
      description="Scanning, sandboxing, license policy."
    >
      <ul>
        <li className="settings-section__checkbox">
          <input type="checkbox" checked={s.secret_scanning} disabled readOnly />
          Secret scanning
        </li>
        <li className="settings-section__checkbox">
          <input
            type="checkbox"
            checked={s.dependency_scanning}
            disabled
            readOnly
          />
          Dependency scanning
        </li>
        <li className="settings-section__checkbox">
          <input
            type="checkbox"
            checked={s.license_policy_enabled}
            disabled
            readOnly
          />
          License policy
        </li>
        <li className="settings-section__checkbox">
          <input
            type="checkbox"
            checked={s.agent_sandbox_required}
            disabled
            readOnly
          />
          Agent sandbox required
        </li>
      </ul>
      <SecretsMetadataTable secrets={[]} />
    </SettingsSection>
  );
}

export function AccessSectionView({
  current,
}: {
  current: RepositorySettings;
}): JSX.Element {
  const a = current.access;
  return (
    <SettingsSection
      title="Access"
      description="Collaborators, teams, deploy keys, app installations."
    >
      <dl className="page__meta-grid">
        <dt>Collaborators</dt>
        <dd>{a.collaborators_count}</dd>
        <dt>Teams</dt>
        <dd>{a.teams_count}</dd>
        <dt>Deploy keys</dt>
        <dd>{a.deploy_keys_count}</dd>
        <dt>App installations</dt>
        <dd>{a.app_installations_count}</dd>
      </dl>
    </SettingsSection>
  );
}

export function CiSectionView({
  current,
}: {
  current: RepositorySettings;
}): JSX.Element {
  const c = current.ci;
  return (
    <SettingsSection
      title="CI / Workflows"
      description="Runner pools, concurrency, retention."
    >
      <dl className="page__meta-grid">
        <dt>Default runner pool</dt>
        <dd>{c.default_runner_pool ?? '—'}</dd>
        <dt>Concurrency limit</dt>
        <dd>{c.concurrency_limit ?? '∞'}</dd>
        <dt>Artifact retention (days)</dt>
        <dd>{c.artifact_retention_days}</dd>
        <dt>Log retention (days)</dt>
        <dd>{c.log_retention_days}</dd>
        <dt>VTI enabled</dt>
        <dd>{c.vti_enabled ? 'yes' : 'no'}</dd>
      </dl>
    </SettingsSection>
  );
}

export function NotificationsSectionView({
  current,
}: {
  current: RepositorySettings;
}): JSX.Element {
  const n = current.notifications;
  return (
    <SettingsSection
      title="Notifications"
      description="Watch defaults, alerting cases."
    >
      <dl className="page__meta-grid">
        <dt>Watch default</dt>
        <dd>{n.watch_default}</dd>
        <dt>Notify on CI failure</dt>
        <dd>{n.notify_on_ci_failure ? 'yes' : 'no'}</dd>
        <dt>Notify on agent completion</dt>
        <dd>{n.notify_on_agent_completion ? 'yes' : 'no'}</dd>
        <dt>Notify on release</dt>
        <dd>{n.notify_on_release ? 'yes' : 'no'}</dd>
      </dl>
    </SettingsSection>
  );
}

export function RetentionSectionView({
  current,
}: {
  current: RepositorySettings;
}): JSX.Element {
  const r = current.retention;
  return (
    <SettingsSection
      title="Retention"
      description="Audit, evidence, workflow runs, log retention windows."
    >
      <dl className="page__meta-grid">
        <dt>Audit (days)</dt>
        <dd>{r.audit_days}</dd>
        <dt>Evidence (days)</dt>
        <dd>{r.evidence_days}</dd>
        <dt>Workflow runs (days)</dt>
        <dd>{r.workflow_run_days}</dd>
        <dt>Logs (days)</dt>
        <dd>{r.log_days}</dd>
      </dl>
    </SettingsSection>
  );
}
