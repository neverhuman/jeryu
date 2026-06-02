// repositorySettingsReadonlySections.tsx — read-only section renderers for
// the Phase 3 settings studio (W-FE-12).
//
// These surfaces render one left-nav category against the in-memory
// `RepositorySettings` snapshot without staging edits (Features / Security /
// Access / CI / Notifications / Retention are read-only in the Phase 3 BFF
// surface). The editable sections live in `./repositorySettingsSections`,
// which re-exports these so import sites see one module.

import { SecretsMetadataTable, SettingsSection } from '../components/settings';
import type { RepositorySettings } from '../api/types';

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
            <input type="checkbox" checked={Boolean(value)} disabled readOnly />
            <span>{key.replaceAll('_', ' ')}</span>
          </li>
        ))}
      </ul>
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
