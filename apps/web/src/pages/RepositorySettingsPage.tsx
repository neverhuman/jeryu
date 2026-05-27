// RepositorySettingsPage.tsx — Phase 3 settings studio (W-FE-12).
//
// Layout: left nav (per §7.4 W-FE-12 categories) + main pane.
// Flow per section:
//   1. User edits fields → "Preview changes" button enables.
//   2. Click → POST `/settings/preview` → renders `<SettingsDiffPreview>`.
//   3. Click "Apply" → PATCH `/settings` with `Idempotency-Key` and
//      `If-Match: "<base_settings_hash>"`.
//   4. On 409 `settings_hash_stale` → show recovery banner that refetches
//      the current snapshot and discards the staged patch.
//
// Phase 3 wires the General / Features / Merge policy / Branch protection /
// Agents / Security / Notifications / Retention sections to the backend
// `SettingsPatch` (which currently exposes general fields per
// `contracts/generated/SettingsPatch`). Deeper editors render against the
// in-memory copy of `RepositorySettings` and stage to the patch via the
// `pendingPatch` state so they're ready when the backend adds those fields.

import { Check, RefreshCcw, ShieldAlert } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import {
  AgentPolicyEditor,
  BranchProtectionEditor,
  MergePolicyEditor,
  SecretsMetadataTable,
  SettingsDiffPreview,
  SettingsLayout,
  SettingsSection,
} from '../components/settings';
import {
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useApplySettingsPatch } from '../hooks/useApplySettingsPatch';
import { usePreviewSettingsPatch } from '../hooks/usePreviewSettingsPatch';
import { useRealtime } from '../hooks/useRealtime';
import { useRepoSettings } from '../hooks/useRepoSettings';
import { useResolveRepo } from '../hooks/useResolveRepo';
import { useSelectionStore } from '../stores/selectionStore';
import type {
  AgentSettings,
  BranchProtectionRule,
  MergeSettings,
  RepositorySettings,
  RepositoryVisibility,
  SettingsPatch,
} from '../api/types';

import './page.css';

function fullNameFromParams(params: Record<string, string | undefined>): string {
  return params.fullName ?? '';
}

interface DraftSettings {
  /** Patch the backend currently understands (description, visibility, …). */
  patch: SettingsPatch;
  /** In-memory copies of nested editors so Phase 3 can validate the UX
   *  even before the backend exposes those patch fields. */
  merge: MergeSettings;
  branchProtection: BranchProtectionRule[];
  agents: AgentSettings;
}

function emptyPatch(): SettingsPatch {
  return {
    description: null,
    homepage: null,
    visibility: null,
    default_branch: null,
    archived: null,
  };
}

function deriveDraft(current: RepositorySettings): DraftSettings {
  return {
    patch: emptyPatch(),
    merge: current.merge,
    branchProtection: current.branch_protection,
    agents: current.agents,
  };
}

