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

import { RefreshCcw, ShieldAlert } from 'lucide-react';
import { useEffect } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import { SettingsLayout, SettingsSection } from '../components/settings';
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
import { useSettingsDraft } from '../hooks/useSettingsDraft';
import { useSelectionStore } from '../stores/selectionStore';

import {
  AccessSectionView,
  AgentSectionView,
  BranchProtectionView,
  CiSectionView,
  FeaturesSectionView,
  GeneralSectionView,
  MergePolicyView,
  NotificationsSectionView,
  RetentionSectionView,
  SecuritySectionView,
} from './repositorySettingsSections';
import {
  PendingChangesPanel,
  sectionTitleFor,
} from './repositorySettingsPanels';

import './page.css';

function fullNameFromParams(params: Record<string, string | undefined>): string {
  return params.fullName ?? '';
}

export interface RepositorySettingsPageProps {
  provider?: string;
  fullName?: string;
  section?: string;
}

export function RepositorySettingsPage(props: RepositorySettingsPageProps = {}): JSX.Element {
  const params = useParams();
  const navigate = useNavigate();
  const provider = props.provider ?? params.provider ?? 'unknown';
  const fullName = props.fullName ?? fullNameFromParams(params);
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
  const {
    draft,
    setDraft,
    hasPendingPatch,
    setPatch,
    handlePreview,
    handleApply,
    handleDiscard,
    handleRecoverFromHashDrift,
    hashDrift,
  } = useSettingsDraft({ current, settings, previewMutation, applyMutation });

  // ── Guards ─────────────────────────────────────────────────────────
  if (resolved.isPending) {
    return (
      <div className="page" data-testid="repo-settings-page">
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
        <div className="page" data-testid="repo-settings-page">
          <PermissionDeniedState
            description="You do not have permission to view this repository."
            missingPermission="settings.read"
          />
        </div>
      );
    }
    return (
      <div className="page" data-testid="repo-settings-page">
        <ErrorState
          title="Repository not found"
          description={resolved.error?.message ?? `No repository ${fullName}.`}
        />
      </div>
    );
  }

  if (settings.isPending) {
    return (
      <div className="page" data-testid="repo-settings-page">
        <LoadingState title="Loading settings…" variant="skeleton" rows={6} />
      </div>
    );
  }

  if (settings.error || !current || !draft) {
    if (settings.error instanceof ApiError && settings.error.status === 403) {
      return (
        <div className="page" data-testid="repo-settings-page">
          <PermissionDeniedState
            description="You do not have permission to manage settings."
            missingPermission="settings.read"
          />
        </div>
      );
    }
    return (
      <div className="page" data-testid="repo-settings-page">
        <ErrorState title="Could not load settings" error={settings.error} />
      </div>
    );
  }

  // Build the section view.
  const sectionTitle = sectionTitleFor(activeSection);

  return (
    <div className="page page--full">
      <header className="page__header">
        <h1 className="page__title">Settings · {current.general.name}</h1>
        <p className="page__subtitle">{sectionTitle}</p>
      </header>

      {hashDrift ? (
        <div className="pr-cockpit__recovery" role="alert">
          <div className="pr-cockpit__recovery-title">
            <ShieldAlert aria-hidden="true" size={14} />
            Settings changed since you opened this page.
          </div>
          <div className="pr-cockpit__recovery-shas">
            Refresh to load the latest snapshot, then re-preview your edits.
          </div>
          <ActionButton
            variant="primary"
            icon={<RefreshCcw aria-hidden="true" size={12} />}
            onClick={handleRecoverFromHashDrift}
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
              return;
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
        <PendingChangesPanel
          hasPendingPatch={hasPendingPatch}
          hashDrift={Boolean(hashDrift)}
          previewMutation={previewMutation}
          applyMutation={applyMutation}
          onPreview={handlePreview}
          onApply={handleApply}
          onDiscard={handleDiscard}
          onDone={() => {
            applyMutation.reset();
            previewMutation.reset();
            navigate(
              `/repos/${encodeURIComponent(provider)}/${fullName}/settings/${activeSection}`,
              { replace: true }
            );
          }}
        />
      </SettingsLayout>
    </div>
  );
}