export function RepositorySettingsPage(): JSX.Element {
  const params = useParams();
  const navigate = useNavigate();
  const provider = params.provider ?? 'unknown';
  const fullName = fullNameFromParams(params);
  const activeSection = params.section ?? 'general';

  const resolved = useResolveRepo(provider, fullName);
  const repoId = resolved.data?.id ?? null;
  const setRepo = useSelectionStore((s) => s.setCurrentRepo);

  useEffect(() => {
    setRepo(repoId);
    return () => setRepo(null);
  }, [repoId, setRepo]);

  useRealtime(repoId ? [`repo.${repoId}`] : []);

  const settings = useRepoSettings(repoId);
  const previewMutation = usePreviewSettingsPatch(repoId);
  const applyMutation = useApplySettingsPatch(repoId);

  const current = settings.data ?? null;
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

  const handleRecoverFromStaleHash = useCallback(() => {
    applyMutation.reset();
    previewMutation.reset();
    void settings.refetch();
    // Don't clear the draft automatically — let the user see what they had
    // and re-preview against the refreshed snapshot.
  }, [applyMutation, previewMutation, settings]);

  const staleHash =
    applyMutation.error instanceof ApiError &&
    applyMutation.error.code === 'settings_hash_stale';

  // ── Guards ─────────────────────────────────────────────────────────
  if (resolved.isPending) {
    return (
      <div className="page">
        <LoadingState
          title="Loading repository…"
          variant="message"
          description="Resolving the repository."
        />
      </div>
    );
  }

  if (resolved.error || !resolved.data) {
    if (resolved.error instanceof ApiError && resolved.error.status === 403) {
      return (
        <div className="page">
          <PermissionDeniedState
            description="You do not have permission to view this repository."
            missingPermission="settings.read"
          />
        </div>
      );
    }
    return (
      <div className="page">
        <ErrorState
          title="Repository not found"
          description={resolved.error?.message ?? `No repository ${fullName}.`}
        />
      </div>
    );
  }

  if (settings.isPending) {
    return (
      <div className="page">
        <LoadingState title="Loading settings…" variant="skeleton" rows={6} />
      </div>
    );
  }

  if (settings.error || !current || !draft) {
    if (settings.error instanceof ApiError && settings.error.status === 403) {
      return (
        <div className="page">
          <PermissionDeniedState
            description="You do not have permission to manage settings."
            missingPermission="settings.read"
          />
        </div>
      );
    }
    return (
      <div className="page">
        <ErrorState title="Could not load settings" error={settings.error} />
      </div>
    );
  }

  // Build the section view.
  const sectionTitle =
    activeSection === 'general'
      ? 'General'
      : activeSection === 'merge-policy'
      ? 'Merge policy'
      : activeSection === 'branch-protection'
      ? 'Branch protection'
      : activeSection === 'features'
      ? 'Features'
      : activeSection === 'ci'
      ? 'CI / Workflows'
      : activeSection === 'agents'
      ? 'Agents'
      : activeSection === 'access'
      ? 'Access'
      : activeSection === 'security'
      ? 'Security'
      : activeSection === 'notifications'
      ? 'Notifications'
      : activeSection === 'retention'
      ? 'Retention'
      : activeSection === 'danger-zone'
      ? 'Danger zone'
      : 'General';

  return (
    <div className="page page--full">
      <header className="page__header">
        <h1 className="page__title">Settings · {current.general.name}</h1>
        <p className="page__subtitle">{sectionTitle}</p>
      </header>

      {staleHash ? (
        <div className="mr-cockpit__recovery" role="alert">
          <div className="mr-cockpit__recovery-title">
            <ShieldAlert aria-hidden="true" size={14} />
            Settings changed since you opened this page.
          </div>
          <div className="mr-cockpit__recovery-shas">
            Refresh to load the latest snapshot, then re-preview your edits.
          </div>
          <ActionButton
            variant="primary"
            icon={<RefreshCcw aria-hidden="true" size={12} />}
            onClick={handleRecoverFromStaleHash}
          >
            Refresh
          </ActionButton>
        </div>
      ) : null}

      <SettingsLayout
        activeSection={activeSection}
        hrefFor={(id) =>
          `/repos/${encodeURIComponent(provider)}/${fullName}/settings/${id}`
        }
        renderLink={({ section, href, children }) => (
          <Link
            to={href}
            replace
            onClick={(e) => {
              // Prevent the dirty draft from being lost silently — confirm
              // when the user navigates with pending changes.
              if (hasPendingPatch) {
                const ok = window.confirm(
                  `Discard pending changes to switch sections?`
                );
                if (!ok) {
                  e.preventDefault();
                  return;
                }
                handleDiscard();
              }
              setRepo(repoId);
              return undefined;
            }}
            aria-current={
              section.id === activeSection ? 'page' : undefined
            }
          >
            {children}
          </Link>
        )}
      >
        {activeSection === 'general' && (
          <GeneralSectionView
            current={current}
            patch={draft.patch}
            setPatch={setPatch}
            disabled={applyMutation.isPending}
          />
        )}

        {activeSection === 'features' && (
          <FeaturesSectionView current={current} />
        )}

        {activeSection === 'merge-policy' && (
          <MergePolicyView
            value={draft.merge}
            onChange={(merge) =>
              setDraft((d) => (d ? { ...d, merge } : d))
            }
            disabled={applyMutation.isPending}
          />
        )}

        {activeSection === 'branch-protection' && (
          <BranchProtectionView
            rules={draft.branchProtection}
            onChange={(branchProtection) =>
              setDraft((d) => (d ? { ...d, branchProtection } : d))
            }
            disabled={applyMutation.isPending}
          />
        )}

        {activeSection === 'agents' && (
          <AgentSectionView
            value={draft.agents}
            onChange={(agents) =>
              setDraft((d) => (d ? { ...d, agents } : d))
            }
            disabled={applyMutation.isPending}
          />
        )}

        {activeSection === 'security' && (
          <SecuritySectionView current={current} />
        )}

        {activeSection === 'access' && <AccessSectionView current={current} />}

        {activeSection === 'ci' && <CiSectionView current={current} />}

        {activeSection === 'notifications' && (
          <NotificationsSectionView current={current} />
        )}

        {activeSection === 'retention' && (
          <RetentionSectionView current={current} />
        )}

        {activeSection === 'danger-zone' && (
          <SettingsSection
            title="Danger zone"
            description="Irreversible repository operations."
          >
            <div className="settings-danger">
              <p className="settings-danger__title">Archive repository</p>
              <p className="settings-danger__hint">
                Archiving freezes the repository to read-only. Run via the
                Action Palette so the side-effect preview confirms blast
                radius first.
              </p>
            </div>
          </SettingsSection>
        )}

        {/* Pending changes panel — always visible while a section is open. */}
        <SettingsSection
          title="Pending changes"
          description="Preview the diff and apply when you're confident in the blast radius."
          actions={
            <>
              <ActionButton
                variant="default"
                onClick={handleDiscard}
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
                onClick={handlePreview}
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
            <ErrorState
              title="Preview failed"
              error={previewMutation.error}
            />
          ) : null}
          {previewMutation.data && previewMutation.data.diffs.length > 0 ? (
            <div className="settings-section__actions">
              <ActionButton
                variant="primary"
                icon={<Check aria-hidden="true" size={12} />}
                onClick={handleApply}
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
          {applyMutation.error && !staleHash ? (
            <ErrorState
              title="Apply failed"
              error={applyMutation.error}
            />
          ) : null}
          {applyMutation.isSuccess ? (
            <ActionButton
              variant="default"
              onClick={() => {
                applyMutation.reset();
                previewMutation.reset();
                navigate(
                  `/repos/${encodeURIComponent(provider)}/${fullName}/settings/${activeSection}`,
                  { replace: true }
                );
              }}
            >
              Done
            </ActionButton>
          ) : null}
        </SettingsSection>
      </SettingsLayout>
    </div>
  );
}

// ─── Per-section renderers (kept local to the page module) ──────────────

function GeneralSectionView({
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

function FeaturesSectionView({
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

function MergePolicyView({
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

function BranchProtectionView({
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

function AgentSectionView({
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

function SecuritySectionView({
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

function AccessSectionView({
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

function CiSectionView({
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

function NotificationsSectionView({
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

function RetentionSectionView({
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
